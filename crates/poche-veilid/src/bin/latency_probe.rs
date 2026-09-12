// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Standalone two-node Veilid private-route latency experiment.
//!
//! This deliberately excludes Poche, DHT rendezvous, persistence, and UI.

use std::{
    ffi::OsStr,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{sync::mpsc, task::JoinHandle};
use veilid_core::{
    RouteBlob, Target, VeilidAPI, VeilidAppCall, VeilidConfig, VeilidUpdate, api_startup,
};

const PUBLIC_OPT_IN: &str = "I_ACCEPT_PUBLIC_NETWORK_TRAFFIC";
const READY_TIMEOUT: Duration = Duration::from_secs(90);
const ROUTE_TIMEOUT: Duration = Duration::from_secs(90);
const MIN_PAYLOAD_BYTES: usize = 13;
const MAX_PAYLOAD_BYTES: usize = 32_768;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Options {
    samples: u32,
    warmup: u32,
    payload_bytes: usize,
    operation_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            samples: 20,
            warmup: 3,
            payload_bytes: 64,
            operation_timeout: Duration::from_secs(30),
        }
    }
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            if argument == "--help" || argument == "-h" {
                return Ok(None);
            }
            let value = arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))?;
            match argument.as_str() {
                "--samples" => {
                    options.samples = parse_positive_u32(&argument, &value)?;
                }
                "--warmup" => {
                    options.warmup = value
                        .parse::<u32>()
                        .map_err(|_| format!("{argument} must be a non-negative integer"))?;
                }
                "--payload-bytes" => {
                    options.payload_bytes = value
                        .parse::<usize>()
                        .map_err(|_| format!("{argument} must be an integer"))?;
                }
                "--timeout-ms" => {
                    options.operation_timeout =
                        Duration::from_millis(u64::from(parse_positive_u32(&argument, &value)?));
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }
        if !(MIN_PAYLOAD_BYTES..=MAX_PAYLOAD_BYTES).contains(&options.payload_bytes) {
            return Err(format!(
                "--payload-bytes must be between {MIN_PAYLOAD_BYTES} and {MAX_PAYLOAD_BYTES}"
            ));
        }
        Ok(Some(options))
    }
}

