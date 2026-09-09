use std::{str::FromStr, time::Duration};

use poche_protocol::RoomId;
use veilid_core::{
    CRYPTO_KIND_VLD0, DHTSchema, KeyPair, RecordKey, RouteId, RoutingContext, Target, VeilidAPI,
    VeilidAPIError, VeilidAppCall, VeilidUpdate,
};

use crate::{
    ApplicationIdentity, MembershipLocator, PublicRoomMetadata, RENDEZVOUS_DFLT_OWNER_SUBKEYS,
    RENDEZVOUS_DHT_SUBKEY, RendezvousError, RendezvousHint, RendezvousRecord, RoomCode,
    RoomCodeError, RoomNetwork, TransportCommandCall, TransportCommandReply, TransportFailure,
    TransportWireError, classify_rendezvous_route_hint, classify_rendezvous_value_hint,
};

const MAX_APP_CALL_BYTES: usize = 32_768;
const DHT_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);
const HOST_ROOM_KEY_PREFIX: &str = "poche.host-room.v1.";
const HOST_ROOM_MAGIC: &[u8; 4] = b"PHR1";
const HOST_ROOM_CHECKSUM_BYTES: usize = 32;
const MAX_HOST_ROOM_SECRET_BYTES: usize = 1_024;

/// Stable, redacted Veilid rendezvous operation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeilidRendezvousError {
    InvalidCode,
    InvalidMembership,
    InvalidRecord,
    NotFound,
    Conflict,
    Unavailable,
    TryAgain,
    Timeout,
    NoConnection,
    StaleRoute,
    WatchRenewal,
    Shutdown,
    OversizedMessage,
}

impl From<RoomCodeError> for VeilidRendezvousError {
    fn from(_: RoomCodeError) -> Self {
        Self::InvalidCode
    }
}

impl From<RendezvousError> for VeilidRendezvousError {
    fn from(_: RendezvousError) -> Self {
        Self::InvalidRecord
    }
}

/// Command-call failure preserving whether transport retry is safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeilidCommandError {
    Transport(TransportFailure),
    Wire(TransportWireError),
}

/// A host's open owner-only DFLT record and allocated current private route.
///
/// Call [`PublishedRoom::close`] before Veilid shutdown to release both Veilid
/// resources. This type intentionally has no diagnostic implementation because
/// its record key includes the DHT encryption secret.
pub struct PublishedRoom {
    record_key: RecordKey,
    route_id: RouteId,
    room_code: RoomCode,
    record: RendezvousRecord,
}

impl PublishedRoom {
    #[must_use]
    pub fn room_code(&self) -> &RoomCode {
        &self.room_code
    }

    #[must_use]
    pub fn record(&self) -> &RendezvousRecord {
        &self.record
    }

    pub fn route_id(&self) -> &RouteId {
        &self.route_id
    }

    /// Release the local route and close its DHT record.
    ///
    /// # Errors
    ///
    /// Returns a redacted availability category if either release fails.
    pub async fn close(self, api: &VeilidAPI) -> Result<(), VeilidRendezvousError> {
        let route_result = api.release_private_route(self.route_id);
        let record_result = api
            .routing_context()
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .close_dht_record(self.record_key)
            .await;
        if route_result.is_err() || record_result.is_err() {
            Err(VeilidRendezvousError::Unavailable)
        } else {
            Ok(())
        }
    }
}

/// A host-restarted room with a fresh private route and the original DHT
/// owner capability. It intentionally cannot reveal or recreate an old invite.
pub struct ResumedHostRoom {
    record_key: RecordKey,
    route_id: RouteId,
    record: RendezvousRecord,
}

impl ResumedHostRoom {
    #[must_use]
    pub fn record(&self) -> &RendezvousRecord {
        &self.record
    }

    pub fn route_id(&self) -> &RouteId {
        &self.route_id
    }

