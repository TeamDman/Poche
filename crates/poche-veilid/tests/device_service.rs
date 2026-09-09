#![cfg(feature = "device-service")]

use ed25519_dalek::{Signer, SigningKey};
use poche_player_client::{
    DeviceClientError, DeviceProfile, DeviceSigner, sign_observation_request,
};
use poche_protocol::*;
use poche_runtime::{
    CertifiedDeviceRoom, LoopbackCodec, OracleRoomActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::SessionState;
use poche_veilid::{VeilidDeviceReply, VeilidDeviceRequest, VeilidDeviceService};

struct TestSigner(SigningKey);
impl DeviceSigner for TestSigner {
    fn sign_device_bytes(
        &self,
        _profile: &DeviceProfile,
        bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError> {
        Ok(signature(&self.0, bytes))
    }
}
fn signature(key: &SigningKey, bytes: &[u8]) -> SignatureBytes {
    SignatureBytes::new(data_encoding::HEXLOWER.encode(&key.sign(bytes).to_bytes())).unwrap()
}

fn test_profile(seed: u8) -> DeviceProfile {
    let root = SigningKey::from_bytes(&[seed; 32]);
    let device = SigningKey::from_bytes(&[seed + 1; 32]);
    let player_id =
        PrincipalId::new(data_encoding::HEXLOWER.encode(&root.verifying_key().to_bytes())).unwrap();
    let device_public = data_encoding::HEXLOWER.encode(&device.verifying_key().to_bytes());
    let device_id = DeviceId::new(device_public.clone()).unwrap();
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new("service-test").unwrap(),
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_public,
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
            key_id: player_id.clone(),
        },
    };
    let sig = signature(
        &root,
        &canonical_device_certificate_bytes(&unsigned).unwrap(),
    );
    DeviceProfile {
        schema_version: DeviceProfile::SCHEMA_VERSION_V1,
        label: "test".to_owned(),
        player_id,
        device_id,
        certificate: unsigned.attach_signature(sig).unwrap(),
        signing_key_handle: "test-key".to_owned(),
    }
}

fn room_service(
    room_id: &RoomId,
    invite: &str,
) -> VeilidDeviceService<OracleSessionGame<2>, OracleRoomActionSource> {
    let mut state = SessionState::<OracleSessionGame<2>>::pending(
        room_id.clone(),
        PrincipalId::new("clock").unwrap(),
        PrincipalId::new("game").unwrap(),
    );
    state
        .invites
        .push(poche_session::InviteRecord::new(invite, u64::MAX).unwrap());
    let adapter = RuntimeLoopbackDeviceAdapter::new(
        state,
        OracleRoomActionSource::new(29, 2, invite, 30, "test-countdown").unwrap(),
        LoopbackCodec::CanonicalNdjson,
    );
    VeilidDeviceService::new(CertifiedDeviceRoom::new(adapter))
}

