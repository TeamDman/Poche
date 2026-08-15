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
    CaptureCancelWire, CaptureProviderAdvertisementWire, CaptureRequestWire, CaptureResponseWire,
    CommandId, CommandPayload, DEVICE_ACTION_SCHEMA_VERSION_V1, DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
    DeviceActionWire, DeviceCertificateWire, DeviceId, DeviceObservationModeWire,
    DeviceObservationRequestWire, DeviceRouteOperationWire, DeviceRouteRequestWire,
    DeviceRouteResultWire, DeviceSignatureIntentWire, MAX_CHAT_BYTES, PrincipalId,
    ProjectionEnvelope, RoomId, SemanticHash, SignatureAlgorithm, SignatureBytes,
    UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
    UnsignedCaptureResponseWire, UnsignedDeviceActionWire, UnsignedDeviceObservationRequestWire,
    UnsignedDeviceRouteRequestWire, canonical_capture_provider_advertisement_bytes,
    canonical_capture_request_bytes, canonical_capture_response_bytes,
    canonical_device_action_bytes, canonical_device_observation_request_bytes,
    canonical_device_route_request_bytes,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "http")]
mod http;
mod loopback;
mod policy;
#[cfg(feature = "protected-store")]
mod protected_store;

#[cfg(feature = "http")]
pub use http::*;
pub use loopback::*;
pub use policy::*;
#[cfg(feature = "protected-store")]
pub use protected_store::*;

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

/// Sign a provider advertisement through the exact profile key handle.
pub fn sign_capture_provider_advertisement(
    profile: &DeviceProfile,
    unsigned: UnsignedCaptureProviderAdvertisementWire,
    signer: &impl DeviceSigner,
) -> Result<CaptureProviderAdvertisementWire, DeviceClientError> {
    profile.validate()?;
    if unsigned.player_id != profile.player_id
        || unsigned.provider_device_id != profile.device_id
        || unsigned.signature_intent.key_id != profile.device_id
    {
        return Err(DeviceClientError::InvalidProfile);
    }
    let bytes = canonical_capture_provider_advertisement_bytes(&unsigned)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    unsigned
        .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
        .map_err(|_| DeviceClientError::SigningFailed)
}

/// Sign one exact-target capture request through the requester's protected
/// device key.
pub fn sign_capture_request(
    profile: &DeviceProfile,
    unsigned: UnsignedCaptureRequestWire,
    signer: &impl DeviceSigner,
) -> Result<CaptureRequestWire, DeviceClientError> {
    profile.validate()?;
    if unsigned.player_id != profile.player_id
        || unsigned.requester_device_id != profile.device_id
        || unsigned.signature_intent.key_id != profile.device_id
    {
        return Err(DeviceClientError::InvalidProfile);
    }
    let bytes = canonical_capture_request_bytes(&unsigned)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    unsigned
        .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
        .map_err(|_| DeviceClientError::SigningFailed)
}

/// Sign a hash-bound provider response without granting access to any room
/// mutation port.
pub fn sign_capture_response(
    profile: &DeviceProfile,
    unsigned: UnsignedCaptureResponseWire,
    signer: &impl DeviceSigner,
) -> Result<CaptureResponseWire, DeviceClientError> {
    profile.validate()?;
    if unsigned.player_id != profile.player_id
        || unsigned.provider_device_id != profile.device_id
        || unsigned.signature_intent.key_id != profile.device_id
    {
        return Err(DeviceClientError::InvalidProfile);
    }
    let bytes = canonical_capture_response_bytes(&unsigned)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    unsigned
        .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
        .map_err(|_| DeviceClientError::SigningFailed)
}

/// Sign one exact-recipient observation/wait request through the profile's
/// protected key handle. The caller supplies a unique request ID so transport
/// retries can remain explicit and inspectable.
pub fn sign_observation_request(
    profile: &DeviceProfile,
    room_id: &RoomId,
    session_epoch: u64,
    request_id: poche_protocol::CorrelationId,
    mode: DeviceObservationModeWire,
    signer: &impl DeviceSigner,
) -> Result<DeviceObservationRequestWire, DeviceClientError> {
    sign_observation_request_with_invite(
        profile,
        room_id,
        session_epoch,
        request_id,
        mode,
        None,
        signer,
    )
}

