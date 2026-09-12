// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/

//! Room-owned route maintenance, independent of rendering and command replay.
use crate::{HostRouteOwner, VeilidDeviceNode, VeilidRendezvousError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, oneshot, watch};
use veilid_core::RouteId;

#[cfg(feature = "native-input-test")]
type RetirementReply = oneshot::Sender<Result<u64, VeilidRendezvousError>>;

#[cfg(all(test, feature = "veilid-mock-test"))]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRouteStatus {
    Healthy {
        route_epoch: u64,
    },
    Recovering {
        attempt: u32,
        error: VeilidRendezvousError,
    },
    Stopped,
    Failed(VeilidRendezvousError),
}

/// Drop requests shutdown without blocking a GUI thread. The task keeps its
/// node alive through bounded cleanup; explicit stop awaits that cleanup.
pub struct RunningHostRoute {
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<(), VeilidRendezvousError>>>,
    status: watch::Receiver<HostRouteStatus>,
    #[cfg(feature = "native-input-test")]
    retirement: tokio::sync::mpsc::Sender<RetirementReply>,
}

impl RunningHostRoute {
    #[must_use]
    pub fn start(
        node: VeilidDeviceNode,
        owner: HostRouteOwner,
        updates: broadcast::Receiver<RouteId>,
    ) -> Self {
        let (stop, receiver) = oneshot::channel();
        let (status, reader) = watch::channel(HostRouteStatus::Healthy {
            route_epoch: owner.record().route_epoch,
        });
        let runtime = node.runtime().clone();
        #[cfg(feature = "native-input-test")]
        let (retirement, retirement_rx) = tokio::sync::mpsc::channel(1);
        let task = runtime.spawn(async move {
            let result = maintain(
                owner,
                updates,
                receiver,
                &status,
                #[cfg(feature = "native-input-test")]
                retirement_rx,
            )
            .await;
            status.send_replace(match result {
                Ok(()) => HostRouteStatus::Stopped,
                Err(error) => HostRouteStatus::Failed(error),
            });
            drop(node);
            result
        });
        Self {
            stop: Some(stop),
            task: Some(task),
            status: reader,
            #[cfg(feature = "native-input-test")]
            retirement,
        }
    }

    #[must_use]
    pub fn status(&self) -> HostRouteStatus {
        *self.status.borrow()
    }

    /// Retire this task's currently published real route. Available only in
    /// opt-in acceptance builds; not a game command or callback simulation.
    /// Returns the retired route epoch, never its capability-bearing bytes.
    ///
    /// # Errors
    /// Returns a redacted stopped-task or actual API-release failure.
    #[cfg(feature = "native-input-test")]
    pub async fn retire_current_route_for_acceptance(&self) -> Result<u64, VeilidRendezvousError> {
        let (send, receive) = oneshot::channel();
        self.retirement
            .send(send)
            .await
            .map_err(|_| VeilidRendezvousError::Shutdown)?;
        receive.await.map_err(|_| VeilidRendezvousError::Shutdown)?
    }

    /// Stop maintenance and wait for local route/record cleanup, not disbanding.
    ///
    /// # Errors
    /// Returns a redacted task or DHT-close failure.
    pub async fn stop(mut self) -> Result<(), VeilidRendezvousError> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task
            .take()
            .ok_or(VeilidRendezvousError::Unavailable)?
            .await
            .map_err(|_| VeilidRendezvousError::Unavailable)?
    }
}

impl Drop for RunningHostRoute {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

fn now() -> Result<u64, VeilidRendezvousError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok())
        .ok_or(VeilidRendezvousError::InvalidRecord)
}

fn retryable(error: VeilidRendezvousError) -> bool {
    matches!(
        error,
        VeilidRendezvousError::TryAgain
            | VeilidRendezvousError::Timeout
            | VeilidRendezvousError::NoConnection
            | VeilidRendezvousError::Unavailable
    )
}

fn apply_update(
    owner: &mut HostRouteOwner,
    update: Result<RouteId, broadcast::error::RecvError>,
) -> bool {
    match update {
        Ok(route) => owner.routes_died(&[route]),
        Err(broadcast::error::RecvError::Lagged(_)) => owner.notifications_lagged(),
        Err(broadcast::error::RecvError::Closed) => return false,
    }
    true
}

