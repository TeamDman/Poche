// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Transport-neutral client used by graphical, CLI, agent, and puppet devices.
//!
//! The client owns public profile/certificate metadata and a signer capability;
//! it never exposes or serializes device secret bytes. Adapters provide exact
//! projections and advertised actions. A renderer or policy selects only an
//! opaque advertised action ID, so neither can construct a privileged command
//! graph outside the shared authority path.

#![allow(
    clippy::missing_errors_doc,
    clippy::unsafe_derive_deserialize,
    reason = "validated reflected wire/profile values use stable redacted DeviceClientError categories after Serde decoding"
)]

use core::fmt;

use facet::Facet;
use poche_protocol::{
    CaptureCancelWire, CaptureRequestWire, CaptureResponseWire, CommandId, CommandPayload,
    DeviceCertificateWire, DeviceId, PrincipalId, ProjectionEnvelope, RoomId, SemanticHash,
    SignatureBytes,
};
use serde::{Deserialize, Serialize};

/// Public, persistable portion of a protected device profile.
///
/// The profile label is a local selector. Player/device authority comes only
/// from the validated certificate. `signing_key_handle` is an opaque key-store
/// locator and must never contain or derive raw key material.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProfile {
    pub schema_version: u16,
    pub label: String,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub certificate: DeviceCertificateWire,
    pub signing_key_handle: String,
}

impl DeviceProfile {
    pub const SCHEMA_VERSION_V1: u16 = 1;

    /// Validate public profile/certificate bindings. Key-store availability and
    /// cryptographic proof are checked through [`DeviceSigner`].
    pub fn validate(&self) -> Result<(), DeviceClientError> {
        self.certificate
            .validate()
            .map_err(|_| DeviceClientError::InvalidProfile)?;
        if self.schema_version != Self::SCHEMA_VERSION_V1
            || self.label.is_empty()
            || self.label.len() > 64
            || self.label.chars().any(char::is_control)
            || self.signing_key_handle.is_empty()
            || self.signing_key_handle.len() > 256
            || self.signing_key_handle.chars().any(char::is_control)
            || self.player_id != self.certificate.player_id
            || self.device_id != self.certificate.device_id
        {
            Err(DeviceClientError::InvalidProfile)
        } else {
            Ok(())
        }
    }
}

/// Signing port whose implementation owns access to protected device keys.
pub trait DeviceSigner {
    /// Sign canonical bytes as the exact device named by `profile`.
    fn sign_device_bytes(
        &self,
        profile: &DeviceProfile,
        canonical_bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError>;
}

/// One action advertised for one exact projection revision.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvertisedAction {
    pub id: String,
    pub label: String,
    pub payload: CommandPayload,
}

impl AdvertisedAction {
    fn validate(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 96
            && self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            && !self.label.is_empty()
            && self.label.len() <= 256
            && !self.label.chars().any(char::is_control)
    }
}

/// Exact-recipient observation and action set delivered atomically by an
/// adapter. Actions expire with this revision and projection hash.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceObservation {
    pub projection: ProjectionEnvelope,
    pub projection_hash: SemanticHash,
    pub actions: Vec<AdvertisedAction>,
}

impl DeviceObservation {
    pub fn validate_for(
        &self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<(), DeviceClientError> {
        if &self.projection.room_id != room_id
            || self.projection.principal_id != profile.player_id
            || self.actions.iter().any(|action| !action.validate())
            || !self.actions.windows(2).all(|pair| pair[0].id < pair[1].id)
        {
            Err(DeviceClientError::InvalidObservation)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn action(&self, action_id: &str) -> Option<&AdvertisedAction> {
        self.actions
            .binary_search_by(|action| action.id.as_str().cmp(action_id))
            .ok()
            .map(|index| &self.actions[index])
    }
}

/// Transport-neutral typed command request before adapter-specific signing and
/// host-authoritative or replicated envelope construction.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceActionRequest {
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub command_id: CommandId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub expected_revision: u64,
    pub expected_projection_hash: SemanticHash,
    pub action_id: String,
    pub payload: CommandPayload,
}

/// Result of invoking an advertised action through an ordinary room adapter.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceActionResult {
    Committed {
        command_id: CommandId,
        revision: u64,
    },
    Denied {
        command_id: CommandId,
        code: String,
    },
}

/// Cooperation messages that do not enter authoritative game history.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum DeviceCooperationRequest {
    Capture(CaptureRequestWire),
    CancelCapture(CaptureCancelWire),
}

/// Exact result of one target-device cooperation exchange.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum DeviceCooperationResult {
    Capture(CaptureResponseWire),
}

