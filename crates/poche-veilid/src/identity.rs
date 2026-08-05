use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use poche_protocol::{
    CommandEnvelope, EventEnvelope, PrincipalId, SignatureAlgorithm, SignatureBytes,
    SnapshotEnvelope, UnsignedCommandEnvelope, UnsignedEventEnvelope, UnsignedSnapshotEnvelope,
    canonical_command_signed_bytes, canonical_command_verification_bytes,
    canonical_event_signed_bytes, canonical_event_verification_bytes,
    canonical_snapshot_signed_bytes, canonical_snapshot_verification_bytes,
};
use serde::{Deserialize, Serialize};

use crate::{IdentityStoragePolicy, IdentityStore, IdentityStoreError, StorageSecurity};

const PUBLIC_IDENTITY_SCHEMA_VERSION: u16 = 1;
const SECRET_MAGIC: &[u8; 4] = b"PCI1";
const SECRET_BYTES: usize = 4 + 32 + 32 + 32;

/// Stable, public application identity. Veilid node IDs and routes are absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationPublicIdentity {
    pub schema_version: u16,
    pub principal_id: PrincipalId,
    pub signing_public_key: String,
    pub encryption_public_key: String,
}

impl ApplicationPublicIdentity {
    /// Validate schema, principal binding, signing key, and recipient key.
    ///
    /// # Errors
    ///
    /// Returns a stable public-identity category.
    pub fn validate(&self) -> Result<(), IdentityCryptoError> {
        validate_public_identity(self).map(|_| ())
    }

    #[cfg(feature = "veilid")]
    pub(crate) fn encryption_key_bytes(&self) -> Result<[u8; 32], IdentityCryptoError> {
        self.validate()?;
        decode_hex::<32>(&self.encryption_public_key)
    }
}

/// Secret application identity.
///
/// This type intentionally implements neither `Debug`, `Display`, `Clone`, nor
/// serialization. Only the public projection may enter logs or snapshots.
///
/// ```compile_fail
/// use poche_veilid::ApplicationIdentity;
/// fn reveal(identity: ApplicationIdentity) { println!("{identity:?}"); }
/// ```
///
/// ```compile_fail
/// use poche_veilid::ApplicationIdentity;
/// fn duplicate(identity: ApplicationIdentity) { let _ = identity.clone(); }
/// ```
pub struct ApplicationIdentity {
    signing: SigningKey,
    encryption: EncryptionSecret,
}

struct EncryptionSecret([u8; 32]);

impl Drop for EncryptionSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Fail-closed application signing or public-key validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityCryptoError {
    InvalidPublicIdentity,
    PrincipalMismatch,
    ProtocolEncoding,
    InvalidSignature,
}

impl ApplicationIdentity {
    /// Load an identity or generate and persist one when absent.
    ///
    /// Storage errors never cause silent identity replacement.
    ///
    /// # Errors
    ///
    /// Returns a credential, backend, corruption, insecure-policy, collision,
    /// or operating-system randomness failure.
    pub async fn load_or_create<S: IdentityStore>(
        store: &S,
        policy: IdentityStoragePolicy,
    ) -> Result<Self, IdentityStoreError> {
        match (store.security(), policy) {
            (StorageSecurity::Protected, _)
            | (
                StorageSecurity::ExplicitInsecureDevelopment,
                IdentityStoragePolicy::AllowExplicitInsecure(_),
            ) => {}
            (
                StorageSecurity::ExplicitInsecureDevelopment,
                IdentityStoragePolicy::RequireProtected,
            ) => {
                return Err(IdentityStoreError::InsecureOptInRequired);
            }
        }
        if let Some(blob) = store.load().await? {
            return Self::from_blob(&blob);
        }
        let mut secret = [0_u8; 64];
        getrandom::fill(&mut secret).map_err(|_| IdentityStoreError::RandomUnavailable)?;
        let mut signing = [0_u8; 32];
        let mut encryption = [0_u8; 32];
        signing.copy_from_slice(&secret[..32]);
        encryption.copy_from_slice(&secret[32..]);
        let identity = Self::from_secret_parts(signing, encryption);
        secret.fill(0);
        let blob = identity.to_blob();
        store.save(&blob).await?;
        Ok(identity)
    }

