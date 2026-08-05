// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Opt-in released-Veilid public-network DHT/private-route probe.

use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use poche_protocol::RoomId;
use poche_veilid::{
    ApplicationIdentity, ExplicitInsecureDevelopment, IdentityStoragePolicy,
    InsecureMemoryIdentityStore, PublicRoomMetadata, RoomCode, RoomNetwork, VeilidRendezvous,
};
use veilid_core::{VeilidAPI, VeilidConfig, VeilidUpdate, api_startup};

const READY_TIMEOUT: Duration = Duration::from_secs(90);
const DHT_TIMEOUT: Duration = Duration::from_secs(45);

fn free_udp_port() -> Result<u16, String> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| error.to_string())?;
    socket
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

fn config(path: &str, namespace: &str, port: u16) -> VeilidConfig {
    let mut config = VeilidConfig::new(
        "poche_veilid_public_test",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    namespace.clone_into(&mut config.namespace);
    config.protected_store.always_use_insecure_storage = true;
    "test-only-password".clone_into(&mut config.protected_store.device_encryption_key_password);
    config.network.upnp = false;
    config.network.protocol.udp.listen_address = format!("0.0.0.0:{port}");
    config.network.protocol.tcp.listen = false;
    config.network.protocol.ws.listen = false;
    config
}

async fn wait_for_public_ready(api: &VeilidAPI, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let state = api
            .get_state()
            .await
            .map_err(|error| format!("{label} state: {error:?}"))?;
        if state.attachment.public_internet_ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{label} public readiness timeout (attachment={:?}, peers={})",
                state.attachment.state,
                state.network.peers.len()
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn now_unix_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    u64::try_from(millis).map_err(|error| error.to_string())
}

async fn identity() -> Result<ApplicationIdentity, String> {
    let store = InsecureMemoryIdentityStore::new(
        ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
    );
    ApplicationIdentity::load_or_create(
        &store,
        IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        ),
    )
    .await
    .map_err(|error| format!("identity: {error:?}"))
}

async fn shutdown(api: VeilidAPI) {
    let _ = api.detach().await;
    api.shutdown().await;
}

#[tokio::main]
#[allow(
    clippy::too_many_lines,
    reason = "the opt-in probe owns two complete public node lifecycles"
)]
async fn main() -> Result<(), String> {
    if std::env::var_os("POCHE_ALLOW_VEILID_PUBLIC_TEST").as_deref()
        != Some(std::ffi::OsStr::new("I_ACCEPT_PUBLIC_NETWORK_TRAFFIC"))
    {
        return Err(
            "set POCHE_ALLOW_VEILID_PUBLIC_TEST=I_ACCEPT_PUBLIC_NETWORK_TRAFFIC to opt in"
                .to_owned(),
        );
    }
    let host_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let client_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
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
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("host startup: {error:?}"))?;
    host.attach()
        .await
        .map_err(|error| format!("host attach: {error:?}"))?;
    wait_for_public_ready(&host, "host").await?;

    let client = api_startup(
        Arc::new(drop),
        config(
            &client_directory.path().to_string_lossy(),
            "client",
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("client startup: {error:?}"))?;
    client
        .attach()
        .await
        .map_err(|error| format!("client attach: {error:?}"))?;
    wait_for_public_ready(&client, "client").await?;

    let now = now_unix_ms()?;
    let host_identity = identity().await?;
    let host_adapter =
        VeilidRendezvous::new(host.clone()).map_err(|error| format!("host adapter: {error:?}"))?;
    let published = host_adapter
        .publish_room(
            &host_identity,
            RoomNetwork::VeilidPublic,
            RoomId::new("public-probe-room").map_err(|error| format!("{error:?}"))?,
            PublicRoomMetadata::new("Poche public probe", 2, true)
                .map_err(|error| format!("{error:?}"))?,
            1,
            1,
            now.saturating_add(10 * 60 * 1_000),
            now,
        )
        .await
        .map_err(|error| format!("publish: {error:?}"))?;
    let encoded_code = published
        .room_code()
        .encode()
        .map_err(|error| format!("encode code: {error:?}"))?;
    let code = RoomCode::decode(encoded_code.expose(), now)
        .map_err(|error| format!("decode code: {error:?}"))?;

    let client_adapter = VeilidRendezvous::new(client.clone())
        .map_err(|error| format!("client adapter: {error:?}"))?;
    let deadline = Instant::now() + DHT_TIMEOUT;
    let resolved = loop {
        match client_adapter.resolve_room(&code, now_unix_ms()?).await {
            Ok(room) => break room,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => return Err(format!("resolve: {error:?}")),
        }
    };

    let responder_api = host.clone();
    let responder_calls = Arc::clone(&calls);
    let responder = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let call = responder_calls
                .lock()
                .expect("call queue poisoned")
                .pop_front();
            if let Some(call) = call {
                if call.message() != b"poche-private-route-ping" {
                    return Err("host received unexpected payload".to_owned());
                }
                responder_api
                    .app_call_reply(call.id(), b"poche-private-route-pong".to_vec())
                    .await
                    .map_err(|error| format!("reply: {error:?}"))?;
                return Ok::<(), String>(());
            }
            if Instant::now() >= deadline {
                return Err("host app-call timeout".to_owned());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    let reply = client_adapter
        .app_call(&resolved, b"poche-private-route-ping".to_vec())
        .await
        .map_err(|error| format!("private route call: {error:?}"))?;
    responder.await.map_err(|error| error.to_string())??;
    if reply != b"poche-private-route-pong" {
        return Err("unexpected private-route response".to_owned());
    }
    let host_state = host
        .get_state()
        .await
        .map_err(|error| format!("host state: {error:?}"))?;
    let client_state = client
        .get_state()
        .await
        .map_err(|error| format!("client state: {error:?}"))?;
    println!(
        "veilid-public-probe: passed dht=true private_route=true app_call=true host_peers={} client_peers={}",
        host_state.network.peers.len(),
        client_state.network.peers.len()
    );
    resolved
        .release(&client)
        .await
        .map_err(|error| format!("resolved cleanup: {error:?}"))?;
    published
        .close(&host)
        .await
        .map_err(|error| format!("published cleanup: {error:?}"))?;
    shutdown(client).await;
    shutdown(host).await;
    Ok(())
}
