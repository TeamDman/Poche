// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    convert::Infallible,
    fmt::Write as _,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        Html, IntoResponse, Redirect, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use datastar::prelude::PatchElements;
use futures_util::{StreamExt as _, stream};
use poche_protocol::{
    CommandPayload, DeviceCustodyWire, GatewayAuthorityModeWire, GatewayProjectionProtectionWire,
    GatewayTrustDisclosureWire, PrincipalId, ProtocolFrame, RoomId,
};
use poche_runtime::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    OracleSessionGame, ScriptedClient,
};
use poche_session::SessionState;
use poche_ui::{
    ConnectionPresentation, EMBEDDED_REPLAY, PresentationInput, PresentationModel, ReplayDeck,
    embedded_spatial_fixture, render_live_semantic_html, render_semantic_html,
    render_semantic_html_with_root_id, render_tabletop_semantic_html,
};
use serde::Deserialize;

mod demo;
mod gateway;
mod tabletop;

use demo::LiveDemo;
use gateway::{
    CommandDecision, GatewayAction, GatewayDeviceRegistration, GatewayLab, GatewayProjectionEvent,
    SignedGatewayCommand,
};
use tabletop::TabletopLab;

const INDEX: &str = include_str!("../web/index.html");
const GATEWAY_INDEX: &str = include_str!("../web/gateway.html");

#[derive(Clone)]
struct AppState {
    replay: Arc<ReplayDeck>,
    rl_episode: Arc<poche_rl::EpisodeTranscript>,
    authority: Arc<Mutex<AuthorityHost>>,
    live: Arc<Mutex<LiveDemo>>,
    gateway: GatewayLab,
    gateway_live: Arc<Mutex<LiveDemo>>,
    tabletop: Arc<Mutex<TabletopLab>>,
}

struct AuthorityHost {
    authority: InProcessAuthority<OracleSessionGame<2>>,
    host: ScriptedClient,
    next_command: u64,
}

impl AuthorityHost {
    fn new() -> Result<Self, String> {
        let state = SessionState::pending(
            room("datastar-spike-room")?,
            principal("system-clock")?,
            principal("system-game")?,
        );
        let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let host = transport
            .connect(principal("host")?)
            .map_err(|error| format!("connect host: {error:?}"))?;
        Ok(Self {
            authority: InProcessAuthority::new(state, transport),
            host,
            next_command: 0,
        })
    }

    fn create_room(&mut self) -> Result<(PresentationModel, String), String> {
        let command_id = format!("web-create-{}", self.next_command);
        self.next_command = self.next_command.saturating_add(1);
        let command = self
            .host
            .command(
                &self.authority.state,
                &command_id,
                CommandPayload::CreateRoom,
            )
            .map_err(|error| format!("build command: {error:?}"))?;
        self.host
            .submit(&mut self.authority.transport, command)
            .map_err(|error| format!("submit command: {error:?}"))?;
        let outcomes = self
            .authority
            .drive_all()
            .map_err(|error| format!("drive authority: {error:?}"))?;
        let outcome = outcomes
            .last()
            .ok_or_else(|| "authority produced no outcome".to_owned())?;
        let disposition = match outcome.disposition {
            AuthorityDisposition::Applied => "applied".to_owned(),
            AuthorityDisposition::Denied(reason) => format!("denied: {reason:?}"),
            AuthorityDisposition::Disconnected => "disconnected".to_owned(),
        };
        let mut projection = None;
        while let Some(frame) = self
            .host
            .receive(&mut self.authority.transport)
            .map_err(|error| format!("receive projection: {error:?}"))?
        {
            if let ProtocolFrame::Projection(envelope) = frame {
                projection = Some(envelope.payload);
            }
        }
        let projection = projection.ok_or_else(|| "authority returned no projection".to_owned())?;
        Ok((
            PresentationModel::from_input(PresentationInput {
                viewer: "host".to_owned(),
                projection,
                legal_actions: Vec::new(),
                connection: ConnectionPresentation::Connected,
                countdown: None,
                chat: Vec::new(),
                notices: Vec::new(),
            }),
            format!(
                "{disposition}; revision {}; events {}",
                outcome.revision, outcome.events
            ),
        ))
    }
}

