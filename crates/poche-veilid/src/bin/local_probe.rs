// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Temporary executable probe for released Veilid's isolated loopback path.

use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use veilid_core::{Target, VeilidAPI, VeilidConfig, VeilidUpdate, api_startup};

fn free_udp_port() -> Result<u16, String> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| error.to_string())?;
    socket
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

fn config(path: &str, namespace: &str, port: u16, bootstrap: Option<u16>) -> VeilidConfig {
    let mut config = VeilidConfig::new(
        "poche_veilid_local_test",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    namespace.clone_into(&mut config.namespace);
    config.protected_store.always_use_insecure_storage = true;
    "test-only-password".clone_into(&mut config.protected_store.device_encryption_key_password);
    config.network.network_key_password = Some("poche-isolated-local-v1".to_owned());
    config.network.upnp = false;
    config.network.detect_address_changes = Some(false);
    config.network.protocol.udp.listen_address = format!("0.0.0.0:{port}");
    config.network.protocol.udp.public_address = None;
    config.network.protocol.tcp.connect = false;
    config.network.protocol.tcp.listen = false;
    config.network.protocol.ws.connect = false;
    config.network.protocol.ws.listen = false;
    config.network.routing_table.bootstrap = bootstrap
        .map(|host_port| vec![format!("udp://127.0.0.1:{host_port}")])
        .unwrap_or_default();
    config.network.routing_table.bootstrap_keys.clear();
    config
}

async fn wait_for_peer(api: &VeilidAPI) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = api
            .get_state()
            .await
            .map_err(|error| format!("{error:?}"))?;
        if !state.network.peers.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "peer timeout (public_ready={}, local_ready={}, peers={})",
                state.attachment.public_internet_ready,
                state.attachment.local_network_ready,
                state.network.peers.len()
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn shutdown(api: VeilidAPI) {
    let _ = api.detach().await;
    api.shutdown().await;
}

#[tokio::main]
#[allow(
    clippy::too_many_lines,
    reason = "the isolated topology probe owns two complete node lifecycles"
)]
async fn main() -> Result<(), String> {
    let host_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let client_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let host_port = free_udp_port()?;
    let client_port = free_udp_port()?;
    let calls = Arc::new(Mutex::new(VecDeque::new()));
    let host_calls = Arc::clone(&calls);
    let host = api_startup(
        Arc::new(move |update| {
            if let VeilidUpdate::AppCall(call) = update {
                host_calls
                    .lock()
                    .expect("call queue poisoned")
                    .push_back(*call);
            }
        }),
        config(
            &host_directory.path().to_string_lossy(),
            "host",
            host_port,
            None,
        ),
    )
    .await
    .map_err(|error| format!("host startup: {error:?}"))?;
    host.attach()
        .await
        .map_err(|error| format!("host attach: {error:?}"))?;

    let responder_api = host.clone();
    let responder_calls = Arc::clone(&calls);
    let responder = tokio::spawn(async move {
        loop {
            let call = responder_calls
                .lock()
                .expect("call queue poisoned")
                .pop_front();
            if let Some(call) = call {
                responder_api
                    .app_call_reply(call.id(), b"poche-local-pong".to_vec())
                    .await
                    .map_err(|error| format!("reply: {error:?}"))?;
                return Ok::<(), String>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    let client = api_startup(
        Arc::new(drop),
        config(
            &client_directory.path().to_string_lossy(),
            "client",
            client_port,
            Some(host_port),
        ),
    )
    .await
    .map_err(|error| format!("client startup: {error:?}"))?;
    client
        .attach()
        .await
        .map_err(|error| format!("client attach: {error:?}"))?;
    if let Err(limit) = wait_for_peer(&client).await {
        responder.abort();
        let client_state = client
            .get_state()
            .await
            .map_err(|error| format!("client state: {error:?}"))?;
        println!(
            "veilid-local-probe: expected-topology-limit release=0.5.7 isolated=true peers={} public_ready={} local_ready={} private_routes=false reason={limit}",
            client_state.network.peers.len(),
            client_state.attachment.public_internet_ready,
            client_state.attachment.local_network_ready,
        );
        shutdown(client).await;
        shutdown(host).await;
        return Ok(());
    }
    let host_id = host
        .get_state()
        .await
        .map_err(|error| format!("host state: {error:?}"))?
        .network
        .node_ids
        .into_iter()
        .next()
        .ok_or_else(|| "host has no node id".to_owned())?;
    let reply = client
        .routing_context()
        .map_err(|error| format!("routing: {error:?}"))?
        .app_call(Target::NodeId(host_id), b"poche-local-ping".to_vec())
        .await
        .map_err(|error| format!("call: {error:?}"))?;
    responder.await.map_err(|error| error.to_string())??;
    if reply != b"poche-local-pong" {
        return Err("unexpected local Veilid response".to_owned());
    }
    let client_state = client
        .get_state()
        .await
        .map_err(|error| format!("client state: {error:?}"))?;
    println!(
        "veilid-local-probe: passed peers={} public_ready={} local_ready={} host_port={} client_port={}",
        client_state.network.peers.len(),
        client_state.attachment.public_internet_ready,
        client_state.attachment.local_network_ready,
        host_port,
        client_port
    );
    shutdown(client).await;
    shutdown(host).await;
    Ok(())
}