#[test]
fn device_dispatch_authenticates_before_returning_observations() {
    let profile = test_profile(31);
    let room_id = RoomId::new("device-service-test").unwrap();
    let service = room_service(&room_id, "test-invite");
    let request = sign_observation_request(
        &profile,
        &room_id,
        0,
        CorrelationId::new("request-one").unwrap(),
        DeviceObservationModeWire::Snapshot,
        &TestSigner(SigningKey::from_bytes(&[32; 32])),
    )
    .unwrap();
    let bytes = VeilidDeviceRequest::Observe(request.clone())
        .encode()
        .unwrap();
    let reply = service.dispatch(&bytes).unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Observation(_)
    ));
    // Mutating a signed field must not return a projection or enroll new data.
    let mut tampered = request;
    tampered.room_id = RoomId::new("other-room").unwrap();
    let reply = service
        .dispatch(&VeilidDeviceRequest::Observe(tampered).encode().unwrap())
        .unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Denied
    ));
    assert!(service.dispatch(b"{}").is_err());

    #[cfg(feature = "veilid-mock-test")]
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use std::{sync::Arc, time::Duration};
        use veilid_core::{Target, VeilidConfig, VeilidUpdate};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_str().unwrap();
        let mut config = VeilidConfig::new(
            "poche_device_rpc_test",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        config.namespace = "server".to_owned();
        let (send, receive) = tokio::sync::mpsc::channel(4);
        let server_node = poche_veilid::VeilidDeviceNode::start(
            config.clone(),
            Arc::new(move |update| {
                if let VeilidUpdate::AppCall(call) = update {
                    let _ = send.try_send(call);
                }
            }),
        )
        .unwrap();
        let server = server_node.api().clone();
        config.namespace = "client".to_owned();
        let client_node = poche_veilid::VeilidDeviceNode::start(config, Arc::new(drop)).unwrap();
        let client = client_node.api().clone();
        server.attach().await.unwrap();
        client.attach().await.unwrap();
        let route = server.new_private_route().await.unwrap();
        let remote = client.import_remote_private_route(route.blob).unwrap();
        use poche_player_client::{DeviceActionResult, PlayerDeviceClient};
        use poche_veilid::{
            ApplicationIdentity, IdentityStoragePolicy, RoomNetwork, VeilidDeviceTransport,
            VeilidProtectedIdentityStore, VeilidRendezvous,
        };
        let identity = ApplicationIdentity::load_or_create(
            &VeilidProtectedIdentityStore::new(server.clone()),
            IdentityStoragePolicy::RequireProtected,
        )
        .await
        .unwrap();
        let (published, handler) = poche_veilid::publish_device_room(
            &server_node,
            &identity,
            RoomNetwork::VeilidLocal,
            "RPC room",
            100,
            receive,
        )
        .await
        .unwrap();
        let room_id = published.record().room_id.clone();
        let bytes = VeilidDeviceRequest::Observe(
            sign_observation_request(
                &profile,
                &room_id,
                0,
                CorrelationId::new("published-observe").unwrap(),
                DeviceObservationModeWire::Snapshot,
                &TestSigner(SigningKey::from_bytes(&[32; 32])),
            )
            .unwrap(),
        )
        .encode()
        .unwrap();
        let joiner = VeilidRendezvous::new(client.clone()).unwrap();
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            client
                .routing_context()
                .unwrap()
                .app_call(Target::RouteId(remote), bytes),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(
            VeilidDeviceReply::decode(&response).unwrap(),
            VeilidDeviceReply::Observation(_)
        ));
        let creator_node = server_node.clone();
        let creator_invitation = published.room_code().encode().unwrap();
        // Match NativeLiveDevice's ordinary worker thread, not a nested Tokio
        // block_on inside a runtime task.
        let guest_room = room_id.clone();
        tokio::task::spawn_blocking(move || {
            std::thread::spawn(move || {
                let (mut device, created_room) = poche_veilid::create_device(
                    creator_node,
                    profile,
                    TestSigner(SigningKey::from_bytes(&[32; 32])),
                    creator_invitation.expose(),
                    101,
                )
                .unwrap();
                assert_eq!(created_room, room_id);
                let created = device.observe(&room_id).unwrap();
                assert_eq!(created.projection.current_revision, 1);
                let seat_action = created
                    .actions
                    .iter()
                    .find(|action| matches!(action.payload, CommandPayload::TakeSeat { seat: 0 }))
                    .unwrap()
                    .id
                    .clone();
                assert!(matches!(
                    device
                        .invoke(&created, &seat_action, CommandId::new("rpc-seat").unwrap())
                        .unwrap(),
                    DeviceActionResult::Committed { .. }
                ));
                let seated = device.observe(&room_id).unwrap();
                assert_eq!(seated.projection.current_revision, 2);
                let disconnected = device.disconnect_route(&room_id).unwrap();
                assert!(!disconnected.member_connected);
                device.rebind_route(&room_id).unwrap();
                let reconnect = device.observe(&room_id).unwrap();
                assert_eq!(reconnect.actions.len(), 1);
                assert!(matches!(
                    reconnect.actions[0].payload,
                    CommandPayload::Reconnect
                ));
                device
                    .invoke(
                        &reconnect,
                        &reconnect.actions[0].id,
                        CommandId::new("rpc-reconnect").unwrap(),
                    )
                    .unwrap();
                assert!(device.observe(&room_id).unwrap().actions.len() > 1);
            })
            .join()
            .unwrap()
        })
        .await
        .unwrap();
        let wrong_transport = VeilidDeviceTransport::from_node(
            client_node.clone(),
            joiner
                .resolve_room(published.room_code(), 102)
                .await
                .unwrap(),
            TestSigner(SigningKey::from_bytes(&[42; 32])),
            1,
            Some(InviteProof::new("wrong-secret").unwrap()),
        )
        .unwrap();
        let invitation = published.room_code().encode().unwrap();
        let guest_node = client_node.clone();
        let creator_node = server_node.clone();
        tokio::task::spawn_blocking(move || {
            std::thread::spawn(move || {
                let guest_profile = test_profile(41);
                let guest_id = guest_profile.player_id.clone();
                let mut rejected =
                    PlayerDeviceClient::new(test_profile(41), wrong_transport).unwrap();
                assert!(matches!(
                    rejected.observe(&guest_room),
                    Err(DeviceClientError::AuthorizationDenied)
                ));
                let (mut guest, joined_room) = poche_veilid::join_device(
                    guest_node.clone(),
                    guest_profile,
                    TestSigner(SigningKey::from_bytes(&[42; 32])),
                    invitation.expose(),
                    102,
                )
                .unwrap();
                assert_eq!(joined_room, guest_room);
                let observation = guest.observe(&guest_room).unwrap();
                assert_eq!(observation.projection.principal_id, guest_id);
                assert!(
                    !observation.actions.iter().any(|action| matches!(
                        action.payload,
                        CommandPayload::TakeSeat { seat: 0 }
                    ))
                );
                let seat = observation
                    .actions
                    .iter()
                    .find(|action| matches!(action.payload, CommandPayload::TakeSeat { seat: 1 }))
                    .unwrap();
                assert!(matches!(
                    guest
                        .invoke(
                            &observation,
                            &seat.id,
                            CommandId::new("guest-seat").unwrap()
                        )
                        .unwrap(),
                    DeviceActionResult::Committed { .. }
                ));
                assert_eq!(
                    guest.observe(&guest_room).unwrap().projection.principal_id,
                    guest_id
                );
                guest.disconnect_route(&guest_room).unwrap();
                drop(guest);
                let (mut resumed, _) = poche_veilid::join_device(
                    guest_node,
                    test_profile(41),
                    TestSigner(SigningKey::from_bytes(&[42; 32])),
                    invitation.expose(),
                    103,
                )
                .unwrap();
                let resumed_view = resumed.observe(&guest_room).unwrap();
                assert!(
                    resumed_view
                        .projection
                        .payload
                        .members
                        .iter()
                        .any(|member| member.principal_id == guest_id
                            && member.connected
                            && member.seat == Some(1))
                );
                let (mut creator, _) = poche_veilid::join_device(
                    creator_node,
                    test_profile(31),
                    TestSigner(SigningKey::from_bytes(&[32; 32])),
                    invitation.expose(),
                    104,
                )
                .unwrap();
                for (is_creator, action, command) in [
                    (false, "room-ready", "guest-ready"),
                    (true, "room-ready", "creator-ready"),
                    (true, "countdown-arm", "creator-arm"),
                ] {
                    let client = if is_creator {
                        &mut creator
                    } else {
                        &mut resumed
                    };
                    let view = client.observe(&guest_room).unwrap();
                    assert!(matches!(
                        client
                            .invoke(&view, action, CommandId::new(command).unwrap())
                            .unwrap(),
                        DeviceActionResult::Committed { .. }
                    ));
                }
                assert_eq!(
                    resumed
                        .observe(&guest_room)
                        .unwrap()
                        .projection
                        .payload
                        .phase,
                    poche_protocol::RoomPhase::Countdown
                );
                let deadline = std::time::Instant::now() + Duration::from_secs(8);
                loop {
                    let view = resumed.observe(&guest_room).unwrap();
                    if view.projection.payload.phase == poche_protocol::RoomPhase::Running {
                        // The scheduler also drives the environment's deal;
                        // wait for an actual hand, not merely GameStarted.
                        if view
                            .projection
                            .payload
                            .own_hand
                            .as_ref()
                            .is_some_and(|hand| !hand.cards.is_empty())
                        {
                            break;
                        }
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "assembled service did not expire countdown and deal"
                    );
                    std::thread::sleep(Duration::from_millis(20));
                }
                let before = resumed.observe(&guest_room).unwrap();
                let hand_card = before
                    .physical_hands
                    .iter()
                    .find(|card| card.face.is_some())
                    .unwrap();
                let mut motion = poche_player_client::PhysicalPoseRequest {
                    certificate: test_profile(41).certificate,
                    room_id: guest_room.clone(),
                    session_epoch: before.projection.session_epoch,
                    card_id: hand_card.id.clone(),
                    generation: 0,
                    claim: true,
                    sequence: 1,
                    position_mm: [100, 200, 300],
                    rotation_millidegrees: [1000, 2000, 3000],
                };
                let accepted = resumed.physical_pose(motion.clone()).unwrap();
                assert_eq!(accepted.generation, 1);
                let other_view = creator.observe(&guest_room).unwrap();
                assert_eq!(
                    other_view.projection.current_revision,
                    before.projection.current_revision
                );
                let remote = other_view
                    .physical_hands
                    .iter()
                    .find(|card| card.id == motion.card_id)
                    .unwrap();
                assert!(remote.face.is_none());
                assert_eq!(remote.pose.as_ref(), Some(&accepted));
                motion.claim = false;
                motion.generation = 1;
                motion.sequence = 2;
                motion.position_mm[0] += 20;
                let moved = resumed.physical_pose(motion.clone()).unwrap();
                let mut outside = motion.clone();
                outside.sequence = 3;
                outside.position_mm[0] = i32::MAX;
                assert_eq!(
                    resumed.physical_pose(outside),
                    Err(DeviceClientError::AuthorizationDenied)
                );
                assert_eq!(
                    resumed.physical_pose(motion.clone()),
                    Err(DeviceClientError::StaleRevision)
                );
                let mut stolen = motion.clone();
                stolen.certificate = test_profile(31).certificate;
                stolen.claim = true;
                stolen.sequence = 1;
                assert_eq!(
                    creator.physical_pose(stolen),
                    Err(DeviceClientError::AuthorizationDenied)
                );
                let final_view = creator.observe(&guest_room).unwrap();
                assert_eq!(
                    final_view.projection.current_revision,
                    before.projection.current_revision
                );
                assert_eq!(
                    final_view.projection.payload.public_game_state,
                    before.projection.payload.public_game_state
                );
                assert_eq!(
                    final_view
                        .physical_hands
                        .iter()
                        .find(|card| card.id == motion.card_id)
                        .unwrap()
                        .pose
                        .as_ref(),
                    Some(&moved)
                );
                assert!(final_view.physical_public.is_empty());
                for step in 0..2 {
                    let creator_view = creator.observe(&guest_room).unwrap();
                    let creator_turn = creator_view.actions.iter().any(|action| matches!(action.payload, CommandPayload::GameAction { action: poche_protocol::GameActionWire::Bid { .. } }));
                    let client = if creator_turn { &mut creator } else { &mut resumed };
                    let view = client.observe(&guest_room).unwrap();
                    let action = view.actions.iter().find(|action| matches!(action.payload, CommandPayload::GameAction { action: poche_protocol::GameActionWire::Bid { .. } })).unwrap();
                    assert!(matches!(client.invoke(&view, &action.id, CommandId::new(format!("pose-bid-{step}")).unwrap()).unwrap(), DeviceActionResult::Committed { .. }));
                }
                let creator_view = creator.observe(&guest_room).unwrap();
                let creator_turn = creator_view.actions.iter().any(|action| matches!(action.payload, CommandPayload::GameAction { action: poche_protocol::GameActionWire::Play { .. } }));
                let client = if creator_turn { &mut creator } else { &mut resumed };
                let view = client.observe(&guest_room).unwrap();
                let action = view.actions.iter().find(|action| matches!(action.payload, CommandPayload::GameAction { action: poche_protocol::GameActionWire::Play { .. } })).unwrap();
                let CommandPayload::GameAction { action: poche_protocol::GameActionWire::Play { card: face } } = action.payload else { unreachable!() };
                let card = view.physical_hands.iter().find(|card| card.face == Some(face)).unwrap();
                let id = card.id.clone();
                let pose = client.physical_pose(poche_player_client::PhysicalPoseRequest {
                    certificate: client.profile().certificate.clone(), room_id: guest_room.clone(), session_epoch: view.projection.session_epoch,
                    card_id: id.clone(), generation: card.pose.as_ref().map_or(0, |pose| pose.generation), claim: true, sequence: 1,
                    position_mm: [220, 180, 110], rotation_millidegrees: [12000, 23000, 34000],
                }).unwrap();
                assert!(matches!(client.invoke(&view, &action.id, CommandId::new("pose-play").unwrap()).unwrap(), DeviceActionResult::Committed { .. }));
                let public = creator.observe(&guest_room).unwrap();
                assert!(!public.physical_hands.iter().any(|card| card.id == id));
                let played = public.physical_public.iter().find(|card| card.id == id).unwrap();
                assert_eq!(played.face, face);
                assert_eq!(played.pose, pose);
            })
            .join()
            .unwrap();
        })
        .await
        .unwrap();
        drop(handler);
        client_node.shutdown().unwrap();
        server_node.shutdown().unwrap();
    });
}
