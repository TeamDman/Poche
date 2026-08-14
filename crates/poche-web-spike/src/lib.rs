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
    extract::{DefaultBodyLimit, Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        Html, IntoResponse, Redirect, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use datastar::prelude::PatchElements;
use ed25519_dalek::{Signer as _, SigningKey};
use futures_util::{StreamExt as _, stream};
use poche_player_client::{DeviceClientError, DeviceProfile, HttpDeviceCooperationCall};
use poche_protocol::{
    CertificateId, CommandPayload, DeviceActionWire, DeviceCapabilityWire, DeviceCustodyWire,
    DeviceId, DeviceObservationRequestWire, GatewayAuthorityModeWire,
    GatewayProjectionProtectionWire, GatewayTrustDisclosureWire, PrincipalId, ProtocolFrame,
    REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, RoomId, SignatureAlgorithm,
    SignatureBytes, SignatureIntent, UnsignedDeviceCertificateWire,
    canonical_device_certificate_bytes,
};
use poche_runtime::{
    AuthorityDisposition, CertifiedDeviceRoom, ClientPort, InProcessAuthority, InProcessTransport,
    LoopbackCodec, OracleRoomActionSource, OracleSessionGame, RuntimeLoopbackDeviceAdapter,
    ScriptedClient,
};
use poche_session::{InviteRecord, SessionState};
use poche_ui::{
    ConnectionPresentation, EMBEDDED_REPLAY, PresentationInput, PresentationModel, ReplayDeck,
    embedded_spatial_fixture, escape_html, render_live_semantic_html, render_semantic_html,
    render_semantic_html_with_root_id, render_tabletop_semantic_html,
};
use serde::Deserialize;
use tokio::sync::broadcast;

mod demo;
mod game;
mod gateway;
mod tabletop;

use demo::LiveDemo;
use game::{BrowserRooms, BrowserSessionEnd};
use gateway::{
    CommandDecision, GatewayAction, GatewayDeviceRegistration, GatewayLab, GatewayProjectionEvent,
    SignedGatewayCommand,
};
use tabletop::TabletopLab;

const INDEX: &str = include_str!("../web/index.html");
const MENU: &str = include_str!("../web/menu.html");
const GAME_JS: &str = include_str!("../web/game.js");
const GATEWAY_INDEX: &str = include_str!("../web/gateway.html");
const TABLETOP_CSS: &str = include_str!("../web/tabletop.css");
const LIVE_UPDATE_CAPACITY: usize = 64;

#[derive(Clone)]
struct AppState {
    replay: Arc<ReplayDeck>,
    rl_episode: Arc<poche_rl::EpisodeTranscript>,
    authority: Arc<Mutex<AuthorityHost>>,
    live: Arc<Mutex<LiveDemo>>,
    live_updates: broadcast::Sender<()>,
    rooms: Arc<Mutex<BrowserRooms>>,
    room_updates: broadcast::Sender<()>,
    gateway: GatewayLab,
    gateway_live: Arc<Mutex<LiveDemo>>,
    tabletop: Arc<Mutex<TabletopLab>>,
    certified_room: Arc<Mutex<WebCertifiedRoom>>,
}

type WebCertifiedRoom = CertifiedDeviceRoom<OracleSessionGame<2>, OracleRoomActionSource>;

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
        .route("/favicon.ico", get(favicon))
        .route("/lab", get(lab_index))
        .route("/game/create", post(game_create))
        .route("/game/join", post(game_join))
        .route("/game/{session}", get(game_view))
        .route("/game/{session}/events", get(game_events))
        .route("/game/{session}/command/{control}", post(game_command))
        .route("/game/{session}/chat", post(game_chat))
        .route("/game/{session}/disconnect", post(game_disconnect))
        .route("/client/{viewer}", get(client_view))
        .route("/view/{viewer}/{ordinal}", get(view_checkpoint))
        .route("/rl/replay", get(rl_replay))
        .route("/rl/replay.ndjson", get(rl_replay_ndjson))
        .route("/authority/create", post(authority_create))
        .route("/authority/reset", post(authority_reset))
        .route("/live/{viewer}", get(live_view))
        .route("/live/{viewer}/events", get(live_events))
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
        .route("/device/v1/observe", post(certified_device_observe))
        .route("/device/v1/invoke", post(certified_device_invoke))
        .route("/device/v1/cooperate", post(certified_device_cooperate))
        .route("/tabletop/{viewer}", get(tabletop_view))
        .route("/tabletop/{viewer}/action/{control}", post(tabletop_action))
        .route("/tabletop/{viewer}/disconnect", post(tabletop_disconnect))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .with_state(state)
}

async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn certified_device_observe(
    State(state): State<AppState>,
    Json(request): Json<DeviceObservationRequestWire>,
) -> Response {
    let result = state
        .certified_room
        .lock()
        .map_err(|_| DeviceClientError::TransportUnavailable)
        .and_then(|mut room| room.observe(request));
    device_api_response(result)
}

async fn certified_device_invoke(
    State(state): State<AppState>,
    Json(action): Json<DeviceActionWire>,
) -> Response {
    let result = state
        .certified_room
        .lock()
        .map_err(|_| DeviceClientError::TransportUnavailable)
        .and_then(|mut room| room.invoke(&action));
    device_api_response(result)
}

async fn certified_device_cooperate(
    State(state): State<AppState>,
    Json(call): Json<HttpDeviceCooperationCall>,
) -> Response {
    let result = state
        .certified_room
        .lock()
        .map_err(|_| DeviceClientError::TransportUnavailable)
        .and_then(|mut room| room.cooperate(&call.certificate, &call.target_device, call.request));
    device_api_response(result)
}

