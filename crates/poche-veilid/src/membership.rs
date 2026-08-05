use std::{fmt, future::Future};

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{
    CommandEnvelope, CommandPayload, EventEnvelope, PROTOCOL_VERSION_V1, PrincipalId, RoomId,
    SIGNATURE_DOMAIN_V1, SignatureAlgorithm, SignatureMetadata, SnapshotEnvelope,
};
use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "insecure-development"))]
use crate::ExplicitInsecureDevelopment;
use crate::{
    ApplicationIdentity, ApplicationPublicIdentity, IdentityCryptoError, IdentityStoragePolicy,
    RendezvousError, RendezvousRecord, RoomCode, RoomNetwork, RoomNetworkWire, StorageSecurity,
    verify_application_bytes, verify_command_signature, verify_event_signature,
    verify_snapshot_signature,
};

const MEMBERSHIP_SCHEMA_VERSION: u16 = 1;
const MEMBERSHIP_SIGNATURE_DOMAIN: &[u8] = b"POCHE\0MEMBERSHIP\0V1";
const RECONNECT_SIGNATURE_DOMAIN: &[u8] = b"POCHE\0RECONNECT-ROUTE\0V1";
const MEMBERSHIP_BLOB_MAGIC: &[u8; 4] = b"PML1";
const MAX_RECIPIENT_ROUTE_BYTES: usize = 8 * 1024;
const MAX_RECOVERY_EVENTS: usize = 1024;
const MAX_RECOVERY_SNAPSHOT_STATE_BYTES: usize = 8 * 1024;

/// Host-signed durable membership bound to one stable application identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipGrant {
    pub schema_version: u16,
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub host_principal: PrincipalId,
    pub member_identity: ApplicationPublicIdentity,
    pub membership_epoch: u64,
    pub credential_epoch: u64,
    pub issued_revision: u64,
}

/// Signed membership certificate. It grants no authority after current host
/// state has removed or revoked the member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipCredential {
    pub grant: MembershipGrant,
    pub signature: SignatureMetadata,
}

/// Stable credential/locator/reconnect failure categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipError {
    InvalidCredential,
    InvalidLocator,
    InvalidReconnect,
    InvalidRecovery,
    Oversized,
    ExpiredRendezvous,
    Missing,
    BackendUnavailable,
    InsecureOptInRequired,
    CorruptStoredMembership,
}

impl MembershipCredential {
    /// Issue a host-signed durable credential for a joined stable identity.
    ///
    /// # Errors
    ///
    /// Rejects invalid identities/epochs or signing failures.
    pub fn issue(
        host: &ApplicationIdentity,
        room_id: RoomId,
        member_identity: ApplicationPublicIdentity,
        membership_epoch: u64,
        credential_epoch: u64,
        issued_revision: u64,
    ) -> Result<Self, MembershipError> {
        member_identity
            .validate()
            .map_err(|_| MembershipError::InvalidCredential)?;
        if membership_epoch == 0 || credential_epoch == 0 {
            return Err(MembershipError::InvalidCredential);
        }
        let host_public = host.public();
        let grant = MembershipGrant {
            schema_version: MEMBERSHIP_SCHEMA_VERSION,
            protocol_version: PROTOCOL_VERSION_V1,
            room_id,
            host_principal: host_public.principal_id.clone(),
            member_identity,
            membership_epoch,
            credential_epoch,
            issued_revision,
        };
        let signature = host
            .sign_application_bytes(&membership_signed_bytes(&grant)?)
            .map_err(|_| MembershipError::InvalidCredential)?;
        Ok(Self {
            grant,
            signature: SignatureMetadata {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: host_public.principal_id,
                signature,
            },
        })
    }

    /// Verify the credential against the expected stable host identity.
    ///
    /// # Errors
    ///
    /// Fails closed for any schema, identity, key, or signature mismatch.
    pub fn verify(&self, host: &ApplicationPublicIdentity) -> Result<(), MembershipError> {
        if self.grant.schema_version != MEMBERSHIP_SCHEMA_VERSION
            || self.grant.protocol_version != PROTOCOL_VERSION_V1
            || self.grant.membership_epoch == 0
            || self.grant.credential_epoch == 0
            || self.grant.host_principal != host.principal_id
            || self.signature.domain_version != SIGNATURE_DOMAIN_V1
            || self.signature.algorithm != SignatureAlgorithm::Ed25519
            || self.signature.key_id != host.principal_id
            || self.grant.member_identity.validate().is_err()
        {
            return Err(MembershipError::InvalidCredential);
        }
        verify_application_bytes(
            host,
            &membership_signed_bytes(&self.grant)?,
            &self.signature.signature,
        )
        .map_err(|_| MembershipError::InvalidCredential)
    }
}

