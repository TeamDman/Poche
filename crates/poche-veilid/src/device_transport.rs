//! Device-level RPC over an already resolved Veilid private route.
//! Call from the native client's dedicated worker, never Bevy's frame thread.
//! Server implementations must authenticate the nested signed requests before
//! dispatch, and return only the requesting device's authorized observation.

use crate::{ResolvedRoom, VeilidRendezvous};
use poche_player_client::{
    DeviceActionRequest, DeviceActionResult, DeviceClientError, DeviceCooperationRequest,
    DeviceCooperationResult, DeviceObservation, DeviceProfile, DeviceSigner, DeviceTransport,
    HttpDeviceCooperationCall, sign_observation_request_with_invite, sign_route_request,
};
use poche_protocol::{
    CorrelationId, DeviceActionWire, DeviceId, DeviceObservationModeWire,
    DeviceObservationRequestWire, DeviceRouteOperationWire, DeviceRouteRequestWire,
    DeviceRouteResultWire, InviteProof, RoomId,
};
use serde::{Deserialize, Serialize};

const MAX_DEVICE_CALL: usize = 30_000;

/// Versioned envelope. Signature verification belongs to the receiving
/// authority; successful decoding is not authorization.
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum VeilidDeviceRequest {
    Observe(DeviceObservationRequestWire),
    Invoke(DeviceActionWire),
    Route(DeviceRouteRequestWire),
    Cooperate(HttpDeviceCooperationCall),
    PhysicalPose(poche_player_client::SignedPhysicalPose),
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum VeilidDeviceReply {
    Observation(DeviceObservation),
    Action(DeviceActionResult),
    Route(DeviceRouteResultWire),
    Cooperation(DeviceCooperationResult),
    PhysicalPose(poche_player_client::PhysicalPoseState),
    /// Stable public failure category; never include backend diagnostic text.
    Denied,
    NoProgress,
    StaleRevision,
    Unavailable,
    /// A restarted authority requires a fresh signed observation from a peer.
    RecoveryChallenge(CorrelationId),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    version: u8,
    body: T,
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, DeviceClientError> {
    let bytes = serde_json::to_vec(&Envelope {
        version: 1,
        body: value,
    })
    .map_err(|_| DeviceClientError::ProtocolViolation)?;
    if bytes.len() > MAX_DEVICE_CALL {
        return Err(DeviceClientError::ProtocolViolation);
    }
    Ok(bytes)
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DeviceClientError> {
    if bytes.len() > MAX_DEVICE_CALL {
        return Err(DeviceClientError::ProtocolViolation);
    }
    let envelope: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|_| DeviceClientError::ProtocolViolation)?;
    if envelope.version != 1 {
        return Err(DeviceClientError::ProtocolViolation);
    }
    Ok(envelope.body)
}

impl VeilidDeviceRequest {
    pub fn encode(&self) -> Result<Vec<u8>, DeviceClientError> {
        encode(self)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, DeviceClientError> {
        decode(bytes)
    }
}
impl VeilidDeviceReply {
    pub fn decode(bytes: &[u8]) -> Result<Self, DeviceClientError> {
        decode(bytes)
    }
    pub fn encode(&self) -> Result<Vec<u8>, DeviceClientError> {
        encode(self)
    }
}

/// Owns a resolved room and signer, not a second game implementation. The
/// supplied Tokio runtime must stay alive for the lifetime of this adapter.
pub struct VeilidDeviceTransport<S> {
    adapter: VeilidRendezvous,
    room: ResolvedRoom,
    runtime: tokio::runtime::Handle,
    signer: S,
    namespace: String,
    sequence: u64,
    epoch: u64,
    invite: Option<InviteProof>,
    // Keep runtime/API lifetime with the client after the menu disappears.
    _node: Option<crate::VeilidDeviceNode>,
}

impl<S> VeilidDeviceTransport<S> {
    /// Supply a bearer proof only for admission of a nonmember. Existing
    /// membership authenticates through the device certificate instead.
    pub fn set_join_invite(&mut self, invite: Option<InviteProof>) {
        self.invite = invite;
    }

    pub fn new(
        adapter: VeilidRendezvous,
        room: ResolvedRoom,
        runtime: tokio::runtime::Handle,
        signer: S,
        epoch: u64,
        invite: Option<InviteProof>,
    ) -> Result<Self, DeviceClientError> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| DeviceClientError::KeyUnavailable)?;
        Ok(Self {
            adapter,
            room,
            runtime,
            signer,
            epoch,
            invite,
            sequence: 0,
            namespace: data_encoding::HEXLOWER.encode(&random),
            _node: None,
        })
    }

    /// Production lifetime-owning constructor. Resolve the room using this
    /// node's API before moving it into the transport.
    pub fn from_node(
        node: crate::VeilidDeviceNode,
        room: ResolvedRoom,
        signer: S,
        epoch: u64,
        invite: Option<InviteProof>,
    ) -> Result<Self, DeviceClientError> {
        let adapter = VeilidRendezvous::new(node.api().clone())
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let mut transport =
            Self::new(adapter, room, node.runtime().clone(), signer, epoch, invite)?;
        transport._node = Some(node);
        Ok(transport)
    }

    fn request_id(&mut self) -> Result<CorrelationId, DeviceClientError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(DeviceClientError::ProtocolViolation)?;
        CorrelationId::new(format!("vd-{}-{}", self.namespace, self.sequence))
            .map_err(|_| DeviceClientError::ProtocolViolation)
    }

    fn require_room(&self, room_id: &RoomId) -> Result<(), DeviceClientError> {
        if self.room.record().room_id != *room_id {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        Ok(())
    }

    fn exchange(
        &self,
        request: VeilidDeviceRequest,
    ) -> Result<VeilidDeviceReply, DeviceClientError> {
        // Handle::block_on would panic inside a runtime. The UI bridge already
        // supplies a dedicated ordinary thread; fail explicitly on misuse.
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(DeviceClientError::TransportUnavailable);
        }
        let bytes = encode(&request)?;
        let call = || {
            self.runtime
                .block_on(self.adapter.app_call(&self.room, bytes.clone()))
                .map_err(|_| DeviceClientError::TransportUnavailable)
        };
        let reply = if matches!(request, VeilidDeviceRequest::Observe(_)) {
            retry_observation(call)?
        } else {
            call()?
        };
        match decode(&reply)? {
            VeilidDeviceReply::Denied => Err(DeviceClientError::AuthorizationDenied),
            VeilidDeviceReply::NoProgress => Err(DeviceClientError::NoProgress),
            VeilidDeviceReply::StaleRevision => Err(DeviceClientError::StaleRevision),
            VeilidDeviceReply::Unavailable => Err(DeviceClientError::TransportUnavailable),
            reply => Ok(reply),
        }
    }
}