fn device_api_response<T: serde::Serialize>(result: Result<T, DeviceClientError>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            let status = match error {
                DeviceClientError::InvalidProfile
                | DeviceClientError::KeyUnavailable
                | DeviceClientError::SigningFailed
                | DeviceClientError::AuthorizationDenied => StatusCode::FORBIDDEN,
                DeviceClientError::StaleRevision | DeviceClientError::NoProgress => {
                    StatusCode::CONFLICT
                }
                DeviceClientError::InvalidObservation
                | DeviceClientError::UnknownAction
                | DeviceClientError::ProtocolViolation => StatusCode::UNPROCESSABLE_ENTITY,
                DeviceClientError::TransportUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, error.to_string()).into_response()
        }
    }
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

async fn index() -> Html<String> {
    Html(MENU.replace("__MENU_NOTICE__", ""))
}

async fn lab_index(State(state): State<AppState>) -> Response {
    live_client_document(&state, "alice")
}

#[derive(Deserialize)]
struct RoomEntryForm {
    player_name: String,
    #[serde(default)]
    room_code: String,
}

async fn game_create(State(state): State<AppState>, Form(entry): Form<RoomEntryForm>) -> Response {
    let result = state
        .rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())
        .and_then(|mut rooms| rooms.create(&entry.player_name));
    match result {
        Ok(session) => {
            let _ = state.room_updates.send(());
            Redirect::to(&format!("/game/{session}")).into_response()
        }
        Err(error) => menu_error(&error),
    }
}

async fn game_join(State(state): State<AppState>, Form(entry): Form<RoomEntryForm>) -> Response {
    let result = state
        .rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())
        .and_then(|mut rooms| rooms.join(&entry.player_name, &entry.room_code));
    match result {
        Ok(session) => {
            let _ = state.room_updates.send(());
            Redirect::to(&format!("/game/{session}")).into_response()
        }
        Err(error) => menu_error(&error),
    }
}

fn menu_error(error: &str) -> Response {
    let notice = format!(
        "<p class=\"menu-notice\" role=\"alert\">{}</p>",
        escape_html(error)
    );
    (
        StatusCode::BAD_REQUEST,
        Html(MENU.replace("__MENU_NOTICE__", &notice)),
    )
        .into_response()
}

async fn game_view(State(state): State<AppState>, Path(session): Path<String>) -> Response {
    match game_document(&state.rooms, &session) {
        Ok(document) => Html(document).into_response(),
        Err(error) => Html(render_session_unavailable_document(&error)).into_response(),
    }
}

async fn game_events(State(state): State<AppState>, Path(session): Path<String>) -> Response {
    let receiver = state.room_updates.subscribe();
    let initial = match game_sse_event(&state.rooms, &session) {
        Ok(event) => event,
        Err(error) => {
            let event = Event::default()
                .event("room")
                .data(render_session_unavailable(&error));
            return Sse::new(stream::once(async move { Ok::<_, Infallible>(event) }))
                .into_response();
        }
    };
    let rooms = Arc::clone(&state.rooms);
    let updates = stream::unfold(
        (receiver, rooms, session),
        |(mut receiver, rooms, session)| async move {
            loop {
                match receiver.recv().await {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(event) = game_sse_event(&rooms, &session) {
                            return Some((Ok::<_, Infallible>(event), (receiver, rooms, session)));
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );
    Sse::new(stream::once(async move { Ok::<_, Infallible>(initial) }).chain(updates))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("poche-room-keep-alive"),
        )
        .into_response()
}

async fn game_command(
    State(state): State<AppState>,
    Path((session, control)): Path<(String, String)>,
) -> Response {
    let result = state
        .rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())
        .and_then(|mut rooms| rooms.command(&session, &control));
    let _ = state.room_updates.send(());
    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

#[derive(Deserialize)]
struct ChatForm {
    text: String,
}

async fn game_chat(
    State(state): State<AppState>,
    Path(session): Path<String>,
    Form(chat): Form<ChatForm>,
) -> Response {
    let result = state
        .rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())
        .and_then(|mut rooms| rooms.chat(&session, &chat.text));
    let _ = state.room_updates.send(());
    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

async fn game_disconnect(State(state): State<AppState>, Path(session): Path<String>) -> Response {
    let result = state
        .rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())
        .and_then(|mut rooms| rooms.disconnect(&session));
    let _ = state.room_updates.send(());
    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::CONFLICT, error).into_response(),
    }
}

fn game_document(rooms: &Arc<Mutex<BrowserRooms>>, session: &str) -> Result<String, String> {
    let fragment = game_fragment(rooms, session)?;
    let session = escape_html(session);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Poche room</title><style>{TABLETOP_CSS}</style></head><body data-game-session="{session}">{fragment}<script>{GAME_JS}</script></body></html>"#
    ))
}

fn game_fragment(rooms: &Arc<Mutex<BrowserRooms>>, session: &str) -> Result<String, String> {
    let rooms = rooms
        .lock()
        .map_err(|_| "room registry lock poisoned".to_owned())?;
    if let Some(end) = rooms.end_state(session)? {
        return Ok(render_session_end(end));
    }
    let view = rooms.view(session)?;
    let projection_hash = hex_hash(view.projection_hash);
    let rendered = render_tabletop_semantic_html(
        &view.live,
        &view.scene,
        "game-shell",
        &format!("/game/{session}/command"),
        &view.supplement,
    )
    .map_err(|error| format!("player table rendering failed: {error:?}"))?;
    Ok(rendered.replacen(
        "id=\"game-shell\"",
        &format!("id=\"game-shell\" data-projection-hash=\"{projection_hash}\""),
        1,
    ))
}