/// Persisted rendezvous capability plus membership credential. The original
/// invite secret is intentionally absent.
pub struct MembershipLocator {
    network: RoomNetwork,
    room_id: RoomId,
    encrypted_record_key: Vec<u8>,
    host_identity: ApplicationPublicIdentity,
    credential: MembershipCredential,
}

impl fmt::Debug for MembershipLocator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MembershipLocator")
            .field("network", &self.network)
            .field("room_id", &self.room_id)
            .field("encrypted_record_key", &"<redacted>")
            .field("host_identity", &self.host_identity)
            .field("credential", &self.credential)
            .finish()
    }
}

impl Drop for MembershipLocator {
    fn drop(&mut self) {
        self.encrypted_record_key.fill(0);
    }
}

impl MembershipLocator {
    /// Replace a successfully redeemed one-time code with a durable locator.
    ///
    /// # Errors
    ///
    /// Requires exact code/record/host/member bindings and a valid host
    /// credential.
    pub fn from_join(
        code: &RoomCode,
        record: &RendezvousRecord,
        credential: MembershipCredential,
        now_unix_ms: u64,
    ) -> Result<Self, MembershipError> {
        record
            .validate_for_code(code, now_unix_ms)
            .map_err(|error| {
                if error == RendezvousError::Expired {
                    MembershipError::ExpiredRendezvous
                } else {
                    MembershipError::InvalidLocator
                }
            })?;
        credential.verify(&record.host_identity)?;
        if credential.grant.room_id != record.room_id
            || credential.grant.membership_epoch != record.session_epoch
        {
            return Err(MembershipError::InvalidLocator);
        }
        let encrypted_record_key =
            code.with_encrypted_record_key(|value| value.as_bytes().to_vec());
        Ok(Self {
            network: code.network(),
            room_id: record.room_id.clone(),
            encrypted_record_key,
            host_identity: record.host_identity.clone(),
            credential,
        })
    }

    fn validate_static_bindings(&self) -> Result<(), MembershipError> {
        self.credential.verify(&self.host_identity)?;
        if self.credential.grant.room_id != self.room_id {
            return Err(MembershipError::InvalidLocator);
        }
        Ok(())
    }

    #[must_use]
    pub const fn network(&self) -> RoomNetwork {
        self.network
    }

    #[must_use]
    pub fn room_id(&self) -> &RoomId {
        &self.room_id
    }

    #[must_use]
    pub fn host_identity(&self) -> &ApplicationPublicIdentity {
        &self.host_identity
    }

    #[must_use]
    pub fn credential(&self) -> &MembershipCredential {
        &self.credential
    }

    /// Validate refreshed rendezvous data without possessing the one-time
    /// invite that originally created this membership.
    ///
    /// # Errors
    ///
    /// Rejects any room, network, host, epoch, expiry, credential, or record
    /// shape mismatch before the advertised route is imported.
    pub fn validate_rendezvous(
        &self,
        record: &RendezvousRecord,
        now_unix_ms: u64,
    ) -> Result<(), MembershipError> {
        record.encode().map_err(|error| {
            if error == RendezvousError::Expired {
                MembershipError::ExpiredRendezvous
            } else {
                MembershipError::InvalidLocator
            }
        })?;
        self.validate_static_bindings()?;
        if RoomNetwork::from(record.network) != self.network
            || record.room_id != self.room_id
            || record.host_identity != self.host_identity
            || record.session_epoch != self.credential.grant.membership_epoch
            || record.expires_at_unix_ms <= now_unix_ms
        {
            return Err(if record.expires_at_unix_ms <= now_unix_ms {
                MembershipError::ExpiredRendezvous
            } else {
                MembershipError::InvalidLocator
            });
        }
        Ok(())
    }