    /// Return the stable public identity used by membership and capabilities.
    ///
    /// # Panics
    ///
    /// Panics only if lowercase hexadecimal for an exact 32-byte key stops
    /// satisfying the protocol's 64-character identifier invariant.
    #[must_use]
    pub fn public(&self) -> ApplicationPublicIdentity {
        let signing = self.signing.verifying_key().to_bytes();
        let encryption = MontgomeryPoint::mul_base_clamped(self.encryption.0).to_bytes();
        ApplicationPublicIdentity {
            schema_version: PUBLIC_IDENTITY_SCHEMA_VERSION,
            principal_id: PrincipalId::new(hex(&signing))
                .expect("a 32-byte lowercase hex key is a valid principal ID"),
            signing_public_key: hex(&signing),
            encryption_public_key: hex(&encryption),
        }
    }

    /// Sign the protocol-defined canonical command domain.
    ///
    /// # Errors
    ///
    /// Rejects a command that claims another stable principal or cannot be
    /// encoded by the canonical protocol codec.
    pub fn sign_command(
        &self,
        command: UnsignedCommandEnvelope,
    ) -> Result<CommandEnvelope, IdentityCryptoError> {
        let public = self.public();
        if command.principal_id != public.principal_id
            || command.signature_intent.key_id != public.principal_id
            || command.signature_intent.algorithm != SignatureAlgorithm::Ed25519
        {
            return Err(IdentityCryptoError::PrincipalMismatch);
        }
        let bytes = canonical_command_signed_bytes(&command)
            .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
        let signature = self.signing.sign(&bytes).to_bytes();
        Ok(command.attach_signature(
            SignatureBytes::new(hex(&signature))
                .map_err(|_| IdentityCryptoError::InvalidSignature)?,
        ))
    }

    /// Sign the protocol-defined canonical authority-event domain.
    ///
    /// # Errors
    ///
    /// Rejects an event with another principal/key intent or invalid encoding.
    pub fn sign_event(
        &self,
        event: UnsignedEventEnvelope,
    ) -> Result<EventEnvelope, IdentityCryptoError> {
        let public = self.public();
        if event.principal_id != public.principal_id
            || event.signature_intent.key_id != public.principal_id
            || event.signature_intent.algorithm != SignatureAlgorithm::Ed25519
        {
            return Err(IdentityCryptoError::PrincipalMismatch);
        }
        let bytes = canonical_event_signed_bytes(&event)
            .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
        let signature = self.signing.sign(&bytes).to_bytes();
        Ok(event.attach_signature(
            SignatureBytes::new(hex(&signature))
                .map_err(|_| IdentityCryptoError::InvalidSignature)?,
        ))
    }

    /// Sign the protocol-defined canonical recovery-snapshot domain.
    ///
    /// # Errors
    ///
    /// Rejects a snapshot with another principal/key intent or invalid
    /// encoding.
    pub fn sign_snapshot(
        &self,
        snapshot: UnsignedSnapshotEnvelope,
    ) -> Result<SnapshotEnvelope, IdentityCryptoError> {
        let public = self.public();
        if snapshot.principal_id != public.principal_id
            || snapshot.signature_intent.key_id != public.principal_id
            || snapshot.signature_intent.algorithm != SignatureAlgorithm::Ed25519
        {
            return Err(IdentityCryptoError::PrincipalMismatch);
        }
        let bytes = canonical_snapshot_signed_bytes(&snapshot)
            .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
        let signature = self.signing.sign(&bytes).to_bytes();
        Ok(snapshot.attach_signature(
            SignatureBytes::new(hex(&signature))
                .map_err(|_| IdentityCryptoError::InvalidSignature)?,
        ))
    }

    pub(crate) fn sign_application_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<SignatureBytes, IdentityCryptoError> {
        SignatureBytes::new(hex(&self.signing.sign(bytes).to_bytes()))
            .map_err(|_| IdentityCryptoError::InvalidSignature)
    }