fn hex_hash(hash: poche_protocol::SemanticHash) -> String {
    hash.0
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

fn render_session_end(end: BrowserSessionEnd) -> String {
    let (eyebrow, heading, explanation) = match end {
        BrowserSessionEnd::LeftRoom => (
            "ROOM LEFT",
            "You have left this room.",
            "This tab's former membership cannot issue more commands. Return to the main menu and use the room code to rejoin as a new spectator.",
        ),
        BrowserSessionEnd::RoomClosed => (
            "ROOM CLOSED",
            "This room has been closed.",
            "The room is permanently closed for every participant in this process.",
        ),
    };
    format!(
        "<main id=\"game-shell\" class=\"game-shell session-end-shell\" data-session-ended=\"true\"><section class=\"session-end-card\"><span>{eyebrow}</span><h1>{heading}</h1><p>{explanation}</p><a class=\"primary-action\" href=\"/\">Return to main menu</a></section></main>"
    )
}

fn render_session_unavailable(error: &str) -> String {
    format!(
        "<main id=\"game-shell\" class=\"game-shell session-end-shell\" data-session-ended=\"true\"><section class=\"session-end-card\"><span>SESSION UNAVAILABLE</span><h1>This table cannot be resumed here.</h1><p>The room server may have restarted, or this device-session address is not known by this process.</p><details><summary>Technical detail</summary><p>{}</p></details><a class=\"primary-action\" href=\"/?forget=1\">Return to main menu</a></section></main>",
        escape_html(error),
    )
}

fn render_session_unavailable_document(error: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Poche session unavailable</title><style>{TABLETOP_CSS}</style></head><body>{}</body></html>"#,
        render_session_unavailable(error),
    )
}

fn game_sse_event(rooms: &Arc<Mutex<BrowserRooms>>, session: &str) -> Result<Event, String> {
    Ok(Event::default()
        .event("room")
        .data(game_fragment(rooms, session)?))
}

async fn client_view(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    live_client_document(&state, &viewer)
}

fn live_client_document(state: &AppState, viewer: &str) -> Response {
    let live = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|demo| live_client_html(&demo, viewer));
    match live {
        Ok(document) => Html(document).into_response(),
        Err(error) => (StatusCode::NOT_FOUND, error).into_response(),
    }
}

fn live_client_html(demo: &LiveDemo, viewer: &str) -> Result<String, String> {
    let live = live_html(demo, viewer)?;
    Ok(INDEX
        .replace("__LIVE_CLIENT__", &live)
        .replace("__CLIENT_NAME__", viewer))
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
    with_live(&state, false, |demo| live_html(demo, &viewer))
}

async fn live_events(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    let receiver = state.live_updates.subscribe();
    let initial = match live_patch_event(&state.live, &viewer) {
        Ok(event) => event,
        Err(error) => return (StatusCode::NOT_FOUND, error).into_response(),
    };
    let live = Arc::clone(&state.live);
    let updates = stream::unfold(
        (receiver, live, viewer),
        |(mut receiver, live, viewer)| async move {
            loop {
                match receiver.recv().await {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(event) = live_patch_event(&live, &viewer) {
                            return Some((Ok::<_, Infallible>(event), (receiver, live, viewer)));
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );
    Sse::new(stream::once(async move { Ok::<_, Infallible>(initial) }).chain(updates))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("poche-live-keep-alive"),
        )
        .into_response()
}

async fn live_command(
    State(state): State<AppState>,
    Path((viewer, control)): Path<(String, String)>,
) -> Response {
    with_live(&state, true, |demo| {
        let _ = demo.control(&viewer, &control);
        live_html(demo, &viewer)
    })
}

async fn live_setup(State(state): State<AppState>, Path(scenario): Path<String>) -> Response {
    with_live(&state, true, |demo| {
        demo.setup(&scenario)?;
        live_html(demo, "alice")
    })
}

async fn live_advance_clock(State(state): State<AppState>) -> Response {
    with_live(&state, true, |demo| {
        demo.advance_clock()?;
        live_html(demo, "alice")
    })
}

async fn live_disconnect(State(state): State<AppState>, Path(viewer): Path<String>) -> Response {
    with_live(&state, true, |demo| {
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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Poche command table</title><style>{TABLETOP_CSS}</style></head><body>{projection}<details class="developer-tools"><summary>Lab</summary><p>Native/HTML fixture <code data-role="native-fixture-hash">{fixture_hash}</code></p><form method="post" action="/tabletop/{viewer}/disconnect"><button type="submit">Simulate transport loss</button></form></details><script>
async function copyPocheDiagnostic(button){{const target=document.getElementById(button.dataset.copyContext);const status=document.getElementById(button.dataset.copyStatus);if(!target)return;try{{await navigator.clipboard.writeText(target.value);if(status)status.textContent='Copied safe diagnostic context to the clipboard.'}}catch(_){{target.focus();target.select();const copied=document.execCommand('copy');if(status)status.textContent=copied?'Copied safe diagnostic context to the clipboard.':'Clipboard access was unavailable; the diagnostic text is selected for manual copying.'}}}}document.addEventListener('click',event=>{{const button=event.target.closest('[data-copy-context]');if(button)void copyPocheDiagnostic(button)}});let dragged=null;document.addEventListener('dragstart',event=>{{dragged=event.target.closest('[data-command-id]')?.dataset.commandId||null}});let target=document.querySelector('#play-target');if(target){{target.addEventListener('dragover',event=>event.preventDefault());target.addEventListener('drop',async event=>{{event.preventDefault();if(!dragged)return;await fetch('/tabletop/{viewer}/action/'+encodeURIComponent(dragged),{{method:'POST'}});location.reload()}})}}
</script></body></html>"#
    ))
}

fn with_live(
    state: &AppState,
    notify: bool,
    operation: impl FnOnce(&mut LiveDemo) -> Result<String, String>,
) -> Response {
    let result = state
        .live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|mut demo| operation(&mut demo));
    match result {
        Ok(elements) => {
            if notify {
                let _ = state.live_updates.send(());
            }
            patch_response(elements)
        }
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

fn live_patch_event(live: &Arc<Mutex<LiveDemo>>, viewer: &str) -> Result<Event, String> {
    let elements = live
        .lock()
        .map_err(|_| "live authority lock poisoned".to_owned())
        .and_then(|demo| live_html(&demo, viewer))?;
    Ok(PatchElements::new(elements).write_as_axum_sse_event())
}

fn spawn_live_clock(
    live: Arc<Mutex<LiveDemo>>,
    updates: broadcast::Sender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let result = live
                .lock()
                .map_err(|_| "live authority lock poisoned".to_owned())
                .and_then(|mut demo| demo.tick_countdown());
            match result {
                Ok(true) => {
                    let _ = updates.send(());
                }
                Ok(false) => {}
                Err(error) => eprintln!("poche-web-spike countdown clock failed: {error}"),
            }
        }
    })
}

fn spawn_room_clock(
    rooms: Arc<Mutex<BrowserRooms>>,
    updates: broadcast::Sender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let result = rooms
                .lock()
                .map_err(|_| "room registry lock poisoned".to_owned())
                .and_then(|mut rooms| rooms.tick_countdowns());
            match result {
                Ok(true) => {
                    let _ = updates.send(());
                }
                Ok(false) => {}
                Err(error) => eprintln!("poche-web-spike player room clock failed: {error}"),
            }
        }
    })
}

