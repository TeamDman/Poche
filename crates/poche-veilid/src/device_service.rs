//! Authenticated device RPC dispatch. This is an adapter around the existing
//! certified room, not a new authority or a replicated consensus algorithm.

use crate::{VeilidDeviceReply, VeilidDeviceRequest};
use poche_player_client::DeviceClientError;
use poche_runtime::{AdvertisedActionSource, CertifiedDeviceRoom};
use poche_session::SessionGame;
use std::sync::{Arc, Mutex};

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
}

impl<G: SessionGame, A> Clone for VeilidDeviceService<G, A> {
    fn clone(&self) -> Self {
        Self {
            room: Arc::clone(&self.room),
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
        }
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
                        // No player command is invented here. Only explicitly
                        // enrolled clock/environment services can act.
                        let result = self.room.lock()
                            .map_err(|_| DeviceClientError::TransportUnavailable)
                            .and_then(|mut room| room.drive_authority_services_elapsed(
                                4, started.elapsed(), std::time::Duration::from_secs(3)));
                        if result.is_err() {
                            eprintln!("poche: authority service tick failed");
                        }
                    }
                    call = incoming.recv(), if calls.len() < 4 => {
                        let Some(call) = call else { break; };
                        let service = self.clone();
                        let api = api.clone();
                        calls.spawn(async move {
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
        let service = self.clone();
        let bytes = call.message().to_vec();
        let result = tokio::task::spawn_blocking(move || service.dispatch(&bytes))
            .await
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let reply = result.or_else(|_| VeilidDeviceReply::Denied.encode())?;
        api.app_call_reply(call.id(), reply)
            .await
            .map_err(|_| DeviceClientError::TransportUnavailable)
    }

    /// Decode then dispatch through certificate/signature checks. Call this
    /// from a worker, not while holding a network callback or rendering lock.
    pub fn dispatch(&self, bytes: &[u8]) -> Result<Vec<u8>, DeviceClientError> {
        let request = VeilidDeviceRequest::decode(bytes)?;
        let result = match request {
            VeilidDeviceRequest::PhysicalPose(request) => self
                .room
                .lock()
                .map_err(|_| DeviceClientError::TransportUnavailable)?
                .physical_pose(&request)
                .map(VeilidDeviceReply::PhysicalPose),
            VeilidDeviceRequest::Observe(request) => self
                .room
                .lock()
                .map_err(|_| DeviceClientError::TransportUnavailable)?
                .observe(request)
                .map(VeilidDeviceReply::Observation),
            VeilidDeviceRequest::Invoke(request) => self
                .room
                .lock()
                .map_err(|_| DeviceClientError::TransportUnavailable)?
                .invoke(&request)
                .map(VeilidDeviceReply::Action),
            VeilidDeviceRequest::Route(request) => self
                .room
                .lock()
                .map_err(|_| DeviceClientError::TransportUnavailable)?
                .change_route(request)
                .map(VeilidDeviceReply::Route),
            VeilidDeviceRequest::Cooperate(call) => {
                let prepared = self
                    .room
                    .lock()
                    .map_err(|_| DeviceClientError::TransportUnavailable)?
                    .prepare_cooperation(&call.certificate, &call.target_device, call.request);
                prepared
                    .and_then(|prepared| prepared.execute())
                    .map(VeilidDeviceReply::Cooperation)
            }
        };
        match result {
            Ok(reply) => reply.encode(),
            Err(DeviceClientError::NoProgress) => VeilidDeviceReply::NoProgress.encode(),
            Err(DeviceClientError::StaleRevision) => VeilidDeviceReply::StaleRevision.encode(),
            Err(DeviceClientError::TransportUnavailable) => VeilidDeviceReply::Unavailable.encode(),
            // Do not return certificates, signatures, or backend errors in a
            // rejected call. Detailed local diagnostics belong outside wire.
            Err(_) => VeilidDeviceReply::Denied.encode(),
        }
    }
}
