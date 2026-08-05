use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{PROTOCOL_VERSION_V1, RoomId};
use serde::{Deserialize, Serialize};

use crate::{ApplicationPublicIdentity, RoomCode, RoomNetwork};

pub const RENDEZVOUS_SCHEMA_VERSION_V1: u16 = 1;
pub const RENDEZVOUS_DHT_SUBKEY: u32 = 0;
pub const RENDEZVOUS_DFLT_OWNER_SUBKEYS: u16 = 1;
pub const MAX_RENDEZVOUS_VALUE_BYTES: usize = 16 * 1024;
const MAX_ROOM_LABEL_BYTES: usize = 64;
const MAX_PRIVATE_ROUTE_BLOB_BYTES: usize = 8 * 1024;
const MAX_SUPPORTED_PLAYERS: u8 = 8;

/// Bounded public room metadata stored in the DHT rendezvous record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRoomMetadata {
    pub label: String,
    pub max_players: u8,
    pub spectators_allowed: bool,
}

impl PublicRoomMetadata {
    /// Construct bounded public metadata.
    ///
    /// # Errors
    ///
    /// Rejects empty/control-bearing labels and unsupported table sizes.
    pub fn new(
        label: impl Into<String>,
        max_players: u8,
        spectators_allowed: bool,
    ) -> Result<Self, RendezvousError> {
        let value = Self {
            label: label.into(),
            max_players,
            spectators_allowed,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), RendezvousError> {
        if self.label.is_empty()
            || self.label.len() > MAX_ROOM_LABEL_BYTES
            || self.label.chars().any(char::is_control)
            || !(2..=MAX_SUPPORTED_PLAYERS).contains(&self.max_players)
        {
            return Err(RendezvousError::InvalidMetadata);
        }
        Ok(())
    }
}

/// Host-owned DFLT subkey value. Invite secrets are structurally absent.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendezvousRecord {
    pub schema_version: u16,
    pub protocol_version: u16,
    pub network: RoomNetworkWire,
    pub room_id: RoomId,
    pub host_identity: ApplicationPublicIdentity,
    pub metadata: PublicRoomMetadata,
    pub session_epoch: u64,
    pub route_epoch: u64,
    pub expires_at_unix_ms: u64,
    private_route_base64url: String,
}

impl fmt::Debug for RendezvousRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RendezvousRecord")
            .field("schema_version", &self.schema_version)
            .field("protocol_version", &self.protocol_version)
            .field("network", &self.network)
            .field("room_id", &self.room_id)
            .field("host_identity", &self.host_identity)
            .field("metadata", &self.metadata)
            .field("session_epoch", &self.session_epoch)
            .field("route_epoch", &self.route_epoch)
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .field("private_route_base64url", &"<redacted-route>")
            .finish()
    }
}

/// Stable JSON representation of a room network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomNetworkWire {
    VeilidPublic,
    VeilidLocal,
}

impl From<RoomNetwork> for RoomNetworkWire {
    fn from(value: RoomNetwork) -> Self {
        match value {
            RoomNetwork::VeilidPublic => Self::VeilidPublic,
            RoomNetwork::VeilidLocal => Self::VeilidLocal,
        }
    }
}

impl From<RoomNetworkWire> for RoomNetwork {
    fn from(value: RoomNetworkWire) -> Self {
        match value {
            RoomNetworkWire::VeilidPublic => Self::VeilidPublic,
            RoomNetworkWire::VeilidLocal => Self::VeilidLocal,
        }
    }
}

/// Stable, secret-free rendezvous validation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousError {
    InvalidMetadata,
    InvalidRoute,
    InvalidEncoding,
    Oversized,
    ProtocolMismatch,
    SchemaMismatch,
    NetworkMismatch,
    HostMismatch,
    Expired,
    InvalidEpoch,
}

