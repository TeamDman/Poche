// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response, Sse},
    routing::{get, post},
};
use datastar::prelude::PatchElements;
use futures_util::stream;
use poche_protocol::{CommandPayload, PrincipalId, ProtocolFrame, RoomId};
use poche_runtime::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    OracleSessionGame, ScriptedClient,
};
use poche_session::SessionState;
use poche_ui::{
    ConnectionPresentation, EMBEDDED_REPLAY, PresentationInput, PresentationModel, ReplayDeck,
    render_live_semantic_html, render_semantic_html, render_semantic_html_with_root_id,
};

mod demo;

use demo::LiveDemo;

const INDEX: &str = include_str!("../web/index.html");

#[derive(Clone)]
struct AppState {
    replay: Arc<ReplayDeck>,
    authority: Arc<Mutex<AuthorityHost>>,
    live: Arc<Mutex<LiveDemo>>,
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
        .route("/authority/create", post(authority_create))
        .route("/authority/reset", post(authority_reset))
        .route("/live/{viewer}", get(live_view))
        .route("/live/{viewer}/command/{control}", post(live_command))
        .route("/live/setup/{stage}", post(live_setup))
        .route("/live/clock/advance", post(live_advance_clock))
        .route("/live/{viewer}/disconnect", post(live_disconnect))
        .route("/live/{viewer}/transcript.ndjson", get(live_transcript))
        .route("/live/{viewer}/replay", get(live_replay))
        .with_state(state)
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
        authority: Arc::new(Mutex::new(AuthorityHost::new()?)),
        live: Arc::new(Mutex::new(LiveDemo::new()?)),
    };
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "poche-web-spike listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AuthorityHost, EMBEDDED_REPLAY, INDEX, LiveDemo, ReplayDeck, live_html};
    use poche_protocol::RoomPhase;
    use poche_ui::render_semantic_html;

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