fn principal(value: &str) -> Result<PrincipalId, String> {
    PrincipalId::new(value).map_err(|_| format!("invalid principal {value}"))
}

fn room(value: &str) -> Result<RoomId, String> {
    RoomId::new(value).map_err(|_| format!("invalid room {value}"))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/view/{viewer}/{ordinal}", get(view_checkpoint))
        .route("/rl/replay", get(rl_replay))
        .route("/rl/replay.ndjson", get(rl_replay_ndjson))
        .route("/authority/create", post(authority_create))
        .route("/authority/reset", post(authority_reset))
        .route("/live/{viewer}", get(live_view))
        .route("/live/{viewer}/command/{control}", post(live_command))
        .route("/live/setup/{stage}", post(live_setup))
        .route("/live/clock/advance", post(live_advance_clock))
        .route("/live/{viewer}/disconnect", post(live_disconnect))
        .route("/live/{viewer}/transcript.ndjson", get(live_transcript))
        .route("/live/{viewer}/replay", get(live_replay))
        .route("/gateway", get(gateway_index))
        .route("/gateway/device/register", post(gateway_register))
        .route("/gateway/device/{device}/events", get(gateway_events))
        .route("/gateway/command", post(gateway_command))
        .route("/gateway/native/revoke/{device}", post(gateway_revoke))
        .route("/gateway/devices", get(gateway_devices))
        .route("/gateway/metrics", get(gateway_metrics))
        .route("/tabletop/{viewer}", get(tabletop_view))
        .route("/tabletop/{viewer}/action/{control}", post(tabletop_action))
        .route("/tabletop/{viewer}/disconnect", post(tabletop_disconnect))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .with_state(state)
}