    #[cfg(feature = "veilid")]
    pub(crate) fn with_encrypted_record_key<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        operation(
            std::str::from_utf8(&self.encrypted_record_key)
                .expect("validated membership record keys are UTF-8"),
        )
    }

    /// Persist this locator only through a caller-selected membership store.
    ///
    /// # Errors
    ///
    /// Enforces protected storage unless explicit insecure development is
    /// acknowledged.
    pub async fn persist<S: MembershipStore>(
        &self,
        store: &S,
        policy: IdentityStoragePolicy,
    ) -> Result<(), MembershipError> {
        validate_storage_policy(store.security(), policy)?;
        let blob = self.to_blob()?;
        store.save(&self.room_id, &blob).await
    }

    /// Load and validate one durable membership.
    ///
    /// # Errors
    ///
    /// Missing/corrupt/backend failures never create or replace membership.
    pub async fn load<S: MembershipStore>(
        store: &S,
        room_id: &RoomId,
        policy: IdentityStoragePolicy,
    ) -> Result<Self, MembershipError> {
        validate_storage_policy(store.security(), policy)?;
        let blob = store.load(room_id).await?.ok_or(MembershipError::Missing)?;
        let locator = Self::from_blob(&blob)?;
        if locator.room_id != *room_id {
            return Err(MembershipError::CorruptStoredMembership);
        }
        locator
            .validate_static_bindings()
            .map_err(|_| MembershipError::CorruptStoredMembership)?;
        Ok(locator)
    }

    fn to_blob(&self) -> Result<SecretMembershipBlob, MembershipError> {
        let stored = StoredMembershipRef {
            schema_version: MEMBERSHIP_SCHEMA_VERSION,
            network: self.network.into(),
            room_id: &self.room_id,
            encrypted_record_key: &self.encrypted_record_key,
            host_identity: &self.host_identity,
            credential: &self.credential,
        };
        let mut encoded =
            serde_json::to_vec(&stored).map_err(|_| MembershipError::CorruptStoredMembership)?;
        let mut bytes = Vec::with_capacity(4 + encoded.len() + 32);
        bytes.extend_from_slice(MEMBERSHIP_BLOB_MAGIC);
        bytes.extend_from_slice(&encoded);
        encoded.fill(0);
        bytes.extend_from_slice(blake3::hash(&bytes).as_bytes());
        Ok(SecretMembershipBlob(bytes))
    }

    fn from_blob(blob: &SecretMembershipBlob) -> Result<Self, MembershipError> {
        blob.with_bytes(|bytes| {
            if bytes.len() < 4 + 32 || &bytes[..4] != MEMBERSHIP_BLOB_MAGIC {
                return Err(MembershipError::CorruptStoredMembership);
            }
            let checksum_offset = bytes.len() - 32;
            if blake3::hash(&bytes[..checksum_offset]).as_bytes() != &bytes[checksum_offset..] {
                return Err(MembershipError::CorruptStoredMembership);
            }
            let stored: StoredMembershipV1 = serde_json::from_slice(&bytes[4..checksum_offset])
                .map_err(|_| MembershipError::CorruptStoredMembership)?;
            if stored.schema_version != MEMBERSHIP_SCHEMA_VERSION
                || RoomId::new(stored.room_id.as_str()).is_err()
                || validate_record_key(&stored.encrypted_record_key).is_err()
            {
                return Err(MembershipError::CorruptStoredMembership);
            }
            let locator = Self {
                network: stored.network.into(),
                room_id: stored.room_id,
                encrypted_record_key: stored.encrypted_record_key,
                host_identity: stored.host_identity,
                credential: stored.credential,
            };
            locator
                .validate_static_bindings()
                .map_err(|_| MembershipError::CorruptStoredMembership)?;
            Ok(locator)
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMembershipV1 {
    schema_version: u16,
    network: RoomNetworkWire,
    room_id: RoomId,
    encrypted_record_key: Vec<u8>,
    host_identity: ApplicationPublicIdentity,
    credential: MembershipCredential,
}

#[derive(Serialize)]
struct StoredMembershipRef<'a> {
    schema_version: u16,
    network: RoomNetworkWire,
    room_id: &'a RoomId,
    encrypted_record_key: &'a [u8],
    host_identity: &'a ApplicationPublicIdentity,
    credential: &'a MembershipCredential,
}

/// Secret membership blob crossing only the protected-store boundary.
pub struct SecretMembershipBlob(Vec<u8>);

impl SecretMembershipBlob {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn with_bytes<R>(&self, operation: impl FnOnce(&[u8]) -> R) -> R {
        operation(&self.0)
    }
}

impl Drop for SecretMembershipBlob {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Protected membership persistence boundary.
pub trait MembershipStore {
    fn security(&self) -> StorageSecurity;

    fn load(
        &self,
        room_id: &RoomId,
    ) -> impl Future<Output = Result<Option<SecretMembershipBlob>, MembershipError>>;

    fn save(
        &self,
        room_id: &RoomId,
        membership: &SecretMembershipBlob,
    ) -> impl Future<Output = Result<(), MembershipError>>;

    fn remove(&self, room_id: &RoomId) -> impl Future<Output = Result<bool, MembershipError>>;
}

/// Explicitly insecure test/development membership store.
#[cfg(any(test, feature = "insecure-development"))]
pub struct InsecureMemoryMembershipStore {
    memberships: std::sync::Mutex<Vec<(RoomId, Vec<u8>)>>,
}

#[cfg(any(test, feature = "insecure-development"))]
impl InsecureMemoryMembershipStore {
    #[must_use]
    pub const fn new(_acknowledgement: ExplicitInsecureDevelopment) -> Self {
        Self {
            memberships: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(any(test, feature = "insecure-development"))]
impl MembershipStore for InsecureMemoryMembershipStore {
    fn security(&self) -> StorageSecurity {
        StorageSecurity::ExplicitInsecureDevelopment
    }

    async fn load(
        &self,
        room_id: &RoomId,
    ) -> Result<Option<SecretMembershipBlob>, MembershipError> {
        Ok(self
            .memberships
            .lock()
            .map_err(|_| MembershipError::BackendUnavailable)?
            .iter()
            .find(|(candidate, _)| candidate == room_id)
            .map(|(_, bytes)| SecretMembershipBlob::new(bytes.clone())))
    }

    async fn save(
        &self,
        room_id: &RoomId,
        membership: &SecretMembershipBlob,
    ) -> Result<(), MembershipError> {
        let mut stored = self
            .memberships
            .lock()
            .map_err(|_| MembershipError::BackendUnavailable)?;
        let bytes = membership.with_bytes(<[u8]>::to_vec);
        if let Some((_, existing)) = stored
            .iter_mut()
            .find(|(candidate, _)| candidate == room_id)
        {
            existing.fill(0);
            *existing = bytes;
        } else {
            stored.push((room_id.clone(), bytes));
        }
        Ok(())
    }

    async fn remove(&self, room_id: &RoomId) -> Result<bool, MembershipError> {
        let mut stored = self
            .memberships
            .lock()
            .map_err(|_| MembershipError::BackendUnavailable)?;
        if let Some(index) = stored
            .iter()
            .position(|(candidate, _)| candidate == room_id)
        {
            let (_, mut bytes) = stored.remove(index);
            bytes.fill(0);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(any(test, feature = "insecure-development"))]
impl Drop for InsecureMemoryMembershipStore {
    fn drop(&mut self) {
        if let Ok(memberships) = self.memberships.get_mut() {
            for (_, bytes) in memberships {
                bytes.fill(0);
            }
        }
    }
}

/// Signed reconnect request carrying a replaceable recipient private route.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconnectRequest {
    pub schema_version: u16,
    pub credential: MembershipCredential,
    pub command: CommandEnvelope,
    pub recipient_route_epoch: u64,
    recipient_route_base64url: String,
    pub signature: SignatureMetadata,
}

impl fmt::Debug for ReconnectRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReconnectRequest")
            .field("schema_version", &self.schema_version)
            .field("credential", &self.credential)
            .field("command", &self.command)
            .field("recipient_route_epoch", &self.recipient_route_epoch)
            .field("recipient_route_base64url", &"<redacted-route>")
            .field("signature", &self.signature)
            .finish()
    }
}

impl ReconnectRequest {
    /// Bind a fresh recipient route to a stable-key signed reconnect command.
    ///
    /// # Errors
    ///
    /// Rejects a non-reconnect command, wrong stable key, invalid credential,
    /// or oversized route.
    pub fn issue(
        member: &ApplicationIdentity,
        credential: MembershipCredential,
        command: CommandEnvelope,
        recipient_route_epoch: u64,
        recipient_route_blob: &[u8],
    ) -> Result<Self, MembershipError> {
        let member_public = member.public();
        if recipient_route_epoch == 0
            || recipient_route_blob.is_empty()
            || recipient_route_blob.len() > MAX_RECIPIENT_ROUTE_BYTES
            || command.payload != CommandPayload::Reconnect
            || command.principal_id != member_public.principal_id
            || credential.grant.member_identity != member_public
            || credential.grant.room_id != command.room_id
            || credential.grant.membership_epoch != command.session_epoch
            || verify_command_signature(&command, &member_public).is_err()
        {
            return Err(MembershipError::InvalidReconnect);
        }
        let mut request = Self {
            schema_version: MEMBERSHIP_SCHEMA_VERSION,
            credential,
            command,
            recipient_route_epoch,
            recipient_route_base64url: BASE64URL_NOPAD.encode(recipient_route_blob),
            signature: placeholder_signature(member_public.principal_id.clone()),
        };
        request.signature.signature = member
            .sign_application_bytes(&reconnect_signed_bytes(&request)?)
            .map_err(|_| MembershipError::InvalidReconnect)?;
        Ok(request)
    }

    /// Verify host credential, member command, route proof, and all bindings.
    ///
    /// # Errors
    ///
    /// A valid credential still fails later if current session policy has
    /// removed or revoked this member.
    pub fn verify(&self, expected_host: &ApplicationPublicIdentity) -> Result<(), MembershipError> {
        self.credential.verify(expected_host)?;
        let member = &self.credential.grant.member_identity;
        let route = self.recipient_route_blob()?;
        if self.schema_version != MEMBERSHIP_SCHEMA_VERSION
            || self.recipient_route_epoch == 0
            || self.command.payload != CommandPayload::Reconnect
            || self.command.room_id != self.credential.grant.room_id
            || self.command.session_epoch != self.credential.grant.membership_epoch
            || self.command.principal_id != member.principal_id
            || self.signature.domain_version != SIGNATURE_DOMAIN_V1
            || self.signature.algorithm != SignatureAlgorithm::Ed25519
            || self.signature.key_id != member.principal_id
            || route.is_empty()
            || route.len() > MAX_RECIPIENT_ROUTE_BYTES
            || BASE64URL_NOPAD.encode(&route) != self.recipient_route_base64url
        {
            return Err(MembershipError::InvalidReconnect);
        }
        verify_command_signature(&self.command, member)
            .map_err(|_| MembershipError::InvalidReconnect)?;
        verify_application_bytes(
            member,
            &reconnect_signed_bytes(self)?,
            &self.signature.signature,
        )
        .map_err(|_| MembershipError::InvalidReconnect)
    }

    /// Decode the replaceable recipient route after signature verification.
    ///
    /// # Errors
    ///
    /// Returns no rejected bytes.
    pub fn recipient_route_blob(&self) -> Result<Vec<u8>, MembershipError> {
        BASE64URL_NOPAD
            .decode(self.recipient_route_base64url.as_bytes())
            .map_err(|_| MembershipError::InvalidReconnect)
    }
}

/// Host-signed viewer snapshot plus gap-free authoritative event tail.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryBundle {
    pub snapshot: SnapshotEnvelope,
    pub event_tail: Vec<EventEnvelope>,
}

impl fmt::Debug for RecoveryBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryBundle")
            .field("room_id", &self.snapshot.room_id)
            .field("session_epoch", &self.snapshot.session_epoch)
            .field("snapshot_revision", &self.snapshot.current_revision)
            .field("snapshot_payload", &"<redacted-viewer-snapshot>")
            .field("event_tail_len", &self.event_tail.len())
            .finish()
    }
}

impl RecoveryBundle {
    /// Verify host provenance and a gap-free revision tail for one membership.
    ///
    /// # Errors
    ///
    /// Rejects wrong room/epoch/host, invalid signatures, gaps, reordering, or
    /// oversized tails.
    pub fn verify(
        &self,
        credential: &MembershipCredential,
        expected_host: &ApplicationPublicIdentity,
    ) -> Result<u64, MembershipError> {
        credential.verify(expected_host)?;
        if self.event_tail.len() > MAX_RECOVERY_EVENTS
            || self.snapshot.payload.state.len() > MAX_RECOVERY_SNAPSHOT_STATE_BYTES
            || self.snapshot.room_id != credential.grant.room_id
            || self.snapshot.session_epoch != credential.grant.membership_epoch
            || self.snapshot.principal_id != expected_host.principal_id
            || self.snapshot.current_revision != self.snapshot.payload.event_tail_revision
        {
            return Err(MembershipError::InvalidRecovery);
        }
        verify_snapshot_signature(&self.snapshot, expected_host)
            .map_err(|_| MembershipError::InvalidRecovery)?;
        let mut revision = self.snapshot.current_revision;
        for event in &self.event_tail {
            revision = revision
                .checked_add(1)
                .ok_or(MembershipError::InvalidRecovery)?;
            if event.room_id != credential.grant.room_id
                || event.session_epoch != credential.grant.membership_epoch
                || event.principal_id != expected_host.principal_id
                || event.current_revision != revision
            {
                return Err(MembershipError::InvalidRecovery);
            }
            verify_event_signature(event, expected_host)
                .map_err(|_| MembershipError::InvalidRecovery)?;
        }
        Ok(revision)
    }
}

fn membership_signed_bytes(grant: &MembershipGrant) -> Result<Vec<u8>, MembershipError> {
    signed_json(MEMBERSHIP_SIGNATURE_DOMAIN, grant)
}

fn reconnect_signed_bytes(request: &ReconnectRequest) -> Result<Vec<u8>, MembershipError> {
    #[derive(Serialize)]
    struct SignedReconnect<'a> {
        schema_version: u16,
        credential: &'a MembershipCredential,
        command: &'a CommandEnvelope,
        recipient_route_epoch: u64,
        recipient_route_base64url: &'a str,
        signature_domain_version: u16,
        signature_algorithm: SignatureAlgorithm,
        signature_key_id: &'a PrincipalId,
    }
    signed_json(
        RECONNECT_SIGNATURE_DOMAIN,
        &SignedReconnect {
            schema_version: request.schema_version,
            credential: &request.credential,
            command: &request.command,
            recipient_route_epoch: request.recipient_route_epoch,
            recipient_route_base64url: &request.recipient_route_base64url,
            signature_domain_version: request.signature.domain_version,
            signature_algorithm: request.signature.algorithm,
            signature_key_id: &request.signature.key_id,
        },
    )
}

