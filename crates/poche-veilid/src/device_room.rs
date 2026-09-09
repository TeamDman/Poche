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

/// Private genesis for the fixed two-seat desktop room. Never publish this:
/// the deal seed and admission proof are authority recovery material.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRoomGenesis {
    schema_version: u16,
    room_id: RoomId,
    seed: u64,
    admission_proof: String,
    invitation: String,
    clock: DeviceProfile,
    environment: DeviceProfile,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DesktopRoomRecovery {
    genesis: DesktopRoomGenesis,
    room: poche_runtime::CertifiedRoomRecovery,
}

/// Terminal private storage marker; deliberately incompatible with a running
/// room envelope so ordinary recovery cannot resurrect it.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRoomDisbanded {
    schema_version: u16,
    room_id: RoomId,
    disbanded: bool,
}

impl DesktopRoomDisbanded {
    pub fn encode(room_id: &RoomId) -> Result<Vec<u8>, DeviceClientError> {
        serde_json::to_vec(&Self { schema_version: 1, room_id: room_id.clone(), disbanded: true }).map_err(|_| DeviceClientError::ProtocolViolation)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, DeviceClientError> {
        let value: Self = serde_json::from_slice(bytes).map_err(|_| DeviceClientError::ProtocolViolation)?;
        if value.schema_version != 1 || !value.disbanded { return Err(DeviceClientError::ProtocolViolation); }
        Ok(value)
    }
}

impl DesktopRoomGenesis {
    /// Assemble a gated recovered service without publishing its route. The
    /// caller must authenticate/decrypt storage and own the original route
    /// capability; expiry/disband persistence remains the lifecycle owner's job.
    pub fn recovered_service(bytes: &[u8], creator: &poche_protocol::PrincipalId, grace: std::time::Duration,
        writer: impl Fn(&DesktopRoomGenesis, &poche_runtime::CertifiedRoomRecovery) -> Result<(), DeviceClientError> + Send + Sync + 'static,
        expiry_writer: impl Fn() -> Result<(), DeviceClientError> + Send + Sync + 'static,
    ) -> Result<(Self, VeilidDeviceService<OracleSessionGame<2>, OracleRoomActionSource>), DeviceClientError> {
        let (genesis, room) = Self::decode_recovery(bytes)?;
        let peers = room.recovery_peers(creator)?;
        if peers.is_empty() { return Err(DeviceClientError::AuthorizationDenied); }
        let presence = crate::RecoveryPresence::new(creator, peers, std::time::Duration::ZERO, grace)?;
        let saved_genesis = genesis.clone();
        let service = VeilidDeviceService::new(room)
            .with_recovery_sink(move |recovery| writer(&saved_genesis, recovery))?
            .with_recovery_expiry_sink(expiry_writer)
            .awaiting_survivor(presence)?;
        Ok((genesis, service))
    }
    pub fn room_id(&self) -> &RoomId { &self.room_id }
    /// Private invitation recovered for the same room, not a new invitation.
    pub fn invitation(&self, now_unix_ms: u64) -> Result<crate::RoomCode, DeviceClientError> {
        let code = crate::RoomCode::decode(&self.invitation, now_unix_ms).map_err(|_| DeviceClientError::ProtocolViolation)?;
        if code.admission_proof().map_err(|_| DeviceClientError::ProtocolViolation)?.expose() != self.admission_proof {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(code)
    }
    /// Encode authority-only material for immediate encrypted persistence.
    /// Caller must not log the returned bytes or expose them to player devices.
    pub fn encode_recovery(&self, room: &poche_runtime::CertifiedRoomRecovery) -> Result<Vec<u8>, DeviceClientError> {
        if room.is_closed() { return DesktopRoomDisbanded::encode(&self.room_id); }
        #[derive(serde::Serialize)]
        struct Envelope<'a> { genesis: &'a DesktopRoomGenesis, room: &'a poche_runtime::CertifiedRoomRecovery }
        serde_json::to_vec(&Envelope { genesis: self, room }).map_err(|_| DeviceClientError::ProtocolViolation)
    }

    /// Decode already authenticated private storage. No route is published.
    pub fn decode_recovery(bytes: &[u8]) -> Result<(Self, CertifiedDeviceRoom<OracleSessionGame<2>, OracleRoomActionSource>), DeviceClientError> {
        if bytes.len() > 32 * 1024 * 1024 { return Err(DeviceClientError::ProtocolViolation); }
        let envelope: DesktopRoomRecovery = serde_json::from_slice(bytes).map_err(|_| DeviceClientError::ProtocolViolation)?;
        let room = envelope.genesis.restore_room(envelope.room)?;
        Ok((envelope.genesis, room))
    }
    fn configuration(&self) -> Result<(SessionState<OracleSessionGame<2>>, OracleRoomActionSource), DeviceClientError> {
        if self.schema_version != 1 { return Err(DeviceClientError::ProtocolViolation); }
        self.invitation(0)?;
        self.clock.validate()?;
        self.environment.validate()?;
        let mut state = SessionState::pending(self.room_id.clone(), self.clock.player_id.clone(), self.environment.player_id.clone());
        state.invites.push(InviteRecord::new_reusable(&self.admission_proof, u64::MAX).map_err(|_| DeviceClientError::ProtocolViolation)?);
        let actions = OracleRoomActionSource::new(self.seed, 2, &self.admission_proof, 30, "countdown")?.without_hand_sharing();
        Ok((state, actions))
    }

    fn fresh_room(&self) -> Result<CertifiedDeviceRoom<OracleSessionGame<2>, OracleRoomActionSource>, DeviceClientError> {
        let (state, actions) = self.configuration()?;
        let adapter = RuntimeLoopbackDeviceAdapter::new(state, actions, LoopbackCodec::CanonicalNdjson);
        let mut physical_secret = [0_u8; 32];
        getrandom::fill(&mut physical_secret).map_err(|_| DeviceClientError::KeyUnavailable)?;
        adapter.enable_physical_identities(physical_secret)?;
        let mut room = CertifiedDeviceRoom::new(adapter);
        room.enroll_authority_service(&self.clock)?;
        room.enroll_authority_service(&self.environment)?;
        Ok(room)
    }

    /// Reconstruct only after authenticating the enclosing encrypted storage.
    /// This does not republish a route or establish surviving-peer liveness.
    pub fn restore_room(&self, recovery: poche_runtime::CertifiedRoomRecovery) -> Result<CertifiedDeviceRoom<OracleSessionGame<2>, OracleRoomActionSource>, DeviceClientError> {
        let (state, actions) = self.configuration()?;
        CertifiedDeviceRoom::from_durable_recovery(state, actions, recovery, LoopbackCodec::CanonicalNdjson)
    }
}

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
    publish_device_room_inner(node, identity, network, label, now_unix_ms, incoming, None).await
}

/// Publish only after an initial private checkpoint has been persisted. The
/// writer must encrypt the envelope; it must never publish it to the DHT.
pub async fn publish_durable_device_room(
    node: &VeilidDeviceNode,
    identity: &ApplicationIdentity,
    network: RoomNetwork,
    label: &str,
    now_unix_ms: u64,
    incoming: tokio::sync::mpsc::Receiver<Box<veilid_core::VeilidAppCall>>,
    writer: impl Fn(&DesktopRoomGenesis, &poche_runtime::CertifiedRoomRecovery) -> Result<(), DeviceClientError> + Send + Sync + 'static,
) -> Result<(PublishedRoom, RunningDeviceService), DeviceClientError> {
    publish_device_room_inner(node, identity, network, label, now_unix_ms, incoming, Some(std::sync::Arc::new(writer))).await
}

type DesktopRecoveryWriter = dyn Fn(&DesktopRoomGenesis, &poche_runtime::CertifiedRoomRecovery) -> Result<(), DeviceClientError> + Send + Sync;

async fn publish_device_room_inner(
    node: &VeilidDeviceNode,
    identity: &ApplicationIdentity,
    network: RoomNetwork,
    label: &str,
    now_unix_ms: u64,
    incoming: tokio::sync::mpsc::Receiver<Box<veilid_core::VeilidAppCall>>,
    writer: Option<std::sync::Arc<DesktopRecoveryWriter>>,
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
        .map_err(|error| {
            eprintln!("poche: rendezvous initialization failed: {error:?}");
            DeviceClientError::TransportUnavailable
        })?;
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
        .map_err(|error| {
            // This enum carries only stable categories, never route/key data.
            eprintln!("poche: lobby publication failed: {error:?}");
            DeviceClientError::TransportUnavailable
        })?;
    let service = (|| {
        let clock = authority_profile("clock")?;
        let environment = authority_profile("game")?;
        let proof = published
            .room_code()
            .admission_proof()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let invitation = published.room_code().encode().map_err(|_| DeviceClientError::ProtocolViolation)?.expose().to_owned();
        let genesis = DesktopRoomGenesis { schema_version: 1, room_id, seed, invitation,
            admission_proof: proof.expose().to_owned(), clock, environment };
        let room = genesis.fresh_room()?;
        let service = VeilidDeviceService::new(room);
        match writer {
            Some(writer) => service.with_recovery_sink(move |recovery| writer(&genesis, recovery)),
            None => Ok(service),
        }
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

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn desktop_genesis_restores_service_configuration_without_regeneration() {
        let terminal = DesktopRoomDisbanded::encode(&RoomId::new("disbanded-room").unwrap()).unwrap();
        assert!(DesktopRoomDisbanded::decode(&terminal).is_ok());
        assert!(DesktopRoomGenesis::decode_recovery(&terminal).is_err());
        let code = crate::RoomCode::issue(RoomNetwork::VeilidLocal, "test-record-key", poche_protocol::PrincipalId::new("ab".repeat(32)).unwrap(), u64::MAX, 0).unwrap();
        let genesis = DesktopRoomGenesis {
            schema_version: 1, room_id: RoomId::new("desktop-recovery").unwrap(),
            seed: 9381, admission_proof: code.admission_proof().unwrap().expose().to_owned(),
            invitation: code.encode().unwrap().expose().to_owned(),
            clock: authority_profile("clock").unwrap(), environment: authority_profile("game").unwrap(),
        };
        let room = genesis.fresh_room().unwrap();
        let before = room.durable_recovery().unwrap();
        let encoded = genesis.encode_recovery(&before).unwrap();
        assert!(DesktopRoomGenesis::recovered_service(&encoded,
            &poche_protocol::PrincipalId::new("not-the-creator").unwrap(), std::time::Duration::from_secs(60),
            |_, _| panic!("invalid recovery must not persist or start serving"),
            || panic!("invalid recovery must not expire"),
        ).is_err());
        let (restored_genesis, restored) = DesktopRoomGenesis::decode_recovery(&encoded).unwrap();
        assert!(serde_json::to_value(&genesis).unwrap() == serde_json::to_value(&restored_genesis).unwrap());
        let mut original = serde_json::to_value(&before).unwrap();
        let mut recovered = serde_json::to_value(restored.durable_recovery().unwrap()).unwrap();
        // Restoring creates fresh local routes above the saved counter floor.
        assert!(recovered["adapter"]["authority"]["next_connection"].as_u64().unwrap() >= original["adapter"]["authority"]["next_connection"].as_u64().unwrap());
        original["adapter"]["authority"]["next_connection"] = serde_json::Value::Null;
        recovered["adapter"]["authority"]["next_connection"] = serde_json::Value::Null;
        assert!(original == recovered, "private recovery state differs");
        let mut invalid: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        invalid["genesis"]["schema_version"] = 2.into();
        assert!(DesktopRoomGenesis::decode_recovery(&serde_json::to_vec(&invalid).unwrap()).is_err());
        invalid["genesis"]["schema_version"] = 1.into();
        invalid["genesis"]["room_id"] = "different-room".into();
        assert!(DesktopRoomGenesis::decode_recovery(&serde_json::to_vec(&invalid).unwrap()).is_err());
        invalid["genesis"]["room_id"] = "desktop-recovery".into();
        invalid["genesis"]["admission_proof"] = "different-proof".into();
        assert!(DesktopRoomGenesis::decode_recovery(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}
