//! Authenticated device RPC dispatch. This is an adapter around the existing
//! certified room, not a new authority or a replicated consensus algorithm.

use crate::{VeilidDeviceReply, VeilidDeviceRequest, redacted_device_call_id};
use poche_player_client::DeviceClientError;
use poche_runtime::{AdvertisedActionSource, CertifiedDeviceRoom, CertifiedRoomRecovery};
use poche_session::SessionGame;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

type RecoverySink = dyn Fn(&CertifiedRoomRecovery) -> Result<(), DeviceClientError> + Send + Sync;
type ExpirySink = dyn Fn() -> Result<(), DeviceClientError> + Send + Sync;

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Lifetime of a bounded service task. Keep this beside the room, not the
/// menu. Dropping it stops accepting calls and cancels the async handlers.
/// It is not replica persistence or a substitute for creator failover.
pub struct RunningDeviceService {
    task: tokio::task::JoinHandle<()>,
    _node: crate::VeilidDeviceNode,
}

impl Drop for RunningDeviceService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Share only enrollment/replay state across concurrent calls. Cooperation
/// executes after releasing this lock, allowing the rendering device to make
/// observation requests while it fulfills a capture request.
pub struct VeilidDeviceService<G: SessionGame, A> {
    room: Arc<Mutex<CertifiedDeviceRoom<G, A>>>,
    recovery_sink: Option<Arc<RecoverySink>>,
    recovery_failed: Arc<AtomicBool>,
    recovery_presence: Arc<Mutex<Option<(crate::RecoveryPresence, std::time::Instant)>>>,
    expiry_sink: Option<Arc<ExpirySink>>,
    expiry_persisted: Arc<AtomicBool>,
}

impl<G: SessionGame, A> Clone for VeilidDeviceService<G, A> {
    fn clone(&self) -> Self {
        Self {
            room: Arc::clone(&self.room),
            recovery_sink: self.recovery_sink.clone(),
            recovery_failed: Arc::clone(&self.recovery_failed),
            recovery_presence: Arc::clone(&self.recovery_presence),
            expiry_sink: self.expiry_sink.clone(),
            expiry_persisted: Arc::clone(&self.expiry_persisted),
        }
    }
}

