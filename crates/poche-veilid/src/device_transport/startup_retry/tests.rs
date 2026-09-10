use super::*;
use crate::{VeilidDeviceReply, VeilidDeviceService};
use ed25519_dalek::{Signer, SigningKey};
use poche_player_client::*;
use poche_protocol::*;
use poche_runtime::{
    CertifiedDeviceRoom, LoopbackCodec, OracleRoomActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::SessionState;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct TestSigner(SigningKey);
impl DeviceSigner for TestSigner {
    fn sign_device_bytes(
        &self,
        _: &DeviceProfile,
        bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError> {
        Ok(
            SignatureBytes::new(data_encoding::HEXLOWER.encode(&self.0.sign(bytes).to_bytes()))
                .unwrap(),
        )
    }
}

fn profile(seed: u8) -> (DeviceProfile, TestSigner) {
    let root = SigningKey::from_bytes(&[seed; 32]);
    let device = SigningKey::from_bytes(&[seed + 1; 32]);
    let player_id =
        PrincipalId::new(data_encoding::HEXLOWER.encode(&root.verifying_key().to_bytes())).unwrap();
    let public = data_encoding::HEXLOWER.encode(&device.verifying_key().to_bytes());
    let device_id = DeviceId::new(public.clone()).unwrap();
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new("startup-replay-test").unwrap(),
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: public,
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
    let signature = SignatureBytes::new(
        data_encoding::HEXLOWER.encode(
            &root
                .sign(&canonical_device_certificate_bytes(&unsigned).unwrap())
                .to_bytes(),
        ),
    )
    .unwrap();
    (
        DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: "startup-test".to_owned(),
            player_id,
            device_id,
            certificate: unsigned.attach_signature(signature).unwrap(),
            signing_key_handle: "test-key".to_owned(),
        },
        TestSigner(device),
    )
}

struct Harness {
    room: RoomId,
    adapter: RuntimeLoopbackDeviceAdapter<OracleSessionGame<2>, OracleRoomActionSource>,
    service: VeilidDeviceService<OracleSessionGame<2>, OracleRoomActionSource>,
    saves: Arc<AtomicUsize>,
}
impl Harness {
    fn new() -> Self {
        let room = RoomId::new("startup-replay-room").unwrap();
        let mut state = SessionState::pending(
            room.clone(),
            PrincipalId::new("clock").unwrap(),
            PrincipalId::new("game").unwrap(),
        );
        state
            .invites
            .push(poche_session::InviteRecord::new("startup-invite", u64::MAX).unwrap());
        let source =
            OracleRoomActionSource::new(29, 2, "startup-invite", 30, "startup-countdown").unwrap();
        let adapter =
            RuntimeLoopbackDeviceAdapter::new(state, source, LoopbackCodec::CanonicalNdjson);
        let saves = Arc::new(AtomicUsize::new(0));
        let written = saves.clone();
        let service = VeilidDeviceService::new(CertifiedDeviceRoom::new(adapter.clone()))
            .with_recovery_sink(move |record| {
                // Exercise real recovery serialization before the simulated
                // network loses a reply, without writing any private fixture.
                assert!(!serde_json::to_vec(record).unwrap().is_empty());
                written.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        Self {
            room,
            adapter,
            service,
            saves,
        }
    }
    fn action(
        &self,
        profile: &DeviceProfile,
        signer: &TestSigner,
        payload: &CommandPayload,
        id: &str,
    ) -> (DeviceActionRequest, VeilidDeviceRequest) {
        let invite = match payload {
            CommandPayload::RedeemInvite { invite } => Some(invite.clone()),
            _ => None,
        };
        let request = sign_observation_request_with_invite(
            profile,
            &self.room,
            self.adapter.session_epoch().unwrap(),
            CorrelationId::new(format!("observe-{id}")).unwrap(),
            DeviceObservationModeWire::Snapshot,
            invite,
            signer,
        )
        .unwrap();
        let reply = self
            .service
            .dispatch(&VeilidDeviceRequest::Observe(request).encode().unwrap())
            .unwrap();
        let VeilidDeviceReply::Observation(observation) =
            VeilidDeviceReply::decode(&reply).unwrap()
        else {
            panic!("expected observation")
        };
        let client = PlayerDeviceClient::new(
            profile.clone(),
            LoopbackDeviceTransport::new(self.adapter.clone()),
        )
        .unwrap();
        let prepared = client
            .prepare_payload(&observation, payload, CommandId::new(id).unwrap())
            .unwrap();
        let request = VeilidDeviceRequest::Invoke(prepared.sign(profile, signer).unwrap());
        (prepared, request)
    }
    fn route(
        &self,
        profile: &DeviceProfile,
        signer: &TestSigner,
        operation: DeviceRouteOperationWire,
        id: &str,
    ) {
        let signed = sign_route_request(
            profile,
            &self.room,
            self.adapter.session_epoch().unwrap(),
            CorrelationId::new(id).unwrap(),
            operation,
            signer,
        )
        .unwrap();
        let reply = self
            .service
            .dispatch(&VeilidDeviceRequest::Route(signed).encode().unwrap())
            .unwrap();
        assert!(matches!(
            VeilidDeviceReply::decode(&reply).unwrap(),
            VeilidDeviceReply::Route(_)
        ));
    }
    fn commit_with_loss(
        &self,
        prepared: &DeviceActionRequest,
        request: &VeilidDeviceRequest,
        after_dispatch: bool,
    ) {
        let frozen = request.encode().unwrap();
        let before = self.adapter.revision().unwrap();
        let mut calls = 0;
        let mut delays = Vec::new();
        let reply = exchange(
            request,
            |bytes, refresh| {
                assert!(bytes == frozen); // Do not print secret-bearing bytes on failure.
                assert!(!refresh);
                calls += 1;
                if calls == 1 && !after_dispatch {
                    return Err(VeilidRendezvousError::Timeout);
                }
                let previous_saves = self.saves.load(Ordering::SeqCst);
                let reply = self.service.dispatch(bytes).unwrap();
                assert!(self.saves.load(Ordering::SeqCst) > previous_saves);
                if calls == 1 {
                    Err(VeilidRendezvousError::Timeout)
                } else {
                    Ok(reply)
                }
            },
            |delay| delays.push(delay),
        )
        .unwrap();
        let VeilidDeviceReply::Action(result) = VeilidDeviceReply::decode(&reply).unwrap() else {
            panic!("expected receipt")
        };
        validate_result(prepared, &result).unwrap();
        assert!(
            matches!(result, DeviceActionResult::Committed { revision, .. } if revision == before + 1)
        );
        assert_eq!(calls, 2);
        assert_eq!(delays, [Duration::from_millis(250)]);
        assert_eq!(self.adapter.revision(), Some(before + 1));
    }
}

#[test]
fn startup_replay_lost_requests_and_receipts_commit_create_join_reconnect_once() {
    for after_dispatch in [false, true] {
        let h = Harness::new();
        let (creator, creator_signer) = profile(51);
        let (guest, guest_signer) = profile(53);
        let (prepared, request) = h.action(
            &creator,
            &creator_signer,
            &CommandPayload::CreateRoom,
            "create",
        );
        h.commit_with_loss(&prepared, &request, after_dispatch);
        assert_eq!(h.adapter.session_epoch(), Some(1));
        let (prepared, request) = h.action(
            &guest,
            &guest_signer,
            &CommandPayload::RedeemInvite {
                invite: InviteProof::new("startup-invite").unwrap(),
            },
            "join",
        );
        h.commit_with_loss(&prepared, &request, after_dispatch);
        assert!(h.adapter.player_is_member(&guest.player_id).unwrap());
        assert!(
            !h.adapter
                .accepts_invite(&InviteProof::new("startup-invite").unwrap())
                .unwrap()
        );
        h.route(
            &guest,
            &guest_signer,
            DeviceRouteOperationWire::Disconnect,
            "disconnect",
        );
        h.route(
            &guest,
            &guest_signer,
            DeviceRouteOperationWire::Rebind,
            "rebind",
        );
        let (prepared, request) = h.action(
            &guest,
            &guest_signer,
            &CommandPayload::Reconnect,
            "reconnect",
        );
        h.commit_with_loss(&prepared, &request, after_dispatch);
    }
}

#[test]
fn startup_replay_is_bounded_and_refreshes_only_on_route_failures() {
    let h = Harness::new();
    let (profile, signer) = profile(61);
    let (_, request) = h.action(&profile, &signer, &CommandPayload::CreateRoom, "bounds");
    for error in [
        VeilidRendezvousError::Timeout,
        VeilidRendezvousError::TryAgain,
        VeilidRendezvousError::NoConnection,
        VeilidRendezvousError::StaleRoute,
        VeilidRendezvousError::WatchRenewal,
        VeilidRendezvousError::Shutdown,
        VeilidRendezvousError::Unavailable,
        VeilidRendezvousError::InvalidRecord,
        VeilidRendezvousError::OversizedMessage,
    ] {
        let retryable = matches!(
            error,
            VeilidRendezvousError::Timeout
                | VeilidRendezvousError::TryAgain
                | VeilidRendezvousError::NoConnection
                | VeilidRendezvousError::StaleRoute
                | VeilidRendezvousError::WatchRenewal
        );
        let mut attempts = Vec::new();
        let mut pauses = Vec::new();
        assert_eq!(
            exchange(
                &request,
                |_, refresh| {
                    attempts.push(refresh);
                    Err(error)
                },
                |delay| pauses.push(delay)
            ),
            Err(DeviceClientError::TransportUnavailable)
        );
        assert_eq!(attempts.len(), if retryable { 3 } else { 1 });
        assert!(!attempts[0]);
        if retryable {
            let refresh = matches!(
                error,
                VeilidRendezvousError::NoConnection
                    | VeilidRendezvousError::StaleRoute
                    | VeilidRendezvousError::WatchRenewal
            );
            assert_eq!(&attempts[1..], &[refresh, refresh]);
            assert_eq!(
                pauses,
                [Duration::from_millis(250), Duration::from_millis(500)]
            );
        } else {
            assert!(pauses.is_empty());
        }
    }
    assert_eq!(h.adapter.revision(), Some(0));
}

#[test]
fn startup_replay_does_not_retry_wire_errors_or_unrelated_actions() {
    let h = Harness::new();
    let (profile, signer) = profile(71);
    let (_, request) = h.action(
        &profile,
        &signer,
        &CommandPayload::CreateRoom,
        "wire-errors",
    );
    for bytes in [
        VeilidDeviceReply::Denied.encode().unwrap(),
        VeilidDeviceReply::Unavailable.encode().unwrap(),
        VeilidDeviceReply::StaleRevision.encode().unwrap(),
        VeilidDeviceReply::NoProgress.encode().unwrap(),
        b"invalid".to_vec(),
    ] {
        let mut calls = 0;
        let actual = exchange(
            &request,
            |_, _| {
                calls += 1;
                Ok(bytes.clone())
            },
            |_| panic!("must not pause"),
        )
        .unwrap();
        assert!(actual == bytes);
        assert_eq!(calls, 1);
    }
    let VeilidDeviceRequest::Invoke(action) = request else {
        unreachable!()
    };
    for payload in [
        CommandPayload::Chat {
            text: "hello".to_owned(),
        },
        CommandPayload::CloseRoom,
        CommandPayload::TakeSeat { seat: 0 },
        CommandPayload::Ready,
    ] {
        // Transport policy inspects the kind, not credentials. The service
        // remains responsible for rejecting this intentionally stale signature.
        let mut action = action.clone();
        action.payload = payload;
        let mut calls = 0;
        assert_eq!(
            exchange(
                &VeilidDeviceRequest::Invoke(action),
                |_, _| {
                    calls += 1;
                    Err(VeilidRendezvousError::Timeout)
                },
                |_| panic!("must not pause")
            ),
            Err(DeviceClientError::TransportUnavailable)
        );
        assert_eq!(calls, 1);
    }
}

#[test]
fn startup_receipts_must_match_command_and_advance_revision() {
    let h = Harness::new();
    let (profile, signer) = profile(81);
    let (request, _) = h.action(&profile, &signer, &CommandPayload::CreateRoom, "receipt");
    for result in [
        DeviceActionResult::Committed {
            command_id: CommandId::new("wrong").unwrap(),
            revision: 1,
        },
        DeviceActionResult::Committed {
            command_id: request.command_id.clone(),
            revision: 0,
        },
        DeviceActionResult::Denied {
            command_id: CommandId::new("wrong").unwrap(),
            code: "D-CLOSED".to_owned(),
        },
    ] {
        assert_eq!(
            validate_result(&request, &result),
            Err(DeviceClientError::ProtocolViolation)
        );
    }
    assert_eq!(
        validate_result(
            &request,
            &DeviceActionResult::Denied {
                command_id: request.command_id.clone(),
                code: "D-CLOSED".to_owned()
            }
        ),
        Ok(())
    );
}

#[test]
fn startup_replay_conflicts_and_evicted_receipts_cannot_reapply_create() {
    let h = Harness::new();
    let (profile, signer) = profile(91);
    let (prepared, request) = h.action(&profile, &signer, &CommandPayload::CreateRoom, "a-create");
    h.commit_with_loss(&prepared, &request, true);
    // A different request with a fresh VALID signature and the original ID
    // must still fail the exact-cache comparison, without changing state.
    let mut conflicting = prepared.clone();
    conflicting.action_id = "different-create".to_owned();
    let reply = h
        .service
        .dispatch(
            &VeilidDeviceRequest::Invoke(conflicting.sign(&profile, &signer).unwrap())
                .encode()
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Denied
    ));
    assert_eq!(h.adapter.revision(), Some(1));
    // The cache is bounded to 256 and evicts the first ordered command key.
    // Populate it through real, distinct committed actions, not private test
    // access to cache internals. The old expected epoch/revision stays frozen.
    for index in 0..256 {
        let (action, request) = h.action(
            &profile,
            &signer,
            &if index % 2 == 0 {
                CommandPayload::TakeSeat { seat: 0 }
            } else {
                CommandPayload::ReleaseSeat
            },
            &format!("z-seat-{index:03}"),
        );
        let reply = h.service.dispatch(&request.encode().unwrap()).unwrap();
        let VeilidDeviceReply::Action(result) = VeilidDeviceReply::decode(&reply).unwrap() else {
            panic!("seat receipt")
        };
        validate_result(&action, &result).unwrap();
        assert!(matches!(result, DeviceActionResult::Committed { .. }));
    }
    assert_eq!(h.adapter.revision(), Some(257));
    let mut calls = 0;
    let reply = exchange(
        &request,
        |bytes, _| {
            calls += 1;
            Ok(h.service.dispatch(bytes).unwrap())
        },
        |_| panic!("an explicit server response must stop"),
    )
    .unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Unavailable
    ));
    assert_eq!(calls, 1);
    assert_eq!(h.adapter.revision(), Some(257));
}