fn signed_json<T: Serialize>(domain: &[u8], value: &T) -> Result<Vec<u8>, MembershipError> {
    let payload = serde_json::to_vec(value).map_err(|_| MembershipError::InvalidCredential)?;
    let length = u32::try_from(payload.len()).map_err(|_| MembershipError::Oversized)?;
    let mut bytes = Vec::with_capacity(domain.len() + 4 + payload.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn placeholder_signature(principal: PrincipalId) -> SignatureMetadata {
    SignatureMetadata {
        domain_version: SIGNATURE_DOMAIN_V1,
        algorithm: SignatureAlgorithm::Ed25519,
        key_id: principal,
        signature: poche_protocol::SignatureBytes::new("00".repeat(64))
            .expect("fixed lowercase signature placeholder is valid"),
    }
}

fn validate_storage_policy(
    security: StorageSecurity,
    policy: IdentityStoragePolicy,
) -> Result<(), MembershipError> {
    match (security, policy) {
        (StorageSecurity::Protected, _)
        | (
            StorageSecurity::ExplicitInsecureDevelopment,
            IdentityStoragePolicy::AllowExplicitInsecure(_),
        ) => Ok(()),
        (StorageSecurity::ExplicitInsecureDevelopment, IdentityStoragePolicy::RequireProtected) => {
            Err(MembershipError::InsecureOptInRequired)
        }
    }
}

fn validate_record_key(value: &[u8]) -> Result<(), MembershipError> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
    {
        Err(MembershipError::InvalidLocator)
    } else {
        Ok(())
    }
}

impl From<IdentityCryptoError> for MembershipError {
    fn from(_: IdentityCryptoError) -> Self {
        Self::InvalidCredential
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InsecureMemoryIdentityStore, PublicRoomMetadata, verify_command_signature};
    use poche_protocol::{
        CommandId, CorrelationId, EventId, EventPayload, RoomPhase, SIGNATURE_DOMAIN_V1,
        SemanticHash, SignatureIntent, SnapshotId, SnapshotPayload, UnsignedCommandEnvelope,
        UnsignedEventEnvelope, UnsignedSnapshotEnvelope,
    };
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    const NOW: u64 = 10_000;
    const CODE_EXPIRY: u64 = 20_000;
    const RECORD_EXPIRY: u64 = 30_000;
    const RECORD_KEY: &str = "VLD0:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    const fn insecure_policy() -> IdentityStoragePolicy {
        IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        )
    }

    fn identity(store: &InsecureMemoryIdentityStore) -> ApplicationIdentity {
        block_on(ApplicationIdentity::load_or_create(
            store,
            insecure_policy(),
        ))
        .unwrap()
    }

    fn fixture(
        host: &ApplicationIdentity,
        member: &ApplicationIdentity,
    ) -> (RoomCode, RendezvousRecord, MembershipLocator) {
        let room_id = RoomId::new("durable-room").unwrap();
        let code = RoomCode::issue(
            RoomNetwork::VeilidLocal,
            RECORD_KEY,
            host.public().principal_id,
            CODE_EXPIRY,
            NOW,
        )
        .unwrap();
        let record = RendezvousRecord::new(
            RoomNetwork::VeilidLocal,
            room_id.clone(),
            host.public(),
            PublicRoomMetadata::new("Durable room", 2, true).unwrap(),
            1,
            1,
            RECORD_EXPIRY,
            NOW,
            b"host-route-epoch-1",
        )
        .unwrap();
        let credential =
            MembershipCredential::issue(host, room_id, member.public(), 1, 1, 7).unwrap();
        let locator = MembershipLocator::from_join(&code, &record, credential, NOW).unwrap();
        (code, record, locator)
    }

    fn reconnect_command(
        member: &ApplicationIdentity,
        room_id: RoomId,
        revision: u64,
        suffix: &str,
    ) -> CommandEnvelope {
        let public = member.public();
        member
            .sign_command(UnsignedCommandEnvelope {
                protocol_version: PROTOCOL_VERSION_V1,
                room_id,
                session_epoch: 1,
                command_id: CommandId::new(format!("reconnect-{suffix}")).unwrap(),
                principal_id: public.principal_id.clone(),
                expected_revision: revision,
                correlation_id: CorrelationId::new(format!("reconnect-correlation-{suffix}"))
                    .unwrap(),
                causation_id: None,
                payload: CommandPayload::Reconnect,
                signature_intent: SignatureIntent {
                    domain_version: SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: public.principal_id,
                },
            })
            .unwrap()
    }

    fn host_event(
        host: &ApplicationIdentity,
        room_id: RoomId,
        revision: u64,
        suffix: &str,
    ) -> EventEnvelope {
        let public = host.public();
        host.sign_event(UnsignedEventEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id,
            session_epoch: 1,
            event_id: EventId::new(format!("recovery-event-{suffix}")).unwrap(),
            principal_id: public.principal_id.clone(),
            current_revision: revision,
            correlation_id: CorrelationId::new(format!("recovery-correlation-{suffix}")).unwrap(),
            causation_id: CommandId::new(format!("recovery-command-{suffix}")).unwrap(),
            payload: EventPayload::RoomCreated,
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: public.principal_id,
            },
        })
        .unwrap()
    }

    #[test]
    fn host_and_client_restart_without_invite_and_accept_a_rotated_host_route() {
        let host_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let member_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let host = identity(&host_store);
        let member = identity(&member_store);
        let host_public = host.public();
        let member_public = member.public();
        let (code, _record, locator) = fixture(&host, &member);

        let text = code.encode().unwrap();
        let mut decoded_code = BASE64URL_NOPAD
            .decode(&text.expose().as_bytes()[3..])
            .unwrap();
        let record_key_len = usize::from(decoded_code[16]);
        let invite_offset = 17 + record_key_len + 32;
        let mut invite_secret = decoded_code[invite_offset..invite_offset + 32].to_vec();
        let blob = locator.to_blob().unwrap();
        blob.with_bytes(|stored| {
            assert!(
                !stored
                    .windows(invite_secret.len())
                    .any(|window| window == invite_secret)
            );
            assert!(
                !stored
                    .windows(text.expose().len())
                    .any(|window| window == text.expose().as_bytes())
            );
        });
        invite_secret.fill(0);
        decoded_code.fill(0);
        assert!(!format!("{locator:?}").contains(RECORD_KEY));

        let membership_store = InsecureMemoryMembershipStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        block_on(locator.persist(&membership_store, insecure_policy())).unwrap();
        let room_id = locator.room_id().clone();
        drop(code);
        drop(locator);
        drop(member);
        drop(host);

        let restarted_host = identity(&host_store);
        let restarted_member = identity(&member_store);
        assert_eq!(restarted_host.public(), host_public);
        assert_eq!(restarted_member.public(), member_public);
        let loaded = block_on(MembershipLocator::load(
            &membership_store,
            &room_id,
            insecure_policy(),
        ))
        .unwrap();
        assert_eq!(loaded.credential().grant.member_identity, member_public);
        loaded
            .credential()
            .verify(&restarted_host.public())
            .unwrap();

        let rotated = RendezvousRecord::new(
            RoomNetwork::VeilidLocal,
            room_id,
            restarted_host.public(),
            PublicRoomMetadata::new("Durable room", 2, true).unwrap(),
            1,
            2,
            RECORD_EXPIRY + 1,
            NOW,
            b"host-route-epoch-2",
        )
        .unwrap();
        assert_eq!(loaded.validate_rendezvous(&rotated, NOW), Ok(()));
        assert_eq!(
            loaded.validate_rendezvous(&rotated, RECORD_EXPIRY + 1),
            Err(MembershipError::ExpiredRendezvous)
        );
    }

    #[test]
    fn recipient_route_rotation_preserves_principal_and_rejects_forgery() {
        let host_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let member_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let attacker_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let host = identity(&host_store);
        let member = identity(&member_store);
        let attacker = identity(&attacker_store);
        let (_code, _record, locator) = fixture(&host, &member);
        let credential = locator.credential().clone();
        let room_id = locator.room_id().clone();

        let first = ReconnectRequest::issue(
            &member,
            credential.clone(),
            reconnect_command(&member, room_id.clone(), 8, "one"),
            1,
            b"member-recipient-route-one",
        )
        .unwrap();
        first.verify(&host.public()).unwrap();
        assert_eq!(
            first.recipient_route_blob().unwrap(),
            b"member-recipient-route-one"
        );
        assert!(!format!("{first:?}").contains("member-recipient-route-one"));

        let principal = member.public().principal_id;
        drop(member);
        let restarted_member = identity(&member_store);
        let second = ReconnectRequest::issue(
            &restarted_member,
            credential.clone(),
            reconnect_command(&restarted_member, room_id.clone(), 9, "two"),
            2,
            b"member-recipient-route-two",
        )
        .unwrap();
        second.verify(&host.public()).unwrap();
        assert_eq!(second.command.principal_id, principal);
        assert_eq!(
            second.credential.grant.member_identity,
            restarted_member.public()
        );

        let attacker_command = reconnect_command(&attacker, room_id, 9, "attacker");
        assert_eq!(
            verify_command_signature(&attacker_command, &attacker.public()),
            Ok(())
        );
        assert_eq!(
            ReconnectRequest::issue(
                &attacker,
                credential,
                attacker_command,
                3,
                b"attacker-route"
            ),
            Err(MembershipError::InvalidReconnect)
        );
        let mut tampered = second;
        tampered.recipient_route_epoch = 3;
        assert_eq!(
            tampered.verify(&host.public()),
            Err(MembershipError::InvalidReconnect)
        );
    }

    #[test]
    fn signed_recovery_is_gap_free_host_bound_and_debug_redacted() {
        let host_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let member_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let wrong_host_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let host = identity(&host_store);
        let member = identity(&member_store);
        let wrong_host = identity(&wrong_host_store);
        let (_code, _record, locator) = fixture(&host, &member);
        let room_id = locator.room_id().clone();
        let public = host.public();
        let snapshot = host
            .sign_snapshot(UnsignedSnapshotEnvelope {
                protocol_version: PROTOCOL_VERSION_V1,
                room_id: room_id.clone(),
                session_epoch: 1,
                snapshot_id: SnapshotId::new("recovery-snapshot").unwrap(),
                principal_id: public.principal_id.clone(),
                current_revision: 10,
                correlation_id: CorrelationId::new("recovery-snapshot-correlation").unwrap(),
                causation_id: EventId::new("recovery-snapshot-cause").unwrap(),
                payload: SnapshotPayload {
                    schema_hash: SemanticHash([1; 32]),
                    state_hash: SemanticHash([2; 32]),
                    event_tail_revision: 10,
                    phase: RoomPhase::Running,
                    members: Vec::new(),
                    state: b"private-hand-marker".to_vec(),
                },
                signature_intent: SignatureIntent {
                    domain_version: SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: public.principal_id,
                },
            })
            .unwrap();
        let bundle = RecoveryBundle {
            snapshot,
            event_tail: vec![
                host_event(&host, room_id.clone(), 11, "eleven"),
                host_event(&host, room_id, 12, "twelve"),
            ],
        };
        assert_eq!(bundle.verify(locator.credential(), &host.public()), Ok(12));
        assert!(!format!("{bundle:?}").contains("private-hand-marker"));
        assert_eq!(
            bundle.verify(locator.credential(), &wrong_host.public()),
            Err(MembershipError::InvalidCredential)
        );

        let mut oversized = bundle.clone();
        oversized.snapshot.payload.state = vec![0; MAX_RECOVERY_SNAPSHOT_STATE_BYTES + 1];
        assert_eq!(
            oversized.verify(locator.credential(), &host.public()),
            Err(MembershipError::InvalidRecovery)
        );

        let mut reordered = bundle.clone();
        reordered.event_tail.swap(0, 1);
        assert_eq!(
            reordered.verify(locator.credential(), &host.public()),
            Err(MembershipError::InvalidRecovery)
        );
        let mut missing = bundle;
        missing.event_tail.remove(0);
        assert_eq!(
            missing.verify(locator.credential(), &host.public()),
            Err(MembershipError::InvalidRecovery)
        );
    }

    #[test]
    fn corrupt_or_unprotected_membership_never_silently_loads() {
        let host_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let member_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let host = identity(&host_store);
        let member = identity(&member_store);
        let (_code, _record, locator) = fixture(&host, &member);
        let mut bytes = locator.to_blob().unwrap().with_bytes(<[u8]>::to_vec);
        bytes[8] ^= 1;
        let corrupt = SecretMembershipBlob::new(bytes);
        assert!(matches!(
            MembershipLocator::from_blob(&corrupt),
            Err(MembershipError::CorruptStoredMembership)
        ));

        let store = InsecureMemoryMembershipStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        assert_eq!(
            block_on(locator.persist(&store, IdentityStoragePolicy::RequireProtected)),
            Err(MembershipError::InsecureOptInRequired)
        );
    }
}