    /// Release the replacement route and close the reopened DHT record while
    /// retaining its protected restart capability.
    ///
    /// # Errors
    ///
    /// Returns a redacted availability category if either release fails.
    pub async fn close(self, api: &VeilidAPI) -> Result<(), VeilidRendezvousError> {
        let route_result = api.release_private_route(self.route_id);
        let record_result = api
            .routing_context()
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .close_dht_record(self.record_key)
            .await;
        if route_result.is_err() || record_result.is_err() {
            Err(VeilidRendezvousError::Unavailable)
        } else {
            Ok(())
        }
    }
}

struct HostRoomCapability {
    room_id: RoomId,
    host_principal: poche_protocol::PrincipalId,
    encrypted_record_key: Vec<u8>,
    owner_keypair: Vec<u8>,
}

impl Drop for HostRoomCapability {
    fn drop(&mut self) {
        self.encrypted_record_key.fill(0);
        self.owner_keypair.fill(0);
    }
}

impl HostRoomCapability {
    fn new(
        room_id: RoomId,
        host_principal: poche_protocol::PrincipalId,
        encrypted_record_key: Vec<u8>,
        owner_keypair: Vec<u8>,
    ) -> Result<Self, VeilidRendezvousError> {
        let value = Self {
            room_id,
            host_principal,
            encrypted_record_key,
            owner_keypair,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), VeilidRendezvousError> {
        if self.encrypted_record_key.is_empty()
            || self.encrypted_record_key.len() > 128
            || self.owner_keypair.is_empty()
            || self.owner_keypair.len() > 256
            || !self.encrypted_record_key.is_ascii()
            || !self.owner_keypair.is_ascii()
        {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, VeilidRendezvousError> {
        self.validate()?;
        let fields = [
            self.room_id.as_str().as_bytes(),
            self.host_principal.as_str().as_bytes(),
            self.encrypted_record_key.as_slice(),
            self.owner_keypair.as_slice(),
        ];
        let payload_len = fields
            .iter()
            .try_fold(0_usize, |length, field| length.checked_add(2 + field.len()))
            .ok_or(VeilidRendezvousError::InvalidRecord)?;
        let total_len = HOST_ROOM_MAGIC.len() + payload_len + HOST_ROOM_CHECKSUM_BYTES;
        if total_len > MAX_HOST_ROOM_SECRET_BYTES {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        let mut output = Vec::with_capacity(total_len);
        output.extend_from_slice(HOST_ROOM_MAGIC);
        for field in fields {
            let length =
                u16::try_from(field.len()).map_err(|_| VeilidRendezvousError::InvalidRecord)?;
            output.extend_from_slice(&length.to_be_bytes());
            output.extend_from_slice(field);
        }
        output.extend_from_slice(blake3::hash(&output).as_bytes());
        Ok(output)
    }

    fn decode(bytes: &[u8]) -> Result<Self, VeilidRendezvousError> {
        if bytes.len() < HOST_ROOM_MAGIC.len() + 8 + HOST_ROOM_CHECKSUM_BYTES
            || bytes.len() > MAX_HOST_ROOM_SECRET_BYTES
            || &bytes[..HOST_ROOM_MAGIC.len()] != HOST_ROOM_MAGIC
        {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        let checksum_offset = bytes.len() - HOST_ROOM_CHECKSUM_BYTES;
        if blake3::hash(&bytes[..checksum_offset]).as_bytes() != &bytes[checksum_offset..] {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        let mut cursor = HOST_ROOM_MAGIC.len();
        let mut take = || -> Result<&[u8], VeilidRendezvousError> {
            if cursor + 2 > checksum_offset {
                return Err(VeilidRendezvousError::InvalidRecord);
            }
            let length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
            cursor += 2;
            let end = cursor
                .checked_add(length)
                .ok_or(VeilidRendezvousError::InvalidRecord)?;
            if end > checksum_offset {
                return Err(VeilidRendezvousError::InvalidRecord);
            }
            let value = &bytes[cursor..end];
            cursor = end;
            Ok(value)
        };
        let room = take()?;
        let principal = take()?;
        let record_key = take()?;
        let owner_keypair = take()?;
        if cursor != checksum_offset {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        let room_id = std::str::from_utf8(room)
            .ok()
            .and_then(|value| RoomId::new(value.to_owned()).ok())
            .ok_or(VeilidRendezvousError::InvalidRecord);
        let host_principal = std::str::from_utf8(principal)
            .ok()
            .and_then(|value| poche_protocol::PrincipalId::new(value.to_owned()).ok())
            .ok_or(VeilidRendezvousError::InvalidRecord);
        match (room_id, host_principal) {
            (Ok(room_id), Ok(host_principal)) => Self::new(
                room_id,
                host_principal,
                record_key.to_vec(),
                owner_keypair.to_vec(),
            ),
            _ => Err(VeilidRendezvousError::InvalidRecord),
        }
    }

    fn with_record_key<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        operation(
            std::str::from_utf8(&self.encrypted_record_key)
                .expect("validated host record keys are UTF-8"),
        )
    }

    fn with_owner_keypair<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        operation(
            std::str::from_utf8(&self.owner_keypair)
                .expect("validated host owner keypairs are UTF-8"),
        )
    }
}

/// A client-validated rendezvous and imported remote private route.
pub struct ResolvedRoom {
    record_key: RecordKey,
    record: RendezvousRecord,
    route_id: RouteId,
}

impl ResolvedRoom {
    #[must_use]
    pub fn record(&self) -> &RendezvousRecord {
        &self.record
    }

    pub fn route_id(&self) -> &RouteId {
        &self.route_id
    }

    /// Release the imported remote route.
    ///
    /// # Errors
    ///
    /// Returns a stable availability category.
    pub async fn release(self, api: &VeilidAPI) -> Result<(), VeilidRendezvousError> {
        let routing = api
            .routing_context()
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        let _ = routing
            .cancel_dht_watch(self.record_key.clone(), None)
            .await;
        let record_result = routing.close_dht_record(self.record_key).await;
        let route_result = api.release_private_route(self.route_id);
        if record_result.is_err() || route_result.is_err() {
            Err(VeilidRendezvousError::Unavailable)
        } else {
            Ok(())
        }
    }
}

/// Pinned Veilid adapter for owner-only DFLT rendezvous records and
/// private-route calls.
pub struct VeilidRendezvous {
    api: VeilidAPI,
    routing: RoutingContext,
}

impl VeilidRendezvous {
    /// Construct a rendezvous adapter from an initialized Veilid API.
    ///
    /// # Errors
    ///
    /// Fails when Veilid is unavailable or shutting down.
    pub fn new(api: VeilidAPI) -> Result<Self, VeilidRendezvousError> {
        let routing = api
            .routing_context()
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        Ok(Self { api, routing })
    }

    /// Clone the initialized API handle for adjacent Veilid crypto operations.
    pub fn api(&self) -> VeilidAPI {
        self.api.clone()
    }

    /// Allocate a current private route, create one owner-only DFLT subkey,
    /// publish the bounded record, and issue a compact invite code.
    ///
    /// # Errors
    ///
    /// All Veilid diagnostics are collapsed to stable categories so encrypted
    /// record keys and route material cannot enter caller logs.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub async fn publish_room(
        &self,
        host_identity: &ApplicationIdentity,
        network: RoomNetwork,
        room_id: RoomId,
        metadata: PublicRoomMetadata,
        session_epoch: u64,
        route_epoch: u64,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<PublishedRoom, VeilidRendezvousError> {
        let route = self
            .api
            .new_private_route()
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        let schema = DHTSchema::dflt(RENDEZVOUS_DFLT_OWNER_SUBKEYS)
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        let Ok(descriptor) = self
            .routing
            .create_dht_record(CRYPTO_KIND_VLD0, schema, None)
            .await
        else {
            let _ = self.api.release_private_route(route.route_id);
            return Err(VeilidRendezvousError::Unavailable);
        };
        let record_key = descriptor.key();
        let Some(owner_keypair) = descriptor.owner_keypair() else {
            self.cleanup_failed_publish(record_key, route.route_id)
                .await;
            return Err(VeilidRendezvousError::Unavailable);
        };
        let record = match RendezvousRecord::new(
            network,
            room_id,
            host_identity.public(),
            metadata,
            session_epoch,
            route_epoch,
            expires_at_unix_ms,
            now_unix_ms,
            &route.blob,
        ) {
            Ok(value) => value,
            Err(error) => {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(error.into());
            }
        };
        let room_code = match RoomCode::issue(
            network,
            &record_key.to_string(),
            host_identity.public().principal_id,
            expires_at_unix_ms,
            now_unix_ms,
        ) {
            Ok(value) => value,
            Err(error) => {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(error.into());
            }
        };
        let encoded = match record.encode() {
            Ok(value) => value,
            Err(error) => {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(error.into());
            }
        };
        match self
            .routing
            .set_dht_value(record_key.clone(), RENDEZVOUS_DHT_SUBKEY, encoded, None)
            .await
        {
            Ok(None) => {}
            Ok(Some(_)) => {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(VeilidRendezvousError::Conflict);
            }
            Err(_) => {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(VeilidRendezvousError::Unavailable);
            }
        }
        if let Ok(true) = self
            .routing
            .flush_dht_record(record_key.clone(), Some(DHT_FLUSH_TIMEOUT))
            .await
        {
            if self
                .persist_published_host_capability(
                    host_identity,
                    &record,
                    &record_key,
                    &owner_keypair,
                )
                .await
                .is_err()
            {
                self.cleanup_failed_publish(record_key, route.route_id)
                    .await;
                return Err(VeilidRendezvousError::Unavailable);
            }
            Ok(PublishedRoom {
                record_key,
                route_id: route.route_id,
                room_code,
                record,
            })
        } else {
            self.cleanup_failed_publish(record_key, route.route_id)
                .await;
            Err(VeilidRendezvousError::Unavailable)
        }
    }

    /// Reopen a protected host-owned DHT record after process restart, rotate
    /// the host private route, and republish the bound rendezvous value.
    ///
    /// # Errors
    ///
    /// Requires the same stable application identity and the protected DHT
    /// owner capability saved by [`Self::publish_room`]. Invalid or missing
    /// protected state fails closed and never creates a replacement room.
    pub async fn resume_host_room(
        &self,
        host_identity: &ApplicationIdentity,
        room_id: &RoomId,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<ResumedHostRoom, VeilidRendezvousError> {
        let capability = self.load_host_capability(room_id).await?;
        if capability.host_principal != host_identity.public().principal_id
            || capability.room_id != *room_id
        {
            return Err(VeilidRendezvousError::InvalidMembership);
        }
        let record_key = capability
            .with_record_key(RecordKey::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        let owner_keypair = capability
            .with_owner_keypair(KeyPair::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        let _descriptor = self
            .routing
            .open_dht_record(record_key.clone(), Some(owner_keypair))
            .await
            .map_err(|_| VeilidRendezvousError::NotFound)?;
        let result = self
            .rotate_resumed_host_route(
                host_identity,
                room_id,
                record_key.clone(),
                expires_at_unix_ms,
                now_unix_ms,
            )
            .await;
        if result.is_err() {
            let _ = self.routing.close_dht_record(record_key).await;
        }
        result
    }

    /// Fetch and validate a code's encrypted DHT record, then import the
    /// advertised private route.
    ///
    /// # Errors
    ///
    /// Invalid/expired/missing/corrupt records fail before route use.
    pub async fn resolve_room(
        &self,
        code: &RoomCode,
        now_unix_ms: u64,
    ) -> Result<ResolvedRoom, VeilidRendezvousError> {
        let record_key = code
            .with_encrypted_record_key(RecordKey::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidCode)?;
        let _descriptor = self
            .routing
            .open_dht_record(record_key.clone(), None)
            .await
            .map_err(|_| VeilidRendezvousError::NotFound)?;
        let result = self.read_open_record(&record_key, code, now_unix_ms).await;
        if result.is_err() {
            let _ = self.routing.close_dht_record(record_key).await;
        }
        result
    }

    /// Fetch a member's current rendezvous record and route without reusing
    /// the original one-time invite code.
    ///
    /// # Errors
    ///
    /// Missing, corrupt, expired, or differently bound records fail before
    /// route import. No membership or record-key material is returned in the
    /// public error.
    pub async fn resolve_membership(
        &self,
        membership: &MembershipLocator,
        now_unix_ms: u64,
    ) -> Result<ResolvedRoom, VeilidRendezvousError> {
        let record_key = membership
            .with_encrypted_record_key(RecordKey::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidMembership)?;
        let _descriptor = self
            .routing
            .open_dht_record(record_key.clone(), None)
            .await
            .map_err(|_| VeilidRendezvousError::NotFound)?;
        let result = self
            .read_open_membership_record(&record_key, membership, now_unix_ms)
            .await;
        if result.is_err() {
            let _ = self.routing.close_dht_record(record_key).await;
        }
        result
    }

    /// Install or renew a non-authoritative DHT watch for the open rendezvous
    /// record. Every notification must still trigger a fresh validated read.
    ///
    /// # Errors
    ///
    /// A refused/dead watch is classified as watch renewal; shutdown and
    /// permanent API failures remain distinct.
    pub async fn renew_rendezvous_watch(
        &self,
        room: &ResolvedRoom,
    ) -> Result<(), VeilidRendezvousError> {
        match self
            .routing
            .watch_dht_values(room.record_key.clone(), None, None, None)
            .await
        {
            Ok(true) => Ok(()),
            Ok(false) => Err(VeilidRendezvousError::WatchRenewal),
            Err(error) => Err(map_veilid_error(&error)),
        }
    }

    /// Classify a Veilid update only as a refresh/lifecycle hint. No value or
    /// notification order is exposed as an authoritative game event.
    #[must_use]
    pub fn classify_update(room: &ResolvedRoom, update: &VeilidUpdate) -> RendezvousHint {
        match update {
            VeilidUpdate::ValueChange(change) if change.key == room.record_key => {
                classify_rendezvous_value_hint(
                    true,
                    change.count != 0 && !change.subkeys.is_empty(),
                )
            }
            VeilidUpdate::RouteChange(change)
                if change.dead_remote_routes.contains(&room.route_id)
                    || change.dead_routes.contains(&room.route_id) =>
            {
                classify_rendezvous_route_hint(true)
            }
            VeilidUpdate::Shutdown => RendezvousHint::Shutdown,
            _ => RendezvousHint::Ignore,
        }
    }

    /// Send a request/reply message to the resolved host private route.
    /// Invite redemption and later commands use this path rather than a DHT
    /// write or fire-and-forget notification.
    ///
    /// # Errors
    ///
    /// Enforces Veilid's exact 32768-byte request and response limit.
    pub async fn app_call(
        &self,
        room: &ResolvedRoom,
        request: Vec<u8>,
    ) -> Result<Vec<u8>, VeilidRendezvousError> {
        if request.len() > MAX_APP_CALL_BYTES {
            return Err(VeilidRendezvousError::OversizedMessage);
        }
        let response = self
            .routing
            .app_call(Target::RouteId(room.route_id.clone()), request)
            .await
            .map_err(|error| map_veilid_error(&error))?;
        if response.len() > MAX_APP_CALL_BYTES {
            return Err(VeilidRendezvousError::OversizedMessage);
        }
        Ok(response)
    }

    /// Send one strict signed command call and decode one strict authority
    /// reply. Retry orchestration uses [`crate::CommandRetryState`] so the
    /// caller cannot mutate the command between attempts.
    ///
    /// # Errors
    ///
    /// Separates retryable transport categories from permanent wire failures.
    pub async fn command_call(
        &self,
        room: &ResolvedRoom,
        call: &TransportCommandCall,
    ) -> Result<TransportCommandReply, VeilidCommandError> {
        let request = call.encode().map_err(VeilidCommandError::Wire)?;
        let response = self
            .app_call(room, request)
            .await
            .map_err(|error| VeilidCommandError::Transport(error.into()))?;
        TransportCommandReply::decode(&response).map_err(VeilidCommandError::Wire)
    }

    /// Decode an incoming host-side `AppCall` without trusting its transport
    /// sender or route as an application principal.
    ///
    /// # Errors
    ///
    /// Returns only bounded, redacted wire categories.
    pub fn decode_command_call(
        incoming: &VeilidAppCall,
    ) -> Result<TransportCommandCall, TransportWireError> {
        TransportCommandCall::decode(incoming.message())
    }

    /// Reply exactly once to an incoming Veilid `AppCall` with a strict
    /// authority result.
    ///
    /// # Errors
    ///
    /// Separates serialization from transport failure and enforces both the
    /// Poche and Veilid response ceilings.
    pub async fn reply_command_call(
        &self,
        incoming: &VeilidAppCall,
        reply: &TransportCommandReply,
    ) -> Result<(), VeilidCommandError> {
        let encoded = reply.encode().map_err(VeilidCommandError::Wire)?;
        self.api
            .app_call_reply(incoming.id(), encoded)
            .await
            .map_err(|error| VeilidCommandError::Transport(map_veilid_error(&error).into()))
    }

    async fn read_open_record(
        &self,
        record_key: &RecordKey,
        code: &RoomCode,
        now_unix_ms: u64,
    ) -> Result<ResolvedRoom, VeilidRendezvousError> {
        let value = self
            .routing
            .get_dht_value(record_key.clone(), RENDEZVOUS_DHT_SUBKEY, true)
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .ok_or(VeilidRendezvousError::NotFound)?;
        let record = RendezvousRecord::decode(value.data())?;
        record.validate_for_code(code, now_unix_ms)?;
        let route_blob = record.private_route_blob()?;
        let route_id = self
            .api
            .import_remote_private_route(route_blob)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        Ok(ResolvedRoom {
            record_key: record_key.clone(),
            record,
            route_id,
        })
    }

    async fn read_open_membership_record(
        &self,
        record_key: &RecordKey,
        membership: &MembershipLocator,
        now_unix_ms: u64,
    ) -> Result<ResolvedRoom, VeilidRendezvousError> {
        let value = self
            .routing
            .get_dht_value(record_key.clone(), RENDEZVOUS_DHT_SUBKEY, true)
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .ok_or(VeilidRendezvousError::NotFound)?;
        let record = RendezvousRecord::decode(value.data())?;
        membership
            .validate_rendezvous(&record, now_unix_ms)
            .map_err(|_| VeilidRendezvousError::InvalidMembership)?;
        let route_blob = record.private_route_blob()?;
        let route_id = self
            .api
            .import_remote_private_route(route_blob)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        Ok(ResolvedRoom {
            record_key: record_key.clone(),
            record,
            route_id,
        })
    }

    async fn rotate_resumed_host_route(
        &self,
        host_identity: &ApplicationIdentity,
        expected_room_id: &RoomId,
        record_key: RecordKey,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<ResumedHostRoom, VeilidRendezvousError> {
        let existing_value = self
            .routing
            .get_dht_value(record_key.clone(), RENDEZVOUS_DHT_SUBKEY, true)
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .ok_or(VeilidRendezvousError::NotFound)?;
        let existing = RendezvousRecord::decode(existing_value.data())?;
        if existing.host_identity != host_identity.public() || existing.room_id != *expected_room_id
        {
            return Err(VeilidRendezvousError::InvalidMembership);
        }
        let route = self
            .api
            .new_private_route()
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        let Some(route_epoch) = existing.route_epoch.checked_add(1) else {
            let _ = self.api.release_private_route(route.route_id);
            return Err(VeilidRendezvousError::InvalidRecord);
        };
        let updated = match RendezvousRecord::new(
            RoomNetwork::from(existing.network),
            existing.room_id,
            host_identity.public(),
            existing.metadata,
            existing.session_epoch,
            route_epoch,
            expires_at_unix_ms,
            now_unix_ms,
            &route.blob,
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = self.api.release_private_route(route.route_id);
                return Err(error.into());
            }
        };
        let encoded = match updated.encode() {
            Ok(value) => value,
            Err(error) => {
                let _ = self.api.release_private_route(route.route_id);
                return Err(error.into());
            }
        };
        let set_result = self
            .routing
            .set_dht_value(record_key.clone(), RENDEZVOUS_DHT_SUBKEY, encoded, None)
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable);
        match set_result {
            Ok(None) => {}
            Ok(Some(_)) => {
                let _ = self.api.release_private_route(route.route_id);
                return Err(VeilidRendezvousError::Conflict);
            }
            Err(error) => {
                let _ = self.api.release_private_route(route.route_id);
                return Err(error);
            }
        }
        if !matches!(
            self.routing
                .flush_dht_record(record_key.clone(), Some(DHT_FLUSH_TIMEOUT))
                .await,
            Ok(true)
        ) {
            let _ = self.api.release_private_route(route.route_id);
            return Err(VeilidRendezvousError::Unavailable);
        }
        Ok(ResumedHostRoom {
            record_key,
            route_id: route.route_id,
            record: updated,
        })
    }

    async fn save_host_capability(
        &self,
        capability: &HostRoomCapability,
    ) -> Result<(), VeilidRendezvousError> {
        let mut encoded = capability.encode()?;
        let result = self
            .api
            .save_user_secret(host_room_store_key(&capability.room_id), encoded.clone())
            .await
            .map(|_| ())
            .map_err(|_| VeilidRendezvousError::Unavailable);
        encoded.fill(0);
        result
    }

    async fn persist_published_host_capability(
        &self,
        host_identity: &ApplicationIdentity,
        record: &RendezvousRecord,
        record_key: &RecordKey,
        owner_keypair: &KeyPair,
    ) -> Result<(), VeilidRendezvousError> {
        let capability = HostRoomCapability::new(
            record.room_id.clone(),
            host_identity.public().principal_id,
            record_key.to_string().into_bytes(),
            owner_keypair.to_string().into_bytes(),
        )?;
        self.save_host_capability(&capability).await
    }

    async fn load_host_capability(
        &self,
        room_id: &RoomId,
    ) -> Result<HostRoomCapability, VeilidRendezvousError> {
        let mut encoded = self
            .api
            .load_user_secret(host_room_store_key(room_id))
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?
            .ok_or(VeilidRendezvousError::NotFound)?;
        let result = HostRoomCapability::decode(&encoded);
        encoded.fill(0);
        result
    }

    async fn cleanup_failed_publish(&self, record_key: RecordKey, route_id: RouteId) {
        let _ = self.routing.close_dht_record(record_key).await;
        let _ = self.api.release_private_route(route_id);
    }
}

fn host_room_store_key(room_id: &RoomId) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-host-room-store-key-v1\0");
    hasher.update(room_id.as_str().as_bytes());
    format!("{HOST_ROOM_KEY_PREFIX}{}", hasher.finalize().to_hex())
}

fn map_veilid_error(error: &VeilidAPIError) -> VeilidRendezvousError {
    match error {
        VeilidAPIError::TryAgain { .. } => VeilidRendezvousError::TryAgain,
        VeilidAPIError::Timeout => VeilidRendezvousError::Timeout,
        VeilidAPIError::NoConnection { .. } => VeilidRendezvousError::NoConnection,
        VeilidAPIError::InvalidTarget { .. } => VeilidRendezvousError::StaleRoute,
        VeilidAPIError::Shutdown | VeilidAPIError::NotInitialized => {
            VeilidRendezvousError::Shutdown
        }
        _ => VeilidRendezvousError::Unavailable,
    }
}

impl From<VeilidRendezvousError> for TransportFailure {
    fn from(error: VeilidRendezvousError) -> Self {
        match error {
            VeilidRendezvousError::TryAgain => Self::TryAgain,
            VeilidRendezvousError::Timeout => Self::Timeout,
            VeilidRendezvousError::NoConnection => Self::NoConnection,
            VeilidRendezvousError::StaleRoute => Self::StaleRoute,
            VeilidRendezvousError::WatchRenewal => Self::WatchRenewal,
            VeilidRendezvousError::Shutdown => Self::Shutdown,
            VeilidRendezvousError::OversizedMessage => Self::Oversized,
            VeilidRendezvousError::InvalidCode
            | VeilidRendezvousError::InvalidMembership
            | VeilidRendezvousError::InvalidRecord => Self::InvalidMessage,
            VeilidRendezvousError::NotFound
            | VeilidRendezvousError::Conflict
            | VeilidRendezvousError::Unavailable => Self::Permanent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_errors_never_carry_secret_context() {
        let errors = [
            VeilidRendezvousError::InvalidCode,
            VeilidRendezvousError::InvalidMembership,
            VeilidRendezvousError::InvalidRecord,
            VeilidRendezvousError::NotFound,
            VeilidRendezvousError::Conflict,
            VeilidRendezvousError::Unavailable,
            VeilidRendezvousError::TryAgain,
            VeilidRendezvousError::Timeout,
            VeilidRendezvousError::NoConnection,
            VeilidRendezvousError::StaleRoute,
            VeilidRendezvousError::WatchRenewal,
            VeilidRendezvousError::Shutdown,
            VeilidRendezvousError::OversizedMessage,
        ];
        for error in errors {
            let rendered = format!("{error:?}");
            assert!(rendered.len() < 32);
            assert!(!rendered.contains("VLD0:"));
        }
    }

    #[test]
    fn host_restart_capability_is_strict_checksummed_and_redacted_by_type() {
        let room_id = RoomId::new("restart-capability-room").unwrap();
        let principal = poche_protocol::PrincipalId::new("a".repeat(64)).unwrap();
        let capability = HostRoomCapability::new(
            room_id.clone(),
            principal,
            b"VLD0:encrypted-record-key".to_vec(),
            b"VLD0:owner-public:owner-secret".to_vec(),
        )
        .unwrap();
        let encoded = capability.encode().unwrap();
        let decoded = HostRoomCapability::decode(&encoded).unwrap();
        assert_eq!(decoded.room_id, room_id);
        assert_eq!(
            decoded.encrypted_record_key,
            capability.encrypted_record_key
        );
        assert_eq!(decoded.owner_keypair, capability.owner_keypair);
        assert!(!host_room_store_key(&room_id).contains(room_id.as_str()));

        let mut corrupt = encoded;
        corrupt[8] ^= 1;
        assert!(HostRoomCapability::decode(&corrupt).is_err());
    }

    #[test]
    fn released_api_errors_map_to_explicit_retry_categories_without_messages() {
        let cases = [
            (
                VeilidAPIError::try_again("secret retry diagnostic"),
                VeilidRendezvousError::TryAgain,
                TransportFailure::TryAgain,
            ),
            (
                VeilidAPIError::timeout(),
                VeilidRendezvousError::Timeout,
                TransportFailure::Timeout,
            ),
            (
                VeilidAPIError::no_connection("secret route diagnostic"),
                VeilidRendezvousError::NoConnection,
                TransportFailure::NoConnection,
            ),
            (
                VeilidAPIError::invalid_target("stale route secret"),
                VeilidRendezvousError::StaleRoute,
                TransportFailure::StaleRoute,
            ),
            (
                VeilidAPIError::shutdown(),
                VeilidRendezvousError::Shutdown,
                TransportFailure::Shutdown,
            ),
        ];
        for (source, expected, retry) in cases {
            let mapped = map_veilid_error(&source);
            assert_eq!(mapped, expected);
            assert_eq!(TransportFailure::from(mapped), retry);
            assert!(!format!("{mapped:?}").contains("secret"));
        }
    }
}