async fn gateway_index() -> Response {
    let disclosure = GatewayTrustDisclosureWire::for_browser(
        GatewayAuthorityModeWire::HostAuthoritative,
        DeviceCustodyWire::BrowserLocal,
        GatewayProjectionProtectionWire::GatewayPlaintext,
    )
    .and_then(|profile| {
        serde_json::to_string_pretty(&profile)
            .map_err(|_| poche_protocol::GatewayDisclosureError::InvalidJson)
    });
    match disclosure {
        Ok(disclosure) => {
            Html(GATEWAY_INDEX.replace("__DISCLOSURE__", &disclosure)).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn gateway_register(
    State(state): State<AppState>,
    Json(registration): Json<GatewayDeviceRegistration>,
) -> Response {
    let html = match state
        .gateway_live
        .lock()
        .map_err(|_| "gateway live authority lock poisoned".to_owned())
        .and_then(|demo| live_html(&demo, "alice"))
    {
        Ok(html) => html,
        Err(error) => return gateway_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    match state.gateway.register_browser(&registration, html) {
        Ok(device) => Json(device).into_response(),
        Err(error) => gateway_error(StatusCode::CONFLICT, &error.to_string()),
    }
}

async fn gateway_command(
    State(state): State<AppState>,
    Json(command): Json<SignedGatewayCommand>,
) -> Response {
    let live = Arc::clone(&state.gateway_live);
    let decision = state.gateway.apply_command(&command, |action| {
        let Ok(mut demo) = live.lock() else {
            return (
                "gateway live authority lock poisoned".to_owned(),
                "<main><h2>Projection unavailable</h2></main>".to_owned(),
            );
        };
        let result = match action {
            GatewayAction::Pause => demo.control("alice", "pause"),
            GatewayAction::Unpause => demo.control("alice", "unpause"),
            GatewayAction::Chat { text } => {
                demo.submit_gateway_payload("alice", CommandPayload::Chat { text: text.clone() })
            }
            GatewayAction::GrantSpectator => demo.control("alice", "grant-hand-0"),
            GatewayAction::RevokeSpectator => demo.control("alice", "revoke-hand-0"),
        };
        let status = result.unwrap_or_else(|error| format!("semantic denial: {error}"));
        let html = live_html(&demo, "alice").unwrap_or_else(|error| {
            format!("<main><h2>Projection unavailable</h2><p>{error}</p></main>")
        });
        (status, html)
    });
    match decision {
        Ok(CommandDecision::Applied(receipt) | CommandDecision::Duplicate(receipt)) => {
            Json(receipt).into_response()
        }
        Err(error) => gateway_error(StatusCode::CONFLICT, &error.to_string()),
    }
}

async fn gateway_revoke(State(state): State<AppState>, Path(device): Path<String>) -> Response {
    let html = state
        .gateway_live
        .lock()
        .map_err(|_| "gateway live authority lock poisoned".to_owned())
        .and_then(|demo| live_html(&demo, "alice"));
    match html.and_then(|html| {
        state
            .gateway
            .revoke_from_native(&device, &html)
            .map_err(|error| error.to_string())
    }) {
        Ok(device) => Json(device).into_response(),
        Err(error) => gateway_error(StatusCode::CONFLICT, &error),
    }
}

async fn gateway_devices(State(state): State<AppState>) -> Response {
    match state.gateway.devices() {
        Ok(devices) => Json(devices).into_response(),
        Err(error) => gateway_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn gateway_metrics(State(state): State<AppState>) -> Response {
    match state.gateway.metrics() {
        Ok(metrics) => Json(metrics).into_response(),
        Err(error) => gateway_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

#[derive(Default, Deserialize)]
struct EventCursor {
    after: Option<u64>,
}

async fn gateway_events(
    State(state): State<AppState>,
    Path(device): Path<String>,
    Query(cursor): Query<EventCursor>,
    headers: HeaderMap,
) -> Response {
    let header_cursor = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let after = cursor.after.or(header_cursor).unwrap_or(0);
    let (history, receiver) = match state.gateway.subscribe(&device, after) {
        Ok(subscription) => subscription,
        Err(error) => return gateway_error(StatusCode::FORBIDDEN, &error.to_string()),
    };
    let history = stream::iter(
        history
            .into_iter()
            .map(|event| Ok::<_, Infallible>(gateway_sse_event(&event))),
    );
    let live = stream::unfold((receiver, device), |(mut receiver, device)| async move {
        loop {
            match receiver.recv().await {
                Ok(event) if event.recipient_device_id == device => {
                    return Some((
                        Ok::<_, Infallible>(gateway_sse_event(&event)),
                        (receiver, device),
                    ));
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(history.chain(live))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("poche-gateway-keep-alive"),
        )
        .into_response()
}

fn gateway_sse_event(event: &GatewayProjectionEvent) -> Event {
    Event::default()
        .id(event.event_id.to_string())
        .event("projection")
        .json_data(event)
        .expect("gateway projection event always serializes")
}

fn gateway_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

async fn rl_replay(State(state): State<AppState>) -> Response {
    match rl_replay_html(&state.rl_episode) {
        Ok(html) => Html(html).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn rl_replay_ndjson(State(state): State<AppState>) -> Response {
    match state.rl_episode.ndjson() {
        Ok(transcript) => (
            [
                ("content-type", "application/x-ndjson; charset=utf-8"),
                (
                    "content-disposition",
                    "attachment; filename=poche-rl-selected-episode.ndjson",
                ),
            ],
            transcript,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("RL replay serialization failed: {error}"),
        )
            .into_response(),
    }
}

fn rl_replay_html(episode: &poche_rl::EpisodeTranscript) -> Result<String, String> {
    let hash = episode
        .semantic_hash()
        .map_err(|error| format!("RL replay hash failed: {error}"))?;
    let mut transitions = String::new();
    for transition in &episode.transitions {
        write!(
                transitions,
                "<li><strong>seat {}</strong> {} · reward {} · distance {} · terminal {}<br><code>{}</code> → <code>{}</code></li>",
                transition.seat,
                transition.action_label,
                transition.reward_at_next_observation,
                transition.decision_time_distance,
                transition.terminal,
                transition.observation_hash,
                transition.next_observation_hash,
            )
            .map_err(|_| "RL replay HTML formatting failed".to_owned())?;
    }
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Poche RL episode replay</title></head><body><main><h1>Poche RL episode replay</h1><p><strong>Empirical replay only; not a proof of policy quality.</strong></p><dl><dt>Spec</dt><dd>{}</dd><dt>Reward</dt><dd>{}</dd><dt>Seed</dt><dd>{}</dd><dt>Seat policies</dt><dd>{:?}</dd><dt>Final scores</dt><dd>{:?}</dd><dt>Score differential (seat 0)</dt><dd>{}</dd><dt>Illegal actions</dt><dd>{}</dd><dt>Episode semantic hash</dt><dd><code>{hash}</code></dd></dl><p><a href=\"/rl/replay.ndjson\">Download inspectable NDJSON</a></p><ol>{transitions}</ol></main></body></html>",
        episode.spec_id,
        episode.reward_id,
        episode.seed,
        episode.seat_policies,
        episode.final_scores,
        episode.score_differential_seat0,
        episode.illegal_action_count,
    ))
}

fn selected_rl_episode() -> Result<poche_rl::EpisodeTranscript, String> {
    if let Some(path) = std::env::var_os("POCHE_RL_EPISODE_PATH") {
        return load_rl_episode(std::path::Path::new(&path));
    }
    let mut random = poche_rl::LegalRandomPolicy::new(0xb453_0002 ^ 0x5000);
    let mut heuristic = poche_rl::HighCardHeuristicPolicy;
    poche_rl::run_episode_with_seat_policies(0xb453_0002, &mut random, &mut heuristic)
        .map_err(|error| format!("selected RL replay failed: {error:?}"))
}

fn load_rl_episode(path: &std::path::Path) -> Result<poche_rl::EpisodeTranscript, String> {
    let bytes =
        std::fs::read(path).map_err(|_| format!("could not read RL episode {}", path.display()))?;
    let episode: poche_rl::EpisodeTranscript = serde_json::from_slice(&bytes)
        .map_err(|_| "RL episode is not strict EpisodeTranscript JSON".to_owned())?;
    let expected = poche_rl::RlSpec::poche_2p_v1()
        .semantic_hash()
        .map_err(|_| "could not hash RL spec".to_owned())?;
    if episode.spec_id != poche_rl::SPEC_ID
        || episode.spec_hash != expected
        || episode.reward_id != poche_rl::REWARD_ID
        || episode.illegal_action_count != 0
    {
        return Err("RL episode failed semantic validation".to_owned());
    }
    Ok(episode)
}

async fn index(State(state): State<AppState>) -> Response {
    let live = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|demo| live_html(&demo, "host"));
    match live {
        Ok(live) => Html(INDEX.replace("__LIVE_CLIENT__", &live)).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn view_checkpoint(
    State(state): State<AppState>,
    Path((viewer, ordinal)): Path<(String, usize)>,
) -> Response {
    let checkpoint = state
        .replay
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.viewer == viewer)
        .nth(ordinal);
    let Some(checkpoint) = checkpoint else {
        return (StatusCode::NOT_FOUND, "unknown viewer checkpoint").into_response();
    };
    patch_response(render_semantic_html(&checkpoint.presentation))
}

async fn authority_create(State(state): State<AppState>) -> Response {
    let result = state
        .authority
        .lock()
        .map_err(|_| "authority lock poisoned".to_owned())
        .and_then(|mut authority| authority.create_room());
    match result {
        Ok((presentation, status)) => patch_response(format!(
            "<div id=\"authority-result\"><p><strong>{status}</strong></p>{}</div>",
            render_semantic_html_with_root_id(&presentation, "authority-projection")
        )),
        Err(error) => (
            StatusCode::CONFLICT,
            format!("typed authority command failed: {error}"),
        )
            .into_response(),
    }
}

async fn authority_reset(State(state): State<AppState>) -> Response {
    let replacement = AuthorityHost::new();
    match replacement {
        Ok(replacement) => match state.authority.lock() {
            Ok(mut authority) => {
                *authority = replacement;
                patch_response(
                    "<div id=\"authority-result\"><p>Authority reset to pending.</p></div>"
                        .to_owned(),
                )
            }
            Err(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "authority lock poisoned").into_response()
            }
        },
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn live_view(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    with_live(&state, |demo| live_html(demo, &viewer))
}

async fn live_command(
    State(state): State<AppState>,
    Path((viewer, control)): Path<(String, String)>,
) -> Response {
    with_live(&state, |demo| {
        demo.control(&viewer, &control)?;
        live_html(demo, &viewer)
    })
}

async fn live_setup(State(state): State<AppState>, Path(scenario): Path<String>) -> Response {
    with_live(&state, |demo| {
        demo.setup(&scenario)?;
        live_html(demo, "host")
    })
}

async fn live_advance_clock(State(state): State<AppState>) -> Response {
    with_live(&state, |demo| {
        demo.advance_clock()?;
        live_html(demo, "host")
    })
}

async fn live_disconnect(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    with_live(&state, |demo| {
        demo.disconnect(&viewer)?;
        live_html(demo, &viewer)
    })
}

async fn live_transcript(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    let transcript = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|demo| {
            demo.transcript(&viewer)
                .ok_or_else(|| "unknown viewer or empty transcript".to_owned())
        });
    match transcript {
        Ok(transcript) => (
            [
                ("content-type", "application/x-ndjson; charset=utf-8"),
                (
                    "content-disposition",
                    "attachment; filename=exact-projection-transcript.ndjson",
                ),
            ],
            transcript,
        )
            .into_response(),
        Err(error) => (StatusCode::NOT_FOUND, error).into_response(),
    }
}

async fn live_replay(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    let replay = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|demo| demo.replay_html(&viewer));
    match replay {
        Ok(replay) => Html(replay).into_response(),
        Err(error) => (StatusCode::NOT_FOUND, error).into_response(),
    }
}

async fn tabletop_view(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    match tabletop_page(&state, &viewer) {
        Ok(page) => Html(page).into_response(),
        Err(error) => (StatusCode::NOT_FOUND, error).into_response(),
    }
}

async fn tabletop_action(
    State(state): State<AppState>,
    Path((viewer, control)): Path<(String, String)>,
) -> Response {
    let result = state
        .tabletop
        .lock()
        .map_err(|_| "tabletop authority lock poisoned".to_owned())
        .and_then(|mut lab| lab.action(&viewer, &control));
    match result {
        Ok(_) => Redirect::to(&format!("/tabletop/{viewer}")).into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

async fn tabletop_disconnect(
    State(state): State<AppState>,
    Path(viewer): Path<String>,
) -> Response {
    let result = state
        .tabletop
        .lock()
        .map_err(|_| "tabletop authority lock poisoned".to_owned())
        .and_then(|mut lab| lab.disconnect(&viewer));
    match result {
        Ok(_) => Redirect::to(&format!("/tabletop/{viewer}")).into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

fn tabletop_page(state: &AppState, viewer: &str) -> Result<String, String> {
    let (live, scene, supplement) = state
        .tabletop
        .lock()
        .map_err(|_| "tabletop authority lock poisoned".to_owned())?
        .projection(viewer)?;
    let projection = render_tabletop_semantic_html(
        &live,
        &scene,
        "semantic-tabletop",
        &format!("/tabletop/{viewer}/action"),
        &supplement,
    )
    .map_err(|error| format!("semantic tabletop scene failed: {error:?}"))?;
    let fixture = embedded_spatial_fixture()?;
    let fixture_hash = poche_spatial::spatial_scene_hash_hex(&fixture.scene)
        .map_err(|error| format!("shared fixture hash failed: {error:?}"))?;
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Poche semantic tabletop</title><style>
body{{font-family:system-ui,sans-serif;max-width:76rem;margin:auto;padding:1rem;background:#f5f1e8;color:#211b16}}nav,.actions,.hand{{display:flex;gap:.6rem;flex-wrap:wrap}}section{{background:#fff;padding:1rem;margin:1rem 0;border-radius:.5rem}}table{{border-collapse:collapse;width:100%}}th,td{{text-align:left;border-bottom:1px solid #ccc;padding:.35rem}}button{{font:inherit;padding:.55rem .8rem}}#play-target{{border:2px dashed #735f3d;padding:2rem;text-align:center;margin-top:1rem}}code{{overflow-wrap:anywhere}}
</style></head><body><nav aria-label="Viewer projections"><a href="/tabletop/alice">Alice</a><a href="/tabletop/bob">Bob</a><a href="/tabletop/spectator">Spectator</a><a href="/">Web experiments</a></nav><aside><p>Native/HTML shared acceptance fixture: <code data-role="native-fixture-hash">{fixture_hash}</code>. This live page has its own exact-recipient fingerprint below.</p></aside>{projection}<form method="post" action="/tabletop/{viewer}/disconnect"><button type="submit">Simulate transport loss</button></form><script>
let dragged=null;document.addEventListener('dragstart',event=>{{dragged=event.target.closest('[data-command-id]')?.dataset.commandId||null}});let target=document.querySelector('#play-target');if(target){{target.addEventListener('dragover',event=>event.preventDefault());target.addEventListener('drop',async event=>{{event.preventDefault();if(!dragged)return;await fetch('/tabletop/{viewer}/action/'+encodeURIComponent(dragged),{{method:'POST'}});location.reload()}})}}
</script></body></html>"#
    ))
}

fn with_live(
    state: &AppState,
    operation: impl FnOnce(&mut LiveDemo) -> Result<String, String>,
) -> Response {
    let result = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|mut demo| operation(&mut demo));
    match result {
        Ok(elements) => patch_response(elements),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

fn live_html(demo: &LiveDemo, viewer: &str) -> Result<String, String> {
    let live = demo.view(viewer)?;
    Ok(render_live_semantic_html(
        &live,
        "live-client",
        &format!("/live/{viewer}/command"),
    ))
}

fn patch_response(elements: String) -> Response {
    let event = PatchElements::new(elements).write_as_axum_sse_event();
    Sse::new(stream::once(async move { Ok::<_, Infallible>(event) })).into_response()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::var("POCHE_WEB_SPIKE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:4174".to_owned())
        .parse::<SocketAddr>()?;
    let state = AppState {
        replay: Arc::new(ReplayDeck::from_json(EMBEDDED_REPLAY)?),
        rl_episode: Arc::new(selected_rl_episode()?),
        authority: Arc::new(Mutex::new(AuthorityHost::new()?)),
        live: Arc::new(Mutex::new(LiveDemo::new()?)),
        gateway: GatewayLab::new()?,
        gateway_live: Arc::new(Mutex::new(gateway_demo()?)),
        tabletop: Arc::new(Mutex::new(TabletopLab::new()?)),
    };
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "poche-web-spike listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn gateway_demo() -> Result<LiveDemo, String> {
    let mut demo = LiveDemo::new()?;
    demo.setup("running")?;
    demo.control("spectator", "request-hand-0")?;
    Ok(demo)
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorityHost, EMBEDDED_REPLAY, GATEWAY_INDEX, INDEX, LiveDemo, ReplayDeck, gateway_demo,
        live_html, load_rl_episode, rl_replay_html, selected_rl_episode,
    };
    use poche_protocol::RoomPhase;
    use poche_ui::render_semantic_html;

    #[test]
    fn selected_rl_replay_is_hash_identical_and_contains_no_hidden_state() {
        let episode = selected_rl_episode().expect("selected episode");
        assert_eq!(
            episode.semantic_hash().unwrap(),
            "7ab24388ab33c34e4763e065e493b2529c6562c8d70b59e909f70920442fc5a9"
        );
        let html = rl_replay_html(&episode).expect("semantic replay");
        assert!(html.contains("Poche RL episode replay"));
        assert!(html.contains("Final scores</dt><dd>[99, 12]"));
        assert!(!html.contains("private_hand"));
        assert!(!html.contains("deck"));
    }

    #[test]
    fn strict_external_learned_episode_uses_the_same_web_renderer() {
        let episode = selected_rl_episode().expect("selected episode");
        let directory = tempfile::tempdir().expect("temporary replay directory");
        let path = directory.path().join("learned.json");
        std::fs::write(&path, episode.canonical_json().expect("episode JSON"))
            .expect("write temporary episode");
        let loaded = load_rl_episode(&path).expect("strict learned episode");
        assert_eq!(
            loaded.semantic_hash().expect("loaded episode hash"),
            episode.semantic_hash().expect("source episode hash")
        );
        let html = rl_replay_html(&loaded).expect("semantic web replay");
        assert!(html.contains("Empirical replay only"));
        assert!(!html.contains("private_hand"));
    }

    #[test]
    fn semantic_bob_fixture_adds_then_removes_only_the_granted_hand() {
        let replay = ReplayDeck::from_json(EMBEDDED_REPLAY).expect("checked fixture");
        let bob = replay
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.viewer == "bob")
            .collect::<Vec<_>>();
        let ungranted = render_semantic_html(&bob[0].presentation);
        let granted = render_semantic_html(&bob[1].presentation);
        let revoked = render_semantic_html(&bob[2].presentation);
        assert!(!ungranted.contains("Granted spectator view"));
        assert!(granted.contains("Granted spectator view: host"));
        assert!(granted.contains("<code>2C</code> <code>3C</code>"));
        assert!(!revoked.contains("Granted spectator view"));
    }

    #[test]
    fn typed_create_round_trip_returns_the_host_lobby_projection() {
        let (presentation, status) = AuthorityHost::new()
            .expect("authority")
            .create_room()
            .expect("typed create command");
        assert_eq!(presentation.room_phase, RoomPhase::Lobby);
        assert_eq!(presentation.viewer, "host");
        assert!(presentation.own_hand.is_none());
        assert_eq!(status, "applied; revision 1; events 1");
        let html =
            poche_ui::render_semantic_html_with_root_id(&presentation, "authority-projection");
        assert!(html.starts_with("<main id=\"authority-projection\">"));
        assert!(!html.contains("id=\"projection\""));
    }

    #[test]
    fn page_uses_semantic_controls_and_the_pinned_reference_client() {
        assert!(INDEX.contains("<button data-on:click="));
        assert!(INDEX.contains("datastar@1.0.0-RC.7/bundles/datastar.js"));
        assert!(INDEX.contains("aria-live=\"polite\""));
        assert!(INDEX.contains("__LIVE_CLIENT__"));
    }

    #[test]
    fn gateway_page_discloses_trust_and_uses_ordinary_accessible_controls() {
        assert!(GATEWAY_INDEX.contains("host-authoritative compatibility lab"));
        assert!(GATEWAY_INDEX.contains("Gateway-custodied fallback is disabled"));
        assert!(GATEWAY_INDEX.contains("aria-live=\"polite\""));
        assert!(GATEWAY_INDEX.contains("<label for=\"chat-text\">"));
        assert!(GATEWAY_INDEX.contains("Drop SSE only"));
        assert!(GATEWAY_INDEX.contains("Retry identical signed HTTP body"));
        assert!(!GATEWAY_INDEX.contains("WebSocket"));
        let demo = gateway_demo().expect("gateway demo");
        let alice = live_html(&demo, "alice").expect("Alice exact projection");
        assert!(alice.contains("Your hand"));
        assert!(!alice.contains("POCHE-LAB"));
    }

    #[test]
    fn live_html_keeps_typed_payloads_and_invite_proofs_server_side() {
        let demo = LiveDemo::new().expect("demo");
        let alice = live_html(&demo, "alice").expect("alice live view");
        assert!(alice.contains("POCHE-LAB-ALICE"));
        assert!(alice.contains("data-command-id=\"join-room\""));
        assert!(!alice.contains("InviteProof"));
        assert!(!alice.contains("RedeemInvite"));
        assert!(!alice.contains("CommandPayload"));
    }
}