impl RendezvousRecord {
    /// Build a bounded current-route record.
    ///
    /// # Errors
    ///
    /// Rejects invalid metadata, epochs, expiry, route size, or public identity.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        network: RoomNetwork,
        room_id: RoomId,
        host_identity: ApplicationPublicIdentity,
        metadata: PublicRoomMetadata,
        session_epoch: u64,
        route_epoch: u64,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
        private_route_blob: &[u8],
    ) -> Result<Self, RendezvousError> {
        metadata.validate()?;
        if session_epoch == 0 || route_epoch == 0 {
            return Err(RendezvousError::InvalidEpoch);
        }
        if expires_at_unix_ms <= now_unix_ms {
            return Err(RendezvousError::Expired);
        }
        validate_route_blob(private_route_blob)?;
        if host_identity.validate().is_err() {
            return Err(RendezvousError::HostMismatch);
        }
        Ok(Self {
            schema_version: RENDEZVOUS_SCHEMA_VERSION_V1,
            protocol_version: PROTOCOL_VERSION_V1,
            network: network.into(),
            room_id,
            host_identity,
            metadata,
            session_epoch,
            route_epoch,
            expires_at_unix_ms,
            private_route_base64url: BASE64URL_NOPAD.encode(private_route_blob),
        })
    }

    /// Encode the DHT value as bounded strict JSON.
    ///
    /// # Errors
    ///
    /// Returns a stable encoding/size category.
    pub fn encode(&self) -> Result<Vec<u8>, RendezvousError> {
        self.validate_shape()?;
        let encoded = serde_json::to_vec(self).map_err(|_| RendezvousError::InvalidEncoding)?;
        if encoded.len() > MAX_RENDEZVOUS_VALUE_BYTES {
            return Err(RendezvousError::Oversized);
        }
        Ok(encoded)
    }

    /// Decode strict bounded JSON and revalidate every refinement.
    ///
    /// # Errors
    ///
    /// Rejects malformed, oversized, unsupported, or invalid values.
    pub fn decode(bytes: &[u8]) -> Result<Self, RendezvousError> {
        if bytes.len() > MAX_RENDEZVOUS_VALUE_BYTES {
            return Err(RendezvousError::Oversized);
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| RendezvousError::InvalidEncoding)?;
        value.validate_shape()?;
        Ok(value)
    }

    /// Validate this record against the capability carried by a room code.
    ///
    /// # Errors
    ///
    /// Fails for network/host/version/expiry mismatches before route import.
    pub fn validate_for_code(
        &self,
        code: &RoomCode,
        now_unix_ms: u64,
    ) -> Result<(), RendezvousError> {
        self.validate_shape()?;
        if RoomNetwork::from(self.network) != code.network() {
            return Err(RendezvousError::NetworkMismatch);
        }
        if &self.host_identity.principal_id != code.host_principal() {
            return Err(RendezvousError::HostMismatch);
        }
        if self.expires_at_unix_ms <= now_unix_ms
            || code.expires_at_unix_ms() <= now_unix_ms
            || code.expires_at_unix_ms() > self.expires_at_unix_ms
        {
            return Err(RendezvousError::Expired);
        }
        Ok(())
    }

    /// Decode the publishable private-route blob after record validation.
    ///
    /// # Errors
    ///
    /// Returns a redacted route/encoding category.
    pub fn private_route_blob(&self) -> Result<Vec<u8>, RendezvousError> {
        let decoded = BASE64URL_NOPAD
            .decode(self.private_route_base64url.as_bytes())
            .map_err(|_| RendezvousError::InvalidRoute)?;
        validate_route_blob(&decoded)?;
        Ok(decoded)
    }

    fn validate_shape(&self) -> Result<(), RendezvousError> {
        if self.schema_version != RENDEZVOUS_SCHEMA_VERSION_V1 {
            return Err(RendezvousError::SchemaMismatch);
        }
        if self.protocol_version != PROTOCOL_VERSION_V1 {
            return Err(RendezvousError::ProtocolMismatch);
        }
        if self.session_epoch == 0 || self.route_epoch == 0 {
            return Err(RendezvousError::InvalidEpoch);
        }
        self.metadata.validate()?;
        let route = self.private_route_blob()?;
        if BASE64URL_NOPAD.encode(&route) != self.private_route_base64url {
            return Err(RendezvousError::InvalidRoute);
        }
        if self.host_identity.validate().is_err() {
            return Err(RendezvousError::HostMismatch);
        }
        Ok(())
    }
}

fn validate_route_blob(route: &[u8]) -> Result<(), RendezvousError> {
    if route.is_empty() || route.len() > MAX_PRIVATE_ROUTE_BLOB_BYTES {
        return Err(RendezvousError::InvalidRoute);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApplicationIdentity, ExplicitInsecureDevelopment, IdentityStoragePolicy,
        InsecureMemoryIdentityStore,
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

    fn public_identity() -> ApplicationPublicIdentity {
        let store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        block_on(ApplicationIdentity::load_or_create(
            &store,
            IdentityStoragePolicy::AllowExplicitInsecure(
                ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
            ),
        ))
        .unwrap()
        .public()
    }

    #[test]
    fn strict_bounded_record_round_trips_without_invite_material() {
        let record = RendezvousRecord::new(
            RoomNetwork::VeilidLocal,
            RoomId::new("rendezvous-room").unwrap(),
            public_identity(),
            PublicRoomMetadata::new("Friday Poche", 3, true).unwrap(),
            1,
            1,
            30_000,
            10_000,
            b"publishable-private-route",
        )
        .unwrap();
        let encoded = record.encode().unwrap();
        assert!(encoded.len() <= MAX_RENDEZVOUS_VALUE_BYTES);
        assert_eq!(RendezvousRecord::decode(&encoded).unwrap(), record);
        let rendered = String::from_utf8(encoded).unwrap();
        assert!(!rendered.contains("invite"));
        assert!(!format!("{record:?}").contains("cHVibGlzaGFibGU"));
    }

    #[test]
    fn record_validation_binds_network_host_and_expiry() {
        const RECORD_KEY: &str = "VLD0:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        let host = public_identity();
        let code = RoomCode::issue(
            RoomNetwork::VeilidLocal,
            RECORD_KEY,
            host.principal_id.clone(),
            20_000,
            10_000,
        )
        .unwrap();
        let mut record = RendezvousRecord::new(
            RoomNetwork::VeilidLocal,
            RoomId::new("bound-room").unwrap(),
            host,
            PublicRoomMetadata::new("Bound", 2, false).unwrap(),
            1,
            1,
            30_000,
            10_000,
            b"route",
        )
        .unwrap();
        assert_eq!(record.validate_for_code(&code, 10_000), Ok(()));
        record.network = RoomNetworkWire::VeilidPublic;
        assert_eq!(
            record.validate_for_code(&code, 10_000),
            Err(RendezvousError::NetworkMismatch)
        );
        record.network = RoomNetworkWire::VeilidLocal;
        assert_eq!(
            record.validate_for_code(&code, 20_000),
            Err(RendezvousError::Expired)
        );
    }
}