/// Sign an immediate discovery request that binds a caller-supplied invite.
/// The ordinary helper above never carries bearer material, and validation
/// rejects invites on waits or pending-room bootstrap reads.
pub fn sign_observation_request_with_invite(
    profile: &DeviceProfile,
    room_id: &RoomId,
    session_epoch: u64,
    request_id: poche_protocol::CorrelationId,
    mode: DeviceObservationModeWire,
    join_invite: Option<poche_protocol::InviteProof>,
    signer: &impl DeviceSigner,
) -> Result<DeviceObservationRequestWire, DeviceClientError> {
    profile.validate()?;
    let unsigned = UnsignedDeviceObservationRequestWire {
        schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
        certificate: profile.certificate.clone(),
        room_id: room_id.clone(),
        session_epoch,
        request_id,
        player_id: profile.player_id.clone(),
        device_id: profile.device_id.clone(),
        mode,
        join_invite,
        signature_intent: DeviceSignatureIntentWire {
            domain_version: DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: profile.device_id.clone(),
        },
    };
    let bytes = canonical_device_observation_request_bytes(&unsigned)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    unsigned
        .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
        .map_err(|_| DeviceClientError::SigningFailed)
}

/// Sign one transport-route disconnect or rebind request through the exact
/// device key. Its independent signature domain prevents an observation or
/// ordinary action from being replayed as a lifecycle transition.
pub fn sign_route_request(
    profile: &DeviceProfile,
    room_id: &RoomId,
    session_epoch: u64,
    request_id: poche_protocol::CorrelationId,
    operation: DeviceRouteOperationWire,
    signer: &impl DeviceSigner,
) -> Result<DeviceRouteRequestWire, DeviceClientError> {
    profile.validate()?;
    let unsigned = UnsignedDeviceRouteRequestWire {
        schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
        certificate: profile.certificate.clone(),
        room_id: room_id.clone(),
        session_epoch,
        request_id,
        player_id: profile.player_id.clone(),
        device_id: profile.device_id.clone(),
        operation,
        signature_intent: DeviceSignatureIntentWire {
            domain_version: DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: profile.device_id.clone(),
        },
    };
    let bytes = canonical_device_route_request_bytes(&unsigned)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    unsigned
        .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
        .map_err(|_| DeviceClientError::SigningFailed)
}

/// One action advertised for one exact projection revision.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvertisedAction {
    pub id: String,
    pub label: String,
    pub payload: CommandPayload,
}

/// One bounded value-bearing action family advertised beside an exact
/// projection. Unlike a concrete [`AdvertisedAction`], a template explicitly
/// identifies the input that a human or CLI may supply after observing it.
/// The receiving adapter resolves the supplied value against the same current
/// template before it can reach the reducer.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvertisedActionTemplate {
    pub id: String,
    pub label: String,
    pub parameter: AdvertisedActionParameter,
}

impl AdvertisedActionTemplate {
    fn validate(&self) -> bool {
        valid_action_identity(&self.id, &self.label)
    }

    #[must_use]
    pub fn accepts(&self, payload: &CommandPayload) -> bool {
        match (&self.parameter, payload) {
            (AdvertisedActionParameter::ChatText, CommandPayload::Chat { text }) => {
                !text.is_empty() && text.len() <= MAX_CHAT_BYTES
            }
            _ => false,
        }
    }
}

/// Input shape accepted by one parameterized action template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum AdvertisedActionParameter {
    ChatText,
}

/// Public chat evidence retained beside an exact device observation. It is
/// bounded by the authority's chat-tail policy and carries no draft text,
/// transport identifiers, or private game projection.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceChatEntry {
    pub revision: u64,
    pub principal_id: PrincipalId,
    pub text: String,
}

impl AdvertisedAction {
    fn validate(&self) -> bool {
        valid_action_identity(&self.id, &self.label)
    }
}

fn valid_action_identity(id: &str, label: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !label.is_empty()
        && label.len() <= 256
        && !label.chars().any(char::is_control)
}