/// Adapter boundary shared by loopback, gateway, and Veilid clients.
pub trait DeviceTransport {
    /// Obtain one atomic exact-recipient observation.
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError>;

    /// Invoke one already validated advertised action.
    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError>;

    /// Wait for a strictly later projection. Adapters own scheduling/timeouts;
    /// reducers and this client never read wall time.
    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError>;

    /// Deliver exact-target cooperation without treating it as a game command.
    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError>;
}

/// Device client that constrains a UI, CLI, agent, or puppet to advertised
/// actions from its own exact observation.
pub struct PlayerDeviceClient<T> {
    profile: DeviceProfile,
    transport: T,
}

impl<T: DeviceTransport> PlayerDeviceClient<T> {
    pub fn new(profile: DeviceProfile, transport: T) -> Result<Self, DeviceClientError> {
        profile.validate()?;
        Ok(Self { profile, transport })
    }

    #[must_use]
    pub const fn profile(&self) -> &DeviceProfile {
        &self.profile
    }

    pub fn observe(&mut self, room_id: &RoomId) -> Result<DeviceObservation, DeviceClientError> {
        let observation = self.transport.observe(&self.profile, room_id)?;
        observation.validate_for(&self.profile, room_id)?;
        Ok(observation)
    }

    pub fn actions(
        &mut self,
        room_id: &RoomId,
    ) -> Result<Vec<AdvertisedAction>, DeviceClientError> {
        Ok(self.observe(room_id)?.actions)
    }

    pub fn invoke(
        &mut self,
        observation: &DeviceObservation,
        action_id: &str,
        command_id: CommandId,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        observation.validate_for(&self.profile, &observation.projection.room_id)?;
        let action = observation
            .action(action_id)
            .ok_or(DeviceClientError::UnknownAction)?;
        let request = DeviceActionRequest {
            room_id: observation.projection.room_id.clone(),
            session_epoch: observation.projection.session_epoch,
            command_id,
            player_id: self.profile.player_id.clone(),
            device_id: self.profile.device_id.clone(),
            expected_revision: observation.projection.current_revision,
            expected_projection_hash: observation.projection_hash,
            action_id: action.id.clone(),
            payload: action.payload.clone(),
        };
        self.transport.invoke(&self.profile, request)
    }

    pub fn wait(
        &mut self,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        let observation = self
            .transport
            .wait(&self.profile, room_id, after_revision)?;
        observation.validate_for(&self.profile, room_id)?;
        if observation.projection.current_revision <= after_revision {
            return Err(DeviceClientError::NoProgress);
        }
        Ok(observation)
    }

    pub fn cooperate(
        &mut self,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.transport
            .cooperate(&self.profile, target_device, request)
    }

    #[must_use]
    pub fn into_transport(self) -> T {
        self.transport
    }
}

/// Stable device-client failure without secret material or rejected bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceClientError {
    InvalidProfile,
    KeyUnavailable,
    SigningFailed,
    TransportUnavailable,
    InvalidObservation,
    UnknownAction,
    StaleRevision,
    NoProgress,
    AuthorizationDenied,
    ProtocolViolation,
}

impl fmt::Display for DeviceClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProfile => "device profile is invalid",
            Self::KeyUnavailable => "protected device key is unavailable",
            Self::SigningFailed => "device signing failed",
            Self::TransportUnavailable => "device transport is unavailable",
            Self::InvalidObservation => "exact-recipient observation is invalid",
            Self::UnknownAction => "action was not advertised for this observation",
            Self::StaleRevision => "action targets a stale revision",
            Self::NoProgress => "wait returned no later revision",
            Self::AuthorizationDenied => "device action was denied",
            Self::ProtocolViolation => "device adapter violated the shared protocol",
        })
    }
}