fn parse_positive_u32(name: &str, value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn usage() {
    println!(
        "Usage: poche-veilid-latency-probe [--samples N] [--warmup N] \
         [--payload-bytes N] [--timeout-ms N]"
    );
    println!("Measures private-route AppCall RTT, AppMessage dispatch, and AppMessage echo RTT.");
}

#[derive(Clone, Copy, Debug)]
struct Summary {
    minimum_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    maximum_ms: f64,
    mean_ms: f64,
}

#[derive(Debug)]
struct ReceivedMessage {
    payload: Vec<u8>,
    received_at: Instant,
}

fn summarize(samples: &[Duration]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let mut milliseconds = samples
        .iter()
        .map(|sample| sample.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    milliseconds.sort_by(f64::total_cmp);
    let sample_count = f64::from(u32::try_from(milliseconds.len()).ok()?);
    Some(Summary {
        minimum_ms: milliseconds[0],
        median_ms: percentile(&milliseconds, 50),
        p95_ms: percentile(&milliseconds, 95),
        maximum_ms: *milliseconds.last()?,
        mean_ms: milliseconds.iter().sum::<f64>() / sample_count,
    })
}

fn percentile(sorted: &[f64], percent: usize) -> f64 {
    let rank = sorted.len().saturating_mul(percent).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn print_summary(label: &str, samples: &[Duration]) {
    let summary = summarize(samples).expect("the CLI requires at least one measured sample");
    println!(
        "summary metric={label} samples={} min_ms={:.3} median_ms={:.3} mean_ms={:.3} p95_ms={:.3} max_ms={:.3}",
        samples.len(),
        summary.minimum_ms,
        summary.median_ms,
        summary.mean_ms,
        summary.p95_ms,
        summary.maximum_ms,
    );
}

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
        "poche_veilid_latency_probe",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    namespace.clone_into(&mut config.namespace);
    config.protected_store.always_use_insecure_storage = true;
    "latency-probe-only-password"
        .clone_into(&mut config.protected_store.device_encryption_key_password);
    config.network.upnp = false;
    config.network.protocol.udp.listen_address = format!("0.0.0.0:{port}");
    config.network.protocol.tcp.listen = false;
    config.network.protocol.ws.listen = false;
    config
}

async fn wait_for_public_ready(api: &VeilidAPI, label: &str) -> Result<Duration, String> {
    let started = Instant::now();
    loop {
        let state = api
            .get_state()
            .await
            .map_err(|error| format!("{label} state: {error:?}"))?;
        if state.attachment.public_internet_ready {
            return Ok(started.elapsed());
        }
        if started.elapsed() >= READY_TIMEOUT {
            return Err(format!(
                "{label} public readiness timeout (attachment={:?}, peers={})",
                state.attachment.state,
                state.network.peers.len()
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn allocate_route(api: &VeilidAPI, label: &str) -> Result<(RouteBlob, Duration), String> {
    let started = Instant::now();
    let deadline = started + ROUTE_TIMEOUT;
    let mut attempts = 0_u32;
    loop {
        attempts += 1;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!("{label} private-route allocation timed out"));
        }
        match tokio::time::timeout(remaining, api.new_private_route()).await {
            Ok(Ok(route)) => {
                println!(
                    "setup node={label} private_route_attempts={attempts} private_route_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1_000.0
                );
                return Ok((route, started.elapsed()));
            }
            Ok(Err(error)) => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "{label} private-route allocation failed after {attempts} attempts: {error:?}"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(_) => return Err(format!("{label} private-route allocation timed out")),
        }
    }
}

fn payload(kind: u8, sequence: u64, bytes: usize) -> Vec<u8> {
    let mut payload = vec![0_u8; bytes];
    payload[..4].copy_from_slice(b"PVL1");
    payload[4] = kind;
    payload[5..13].copy_from_slice(&sequence.to_be_bytes());
    for (index, byte) in payload[13..].iter_mut().enumerate() {
        let offset = u64::try_from(index).expect("payload length is bounded");
        *byte = u8::try_from((sequence + offset) % 251).expect("modulo bounds the byte");
    }
    payload
}

async fn receive_exact_echo(
    receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected: &[u8],
    timeout: Duration,
) -> Result<(), String> {
    let received = tokio::time::timeout(timeout, receiver.recv())
        .await
        .map_err(|_| "AppMessage echo timed out".to_owned())?
        .ok_or_else(|| "AppMessage echo channel closed".to_owned())?;
    if received != expected {
        return Err("AppMessage echo payload or sequence did not match".to_owned());
    }
    Ok(())
}

async fn receive_exact_delivery(
    receiver: &mut mpsc::UnboundedReceiver<ReceivedMessage>,
    expected: &[u8],
    timeout: Duration,
) -> Result<Instant, String> {
    let received = tokio::time::timeout(timeout, receiver.recv())
        .await
        .map_err(|_| "AppMessage delivery timed out".to_owned())?
        .ok_or_else(|| "AppMessage delivery channel closed".to_owned())?;
    if received.payload != expected {
        return Err("AppMessage delivered payload or sequence did not match".to_owned());
    }
    Ok(received.received_at)
}

fn spawn_call_echo(
    api: VeilidAPI,
    mut calls: mpsc::UnboundedReceiver<VeilidAppCall>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(call) = calls.recv().await {
            let response = call.message().to_vec();
            if let Err(error) = api.app_call_reply(call.id(), response).await {
                eprintln!("AppCall echo responder failed: {error:?}");
                return;
            }
        }
    })
}

fn spawn_message_echo(
    api: &VeilidAPI,
    target: veilid_core::RouteId,
    mut messages: mpsc::UnboundedReceiver<ReceivedMessage>,
    deliveries: mpsc::UnboundedSender<ReceivedMessage>,
) -> Result<JoinHandle<()>, String> {
    let routing = api
        .routing_context()
        .map_err(|error| format!("message echo routing context: {error:?}"))?;
    Ok(tokio::spawn(async move {
        while let Some(message) = messages.recv().await {
            let _ = deliveries.send(ReceivedMessage {
                payload: message.payload.clone(),
                received_at: message.received_at,
            });
            if let Err(error) = routing
                .app_message(Target::RouteId(target.clone()), message.payload)
                .await
            {
                eprintln!("AppMessage echo responder failed: {error:?}");
                return;
            }
        }
    }))
}

async fn shutdown(api: VeilidAPI) {
    let _ = api.detach().await;
    api.shutdown().await;
}

#[tokio::main]
#[allow(
    clippy::too_many_lines,
    reason = "the probe owns two complete public node lifecycles and three measurements"
)]
async fn main() -> Result<(), String> {
    let Some(options) = Options::parse(std::env::args().skip(1))? else {
        usage();
        return Ok(());
    };
    if std::env::var_os("POCHE_ALLOW_VEILID_PUBLIC_TEST").as_deref()
        != Some(OsStr::new(PUBLIC_OPT_IN))
    {
        return Err(format!(
            "set POCHE_ALLOW_VEILID_PUBLIC_TEST={PUBLIC_OPT_IN} to opt in"
        ));
    }

    println!(
        "veilid-latency-probe version=1 samples={} warmup={} payload_bytes={} timeout_ms={} dht=false poche=false private_route=true sender_safety=default",
        options.samples,
        options.warmup,
        options.payload_bytes,
        options.operation_timeout.as_millis()
    );

    let node_a_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let node_b_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let (node_a_message_tx, mut node_a_message_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (node_b_call_tx, node_b_call_rx) = mpsc::unbounded_channel::<VeilidAppCall>();
    let (node_b_message_tx, node_b_message_rx) = mpsc::unbounded_channel::<ReceivedMessage>();
    let (delivery_tx, mut delivery_rx) = mpsc::unbounded_channel::<ReceivedMessage>();

    let node_a_started = Instant::now();
    let node_a = api_startup(
        Arc::new(move |update| {
            if let VeilidUpdate::AppMessage(message) = update {
                let _ = node_a_message_tx.send(message.message().to_vec());
            }
        }),
        config(
            &node_a_directory.path().to_string_lossy(),
            "latency-a",
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("node A startup: {error:?}"))?;
    println!(
        "setup node=A api_startup_ms={:.3}",
        node_a_started.elapsed().as_secs_f64() * 1_000.0
    );

    let node_b_started = Instant::now();
    let node_b = api_startup(
        Arc::new(move |update| match update {
            VeilidUpdate::AppCall(call) => {
                let _ = node_b_call_tx.send(*call);
            }
            VeilidUpdate::AppMessage(message) => {
                let _ = node_b_message_tx.send(ReceivedMessage {
                    payload: message.message().to_vec(),
                    received_at: Instant::now(),
                });
            }
            _ => {}
        }),
        config(
            &node_b_directory.path().to_string_lossy(),
            "latency-b",
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("node B startup: {error:?}"))?;
    println!(
        "setup node=B api_startup_ms={:.3}",
        node_b_started.elapsed().as_secs_f64() * 1_000.0
    );

    node_a
        .attach()
        .await
        .map_err(|error| format!("node A attach: {error:?}"))?;
    node_b
        .attach()
        .await
        .map_err(|error| format!("node B attach: {error:?}"))?;
    let (node_a_ready, node_b_ready) = tokio::try_join!(
        wait_for_public_ready(&node_a, "A"),
        wait_for_public_ready(&node_b, "B")
    )?;
    let node_a_state = node_a
        .get_state()
        .await
        .map_err(|error| format!("node A state: {error:?}"))?;
    let node_b_state = node_b
        .get_state()
        .await
        .map_err(|error| format!("node B state: {error:?}"))?;
    println!(
        "setup node=A ready_ms={:.3} peers={} node=B ready_ms={:.3} peers={}",
        node_a_ready.as_secs_f64() * 1_000.0,
        node_a_state.network.peers.len(),
        node_b_ready.as_secs_f64() * 1_000.0,
        node_b_state.network.peers.len()
    );

    let ((node_a_route, _), (node_b_route, _)) =
        tokio::try_join!(allocate_route(&node_a, "A"), allocate_route(&node_b, "B"))?;
    let node_a_to_b = node_a
        .import_remote_private_route(node_b_route.blob.clone())
        .map_err(|error| format!("node A import node B route: {error:?}"))?;
    let node_b_to_a = node_b
        .import_remote_private_route(node_a_route.blob.clone())
        .map_err(|error| format!("node B import node A route: {error:?}"))?;
    println!("setup route_exchange=in_process import_network_round_trips=0");

    let call_echo = spawn_call_echo(node_b.clone(), node_b_call_rx);
    let message_echo =
        spawn_message_echo(&node_b, node_b_to_a.clone(), node_b_message_rx, delivery_tx)?;
    let node_a_routing = node_a
        .routing_context()
        .map_err(|error| format!("node A routing context: {error:?}"))?;
    let total_iterations = options
        .warmup
        .checked_add(options.samples)
        .ok_or_else(|| "warmup plus samples overflowed".to_owned())?;
    let sample_capacity = usize::try_from(options.samples)
        .map_err(|_| "sample count does not fit this platform".to_owned())?;
    let mut app_call_rtt = Vec::with_capacity(sample_capacity);
    let mut app_message_dispatch = Vec::with_capacity(sample_capacity);
    let mut app_message_delivery = Vec::with_capacity(sample_capacity);
    let mut app_message_echo_rtt = Vec::with_capacity(sample_capacity);

    for iteration in 0..total_iterations {
        let call_payload = payload(1, u64::from(iteration), options.payload_bytes);
        let call_started = Instant::now();
        let reply = tokio::time::timeout(
            options.operation_timeout,
            node_a_routing.app_call(Target::RouteId(node_a_to_b.clone()), call_payload.clone()),
        )
        .await
        .map_err(|_| format!("AppCall iteration {iteration} timed out"))?
        .map_err(|error| format!("AppCall iteration {iteration}: {error:?}"))?;
        let call_elapsed = call_started.elapsed();
        if reply != call_payload {
            return Err(format!("AppCall iteration {iteration} reply did not match"));
        }

        let message_payload = payload(2, u64::from(iteration), options.payload_bytes);
        let message_started = Instant::now();
        node_a_routing
            .app_message(
                Target::RouteId(node_a_to_b.clone()),
                message_payload.clone(),
            )
            .await
            .map_err(|error| format!("AppMessage iteration {iteration}: {error:?}"))?;
        let dispatch_elapsed = message_started.elapsed();
        let delivered_at = receive_exact_delivery(
            &mut delivery_rx,
            &message_payload,
            options.operation_timeout,
        )
        .await
        .map_err(|error| format!("AppMessage iteration {iteration}: {error}"))?;
        let delivery_elapsed = delivered_at.saturating_duration_since(message_started);
        receive_exact_echo(
            &mut node_a_message_rx,
            &message_payload,
            options.operation_timeout,
        )
        .await
        .map_err(|error| format!("AppMessage iteration {iteration}: {error}"))?;
        let echo_elapsed = message_started.elapsed();

        if iteration >= options.warmup {
            let sample = iteration - options.warmup + 1;
            println!(
                "sample index={sample} app_call_rtt_ms={:.3} app_message_dispatch_ms={:.3} app_message_delivery_ms={:.3} app_message_echo_rtt_ms={:.3}",
                call_elapsed.as_secs_f64() * 1_000.0,
                dispatch_elapsed.as_secs_f64() * 1_000.0,
                delivery_elapsed.as_secs_f64() * 1_000.0,
                echo_elapsed.as_secs_f64() * 1_000.0
            );
            app_call_rtt.push(call_elapsed);
            app_message_dispatch.push(dispatch_elapsed);
            app_message_delivery.push(delivery_elapsed);
            app_message_echo_rtt.push(echo_elapsed);
        }
    }

    print_summary("app_call_rtt", &app_call_rtt);
    print_summary("app_message_dispatch", &app_message_dispatch);
    print_summary("app_message_delivery", &app_message_delivery);
    print_summary("app_message_echo_rtt", &app_message_echo_rtt);
    println!(
        "interpretation app_message_dispatch=sender_acceptance_only app_message_delivery=receiver_callback_one_way app_message_echo_rtt=end_to_end_round_trip"
    );

    call_echo.abort();
    message_echo.abort();
    node_a
        .release_private_route(node_a_to_b)
        .map_err(|error| format!("node A imported route cleanup: {error:?}"))?;
    node_b
        .release_private_route(node_b_to_a)
        .map_err(|error| format!("node B imported route cleanup: {error:?}"))?;
    node_a
        .release_private_route(node_a_route.route_id)
        .map_err(|error| format!("node A local route cleanup: {error:?}"))?;
    node_b
        .release_private_route(node_b_route.route_id)
        .map_err(|error| format!("node B local route cleanup: {error:?}"))?;
    shutdown(node_b).await;
    shutdown(node_a).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_bounded() {
        let parsed = Options::parse([
            "--samples".to_owned(),
            "7".to_owned(),
            "--warmup".to_owned(),
            "0".to_owned(),
            "--payload-bytes".to_owned(),
            "13".to_owned(),
            "--timeout-ms".to_owned(),
            "1250".to_owned(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.samples, 7);
        assert_eq!(parsed.warmup, 0);
        assert_eq!(parsed.payload_bytes, 13);
        assert_eq!(parsed.operation_timeout, Duration::from_millis(1_250));
        assert!(Options::parse(["--samples".to_owned(), "0".to_owned()]).is_err());
        assert!(Options::parse(["--payload-bytes".to_owned(), "12".to_owned()]).is_err());
    }

    #[test]
    fn payload_is_correlated_and_exact_size() {
        let first = payload(2, 41, 64);
        let second = payload(2, 42, 64);
        assert_eq!(first.len(), 64);
        assert_eq!(&first[..4], b"PVL1");
        assert_eq!(first[4], 2);
        assert_eq!(u64::from_be_bytes(first[5..13].try_into().unwrap()), 41);
        assert_ne!(first, second);
    }

    #[test]
    fn summary_uses_nearest_rank_percentiles() {
        let samples = (1_u64..=20).map(Duration::from_millis).collect::<Vec<_>>();
        let summary = summarize(&samples).unwrap();
        assert_eq!(summary.minimum_ms, 1.0);
        assert_eq!(summary.median_ms, 10.0);
        assert_eq!(summary.p95_ms, 19.0);
        assert_eq!(summary.maximum_ms, 20.0);
        assert_eq!(summary.mean_ms, 10.5);
    }
}