impl<G, A> VeilidDeviceService<G, A>
where
    G: SessionGame + Send + 'static,
    G::Error: Send,
    A: AdvertisedActionSource<G>,
{
    pub fn new(room: CertifiedDeviceRoom<G, A>) -> Self {
        Self {
            room: Arc::new(Mutex::new(room)),
            recovery_sink: None,
            recovery_failed: Arc::new(AtomicBool::new(false)),
            recovery_presence: Arc::new(Mutex::new(None)),
            expiry_sink: None,
            expiry_persisted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Hold mutations until an existing non-creator member answers a fresh
    /// challenge. The gate's clock must begin at Duration::ZERO.
    pub fn awaiting_survivor(
        self,
        presence: crate::RecoveryPresence,
    ) -> Result<Self, DeviceClientError> {
        *self
            .recovery_presence
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)? =
            Some((presence, std::time::Instant::now()));
        Ok(self)
    }

    pub fn recovery_ready(&self) -> Result<bool, DeviceClientError> {
        let presence = self
            .recovery_presence
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        // Keep the desktop monitor alive long enough to retry a transient
        // terminal-write failure. Ordinary service remains frozen throughout;
        // the caller's existing timeout bounds how long recovery can wait.
        if self.expiry_sink.is_some()
            && presence
                .as_ref()
                .is_some_and(|(gate, started)| gate.expired_without_survivor(started.elapsed()))
            && !self.expiry_persisted.load(Ordering::Acquire)
        {
            self.persist_expiry();
            if !self.expiry_persisted.load(Ordering::Acquire) {
                return Ok(false);
            }
        }
        if self.recovery_failed.load(Ordering::Acquire) {
            return Err(DeviceClientError::TransportUnavailable);
        }
        Ok(presence.as_ref().is_none_or(|(gate, _)| gate.witnessed()))
    }

    /// Persist a terminal record when the recovery grace expires. The callback
    /// must replace the active encrypted checkpoint, not merely log expiry.
    pub fn with_recovery_expiry_sink(
        mut self,
        sink: impl Fn() -> Result<(), DeviceClientError> + Send + Sync + 'static,
    ) -> Self {
        self.expiry_sink = Some(Arc::new(sink));
        self
    }

    // Called under the presence mutex. Failed persistence may be retried, but
    // no ordinary operation or checkpoint save may proceed after expiry.
    fn persist_expiry(&self) {
        self.recovery_failed.store(true, Ordering::Release);
        if !self.expiry_persisted.load(Ordering::Acquire) {
            if let Some(sink) = &self.expiry_sink {
                if sink().is_ok() {
                    self.expiry_persisted.store(true, Ordering::Release);
                }
            }
        }
    }

    /// Attach private durable storage before serving any requests. The initial
    /// checkpoint must save successfully. A later failure freezes all clones;
    /// only recovery from authenticated storage may start a new service.
    pub fn with_recovery_sink(
        mut self,
        sink: impl Fn(&CertifiedRoomRecovery) -> Result<(), DeviceClientError> + Send + Sync + 'static,
    ) -> Result<Self, DeviceClientError> {
        self.recovery_sink = Some(Arc::new(sink));
        self.with_room("recovery_attach", "local", |_| Ok(()))?;
        Ok(self)
    }

    fn with_room<T>(
        &self,
        operation_name: &'static str,
        call_id: &str,
        operation: impl FnOnce(&mut CertifiedDeviceRoom<G, A>) -> Result<T, DeviceClientError>,
    ) -> Result<T, DeviceClientError> {
        let total_started = Instant::now();
        let lock_started = Instant::now();
        let mut room = self
            .room
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let lock_wait_us = elapsed_micros(lock_started);
        if self.recovery_failed.load(Ordering::Acquire) {
            return Err(DeviceClientError::TransportUnavailable);
        }
        let operation_started = Instant::now();
        let result = operation(&mut room);
        let operation_us = elapsed_micros(operation_started);
        // Persist even a denied operation: enrollment/replay metadata can have
        // changed before rejection. Never release the lock before saving.
        let mut snapshot_us = 0;
        let mut persistence_us = 0;
        let persistence: Result<(), DeviceClientError> = if let Some(sink) = &self.recovery_sink {
            let snapshot_started = Instant::now();
            let snapshot = room.durable_recovery();
            snapshot_us = elapsed_micros(snapshot_started);
            match snapshot {
                Ok(snapshot) => {
                    let persistence_started = Instant::now();
                    let saved = sink(&snapshot);
                    persistence_us = elapsed_micros(persistence_started);
                    saved
                }
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        };
        tracing::trace!(
            target: "poche_latency",
            event = "authority_room_operation",
            operation = operation_name,
            call_id,
            lock_wait_us,
            operation_us,
            snapshot_us,
            persistence_us,
            total_us = elapsed_micros(total_started),
            operation_success = result.is_ok(),
            persistence_success = persistence.is_ok(),
            recovery_enabled = self.recovery_sink.is_some(),
        );
        if persistence.is_err() {
            self.recovery_failed.store(true, Ordering::Release);
            return Err(DeviceClientError::TransportUnavailable);
        }
        result
    }

    /// Serve a bounded callback queue with at most four concurrent calls.
    /// Independent handlers let observations progress while a capture request
    /// waits on a cooperating device. The callback must use try_send, never
    /// block Veilid's update loop; queue overflow is a retryable lost call.
    pub fn serve(
        self,
        node: crate::VeilidDeviceNode,
        mut incoming: tokio::sync::mpsc::Receiver<Box<veilid_core::VeilidAppCall>>,
    ) -> RunningDeviceService {
        let api = node.api().clone();
        let task = node.runtime().spawn(async move {
            let mut calls = tokio::task::JoinSet::new();
            let started = std::time::Instant::now();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let presence = self.recovery_presence.lock().expect("recovery gate lock");
                        if let Some((gate, started)) = presence.as_ref() {
                            if !gate.witnessed() {
                                if gate.expired_without_survivor(started.elapsed()) { self.persist_expiry(); }
                                continue;
                            }
                        }
                        // No player command is invented here. Only explicitly
                        // enrolled clock/environment services can act.
                        let result = self.with_room("authority_tick", "local", |room| room.drive_authority_services_elapsed(
                                4, started.elapsed(), std::time::Duration::from_secs(3)));
                        if result.is_err() {
                            eprintln!("poche: authority service tick failed");
                        }
                    }
                    call = incoming.recv(), if calls.len() < 4 => {
                        let Some(call) = call else { break; };
                        let call_id = redacted_device_call_id(call.message());
                        tracing::trace!(
                            target: "poche_latency",
                            event = "authority_call_dequeued",
                            call_id,
                            request_bytes = call.message().len(),
                            active_handlers = calls.len(),
                        );
                        let service = self.clone();
                        let api = api.clone();
                        let handler_queued_at = Instant::now();
                        calls.spawn(async move {
                            tracing::trace!(
                                target: "poche_latency",
                                event = "authority_handler_started",
                                call_id,
                                handler_schedule_us = elapsed_micros(handler_queued_at),
                            );
                            // Stable wire errors are returned by dispatch. A
                            // failed reply means the remote call must time out.
                            let _ = service.answer_app_call(&api, &call).await;
                        });
                    }
                    _ = calls.join_next(), if !calls.is_empty() => {}
                }
            }
            calls.abort_all();
        });
        RunningDeviceService { task, _node: node }
    }

    /// Run reducer dispatch away from the Veilid update callback and answer
    /// the exact AppCall operation. The owner must bound concurrent handlers;
    /// malformed requests receive only a constant denial envelope.
    pub async fn answer_app_call(
        &self,
        api: &veilid_core::VeilidAPI,
        call: &veilid_core::VeilidAppCall,
    ) -> Result<(), DeviceClientError> {
        let answer_started = Instant::now();
        let service = self.clone();
        let bytes = call.message().to_vec();
        let call_id = redacted_device_call_id(&bytes);
        let dispatch_started = Instant::now();
        let worker_queued_at = Instant::now();
        let worker_call_id = call_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            tracing::trace!(
                target: "poche_latency",
                event = "authority_dispatch_worker_started",
                call_id = worker_call_id,
                worker_schedule_us = elapsed_micros(worker_queued_at),
            );
            service.dispatch(&bytes)
        })
        .await
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let dispatch_us = elapsed_micros(dispatch_started);
        let dispatch_success = result.is_ok();
        let reply = result.or_else(|_| VeilidDeviceReply::Denied.encode())?;
        let reply_bytes = reply.len();
        let reply_started = Instant::now();
        let reply_result = api
            .app_call_reply(call.id(), reply)
            .await
            .map_err(|_| DeviceClientError::TransportUnavailable);
        tracing::trace!(
            target: "poche_latency",
            event = "authority_call_answered",
            call_id,
            dispatch_us,
            reply_us = elapsed_micros(reply_started),
            total_us = elapsed_micros(answer_started),
            reply_bytes,
            dispatch_success,
            reply_success = reply_result.is_ok(),
        );
        reply_result
    }

    /// Decode then dispatch through certificate/signature checks. Call this
    /// from a worker, not while holding a network callback or rendering lock.
    pub fn dispatch(&self, bytes: &[u8]) -> Result<Vec<u8>, DeviceClientError> {
        let dispatch_started = Instant::now();
        let decode_started = Instant::now();
        let call_id = redacted_device_call_id(bytes);
        let request = VeilidDeviceRequest::decode(bytes)?;
        let decode_us = elapsed_micros(decode_started);
        let operation = request.operation_name();
        tracing::trace!(
            target: "poche_latency",
            event = "authority_request_decoded",
            operation,
            call_id,
            request_bytes = bytes.len(),
            decode_us,
        );
        let mut presence = self
            .recovery_presence
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if let Some((gate, started)) = presence.as_mut() {
            if !gate.witnessed() {
                if gate.expired_without_survivor(started.elapsed()) {
                    self.persist_expiry();
                    return VeilidDeviceReply::Unavailable.encode();
                }
                let VeilidDeviceRequest::Observe(observation) = &request else {
                    return VeilidDeviceReply::NoProgress.encode();
                };
                // Authenticate using the normal certified boundary before
                // accepting evidence; stored connection flags are not proof.
                let result = self.with_room("observe_recovery", &call_id, |room| {
                    room.observe(observation.clone())
                });
                return match result {
                    Ok(value) => {
                        if gate.accept_authenticated(
                            &observation.player_id,
                            &observation.request_id,
                            started.elapsed(),
                        ) {
                            VeilidDeviceReply::Observation(value).encode()
                        } else {
                            VeilidDeviceReply::RecoveryChallenge(gate.challenge().clone()).encode()
                        }
                    }
                    Err(DeviceClientError::TransportUnavailable) => {
                        VeilidDeviceReply::Unavailable.encode()
                    }
                    Err(_) => VeilidDeviceReply::Denied.encode(),
                };
            }
        }
        drop(presence);
        let result = match request {
            VeilidDeviceRequest::PhysicalPose(request) => self
                .with_room("physical_pose", &call_id, |room| {
                    room.physical_pose(&request)
                })
                .map(VeilidDeviceReply::PhysicalPose),
            VeilidDeviceRequest::Observe(request) => self
                .with_room("observe", &call_id, |room| room.observe(request))
                .map(VeilidDeviceReply::Observation),
            VeilidDeviceRequest::Invoke(request) => self
                .with_room("invoke", &call_id, |room| room.invoke(&request))
                .map(VeilidDeviceReply::Action),
            VeilidDeviceRequest::Route(request) => self
                .with_room("route", &call_id, |room| room.change_route(request))
                .map(VeilidDeviceReply::Route),
            VeilidDeviceRequest::Cooperate(call) => {
                let prepared = self.with_room("cooperate", &call_id, |room| {
                    room.prepare_cooperation(&call.certificate, &call.target_device, call.request)
                });
                prepared
                    .and_then(|prepared| prepared.execute())
                    .map(VeilidDeviceReply::Cooperation)
            }
        };
        let reply = match result {
            Ok(reply) => reply.encode(),
            Err(DeviceClientError::NoProgress) => VeilidDeviceReply::NoProgress.encode(),
            Err(DeviceClientError::StaleRevision) => VeilidDeviceReply::StaleRevision.encode(),
            Err(DeviceClientError::TransportUnavailable) => VeilidDeviceReply::Unavailable.encode(),
            // Do not return certificates, signatures, or backend errors in a
            // rejected call. Detailed local diagnostics belong outside wire.
            Err(_) => VeilidDeviceReply::Denied.encode(),
        };
        tracing::trace!(
            target: "poche_latency",
            event = "authority_dispatch_complete",
            operation,
            call_id,
            dispatch_us = elapsed_micros(dispatch_started),
            success = reply.is_ok(),
        );
        reply
    }
}