/// Exact-recipient observation and action set delivered atomically by an
/// adapter. Actions expire with this revision and projection hash.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceObservation {
    pub projection: ProjectionEnvelope,
    pub projection_hash: SemanticHash,
    pub actions: Vec<AdvertisedAction>,
    pub action_templates: Vec<AdvertisedActionTemplate>,
    pub chat_tail: Vec<DeviceChatEntry>,
    /// Signed providers owned by this exact player root in this room/session.
    /// Other players' devices are absent from the object graph.
    pub capture_providers: Vec<CaptureProviderAdvertisementWire>,
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
            || self
                .action_templates
                .iter()
                .any(|template| !template.validate())
            || !self
                .action_templates
                .windows(2)
                .all(|pair| pair[0].id < pair[1].id)
            || self.actions.iter().any(|action| {
                self.action_templates
                    .binary_search_by(|template| template.id.as_str().cmp(&action.id))
                    .is_ok()
            })
            || self.chat_tail.iter().any(|entry| {
                entry.revision > self.projection.current_revision
                    || entry.text.is_empty()
                    || entry.text.len() > MAX_CHAT_BYTES
                    || !entry.principal_id.validate()
            })
            || !self
                .chat_tail
                .windows(2)
                .all(|pair| pair[0].revision <= pair[1].revision)
            || self.capture_providers.iter().any(|provider| {
                provider.validate().is_err()
                    || provider.room_id != self.projection.room_id
                    || provider.membership_epoch != self.projection.session_epoch
                    || provider.player_id != profile.player_id
            })
            || !self
                .capture_providers
                .windows(2)
                .all(|pair| pair[0].provider_device_id < pair[1].provider_device_id)
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

    /// Resolve one convenience payload against the exact advertised action
    /// set. Zero matches mean unavailable; duplicate matches are a protocol
    /// violation rather than an arbitrary client-side choice.
    pub fn action_for_payload(
        &self,
        payload: &CommandPayload,
    ) -> Result<&AdvertisedAction, DeviceClientError> {
        let mut matches = self
            .actions
            .iter()
            .filter(|action| &action.payload == payload);
        let action = matches.next().ok_or(DeviceClientError::UnknownAction)?;
        if matches.next().is_some() {
            Err(DeviceClientError::ProtocolViolation)
        } else {
            Ok(action)
        }
    }

    /// Resolve a value-bearing payload against an action template advertised
    /// for this exact revision. The template remains opaque to the caller;
    /// only its bounded parameter type determines whether the payload fits.
    pub fn template_for_payload(
        &self,
        payload: &CommandPayload,
    ) -> Result<&AdvertisedActionTemplate, DeviceClientError> {
        let mut matches = self
            .action_templates
            .iter()
            .filter(|template| template.accepts(payload));
        let template = matches.next().ok_or(DeviceClientError::UnknownAction)?;
        if matches.next().is_some() {
            Err(DeviceClientError::ProtocolViolation)
        } else {
            Ok(template)
        }
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

impl DeviceActionRequest {
    /// Bind this prepared action to the profile certificate and sign every
    /// semantic field through the profile's protected device-key handle.
    pub fn sign(
        &self,
        profile: &DeviceProfile,
        signer: &impl DeviceSigner,
    ) -> Result<DeviceActionWire, DeviceClientError> {
        profile.validate()?;
        if self.player_id != profile.player_id || self.device_id != profile.device_id {
            return Err(DeviceClientError::InvalidProfile);
        }
        let unsigned = UnsignedDeviceActionWire {
            schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
            certificate: profile.certificate.clone(),
            room_id: self.room_id.clone(),
            session_epoch: self.session_epoch,
            command_id: self.command_id.clone(),
            player_id: self.player_id.clone(),
            device_id: self.device_id.clone(),
            expected_revision: self.expected_revision,
            expected_projection_hash: self.expected_projection_hash,
            action_id: self.action_id.clone(),
            payload: self.payload.clone(),
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: self.device_id.clone(),
            },
        };
        let bytes = canonical_device_action_bytes(&unsigned)
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        unsigned
            .attach_signature(signer.sign_device_bytes(profile, &bytes)?)
            .map_err(|_| DeviceClientError::SigningFailed)
    }
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

/// HTTP cooperation carrier. The nested capture request remains independently
/// device-signed; this wrapper supplies the root-certified requester profile
/// and exact target needed to locate the ordinary device route.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpDeviceCooperationCall {
    pub certificate: DeviceCertificateWire,
    pub target_device: DeviceId,
    pub request: DeviceCooperationRequest,
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

    /// Disconnect or rebind this exact device's transport route. Rebinding
    /// does not mutate durable membership; a disconnected member must still
    /// invoke the advertised `Reconnect` action through the ordinary port.
    fn route(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError>;

    /// Deliver exact-target cooperation without treating it as a game command.
    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError>;
}

impl<T: DeviceTransport + ?Sized> DeviceTransport for Box<T> {
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        (**self).observe(profile, room_id)
    }

    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        (**self).invoke(profile, request)
    }

    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        (**self).wait(profile, room_id, after_revision)
    }

    fn route(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        (**self).route(profile, room_id, operation)
    }

    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        (**self).cooperate(profile, target_device, request)
    }
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
        let request = self.prepare(observation, action_id, command_id)?;
        self.transport.invoke(&self.profile, request)
    }

    /// Prepare the exact transport-neutral request for an opaque advertised
    /// action without sending it. This is the canonical parity seam for GUI,
    /// CLI, policy, and puppet entry points.
    pub fn prepare(
        &self,
        observation: &DeviceObservation,
        action_id: &str,
        command_id: CommandId,
    ) -> Result<DeviceActionRequest, DeviceClientError> {
        observation.validate_for(&self.profile, &observation.projection.room_id)?;
        let action = observation
            .action(action_id)
            .ok_or(DeviceClientError::UnknownAction)?;
        Ok(DeviceActionRequest {
            room_id: observation.projection.room_id.clone(),
            session_epoch: observation.projection.session_epoch,
            command_id,
            player_id: self.profile.player_id.clone(),
            device_id: self.profile.device_id.clone(),
            expected_revision: observation.projection.current_revision,
            expected_projection_hash: observation.projection_hash,
            action_id: action.id.clone(),
            payload: action.payload.clone(),
        })
    }

    /// Resolve a typed convenience payload through the exact current action
    /// set and invoke the corresponding opaque action ID through the ordinary
    /// device transport.
    pub fn invoke_payload(
        &mut self,
        observation: &DeviceObservation,
        payload: &CommandPayload,
        command_id: CommandId,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        let request = self.prepare_payload(observation, payload, command_id)?;
        self.transport.invoke(&self.profile, request)
    }

    /// Prepare a typed convenience action through the same exact advertised
    /// action record used by [`Self::prepare`].
    pub fn prepare_payload(
        &self,
        observation: &DeviceObservation,
        payload: &CommandPayload,
        command_id: CommandId,
    ) -> Result<DeviceActionRequest, DeviceClientError> {
        if let Ok(action) = observation.action_for_payload(payload) {
            return self.prepare(observation, &action.id, command_id);
        }
        observation.validate_for(&self.profile, &observation.projection.room_id)?;
        let template = observation.template_for_payload(payload)?;
        Ok(DeviceActionRequest {
            room_id: observation.projection.room_id.clone(),
            session_epoch: observation.projection.session_epoch,
            command_id,
            player_id: self.profile.player_id.clone(),
            device_id: self.profile.device_id.clone(),
            expected_revision: observation.projection.current_revision,
            expected_projection_hash: observation.projection_hash,
            action_id: template.id.clone(),
            payload: payload.clone(),
        })
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

    pub fn disconnect_route(
        &mut self,
        room_id: &RoomId,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        self.change_route(room_id, DeviceRouteOperationWire::Disconnect)
    }

    pub fn rebind_route(
        &mut self,
        room_id: &RoomId,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        self.change_route(room_id, DeviceRouteOperationWire::Rebind)
    }

    fn change_route(
        &mut self,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        let result = self.transport.route(&self.profile, room_id, operation)?;
        result
            .validate()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        if result.room_id != *room_id
            || result.player_id != self.profile.player_id
            || result.device_id != self.profile.device_id
            || result.operation != operation
        {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(result)
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

    /// Borrow the configured adapter for transport-specific cooperation lanes
    /// such as bounded artifact streaming. Gameplay callers should continue
    /// to use the transport-neutral methods above.
    #[must_use]
    pub const fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
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

    impl LoopbackDeviceAuthority for FixtureTransport {
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

        fn route(
            &mut self,
            profile: &DeviceProfile,
            room_id: &RoomId,
            operation: DeviceRouteOperationWire,
        ) -> Result<DeviceRouteResultWire, DeviceClientError> {
            Ok(DeviceRouteResultWire {
                schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
                room_id: room_id.clone(),
                session_epoch: self.observation.projection.session_epoch,
                player_id: profile.player_id.clone(),
                device_id: profile.device_id.clone(),
                operation,
                authoritative_revision: self.observation.projection.current_revision,
                route_connected: matches!(operation, DeviceRouteOperationWire::Rebind),
                member_connected: true,
            })
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
            device_encryption_public_key: "ee".repeat(32),
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
            action_templates: Vec::new(),
            chat_tail: Vec::new(),
            capture_providers: Vec::new(),
        };
        (profile, observation)
    }

    #[test]
    fn client_invokes_only_an_action_advertised_for_exact_revision() {
        let (profile, observation) = fixture();
        let authority = FixtureTransport {
            observation,
            invoked: Vec::new(),
        };
        let transport = LoopbackDeviceTransport::new(authority);
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
        let transport = client.into_transport().into_authority();
        assert_eq!(transport.invoked.len(), 1);
        assert_eq!(transport.invoked[0].expected_revision, 7);
        assert_eq!(
            transport.invoked[0].expected_projection_hash,
            SemanticHash([7; 32])
        );
        assert_eq!(transport.invoked[0].device_id.as_str(), "22".repeat(32));
    }

    #[test]
    fn parameterized_chat_is_prepared_only_from_an_exact_advertised_template() {
        let (profile, mut observation) = fixture();
        observation.action_templates = vec![AdvertisedActionTemplate {
            id: "chat-send".to_owned(),
            label: "Send chat".to_owned(),
            parameter: AdvertisedActionParameter::ChatText,
        }];
        let client = PlayerDeviceClient::new(
            profile,
            LoopbackDeviceTransport::new(FixtureTransport {
                observation: observation.clone(),
                invoked: Vec::new(),
            }),
        )
        .unwrap();
        let prepared = client
            .prepare_payload(
                &observation,
                &CommandPayload::Chat {
                    text: "typed at the CLI".to_owned(),
                },
                CommandId::new("chat-command").unwrap(),
            )
            .unwrap();
        assert_eq!(prepared.action_id, "chat-send");
        assert_eq!(
            prepared.payload,
            CommandPayload::Chat {
                text: "typed at the CLI".to_owned()
            }
        );
        assert_eq!(
            client.prepare_payload(
                &observation,
                &CommandPayload::Chat {
                    text: String::new(),
                },
                CommandId::new("empty-chat-command").unwrap(),
            ),
            Err(DeviceClientError::UnknownAction)
        );
    }

    #[test]
    fn entry_point_parity_produces_the_identical_canonical_request() {
        let (profile, observation) = fixture();
        let authority = FixtureTransport {
            observation: observation.clone(),
            invoked: Vec::new(),
        };
        let client =
            PlayerDeviceClient::new(profile, LoopbackDeviceTransport::new(authority)).unwrap();
        let by_id = client
            .prepare(
                &observation,
                "ready",
                CommandId::new("parity-command").unwrap(),
            )
            .unwrap();
        let by_payload = client
            .prepare_payload(
                &observation,
                &CommandPayload::Ready,
                CommandId::new("parity-command").unwrap(),
            )
            .unwrap();
        let selected = AdvertisedActionPolicy::FirstLegal
            .select(&observation, PolicyScope::AllAdvertised)
            .unwrap();
        let by_policy = client
            .prepare(
                &observation,
                &selected.id,
                CommandId::new("parity-command").unwrap(),
            )
            .unwrap();
        assert_eq!(by_id, by_payload);
        assert_eq!(by_id, by_policy);
        assert_eq!(
            serde_json::to_vec(&by_id).unwrap(),
            serde_json::to_vec(&by_payload).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&by_id).unwrap(),
            serde_json::to_vec(&by_policy).unwrap()
        );
    }

    #[test]
    fn convenience_resolution_fails_closed_for_duplicate_payloads() {
        let (profile, mut observation) = fixture();
        observation.actions.push(AdvertisedAction {
            id: "ready-again".to_owned(),
            label: "Ready through a conflicting control".to_owned(),
            payload: CommandPayload::Ready,
        });
        observation
            .actions
            .sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(
            observation.action_for_payload(&CommandPayload::Ready),
            Err(DeviceClientError::ProtocolViolation)
        );
        assert!(
            observation
                .validate_for(&profile, &observation.projection.room_id)
                .is_ok()
        );
    }

    #[test]
    fn wait_refuses_an_adapter_that_returns_no_progress() {
        let (profile, observation) = fixture();
        let authority = FixtureTransport {
            observation,
            invoked: Vec::new(),
        };
        let transport = LoopbackDeviceTransport::new(authority);
        let mut client = PlayerDeviceClient::new(profile, transport).unwrap();
        assert_eq!(
            client.wait(&RoomId::new("room-1").unwrap(), 7),
            Err(DeviceClientError::NoProgress)
        );
    }
}
