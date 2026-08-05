use std::{str::FromStr, time::Duration};

use poche_protocol::RoomId;
use veilid_core::{
    CRYPTO_KIND_VLD0, DHTSchema, RecordKey, RouteId, RoutingContext, Target, VeilidAPI,
};

use crate::{
    ApplicationIdentity, PublicRoomMetadata, RENDEZVOUS_DFLT_OWNER_SUBKEYS, RENDEZVOUS_DHT_SUBKEY,
    RendezvousError, RendezvousRecord, RoomCode, RoomCodeError, RoomNetwork,
};

const MAX_APP_CALL_BYTES: usize = 32_768;
const DHT_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Stable, redacted Veilid rendezvous operation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeilidRendezvousError {
    InvalidCode,
    InvalidRecord,
    NotFound,
    Conflict,
    Unavailable,
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

/// A client-validated rendezvous and imported remote private route.
pub struct ResolvedRoom {
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
    pub fn release(self, api: &VeilidAPI) -> Result<(), VeilidRendezvousError> {
        api.release_private_route(self.route_id)
            .map_err(|_| VeilidRendezvousError::Unavailable)
    }
}

/// Released Veilid 0.5.7 adapter for owner-only DFLT rendezvous records and
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

    /// Allocate a current private route, create one owner-only DFLT subkey,
    /// publish the bounded record, and issue a compact invite code.
    ///
    /// # Errors
    ///
    /// All Veilid diagnostics are collapsed to stable categories so encrypted
    /// record keys and route material cannot enter caller logs.
    #[allow(clippy::too_many_arguments)]
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
        let _ = self.routing.close_dht_record(record_key).await;
        result
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
            .map_err(|_| VeilidRendezvousError::Unavailable)?;
        if response.len() > MAX_APP_CALL_BYTES {
            return Err(VeilidRendezvousError::OversizedMessage);
        }
        Ok(response)
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
        Ok(ResolvedRoom { record, route_id })
    }

    async fn cleanup_failed_publish(&self, record_key: RecordKey, route_id: RouteId) {
        let _ = self.routing.close_dht_record(record_key).await;
        let _ = self.api.release_private_route(route_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_errors_never_carry_secret_context() {
        let errors = [
            VeilidRendezvousError::InvalidCode,
            VeilidRendezvousError::InvalidRecord,
            VeilidRendezvousError::NotFound,
            VeilidRendezvousError::Conflict,
            VeilidRendezvousError::Unavailable,
            VeilidRendezvousError::OversizedMessage,
        ];
        for error in errors {
            let rendered = format!("{error:?}");
            assert!(rendered.len() < 32);
            assert!(!rendered.contains("VLD0:"));
        }
    }
}
