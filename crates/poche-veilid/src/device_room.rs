//! Assembly of the initial two-seat room service. Replication/failover remains
//! a separate responsibility; retaining this task alone is not decentralization.

use crate::{
    ApplicationIdentity, PublicRoomMetadata, PublishedRoom, RoomNetwork, RunningDeviceService,
    VeilidDeviceNode, VeilidDeviceService, VeilidRendezvous,
};
use poche_player_client::DeviceClientError;
use poche_protocol::{PrincipalId, RoomId};
use poche_runtime::{
    CertifiedDeviceRoom, LoopbackCodec, OracleRoomActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::{InviteRecord, SessionState};

/// Publish and serve a fresh two-seat room. Keep both returned owners alive
/// beside the room, initialize the creator, then expose the invitation.
/// The lobby-lifetime invitation has no short wall-clock timeout. Closing the
/// session and withdrawing its published route must revoke it on disband.
pub async fn publish_device_room(
    node: &VeilidDeviceNode,
    identity: &ApplicationIdentity,
    network: RoomNetwork,
    label: &str,
    now_unix_ms: u64,
    incoming: tokio::sync::mpsc::Receiver<Box<veilid_core::VeilidAppCall>>,
) -> Result<(PublishedRoom, RunningDeviceService), DeviceClientError> {
    let mut entropy = [0_u8; 24];
    getrandom::fill(&mut entropy).map_err(|_| DeviceClientError::KeyUnavailable)?;
    let room_id = RoomId::new(format!(
        "room-{}",
        data_encoding::HEXLOWER.encode(&entropy[..16])
    ))
    .map_err(|_| DeviceClientError::ProtocolViolation)?;
    let seed = u64::from_le_bytes(
        entropy[16..]
            .try_into()
            .map_err(|_| DeviceClientError::ProtocolViolation)?,
    );
    let metadata = PublicRoomMetadata::new(label, 2, true)
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    let adapter = VeilidRendezvous::new(node.api().clone())
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
    let published = adapter
        .publish_room(
            identity,
            network,
            room_id.clone(),
            metadata,
            1,
            1,
            u64::MAX,
            now_unix_ms,
        )
        .await
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
    let service = (|| {
        let proof = published
            .room_code()
            .admission_proof()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let mut state = SessionState::<OracleSessionGame<2>>::pending(
            room_id,
            PrincipalId::new("clock").map_err(|_| DeviceClientError::ProtocolViolation)?,
            PrincipalId::new("game").map_err(|_| DeviceClientError::ProtocolViolation)?,
        );
        state.invites.push(
            InviteRecord::new_reusable(proof.expose(), u64::MAX)
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
        );
        let actions = OracleRoomActionSource::new(seed, 2, proof.expose(), 30, "countdown")?;
        Ok::<_, DeviceClientError>(VeilidDeviceService::new(CertifiedDeviceRoom::new(
            RuntimeLoopbackDeviceAdapter::new(state, actions, LoopbackCodec::CanonicalNdjson),
        )))
    })();
    match service {
        Ok(service) => Ok((published, service.serve(node.clone(), incoming))),
        Err(error) => {
            let _ = published.close(node.api()).await;
            Err(error)
        }
    }
}