async fn maintain(
    mut owner: HostRouteOwner,
    mut updates: broadcast::Receiver<RouteId>,
    mut stop: oneshot::Receiver<()>,
    status: &watch::Sender<HostRouteStatus>,
    #[cfg(feature = "native-input-test")] mut retirement: tokio::sync::mpsc::Receiver<
        RetirementReply,
    >,
) -> Result<(), VeilidRendezvousError> {
    let mut failures = 0_u32;
    let result = loop {
        // Drain a bounded batch, coalescing duplicate deaths before allocating.
        for _ in 0..32 {
            match updates.try_recv() {
                Ok(route) => owner.routes_died(&[route]),
                Err(broadcast::error::TryRecvError::Lagged(_)) => owner.notifications_lagged(),
                Err(_) => break,
            }
        }
        if owner.needs_recovery() {
            let time = match now() {
                Ok(time) => time,
                Err(error) => break Err(error),
            };
            // Candidate ownership is recorded before any publication await.
            // Cancellation/drop releases every route handle returned to us.
            let recovered = tokio::select! {
                biased;
                _ = &mut stop => break Ok(()),
                result = tokio::time::timeout(Duration::from_secs(45), owner.recover(time)) =>
                    result.unwrap_or(Err(VeilidRendezvousError::Timeout)),
            };
            match recovered {
                Ok(()) => {
                    failures = 0;
                    status.send_replace(HostRouteStatus::Healthy {
                        route_epoch: owner.record().route_epoch,
                    });
                    eprintln!(
                        "poche: room route renewed; epoch {}",
                        owner.record().route_epoch
                    );
                    continue;
                }
                Err(error) if retryable(error) => {
                    failures = failures.saturating_add(1);
                    status.send_replace(HostRouteStatus::Recovering {
                        attempt: failures,
                        error,
                    });
                    if failures.is_power_of_two() {
                        eprintln!("poche: room route recovery attempt {failures}: {error:?}");
                    }
                    let backoff = Duration::from_secs(1_u64 << failures.saturating_sub(1).min(3));
                    tokio::select! {
                        biased;
                        _ = &mut stop => break Ok(()),
                        () = tokio::time::sleep(backoff) => {},
                    }
                    continue;
                }
                Err(error) => {
                    eprintln!("poche: room route maintenance stopped: {error:?}");
                    break Err(error);
                }
            }
        }
        match wait_event(
            &mut updates,
            &mut stop,
            #[cfg(feature = "native-input-test")]
            &mut retirement,
        )
        .await
        {
            RouteEvent::Stop => break Ok(()),
            RouteEvent::Update(update) => {
                if !apply_update(&mut owner, update) {
                    break Ok(());
                }
            }
            #[cfg(feature = "native-input-test")]
            RouteEvent::Retire(reply) => {
                let retired = tokio::select! {
                    biased;
                    _ = &mut stop => break Ok(()),
                    result = tokio::time::timeout(Duration::from_secs(10), owner.retire_for_acceptance()) =>
                        result.unwrap_or(Err(VeilidRendezvousError::Timeout)),
                };
                let _ = reply.send(retired);
            }
        }
    };
    // Even a timeout drops the owner and synchronously releases known routes.
    let closed = tokio::time::timeout(Duration::from_secs(10), owner.close())
        .await
        .unwrap_or(Err(VeilidRendezvousError::Timeout));
    result.and(closed)
}

enum RouteEvent {
    Stop,
    Update(Result<RouteId, broadcast::error::RecvError>),
    #[cfg(feature = "native-input-test")]
    Retire(RetirementReply),
}

async fn wait_event(
    updates: &mut broadcast::Receiver<RouteId>,
    stop: &mut oneshot::Receiver<()>,
    #[cfg(feature = "native-input-test")] retirement: &mut tokio::sync::mpsc::Receiver<
        RetirementReply,
    >,
) -> RouteEvent {
    #[cfg(feature = "native-input-test")]
    {
        tokio::select! {
            biased;
            _ = stop => RouteEvent::Stop,
            update = updates.recv() => RouteEvent::Update(update),
            request = retirement.recv() => request.map_or(RouteEvent::Stop, RouteEvent::Retire),
        }
    }
    #[cfg(not(feature = "native-input-test"))]
    {
        tokio::select! {
            biased;
            _ = stop => RouteEvent::Stop,
            update = updates.recv() => RouteEvent::Update(update),
        }
    }
}