pub(crate) fn retry_observation<T>(
    mut read: impl FnMut() -> Result<T, DeviceClientError>,
) -> Result<T, DeviceClientError> {
    for attempt in 0..3 {
        match read() {
            Err(DeviceClientError::TransportUnavailable) if attempt < 2 => {
                eprintln!("poche: observation RPC unavailable; retrying read");
                std::thread::sleep(std::time::Duration::from_millis(250 * (attempt + 1)));
            }
            result => return result,
        }
    }
    unreachable!("last read always returns")
}

impl<S: DeviceSigner> DeviceTransport for VeilidDeviceTransport<S> {
    fn physical_pose(
        &mut self,
        profile: &DeviceProfile,
        request: poche_player_client::PhysicalPoseRequest,
    ) -> Result<poche_player_client::PhysicalPoseState, DeviceClientError> {
        self.require_room(&request.room_id)?;
        let signed = request.sign(profile, &self.signer)?;
        match self.exchange(VeilidDeviceRequest::PhysicalPose(signed))? {
            VeilidDeviceReply::PhysicalPose(state) => Ok(state),
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        self.require_room(room_id)?;
        let id = self.request_id()?;
        let request = sign_observation_request_with_invite(
            profile,
            room_id,
            self.epoch,
            id,
            DeviceObservationModeWire::Snapshot,
            self.invite.clone(),
            &self.signer,
        )?;
        let reply = self.exchange(VeilidDeviceRequest::Observe(request))?;
        let reply = if let VeilidDeviceReply::RecoveryChallenge(challenge) = reply {
            let response = sign_observation_request_with_invite(profile, room_id, self.epoch, challenge,
                DeviceObservationModeWire::Snapshot, self.invite.clone(), &self.signer)?;
            self.exchange(VeilidDeviceRequest::Observe(response))?
        } else { reply };
        match reply {
            VeilidDeviceReply::Observation(value) => Ok(value),
            VeilidDeviceReply::RecoveryChallenge(_) => Err(DeviceClientError::NoProgress),
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        self.require_room(&request.room_id)?;
        let creates = request.session_epoch == 0
            && matches!(request.payload, poche_protocol::CommandPayload::CreateRoom);
        let joins = matches!(
            request.payload,
            poche_protocol::CommandPayload::RedeemInvite { .. }
        );
        let signed = request.sign(profile, &self.signer)?;
        match self.exchange(VeilidDeviceRequest::Invoke(signed))? {
            VeilidDeviceReply::Action(value) => {
                if creates && matches!(value, DeviceActionResult::Committed { .. }) {
                    self.epoch = 1;
                }
                if joins && matches!(value, DeviceActionResult::Committed { .. }) {
                    self.invite = None;
                }
                Ok(value)
            }
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        self.require_room(room_id)?;
        let id = self.request_id()?;
        let request = sign_observation_request_with_invite(
            profile,
            room_id,
            self.epoch,
            id,
            DeviceObservationModeWire::Wait { after_revision },
            None,
            &self.signer,
        )?;
        match self.exchange(VeilidDeviceRequest::Observe(request))? {
            VeilidDeviceReply::Observation(value) => Ok(value),
            VeilidDeviceReply::RecoveryChallenge(_) => self.observe(profile, room_id),
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
    fn route(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        self.require_room(room_id)?;
        let id = self.request_id()?;
        let request =
            sign_route_request(profile, room_id, self.epoch, id, operation, &self.signer)?;
        match self.exchange(VeilidDeviceRequest::Route(request))? {
            VeilidDeviceReply::Route(value) => Ok(value),
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        let call = HttpDeviceCooperationCall {
            certificate: profile.certificate.clone(),
            target_device: target_device.clone(),
            request,
        };
        match self.exchange(VeilidDeviceRequest::Cooperate(call))? {
            VeilidDeviceReply::Cooperation(value) => Ok(value),
            _ => Err(DeviceClientError::ProtocolViolation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_rejects_unknown_versions_fields_and_oversized_data() {
        assert!(
            decode::<VeilidDeviceReply>(br#"{"version":2,"body":{"result":"denied"}}"#).is_err()
        );
        assert!(
            decode::<VeilidDeviceReply>(br#"{"version":1,"extra":0,"body":{"result":"denied"}}"#)
                .is_err()
        );
        assert!(decode::<VeilidDeviceReply>(&vec![b' '; MAX_DEVICE_CALL + 1]).is_err());
        assert!(matches!(
            decode::<VeilidDeviceReply>(&VeilidDeviceReply::NoProgress.encode().unwrap()).unwrap(),
            VeilidDeviceReply::NoProgress
        ));
    }
}