    #[cfg(feature = "veilid")]
    pub(crate) fn with_encryption_secret<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(&self.encryption.0)
    }

    fn from_blob(blob: &crate::SecretIdentityBlob) -> Result<Self, IdentityStoreError> {
        blob.with_bytes(|bytes| {
            if bytes.len() != SECRET_BYTES || &bytes[..4] != SECRET_MAGIC {
                return Err(IdentityStoreError::CorruptIdentity);
            }
            let expected = blake3::hash(&bytes[..68]);
            if expected.as_bytes() != &bytes[68..] {
                return Err(IdentityStoreError::CorruptIdentity);
            }
            let signing = bytes[4..36]
                .try_into()
                .map_err(|_| IdentityStoreError::CorruptIdentity)?;
            let encryption = bytes[36..68]
                .try_into()
                .map_err(|_| IdentityStoreError::CorruptIdentity)?;
            Ok(Self::from_secret_parts(signing, encryption))
        })
    }

    fn from_secret_parts(signing: [u8; 32], encryption: [u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&signing),
            encryption: EncryptionSecret(encryption),
        }
    }

    fn to_blob(&self) -> crate::SecretIdentityBlob {
        let mut bytes = Vec::with_capacity(SECRET_BYTES);
        bytes.extend_from_slice(SECRET_MAGIC);
        bytes.extend_from_slice(&self.signing.to_bytes());
        bytes.extend_from_slice(&self.encryption.0);
        let checksum = blake3::hash(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        crate::SecretIdentityBlob::new(bytes)
    }
}

/// Strictly verify an application-signed command against an expected public
/// identity. Every signed semantic field, including revision and command ID,
/// is thereby replay-bound.
///
/// # Errors
///
/// Returns a public-identity, principal, codec, or signature failure.
pub fn verify_command_signature(
    command: &CommandEnvelope,
    identity: &ApplicationPublicIdentity,
) -> Result<(), IdentityCryptoError> {
    let key = validate_public_identity(identity)?;
    if command.principal_id != identity.principal_id
        || command.signature.key_id != identity.principal_id
        || command.signature.algorithm != SignatureAlgorithm::Ed25519
    {
        return Err(IdentityCryptoError::PrincipalMismatch);
    }
    let bytes = canonical_command_verification_bytes(command)
        .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
    verify(&key, &bytes, command.signature.signature.as_str())
}

/// Strictly verify an authority-signed event against its stable public key.
///
/// # Errors
///
/// Returns a public-identity, principal, codec, or signature failure.
pub fn verify_event_signature(
    event: &EventEnvelope,
    identity: &ApplicationPublicIdentity,
) -> Result<(), IdentityCryptoError> {
    let key = validate_public_identity(identity)?;
    if event.principal_id != identity.principal_id
        || event.signature.key_id != identity.principal_id
        || event.signature.algorithm != SignatureAlgorithm::Ed25519
    {
        return Err(IdentityCryptoError::PrincipalMismatch);
    }
    let bytes = canonical_event_verification_bytes(event)
        .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
    verify(&key, &bytes, event.signature.signature.as_str())
}

/// Strictly verify an authority-signed recovery snapshot.
///
/// # Errors
///
/// Returns a public-identity, principal, codec, or signature failure.
pub fn verify_snapshot_signature(
    snapshot: &SnapshotEnvelope,
    identity: &ApplicationPublicIdentity,
) -> Result<(), IdentityCryptoError> {
    let key = validate_public_identity(identity)?;
    if snapshot.principal_id != identity.principal_id
        || snapshot.signature.key_id != identity.principal_id
        || snapshot.signature.algorithm != SignatureAlgorithm::Ed25519
    {
        return Err(IdentityCryptoError::PrincipalMismatch);
    }
    let bytes = canonical_snapshot_verification_bytes(snapshot)
        .map_err(|_| IdentityCryptoError::ProtocolEncoding)?;
    verify(&key, &bytes, snapshot.signature.signature.as_str())
}

pub(crate) fn verify_application_bytes(
    identity: &ApplicationPublicIdentity,
    bytes: &[u8],
    signature: &SignatureBytes,
) -> Result<(), IdentityCryptoError> {
    let key = validate_public_identity(identity)?;
    verify(&key, bytes, signature.as_str())
}