impl std::error::Error for DeviceClientError {}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CertificateId, CorrelationId, DeviceCapabilityWire, DeviceCustodyWire, EventId,
        ProjectionId, ProjectionPayload, REPLICATION_SCHEMA_VERSION_V1,
        REPLICATION_SIGNATURE_DOMAIN_V1, RoomPhase, SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
        SignatureIntent, SignatureMetadata, UnsignedDeviceCertificateWire,
    };

    use super::*;

    #[derive(Clone)]
    struct FixtureTransport {
        observation: DeviceObservation,
        invoked: Vec<DeviceActionRequest>,
    }

    impl DeviceTransport for FixtureTransport {
        fn observe(
            &mut self,
            _profile: &DeviceProfile,
            _room_id: &RoomId,
        ) -> Result<DeviceObservation, DeviceClientError> {
            Ok(self.observation.clone())
        }

        fn invoke(
            &mut self,
            _profile: &DeviceProfile,
            request: DeviceActionRequest,
        ) -> Result<DeviceActionResult, DeviceClientError> {
            self.invoked.push(request.clone());
            Ok(DeviceActionResult::Committed {
                command_id: request.command_id,
                revision: request.expected_revision + 1,
            })
        }

        fn wait(
            &mut self,
            _profile: &DeviceProfile,
            _room_id: &RoomId,
            _after_revision: u64,
        ) -> Result<DeviceObservation, DeviceClientError> {
            Ok(self.observation.clone())
        }

        fn cooperate(
            &mut self,
            _profile: &DeviceProfile,
            _target_device: &DeviceId,
            _request: DeviceCooperationRequest,
        ) -> Result<DeviceCooperationResult, DeviceClientError> {
            Err(DeviceClientError::TransportUnavailable)
        }
    }

    fn signature(byte: &str) -> SignatureBytes {
        SignatureBytes::new(byte.repeat(128)).unwrap()
    }

    fn fixture() -> (DeviceProfile, DeviceObservation) {
        let player_key = "11".repeat(32);
        let device_key = "22".repeat(32);
        let player = PrincipalId::new(player_key).unwrap();
        let device = DeviceId::new(device_key.clone()).unwrap();
        let certificate = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new("device-certificate-1").unwrap(),
            player_id: player.clone(),
            device_id: device.clone(),
            device_signing_public_key: device_key,
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player.clone(),
            },
        }
        .attach_signature(signature("0"))
        .unwrap();
        let profile = DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: "alice-cli".to_owned(),
            player_id: player.clone(),
            device_id: device,
            certificate,
            signing_key_handle: "windows-credential:poche/alice-cli".to_owned(),
        };
        let projection = ProjectionEnvelope {
            protocol_version: 1,
            room_id: RoomId::new("room-1").unwrap(),
            session_epoch: 1,
            projection_id: ProjectionId::new("projection-7").unwrap(),
            principal_id: player.clone(),
            current_revision: 7,
            projection_epoch: 1,
            correlation_id: CorrelationId::new("correlation-7").unwrap(),
            causation_id: EventId::new("event-7").unwrap(),
            payload: ProjectionPayload {
                phase: RoomPhase::Lobby,
                members: Vec::new(),
                public_game_state: None,
                own_hand: None,
                granted_hands: Vec::new(),
                public_history: Vec::new(),
            },
            signature: SignatureMetadata {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player,
                signature: signature("1"),
            },
        };
        let observation = DeviceObservation {
            projection,
            projection_hash: SemanticHash([7; 32]),
            actions: vec![AdvertisedAction {
                id: "ready".to_owned(),
                label: "Ready".to_owned(),
                payload: CommandPayload::Ready,
            }],
        };
        (profile, observation)
    }

    #[test]
    fn client_invokes_only_an_action_advertised_for_exact_revision() {
        let (profile, observation) = fixture();
        let transport = FixtureTransport {
            observation,
            invoked: Vec::new(),
        };
        let mut client = PlayerDeviceClient::new(profile, transport).unwrap();
        let room = RoomId::new("room-1").unwrap();
        let observed = client.observe(&room).unwrap();
        assert_eq!(
            client.invoke(
                &observed,
                "unadvertised",
                CommandId::new("command-bad").unwrap()
            ),
            Err(DeviceClientError::UnknownAction)
        );
        let result = client
            .invoke(&observed, "ready", CommandId::new("command-1").unwrap())
            .unwrap();
        assert_eq!(
            result,
            DeviceActionResult::Committed {
                command_id: CommandId::new("command-1").unwrap(),
                revision: 8,
            }
        );
        let transport = client.into_transport();
        assert_eq!(transport.invoked.len(), 1);
        assert_eq!(transport.invoked[0].expected_revision, 7);
        assert_eq!(
            transport.invoked[0].expected_projection_hash,
            SemanticHash([7; 32])
        );
        assert_eq!(transport.invoked[0].device_id.as_str(), "22".repeat(32));
    }

    #[test]
    fn wait_refuses_an_adapter_that_returns_no_progress() {
        let (profile, observation) = fixture();
        let transport = FixtureTransport {
            observation,
            invoked: Vec::new(),
        };
        let mut client = PlayerDeviceClient::new(profile, transport).unwrap();
        assert_eq!(
            client.wait(&RoomId::new("room-1").unwrap(), 7),
            Err(DeviceClientError::NoProgress)
        );
    }
}
