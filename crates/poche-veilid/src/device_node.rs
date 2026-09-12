//! Desktop node lifetime, independent of Bevy and its frame loop.

use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use tokio::runtime::{Handle, Runtime};
use veilid_core::{VeilidAPI, VeilidConfig, VeilidUpdate, api_startup};

/// Keeps the network runtime alive until the last device owner releases it.
/// Startup and explicit shutdown are blocking: call them on a connection worker.
/// Dropping an owner never waits on the graphical frame thread.
#[derive(Clone)]
pub struct VeilidDeviceNode(Arc<NodeOwner>);

struct NodeOwner {
    api: VeilidAPI,
    runtime: Handle,
    stop: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
    local_routes: tokio::sync::broadcast::Sender<veilid_core::RouteId>,
    #[cfg(feature = "veilid-mock-test")]
    update_probe: Arc<dyn Fn(VeilidUpdate) + Send + Sync>,
}

impl Drop for NodeOwner {
    fn drop(&mut self) {
        // The worker performs asynchronous API shutdown and drops Runtime on
        // its own ordinary thread, never from within a Tokio task or Bevy.
        self.stop.take();
    }
}

impl VeilidDeviceNode {
    /// Start an unattached node. The caller supplies isolated storage/config
    /// and a nonblocking update callback; attachment is an explicit next step.
    /// Failures are deliberately redacted, not Veilid configuration dumps.
    pub fn start(
        config: VeilidConfig,
        update: Arc<dyn Fn(VeilidUpdate) + Send + Sync>,
    ) -> Result<Self, &'static str> {
        Self::start_inner(config, update, None)
    }

    /// Start while owning the caller's already-locked storage lease. The
    /// network worker retains it until API shutdown and runtime destruction,
    /// including startup failures and asynchronous last-owner Drop.
    ///
    /// # Errors
    /// Returns the same redacted startup failures as [`Self::start`].
    pub fn start_with_storage_lease(
        config: VeilidConfig,
        update: Arc<dyn Fn(VeilidUpdate) + Send + Sync>,
        lease: std::fs::File,
    ) -> Result<Self, &'static str> {
        Self::start_inner(config, update, Some(lease))
    }

    fn start_inner(
        config: VeilidConfig,
        update: Arc<dyn Fn(VeilidUpdate) + Send + Sync>,
        lease: Option<std::fs::File>,
    ) -> Result<Self, &'static str> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        // Fixed-size route identifiers, never unbounded update payloads. A
        // lagging maintenance reader must conservatively revalidate its route.
        let (local_routes, _) = tokio::sync::broadcast::channel(32);
        let route_sender = local_routes.clone();
        let original_update = update;
        let update = Arc::new(move |update: VeilidUpdate| {
            if let VeilidUpdate::RouteChange(change) = &update {
                for route in &change.dead_routes {
                    let _ = route_sender.send(route.clone());
                }
            }
            original_update(update);
        });
        #[cfg(feature = "veilid-mock-test")]
        let update_probe = update.clone();
        let worker = thread::Builder::new()
            .name("poche-veilid-node".to_owned())
            .spawn(move || {
                // Declared before Runtime: reverse drop order holds the lock
                // through shutdown, runtime destruction and all early returns.
                let _storage_lease = lease;
                let Ok(runtime) = Runtime::new() else {
                    let _ = ready_tx.send(Err("network runtime could not start"));
                    return;
                };
                let api = match runtime.block_on(api_startup(update, config)) {
                    Ok(api) => api,
                    Err(_) => {
                        let _ = ready_tx.send(Err("network node could not start"));
                        return;
                    }
                };
                if ready_tx
                    .send(Ok((api.clone(), runtime.handle().clone())))
                    .is_ok()
                {
                    let _ = stop_rx.recv();
                }
                runtime.block_on(api.shutdown());
            })
            .map_err(|_| "network worker could not start")?;
        match ready_rx.recv() {
            Ok(Ok((api, runtime))) => Ok(Self(Arc::new(NodeOwner {
                api,
                runtime,
                stop: Some(stop_tx),
                worker: Some(worker),
                local_routes,
                #[cfg(feature = "veilid-mock-test")]
                update_probe,
            }))),
            _ => {
                drop(stop_tx);
                let _ = worker.join();
                Err("network node could not start")
            }
        }
    }

    pub fn api(&self) -> &VeilidAPI {
        &self.0.api
    }

    pub fn runtime(&self) -> &Handle {
        &self.0.runtime
    }

    /// Subscribe before publishing/restoring a room, so route deaths during
    /// startup cannot fall between publication and maintenance registration.
    #[must_use]
    pub fn local_route_updates(&self) -> tokio::sync::broadcast::Receiver<veilid_core::RouteId> {
        self.0.local_routes.subscribe()
    }

    /// The pinned mock does not emit `RouteChange` on release. Acceptance tests
    /// supply that missing notification through the production callback. This
    /// hook is absent from production builds and cannot inject game messages.
    #[cfg(feature = "veilid-mock-test")]
    pub fn simulate_route_change_for_mock(&self, change: veilid_core::VeilidRouteChange) {
        (self.0.update_probe)(VeilidUpdate::RouteChange(Box::new(change)));
    }

    /// Attach and wait for actual public-network readiness, with a bounded
    /// deadline. A successful attach request alone does not mean the node can
    /// publish DHT records/private routes. Call through the connection worker.
    pub async fn attach_public(&self, timeout: std::time::Duration) -> Result<(), &'static str> {
        tokio::time::timeout(timeout, async {
            self.api()
                .attach()
                .await
                .map_err(|_| "network attachment failed")?;
            loop {
                let state = self
                    .api()
                    .get_state()
                    .await
                    .map_err(|_| "network state unavailable")?;
                if state.attachment.public_internet_ready {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| "network readiness timed out")?
    }

    /// Wait for clean shutdown when this is the final owner. Ordinary Drop is
    /// nonblocking; acceptance harnesses use this to verify shutdown completed.
    pub fn shutdown(self) -> Result<(), &'static str> {
        let mut owner = Arc::try_unwrap(self.0).map_err(|_| "network node is still in use")?;
        owner.stop.take();
        owner
            .worker
            .take()
            .ok_or("network worker missing")?
            .join()
            .map_err(|_| "network worker failed")
    }
}