fn validate_public_identity(
    identity: &ApplicationPublicIdentity,
) -> Result<VerifyingKey, IdentityCryptoError> {
    if identity.schema_version != PUBLIC_IDENTITY_SCHEMA_VERSION
        || identity.principal_id.as_str() != identity.signing_public_key
    {
        return Err(IdentityCryptoError::InvalidPublicIdentity);
    }
    let signing = decode_hex::<32>(&identity.signing_public_key)?;
    let _encryption = decode_hex::<32>(&identity.encryption_public_key)?;
    VerifyingKey::from_bytes(&signing).map_err(|_| IdentityCryptoError::InvalidPublicIdentity)
}

fn verify(key: &VerifyingKey, message: &[u8], signature: &str) -> Result<(), IdentityCryptoError> {
    let signature = Signature::from_bytes(&decode_hex::<64>(signature)?);
    key.verify_strict(message, &signature)
        .map_err(|_| IdentityCryptoError::InvalidSignature)
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], IdentityCryptoError> {
    if value.len() != N * 2 {
        return Err(IdentityCryptoError::InvalidPublicIdentity);
    }
    let mut bytes = [0_u8; N];
    for (target, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let high = nibble(pair[0]).ok_or(IdentityCryptoError::InvalidPublicIdentity)?;
        let low = nibble(pair[1]).ok_or(IdentityCryptoError::InvalidPublicIdentity)?;
        *target = (high << 4) | low;
    }
    Ok(bytes)
}

fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExplicitInsecureDevelopment, IdentityStoragePolicy, IdentityStore,
        InsecureMemoryIdentityStore, SecretIdentityBlob,
    };
    use poche_protocol::{
        CommandId, CommandPayload, CorrelationId, EventId, EventPayload, PROTOCOL_VERSION_V1,
        RoomId, RoomPhase, SIGNATURE_DOMAIN_V1, SemanticHash, SignatureIntent, SnapshotId,
        SnapshotPayload,
    };
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

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

    fn unsigned(identity: &ApplicationPublicIdentity, revision: u64) -> UnsignedCommandEnvelope {
        UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("identity-room").unwrap(),
            session_epoch: 1,
            command_id: CommandId::new("identity-command").unwrap(),
            principal_id: identity.principal_id.clone(),
            expected_revision: revision,
            correlation_id: CorrelationId::new("identity-correlation").unwrap(),
            causation_id: None,
            payload: CommandPayload::Ready,
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: identity.principal_id.clone(),
            },
        }
    }

    #[test]
    fn restart_preserves_stable_application_keys_and_public_snapshot_is_safe() {
        let store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let policy = IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let first = block_on(ApplicationIdentity::load_or_create(&store, policy)).unwrap();
        let public = first.public();
        drop(first);
        let second = block_on(ApplicationIdentity::load_or_create(&store, policy)).unwrap();
        assert_eq!(second.public(), public);
        let snapshot = serde_json::to_string(&public).unwrap();
        let stored = block_on(store.load()).unwrap().unwrap();
        stored.with_bytes(|secret| {
            assert!(
                !snapshot
                    .as_bytes()
                    .windows(32)
                    .any(|window| { window == &secret[4..36] || window == &secret[36..68] })
            );
        });
    }

    struct FailingStore(IdentityStoreError);

    impl IdentityStore for FailingStore {
        fn security(&self) -> StorageSecurity {
            StorageSecurity::Protected
        }

        async fn load(&self) -> Result<Option<SecretIdentityBlob>, IdentityStoreError> {
            Err(self.0)
        }

        async fn save(&self, _identity: &SecretIdentityBlob) -> Result<(), IdentityStoreError> {
            panic!("load failure must not silently generate or save an identity")
        }
    }

    #[test]
    fn wrong_or_missing_storage_credentials_never_replace_identity() {
        for error in [
            IdentityStoreError::MissingCredentials,
            IdentityStoreError::CredentialsRejected,
        ] {
            let result = block_on(ApplicationIdentity::load_or_create(
                &FailingStore(error),
                IdentityStoragePolicy::RequireProtected,
            ));
            assert!(matches!(result, Err(observed) if observed == error));
        }
    }

    struct CorruptStore;

    impl IdentityStore for CorruptStore {
        fn security(&self) -> StorageSecurity {
            StorageSecurity::Protected
        }

        async fn load(&self) -> Result<Option<SecretIdentityBlob>, IdentityStoreError> {
            Ok(Some(SecretIdentityBlob::new(vec![0_u8; SECRET_BYTES])))
        }

        async fn save(&self, _identity: &SecretIdentityBlob) -> Result<(), IdentityStoreError> {
            panic!("corrupt stored material must not be replaced with a new identity")
        }
    }

    #[test]
    fn corrupt_stored_identity_fails_without_replacement() {
        assert!(matches!(
            block_on(ApplicationIdentity::load_or_create(
                &CorruptStore,
                IdentityStoragePolicy::RequireProtected,
            )),
            Err(IdentityStoreError::CorruptIdentity)
        ));
    }

    #[test]
    fn signatures_bind_principal_command_and_revision_and_fail_under_wrong_key() {
        let first_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let second_store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let policy = IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let first = block_on(ApplicationIdentity::load_or_create(&first_store, policy)).unwrap();
        let second = block_on(ApplicationIdentity::load_or_create(&second_store, policy)).unwrap();
        let signed = first.sign_command(unsigned(&first.public(), 7)).unwrap();
        assert_eq!(verify_command_signature(&signed, &first.public()), Ok(()));
        assert_eq!(
            verify_command_signature(&signed, &second.public()),
            Err(IdentityCryptoError::PrincipalMismatch)
        );
        let mut replay_changed = signed;
        replay_changed.expected_revision = 8;
        assert_eq!(
            verify_command_signature(&replay_changed, &first.public()),
            Err(IdentityCryptoError::InvalidSignature)
        );

        let public = first.public();
        let event = UnsignedEventEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("identity-room").unwrap(),
            session_epoch: 1,
            event_id: EventId::new("identity-event").unwrap(),
            principal_id: public.principal_id.clone(),
            current_revision: 8,
            correlation_id: CorrelationId::new("identity-correlation").unwrap(),
            causation_id: CommandId::new("identity-command").unwrap(),
            payload: EventPayload::RoomCreated,
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: public.principal_id.clone(),
            },
        };
        let signed_event = first.sign_event(event).unwrap();
        assert_eq!(verify_event_signature(&signed_event, &public), Ok(()));
        assert_eq!(
            verify_event_signature(&signed_event, &second.public()),
            Err(IdentityCryptoError::PrincipalMismatch)
        );
        let mut revision_changed = signed_event;
        revision_changed.current_revision = 9;
        assert_eq!(
            verify_event_signature(&revision_changed, &public),
            Err(IdentityCryptoError::InvalidSignature)
        );

        let snapshot = UnsignedSnapshotEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("identity-room").unwrap(),
            session_epoch: 1,
            snapshot_id: SnapshotId::new("identity-snapshot").unwrap(),
            principal_id: public.principal_id.clone(),
            current_revision: 8,
            correlation_id: CorrelationId::new("identity-snapshot-correlation").unwrap(),
            causation_id: EventId::new("identity-snapshot-cause").unwrap(),
            payload: SnapshotPayload {
                schema_hash: SemanticHash([1; 32]),
                state_hash: SemanticHash([2; 32]),
                event_tail_revision: 8,
                phase: RoomPhase::Lobby,
                members: Vec::new(),
                state: b"viewer-scoped-state".to_vec(),
            },
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: public.principal_id.clone(),
            },
        };
        let signed_snapshot = first.sign_snapshot(snapshot).unwrap();
        assert_eq!(verify_snapshot_signature(&signed_snapshot, &public), Ok(()));
        assert_eq!(
            verify_snapshot_signature(&signed_snapshot, &second.public()),
            Err(IdentityCryptoError::PrincipalMismatch)
        );
        let mut changed_snapshot = signed_snapshot;
        changed_snapshot.payload.state.push(0xff);
        assert_eq!(
            verify_snapshot_signature(&changed_snapshot, &public),
            Err(IdentityCryptoError::InvalidSignature)
        );
    }
}
