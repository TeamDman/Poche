//! Assembly of the initial two-seat room service. Replication/failover remains
//! a separate responsibility; retaining this task alone is not decentralization.

use crate::{
    ApplicationIdentity, PublicRoomMetadata, PublishedRoom, RoomNetwork, RunningDeviceService,
    VeilidDeviceNode, VeilidDeviceService, VeilidRendezvous,
};
use poche_player_client::{DeviceClientError, DeviceProfile};
use poche_protocol::RoomId;
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
        let clock = authority_profile("clock")?;
        let environment = authority_profile("game")?;
        let proof = published
            .room_code()
            .admission_proof()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let mut state = SessionState::<OracleSessionGame<2>>::pending(
            room_id,
            clock.player_id.clone(),
            environment.player_id.clone(),
        );
        state.invites.push(
            InviteRecord::new_reusable(proof.expose(), u64::MAX)
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
        );
        let actions = OracleRoomActionSource::new(seed, 2, proof.expose(), 30, "countdown")?
            .without_hand_sharing();
        let adapter =
            RuntimeLoopbackDeviceAdapter::new(state, actions, LoopbackCodec::CanonicalNdjson);
        let mut physical_secret = [0_u8; 32];
        getrandom::fill(&mut physical_secret).map_err(|_| DeviceClientError::KeyUnavailable)?;
        adapter.enable_physical_identities(physical_secret)?;
        let mut room = CertifiedDeviceRoom::new(adapter);
        room.enroll_authority_service(&clock)?;
        room.enroll_authority_service(&environment)?;
        Ok::<_, DeviceClientError>(VeilidDeviceService::new(room))
    })();
    match service {
        Ok(service) => Ok((published, service.serve(node.clone(), incoming))),
        Err(error) => {
            let _ = published.close(node.api()).await;
            Err(error)
        }
    }
}

// Internal reducer services, not recoverable player identities or network
// signers. Their certificates only authorize the bounded local scheduler.
fn authority_profile(label: &str) -> Result<DeviceProfile, DeviceClientError> {
    use ed25519_dalek::{Signer as _, SigningKey};
    use poche_protocol::*;
    let key = || -> Result<SigningKey, DeviceClientError> {
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|_| DeviceClientError::KeyUnavailable)?;
        Ok(SigningKey::from_bytes(&seed))
    };
    let root = key()?;
    let device = key()?;
    let encryption = key()?;
    let hex = |bytes: &[u8]| data_encoding::HEXLOWER.encode(bytes);
    let invalid = |_| DeviceClientError::InvalidProfile;
    let player_id = PrincipalId::new(hex(&root.verifying_key().to_bytes())).map_err(invalid)?;
    let device_public = hex(&device.verifying_key().to_bytes());
    let device_id = DeviceId::new(device_public.clone()).map_err(invalid)?;
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new(format!("service-{label}")).map_err(invalid)?,
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_public,
        device_encryption_public_key: hex(
            &curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped(encryption.to_bytes())
                .to_bytes(),
        ),
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities: vec![DeviceCapabilityWire::Propose],
        custody: DeviceCustodyWire::NativeLocal,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: player_id.clone(),
        },
    };
    let bytes = canonical_device_certificate_bytes(&unsigned)
        .map_err(|_| DeviceClientError::InvalidProfile)?;
    let signature = SignatureBytes::new(hex(&root.sign(&bytes).to_bytes()))
        .map_err(|_| DeviceClientError::InvalidProfile)?;
    Ok(DeviceProfile {
        schema_version: DeviceProfile::SCHEMA_VERSION_V1,
        label: label.to_owned(),
        player_id,
        device_id,
        certificate: unsigned
            .attach_signature(signature)
            .map_err(|_| DeviceClientError::InvalidProfile)?,
        signing_key_handle: format!("ephemeral-authority-service:{label}"),
    })
}