#[cfg(all(test, feature = "veilid-mock-test"))]
mod tests {
    use super::*;

    #[test]
    fn runtime_survives_clone_and_shutdown_releases_node_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_str().unwrap();
        let config = VeilidConfig::new(
            "poche_node_owner",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        let node = VeilidDeviceNode::start(config.clone(), Arc::new(drop)).unwrap();
        let survivor = node.clone();
        drop(node);
        survivor
            .runtime()
            .block_on(survivor.attach_public(std::time::Duration::from_secs(2)))
            .unwrap();
        survivor.shutdown().unwrap();
        let restarted = VeilidDeviceNode::start(config, Arc::new(drop)).unwrap();
        restarted.shutdown().unwrap();
    }

    #[test]
    fn storage_lease_survives_frontend_drop_until_network_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let lock_path = directory.path().join("instance.lock");
        let open_lock = || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)
                .unwrap()
        };
        let lease = open_lock();
        lease.try_lock().unwrap();
        let path = directory.path().to_str().unwrap();
        let config = VeilidConfig::new(
            "poche_node_lease",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        let node =
            VeilidDeviceNode::start_with_storage_lease(config.clone(), Arc::new(drop), lease)
                .unwrap();
        let maintenance_owner = node.clone();
        drop(node);
        assert!(
            open_lock().try_lock().is_err(),
            "frontend drop must not release storage still used by a network task"
        );
        let failure_lock_path = directory.path().join("failed-start.lock");
        let open_failure_lock = || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&failure_lock_path)
                .unwrap()
        };
        let failed_lease = open_failure_lock();
        failed_lease.try_lock().unwrap();
        assert!(
            VeilidDeviceNode::start_with_storage_lease(
                config.clone(),
                Arc::new(drop),
                failed_lease
            )
            .is_err(),
            "duplicate node namespace must fail startup"
        );
        open_failure_lock()
            .try_lock()
            .expect("failed startup must release its supplied lease");
        maintenance_owner.shutdown().unwrap();
        let lease = open_lock();
        lease
            .try_lock()
            .expect("shutdown must release the profile lock");
        let restarted =
            VeilidDeviceNode::start_with_storage_lease(config, Arc::new(drop), lease).unwrap();
        restarted.shutdown().unwrap();
        open_lock().try_lock().unwrap();
    }
}
