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
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("poche-veilid-node".to_owned())
            .spawn(move || {
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
}
