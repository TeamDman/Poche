//! Actual worker + rules authority with deterministic transport-boundary loss.
use super::*;
use poche_player_client::{
    DeviceActionRequest, DeviceCooperationRequest, DeviceCooperationResult, DeviceProfile,
    LoopbackDeviceTransport,
};
use poche_protocol::{DeviceId, DeviceRouteOperationWire, DeviceRouteResultWire};
use poche_runtime::{LoopbackCodec, OracleGameActionSource, RuntimeLoopbackDeviceAdapter};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;

struct ReadFaults<T> {
    inner: T,
    failures: VecDeque<DeviceClientError>,
    invoked: bool,
    lose_ack: bool,
    stall_reads: bool,
    calls: Arc<AtomicUsize>,
    receipt: Arc<Mutex<Option<DeviceActionResult>>>,
}

impl<T: DeviceTransport> DeviceTransport for ReadFaults<T> {
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        if self.invoked
            && let Some(error) = self.failures.pop_front()
        {
            return Err(error);
        }
        if self.invoked && self.stall_reads {
            return Err(DeviceClientError::NoProgress);
        }
        self.inner.observe(profile, room)
    }
    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let receipt = self.inner.invoke(profile, request)?;
        *self.receipt.lock().unwrap() = Some(receipt.clone());
        self.invoked = true;
        if self.lose_ack {
            Err(DeviceClientError::TransportUnavailable)
        } else {
            Ok(receipt)
        }
    }
    fn wait(
        &mut self,
        _: &DeviceProfile,
        _: &RoomId,
        _: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        panic!("the native worker must use snapshot reads");
    }
    fn route(
        &mut self,
        profile: &DeviceProfile,
        room: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        self.inner.route(profile, room, operation)
    }
    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.inner.cooperate(profile, target, request)
    }
}

fn exercise(lose_ack: bool, stop_during_confirmation: bool) {
    let (state, actor) = crate::tests::running_play_state();
    let room = state.room_id.clone();
    let profile = crate::tests::certified_profile(&actor, "receipt-renderer", "77");
    let sibling = crate::tests::certified_profile(&actor, "receipt-sibling", "88");
    let other_player = state
        .members
        .iter()
        .find(|member| member.principal_id != actor)
        .unwrap()
        .principal_id
        .clone();
    let other = crate::tests::certified_profile(&other_player, "receipt-other", "99");
    let adapter = RuntimeLoopbackDeviceAdapter::new(
        state,
        OracleGameActionSource::default(),
        LoopbackCodec::Typed,
    );
    adapter.enroll(&profile).unwrap();
    adapter.enroll(&sibling).unwrap();
    adapter.enroll(&other).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let receipt = Arc::new(Mutex::new(None));
    let transport = ReadFaults {
        inner: LoopbackDeviceTransport::new(adapter.clone()),
        failures: if stop_during_confirmation {
            VecDeque::new()
        } else {
            VecDeque::from([
                DeviceClientError::TransportUnavailable,
                DeviceClientError::NoProgress,
                DeviceClientError::StaleRevision,
            ])
        },
        invoked: false,
        lose_ack,
        stall_reads: stop_during_confirmation,
        calls: calls.clone(),
        receipt: receipt.clone(),
    };
    let mut live = NativeLiveDevice::connect(
        PlayerDeviceClient::new(profile, transport).unwrap(),
        room.clone(),
    )
    .unwrap();
    let before = live.observation().clone();
    let action = before.actions[0].id.clone();
    live.submit_action(&action).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_error = false;
    loop {
        if let Err(error) = live.poll() {
            assert!(
                [
                    DeviceClientError::TransportUnavailable,
                    DeviceClientError::NoProgress,
                    DeviceClientError::StaleRevision,
                ]
                .iter()
                .any(|expected| expected.to_string() == error),
                "unexpected worker error: {error}"
            );
            saw_error = true;
        }
        if saw_error && stop_during_confirmation {
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            let (finished, done) = mpsc::channel();
            thread::spawn(move || {
                let _ = finished.send(live.shutdown());
            });
            done.recv_timeout(Duration::from_secs(2))
                .expect("confirmation reads ignored client shutdown")
                .unwrap();
            return;
        }
        if live.observation().projection.current_revision > before.projection.current_revision {
            break;
        }
        assert!(Instant::now() < deadline, "receipt recovery timed out");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(saw_error, "faults were not exercised");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a lost read must not replay the command"
    );
    assert_eq!(
        live.observation().projection.current_revision,
        before.projection.current_revision + 1
    );
    assert_eq!(
        live.observation().projection.payload.public_history.len(),
        before.projection.payload.public_history.len() + 1
    );
    let mut peer = PlayerDeviceClient::new(sibling, LoopbackDeviceTransport::new(adapter)).unwrap();
    let peer = peer.observe(&room).unwrap();
    assert_eq!(
        peer.projection.payload.public_game_state,
        live.observation().projection.payload.public_game_state
    );
    assert_eq!(
        peer.projection.payload.public_history,
        live.observation().projection.payload.public_history
    );
    if lose_ack {
        assert!(
            live.last_result().is_none(),
            "an observed revision is not an invented ACK"
        );
    } else {
        assert_eq!(
            live.last_result(),
            receipt.lock().unwrap().as_ref(),
            "a received receipt must survive failed confirmation reads"
        );
        assert!(matches!(
            live.last_result(),
            Some(DeviceActionResult::Committed { .. })
        ));
    }
    live.shutdown().unwrap();
}

#[test]
fn received_receipt_survives_failed_confirmation_reads() {
    exercise(false, false);
}

#[test]
fn unknown_outcome_is_not_fabricated_from_a_later_revision() {
    exercise(true, false);
}

#[test]
fn shutdown_during_confirmation_does_not_wait_forever_for_progress() {
    exercise(false, true);
}