fn spawn_certified_services(room: Arc<Mutex<WebCertifiedRoom>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(50));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let result = room
                .lock()
                .map_err(|_| "certified device room lock poisoned".to_owned())
                .and_then(|mut room| {
                    room.drive_authority_services(16)
                        .map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                eprintln!("poche-web-spike certified authority service failed: {error}");
            }
        }
    })
}

/// Run the production-shaped browser surface using the configured address.
///
/// # Errors
///
/// Returns fixture, address, listener, or server failures.
pub async fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::var("POCHE_WEB_SPIKE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:4174".to_owned())
        .parse::<SocketAddr>()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "poche-web-spike listening on http://{}",
        listener.local_addr()?
    );
    serve(listener).await
}

/// Serve the complete browser surface on a caller-owned listener. Puppet and
/// test harnesses bind port zero so concurrent runs never share identity or
/// state, while the standalone binary retains its ordinary configured port.
///
/// # Errors
///
/// Returns fixture construction or Axum serving failures.
pub async fn serve(listener: tokio::net::TcpListener) -> Result<(), Box<dyn std::error::Error>> {
    let (live_updates, _) = broadcast::channel(LIVE_UPDATE_CAPACITY);
    let (room_updates, _) = broadcast::channel(LIVE_UPDATE_CAPACITY);
    let state = AppState {
        replay: Arc::new(ReplayDeck::from_json(EMBEDDED_REPLAY)?),
        rl_episode: Arc::new(selected_rl_episode()?),
        authority: Arc::new(Mutex::new(AuthorityHost::new()?)),
        live: Arc::new(Mutex::new(LiveDemo::named("main-live")?)),
        live_updates,
        rooms: Arc::new(Mutex::new(BrowserRooms::default())),
        room_updates,
        gateway: GatewayLab::new()?,
        gateway_live: Arc::new(Mutex::new(gateway_demo()?)),
        tabletop: Arc::new(Mutex::new(TabletopLab::new()?)),
        certified_room: Arc::new(Mutex::new(certified_device_room()?)),
    };
    let _clock_task = spawn_live_clock(Arc::clone(&state.live), state.live_updates.clone());
    let _room_clock_task = spawn_room_clock(Arc::clone(&state.rooms), state.room_updates.clone());
    let _certified_service_task = spawn_certified_services(Arc::clone(&state.certified_room));
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn gateway_demo() -> Result<LiveDemo, String> {
    let mut demo = LiveDemo::named("browser-gateway")?;
    demo.setup("running")?;
    demo.control("spectator", "request-hand-0")?;
    Ok(demo)
}

fn certified_device_room() -> Result<WebCertifiedRoom, String> {
    const INVITE: &str = "certified-device-join-v1";
    let clock = ephemeral_service_profile("certified-authority-clock")?;
    let environment = ephemeral_service_profile("certified-game-environment")?;
    let mut state = SessionState::pending(
        room("certified-device-room")?,
        clock.player_id.clone(),
        environment.player_id.clone(),
    );
    state
        .invites
        .push(InviteRecord::new(INVITE, u64::MAX).map_err(|error| format!("{error:?}"))?);
    let actions = OracleRoomActionSource::new(0x5eed, 2, INVITE, 3, "certified-countdown")
        .map_err(|error| error.to_string())?;
    let mut room = CertifiedDeviceRoom::new(RuntimeLoopbackDeviceAdapter::new(
        state,
        actions,
        LoopbackCodec::CanonicalNdjson,
    ));
    room.enroll_authority_service(&clock)
        .map_err(|error| error.to_string())?;
    room.enroll_authority_service(&environment)
        .map_err(|error| error.to_string())?;
    Ok(room)
}

fn ephemeral_service_profile(label: &str) -> Result<DeviceProfile, String> {
    let mut root_seed = [0_u8; 32];
    let mut device_seed = [0_u8; 32];
    getrandom::fill(&mut root_seed).map_err(|_| "service root entropy unavailable".to_owned())?;
    getrandom::fill(&mut device_seed)
        .map_err(|_| "service device entropy unavailable".to_owned())?;
    let root = SigningKey::from_bytes(&root_seed);
    let device = SigningKey::from_bytes(&device_seed);
    let player_id = PrincipalId::new(hex_bytes(&root.verifying_key().to_bytes()))
        .map_err(|_| "invalid service root identity".to_owned())?;
    let device_public = hex_bytes(&device.verifying_key().to_bytes());
    let device_id = DeviceId::new(device_public.clone())
        .map_err(|_| "invalid service device identity".to_owned())?;
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new(format!("service-{label}"))
            .map_err(|_| "invalid service certificate ID".to_owned())?,
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_public,
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities: vec![DeviceCapabilityWire::Propose],
        custody: DeviceCustodyWire::NativeLocal,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: player_id.clone(),
        },
    };
    let signature = SignatureBytes::new(hex_bytes(
        &root
            .sign(
                &canonical_device_certificate_bytes(&unsigned)
                    .map_err(|_| "invalid service certificate".to_owned())?,
            )
            .to_bytes(),
    ))
    .map_err(|_| "invalid service certificate signature".to_owned())?;
    let certificate = unsigned
        .attach_signature(signature)
        .map_err(|_| "invalid signed service certificate".to_owned())?;
    Ok(DeviceProfile {
        schema_version: DeviceProfile::SCHEMA_VERSION_V1,
        label: label.to_owned(),
        player_id,
        device_id,
        certificate,
        signing_key_handle: format!("ephemeral-authority-service:{label}"),
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorityHost, BrowserRooms, EMBEDDED_REPLAY, GATEWAY_INDEX, INDEX, LiveDemo, MENU,
        ReplayDeck, game_document, gateway_demo, live_client_html, live_html, load_rl_episode,
        render_session_unavailable_document, rl_replay_html, selected_rl_episode, spawn_live_clock,
    };
    use poche_protocol::{CommandPayload, GameActionWire, RoomPhase};
    use poche_ui::render_semantic_html;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::sync::broadcast;

    use ed25519_dalek::{Signer as _, SigningKey};
    use poche_player_client::{
        AdvertisedActionPolicy, DeviceClientError, DeviceProfile, DeviceSigner,
        HttpDeviceTransport, PlayerDeviceClient, PolicyScope,
    };
    use poche_protocol::{
        CapturePrivacyWire, CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
        CertificateId, CommandId, DEVICE_COOPERATION_SCHEMA_VERSION_V1,
        DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1, DeviceCapabilityWire, DeviceCustodyWire, DeviceId,
        DeviceSignatureIntentWire, InviteProof, PrincipalId, REPLICATION_SCHEMA_VERSION_V1,
        REPLICATION_SIGNATURE_DOMAIN_V1, RoomId, SignatureAlgorithm, SignatureBytes,
        SignatureIntent, UnsignedCaptureRequestWire, UnsignedDeviceCertificateWire,
        canonical_device_certificate_bytes,
    };

    struct TestHttpSigner(SigningKey);

    impl DeviceSigner for TestHttpSigner {
        fn sign_device_bytes(
            &self,
            _profile: &DeviceProfile,
            canonical_bytes: &[u8],
        ) -> Result<SignatureBytes, DeviceClientError> {
            Ok(SignatureBytes::new(hex(&self.0.sign(canonical_bytes).to_bytes())).unwrap())
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").unwrap();
            output
        })
    }

    fn certified_http_profile(label: &str, root_seed: u8, device_seed: u8) -> DeviceProfile {
        let root = SigningKey::from_bytes(&[root_seed; 32]);
        let device = SigningKey::from_bytes(&[device_seed; 32]);
        let player_id = PrincipalId::new(hex(&root.verifying_key().to_bytes())).unwrap();
        let device_id = DeviceId::new(hex(&device.verifying_key().to_bytes())).unwrap();
        let unsigned = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("web-http-{label}")).unwrap(),
            player_id: player_id.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_id.as_str().to_owned(),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player_id.clone(),
            },
        };
        let signature = SignatureBytes::new(hex(&root
            .sign(&canonical_device_certificate_bytes(&unsigned).unwrap())
            .to_bytes()))
        .unwrap();
        DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: label.to_owned(),
            player_id,
            device_id,
            certificate: unsigned.attach_signature(signature).unwrap(),
            signing_key_handle: format!("test-protected:{label}"),
        }
    }

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
        assert!(granted.contains("<code>2♣</code> <code>3♣</code>"));
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
        assert!(INDEX.contains("href=\"/client/alice\""));
        assert!(!INDEX.contains("/client/host"));
        assert!(INDEX.contains("Open Alice client in a new tab"));
        assert!(INDEX.contains("/live/__CLIENT_NAME__/events"));
        assert!(INDEX.contains("data-copy-text"));
        assert!(INDEX.contains("aria-live=\"polite\""));
        assert!(INDEX.contains("__LIVE_CLIENT__"));
        assert!(INDEX.contains("__CLIENT_NAME__"));
    }

    #[test]
    fn player_menu_is_not_the_fixed_identity_developer_fixture() {
        assert!(MENU.contains("Player name"));
        assert!(MENU.contains("Create lobby"));
        assert!(MENU.contains("Join lobby"));
        assert!(MENU.contains("sessionStorage"));
        assert!(MENU.contains("/lab"));
        assert!(!MENU.contains("POCHE-LAB-ALICE"));
        assert!(!MENU.contains("POCHE-LAB-BOB"));
        assert!(!MENU.contains("Deterministic scenario"));
    }

    #[test]
    fn player_room_document_is_self_contained_and_exact_recipient() {
        let mut registry = BrowserRooms::default();
        let session = registry.create("Teamy").expect("player room");
        registry
            .command(&session, "take-seat-0")
            .expect("take a seat");
        let rooms = Arc::new(Mutex::new(registry));
        let document = game_document(&rooms, &session).expect("player room document");

        assert!(document.contains("Teamy"));
        assert!(document.contains("PCH-"));
        assert!(
            document.contains("new EventSource(`/game/${encodeURIComponent(session)}/events`)")
        );
        assert!(document.contains("fetch(form.action, options)"));
        assert!(document.contains("new FormData(form)"));
        assert!(document.contains("application/x-www-form-urlencoded;charset=UTF-8"));
        assert!(document.contains("new URLSearchParams(new FormData(form)).toString()"));
        assert!(document.contains("data-copy-text"));
        assert!(document.contains("data-copy-context"));
        assert!(document.contains("writeClipboard(target.value)"));
        assert!(document.contains("data-confirm="));
        assert!(document.contains("window.confirm(confirmation)"));
        assert!(document.contains("data-chat-form"));
        assert_eq!(document.matches("data-command-id=\"ready\"").count(), 2);
        assert_eq!(
            document.matches("data-command-id=\"release-seat\"").count(),
            2
        );
        assert!(!document.contains("POCHE-LAB"));
        assert!(!document.contains("datastar@"));
        assert!(!document.contains("Alice"));
        assert!(!document.contains("Bob"));
    }

    #[test]
    fn current_bidder_has_the_same_typed_bids_in_table_and_palette() {
        let mut registry = BrowserRooms::default();
        let first = registry.create("First").expect("create room");
        let room_code = registry
            .view(&first)
            .expect("room view")
            .supplement
            .room_code
            .expect("room code");
        let second = registry.join("Second", &room_code).expect("join room");
        for (session, command) in [
            (&first, "take-seat-0"),
            (&second, "take-seat-1"),
            (&first, "ready"),
            (&second, "ready"),
            (&first, "arm-countdown"),
        ] {
            registry.command(session, command).expect(command);
        }
        for _ in 0..3 {
            registry.tick_countdowns().expect("countdown tick");
        }
        let actor = [&first, &second]
            .into_iter()
            .find(|session| {
                registry
                    .view(session)
                    .expect("actor candidate")
                    .live
                    .controls
                    .iter()
                    .any(|control| {
                        matches!(
                            control.payload,
                            CommandPayload::GameAction {
                                action: GameActionWire::Bid { .. }
                            }
                        )
                    })
            })
            .expect("one bidder");
        let controls = registry
            .view(actor)
            .expect("actor view")
            .live
            .controls
            .into_iter()
            .filter(|control| {
                matches!(
                    control.payload,
                    CommandPayload::GameAction {
                        action: GameActionWire::Bid { .. }
                    }
                )
            })
            .collect::<Vec<_>>();
        let rooms = Arc::new(Mutex::new(registry));
        let document = game_document(&rooms, actor).expect("bidder document");
        assert!(document.contains("Place your bid at the table"));
        for control in controls {
            assert_eq!(
                document
                    .matches(&format!("data-command-id=\"{}\"", control.id))
                    .count(),
                2,
                "bid must exist in the table speech control and complete palette"
            );
        }
    }

    #[test]
    fn unknown_session_has_an_explanatory_non_game_document() {
        let document =
            render_session_unavailable_document("This player session is unknown or expired.");
        assert!(document.contains("SESSION UNAVAILABLE"));
        assert!(document.contains("The room server may have restarted"));
        assert!(document.contains("/?forget=1"));
        assert!(!document.contains("data-command-id="));
    }

    #[test]
    fn player_chat_is_escaped_in_the_complete_room_document() {
        let mut registry = BrowserRooms::default();
        let session = registry.create("Chatter").expect("player room");
        registry
            .chat(&session, "hello <table> & friends")
            .expect("typed chat");
        let rooms = Arc::new(Mutex::new(registry));
        let document = game_document(&rooms, &session).expect("chat room document");

        assert!(document.contains("hello &lt;table&gt; &amp; friends"));
        assert!(!document.contains("<table> & friends"));
    }

    #[test]
    fn closed_player_sessions_render_a_terminal_screen_instead_of_stale_game_controls() {
        let mut registry = BrowserRooms::default();
        let session = registry.create("Closer").expect("player room");
        registry
            .command(&session, "close-room")
            .expect("close room");
        let rooms = Arc::new(Mutex::new(registry));
        let document = game_document(&rooms, &session).expect("closed room document");

        assert!(document.contains("This room has been closed."));
        assert!(document.contains("data-session-ended=\"true\""));
        assert!(document.contains("Return to main menu"));
        assert!(!document.contains("data-command-id="));
    }

    #[test]
    fn either_peer_can_create_and_the_other_can_join() {
        let mut demo = LiveDemo::named("path-client-test").expect("demo");
        let alice_pending = live_client_html(&demo, "alice").expect("Alice pending page");
        let bob_pending = live_client_html(&demo, "bob").expect("Bob pending page");
        assert!(alice_pending.contains("data-command-id=\"create-room\""));
        assert!(bob_pending.contains("data-command-id=\"create-room\""));
        assert!(!alice_pending.contains("data-command-id=\"join-room\""));
        assert!(!bob_pending.contains("data-command-id=\"join-room\""));

        demo.control("alice", "create-room").expect("create room");

        let creator = live_client_html(&demo, "alice").expect("Alice creator page");
        assert!(creator.contains("role=coordinator"));
        assert!(creator.contains("data-command-id=\"take-seat-0\""));
        assert!(!creator.contains("POCHE-LAB-ALICE"));
        assert!(creator.contains("POCHE-LAB-BOB"));

        let candidate = live_client_html(&demo, "bob").expect("Bob client page");
        assert!(candidate.contains("This tab is the <strong>bob</strong> client"));
        assert!(candidate.contains("data-command-id=\"join-room\""));
        assert!(candidate.contains("Join this room"));

        demo.control("bob", "join-room").expect("join room");
        let joined = live_client_html(&demo, "bob").expect("joined Bob client page");
        assert!(!joined.contains("data-command-id=\"join-room\""));
        assert!(joined.contains("data-command-id=\"take-seat-0\""));

        let mut bob_created = LiveDemo::named("bob-created-room-test").expect("demo");
        bob_created
            .control("bob", "create-room")
            .expect("Bob creates room");
        assert!(
            bob_created
                .view("bob")
                .expect("Bob coordinator view")
                .command("take-seat-0")
                .is_some()
        );
        assert!(
            bob_created
                .view("alice")
                .expect("Alice candidate view")
                .command("join-room")
                .is_some()
        );
        assert!(live_client_html(&bob_created, "host").is_err());
    }

    #[tokio::test]
    async fn executable_clock_publishes_countdown_ticks_until_running() {
        let mut demo = LiveDemo::named("clock-task-test").expect("demo");
        demo.setup("countdown").expect("countdown setup");
        let live = Arc::new(Mutex::new(demo));
        let (updates, mut receiver) = broadcast::channel(8);
        let clock = spawn_live_clock(Arc::clone(&live), updates);

        for _ in 0..3 {
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("countdown update deadline")
                .expect("countdown update");
        }
        assert_eq!(
            live.lock()
                .expect("live lock")
                .view("alice")
                .unwrap()
                .projection
                .room_phase,
            RoomPhase::Running,
        );
        clock.abort();
        let _ = clock.await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[expect(
        clippy::too_many_lines,
        reason = "the external-device acceptance keeps signed discovery, lifecycle, persistent policy play, service progress, and convergence evidence together"
    )]
    async fn signed_http_device_observes_invokes_waits_and_retries() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            super::serve(listener)
                .await
                .expect("certified device test server");
        });

        let profile = certified_http_profile("host", 51, 52);
        let transport = HttpDeviceTransport::new(
            format!("http://{address}"),
            0,
            TestHttpSigner(SigningKey::from_bytes(&[52; 32])),
        )
        .expect("loopback HTTP transport");
        let mut client = PlayerDeviceClient::new(profile, transport).expect("device client");
        let room_id = RoomId::new("certified-device-room").unwrap();
        let pending = client.observe(&room_id).expect("signed pending view");
        assert_eq!(pending.projection.current_revision, 0);
        assert_eq!(pending.actions[0].id, "room-create");

        let result = client
            .invoke(
                &pending,
                "room-create",
                CommandId::new("http-create-room").unwrap(),
            )
            .expect("signed HTTP action");
        assert!(matches!(
            result,
            poche_player_client::DeviceActionResult::Committed { revision: 1, .. }
        ));
        let lobby = client.observe(&room_id).expect("epoch-one lobby view");
        assert_eq!(lobby.projection.current_revision, 1);
        assert!(lobby.action("room-take-seat-0").is_some());
        assert!(lobby.action("room-take-seat-1").is_some());
        assert!(lobby.action("room-close").is_some());
        let waited = client.wait(&room_id, 0).expect("strictly later view");
        assert_eq!(waited.projection.current_revision, 1);

        let hidden_join_profile = certified_http_profile("guest", 53, 54);
        let hidden_join_transport = HttpDeviceTransport::new(
            format!("http://{address}"),
            1,
            TestHttpSigner(SigningKey::from_bytes(&[54; 32])),
        )
        .expect("guest transport without invite");
        let mut hidden_join_client =
            PlayerDeviceClient::new(hidden_join_profile.clone(), hidden_join_transport).unwrap();
        let public_only = hidden_join_client
            .observe(&room_id)
            .expect("nonmember public discovery");
        assert!(public_only.action("room-join").is_none());

        let guest_transport = HttpDeviceTransport::new(
            format!("http://{address}"),
            1,
            TestHttpSigner(SigningKey::from_bytes(&[54; 32])),
        )
        .expect("guest transport")
        .with_join_invite(InviteProof::new("certified-device-join-v1").unwrap());
        let mut guest = PlayerDeviceClient::new(hidden_join_profile, guest_transport).unwrap();
        let discovered = guest
            .observe(&room_id)
            .expect("invite-bound room discovery");
        assert_eq!(discovered.actions.len(), 1);
        assert_eq!(discovered.actions[0].id, "room-join");
        let joined = guest
            .invoke(
                &discovered,
                "room-join",
                CommandId::new("http-join-room").unwrap(),
            )
            .expect("signed invite redemption");
        assert!(matches!(
            joined,
            poche_player_client::DeviceActionResult::Committed { revision: 2, .. }
        ));
        let guest_lobby = guest
            .observe(&room_id)
            .expect("joined exact-recipient view");
        assert_eq!(guest_lobby.projection.current_revision, 2);
        assert!(guest_lobby.action("room-take-seat-0").is_some());
        assert_eq!(guest_lobby.action_templates[0].id, "chat-send");
        let chat = guest
            .invoke_payload(
                &guest_lobby,
                &CommandPayload::Chat {
                    text: "hello across the signed HTTP boundary".to_owned(),
                },
                CommandId::new("http-chat-guest").unwrap(),
            )
            .expect("parameterized signed chat action");
        assert!(matches!(
            chat,
            poche_player_client::DeviceActionResult::Committed { revision: 3, .. }
        ));
        let host_after_chat = client.observe(&room_id).expect("host chat tail");
        assert!(matches!(
            host_after_chat.chat_tail.as_slice(),
            [poche_player_client::DeviceChatEntry { text, .. }]
                if text == "hello across the signed HTTP boundary"
        ));
        let unknown_target = DeviceId::new("11".repeat(32)).unwrap();
        let unavailable_capture = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new("http-unknown-provider").unwrap(),
            room_id: room_id.clone(),
            membership_epoch: 1,
            player_id: guest.profile().player_id.clone(),
            requester_device_id: guest.profile().device_id.clone(),
            provider_device_id: unknown_target.clone(),
            observed_revision: guest_lobby.projection.current_revision,
            expires_at_unix_ms: 1_000,
            replay_nonce: "http-unknown-provider-nonce".to_owned(),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            viewport: None,
            label: "unknown external provider".to_owned(),
            max_total_bytes: 1024,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: guest.profile().device_id.clone(),
            },
        }
        .attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
        .unwrap();
        assert_eq!(
            guest.cooperate(
                &unknown_target,
                poche_player_client::DeviceCooperationRequest::Capture(unavailable_capture)
            ),
            Err(DeviceClientError::AuthorizationDenied),
            "the HTTP cooperation route must fail closed for an unknown exact target"
        );

        macro_rules! invoke_lifecycle {
            ($client:expr, $action:literal, $command:literal) => {{
                let observation = $client.observe(&room_id).expect("lifecycle view");
                $client
                    .invoke(
                        &observation,
                        $action,
                        CommandId::new($command).expect("lifecycle command ID"),
                    )
                    .expect("signed lifecycle action");
            }};
        }
        invoke_lifecycle!(client, "room-take-seat-0", "http-seat-host");
        invoke_lifecycle!(guest, "room-take-seat-1", "http-seat-guest");
        invoke_lifecycle!(client, "room-ready", "http-ready-host");
        invoke_lifecycle!(guest, "room-ready", "http-ready-guest");
        invoke_lifecycle!(client, "countdown-arm", "http-countdown-arm");

        let policy = AdvertisedActionPolicy::FirstLegal;
        let mut command_sequence = 0_u32;
        let terminal = loop {
            assert!(
                command_sequence < 1_000,
                "external certified players should finish a bounded game"
            );
            let mut progressed = false;
            for (label, player) in [("host", &mut client), ("guest", &mut guest)] {
                let observation = player.observe(&room_id).expect("external agent view");
                if observation.projection.payload.phase == RoomPhase::PostGame {
                    break;
                }
                let Some(action_id) = policy
                    .select(&observation, PolicyScope::PlayerGameActions)
                    .map(|action| action.id.clone())
                else {
                    continue;
                };
                player
                    .invoke(
                        &observation,
                        &action_id,
                        CommandId::new(format!("http-agent-{label}-{command_sequence}"))
                            .expect("agent command ID"),
                    )
                    .expect("external policy action");
                command_sequence = command_sequence.saturating_add(1);
                progressed = true;
            }
            let observation = client.observe(&room_id).expect("terminal check");
            if observation.projection.payload.phase == RoomPhase::PostGame {
                break observation;
            }
            if !progressed {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        assert!(command_sequence > 100);
        assert_eq!(terminal.projection.payload.phase, RoomPhase::PostGame);
        assert!(!terminal.projection.payload.public_history.is_empty());
        assert_eq!(
            guest
                .observe(&room_id)
                .expect("guest terminal view")
                .projection
                .current_revision,
            terminal.projection.current_revision
        );

        server.abort();
        let _ = server.await;
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
        assert!(!alice.contains("POCHE-LAB-ALICE"));
        assert!(alice.contains("POCHE-LAB-BOB"));
    }

    #[test]
    fn live_html_keeps_typed_payloads_and_invite_proofs_server_side() {
        let mut demo = LiveDemo::named("test-live").expect("demo");
        let pending = live_html(&demo, "alice").expect("Alice pending view");
        assert!(!pending.contains("POCHE-LAB"));
        assert!(!pending.contains("data-command-id=\"join-room\""));
        demo.control("bob", "create-room")
            .expect("Bob creates room");
        let alice = live_html(&demo, "alice").expect("Alice candidate view");
        assert!(alice.contains("POCHE-LAB-ALICE"));
        assert!(!alice.contains("POCHE-LAB-BOB"));
        assert!(alice.contains("data-command-id=\"join-room\""));
        assert!(!alice.contains("InviteProof"));
        assert!(!alice.contains("RedeemInvite"));
        assert!(!alice.contains("CommandPayload"));
    }
}
