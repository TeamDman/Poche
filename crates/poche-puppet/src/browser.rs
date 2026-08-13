// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Real-browser automation and renderer evidence for the semantic puppet.
//!
//! The harness launches a private headless Chromium/Edge profile, creates a
//! distinct browser context for every participant, and drives only ordinary
//! controls on the production-shaped Axum/SSE page. CDP is presentation I/O:
//! it cannot call the reducer or invent game commands.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use async_tungstenite::{
    WebSocketStream,
    tokio::{ConnectStream, connect_async},
    tungstenite::Message,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use futures_util::StreamExt as _;
use poche_capture::{
    CaptureQualification, CaptureSurfaceMetadata, RawCaptureArtifact, RawCaptureBundle,
};
use poche_protocol::{
    CaptureArtifactId, CaptureProviderKindWire, CaptureRepresentationWire, CaptureViewportWire,
    SemanticHash,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{PuppetError, PuppetErrorCode};

const WIDE_WIDTH: u32 = 1280;
const WIDE_HEIGHT: u32 = 720;
const NARROW_WIDTH: u32 = 390;
const NARROW_HEIGHT: u32 = 844;
const CDP_TIMEOUT: Duration = Duration::from_secs(10);
const GAME_STEP_LIMIT: u32 = 400;

/// Browser-visible behavior retained beside graphical artifacts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserRunSummary {
    pub schema: String,
    pub browser_contexts: u8,
    pub player_choices: u32,
    pub terminal_revision: u64,
    pub post_validation_revision: u64,
    pub public_history_events: usize,
    pub chat_round_trip: bool,
    pub disconnect_resume_reconnect: bool,
    pub console_errors: usize,
    pub network_failures: usize,
}

/// One complete browser run before the common requester-side pipeline chooses
/// persistence paths.
pub struct BrowserHarnessResult {
    pub checkpoints: Vec<RawCaptureBundle>,
    pub summary: BrowserRunSummary,
}

/// Run a real, hidden browser through a complete two-player game.
///
/// # Errors
///
/// Returns a stable browser-unavailable error when no supported executable is
/// installed, or a protocol/evidence error when the browser surface fails to
/// reach or represent a required state.
pub fn run_browser_game(seed: u64) -> Result<BrowserHarnessResult, PuppetError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| browser_unavailable())?;
    runtime.block_on(run_browser_game_async(seed))
}

#[allow(
    clippy::too_many_lines,
    reason = "the real-browser orchestration keeps one ownership scope for the server, private profile, browser process, isolated contexts, and cleanup guards"
)]
async fn run_browser_game_async(seed: u64) -> Result<BrowserHarnessResult, PuppetError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| browser_unavailable())?;
    let address = listener.local_addr().map_err(|_| browser_unavailable())?;
    let server = tokio::spawn(async move {
        let _ = poche_web_spike::serve(listener).await;
    });
    let _server = ServerGuard(server);
    let origin = format!("http://{address}");

    let profile = tempfile::Builder::new()
        .prefix("poche-browser-puppet-")
        .tempdir()
        .map_err(|_| browser_unavailable())?;
    let child = launch_browser(profile.path())?;
    let _browser = BrowserGuard(child);
    let websocket = wait_for_devtools(profile.path()).await?;
    let (socket, _) = connect_async(&websocket)
        .await
        .map_err(|_| browser_unavailable())?;
    let mut cdp = Cdp::new(socket);

    let alice = cdp.new_page(&origin).await?;
    let bob = cdp.new_page(&origin).await?;
    let spectator = cdp.new_page(&origin).await?;
    create_room(&mut cdp, &alice, "Alice Browser").await?;
    let room_code = cdp
        .eval_string(
            &alice,
            "document.querySelector('.room-code-chip strong')?.textContent?.trim() ?? ''",
        )
        .await?;
    if room_code.is_empty() {
        return Err(browser_protocol());
    }
    join_room(&mut cdp, &bob, "Bob Browser", &room_code).await?;
    join_room(&mut cdp, &spectator, "Spectator Browser", &room_code).await?;
    cdp.click_command(&alice, "take-seat-0").await?;
    cdp.click_command(&bob, "take-seat-1").await?;
    cdp.click_command(&alice, "ready").await?;
    cdp.click_command(&bob, "ready").await?;
    cdp.click_command(&alice, "arm-countdown").await?;
    cdp.wait_revision(&alice, 10).await?;
    cdp.wait_bool(
        &alice,
        "document.querySelector('.turn-banner > span')?.textContent.startsWith('Bidding') === true",
    )
    .await?;

    let mut checkpoints = Vec::new();
    let mut captured = BTreeSet::new();
    maybe_capture(&mut cdp, &alice, seed, &mut captured, &mut checkpoints).await?;
    let mut choices = 0_u32;
    loop {
        let state = cdp.page_state(&alice).await?;
        if state.room_phase == "PostGame" {
            break;
        }
        if choices >= GAME_STEP_LIMIT {
            return Err(browser_protocol());
        }
        let mut acted = false;
        for page in [&alice, &bob] {
            let Some(label) = cdp.next_game_action(page).await? else {
                continue;
            };
            let before = cdp.page_state(page).await?.revision;
            cdp.click_action_label(page, &label).await?;
            cdp.wait_after_revision(page, before).await?;
            cdp.wait_at_least_revision(&alice, before.saturating_add(1))
                .await?;
            choices = choices.saturating_add(1);
            maybe_capture(&mut cdp, &alice, seed, &mut captured, &mut checkpoints).await?;
            acted = true;
            break;
        }
        if !acted {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    maybe_capture(&mut cdp, &alice, seed, &mut captured, &mut checkpoints).await?;
    let terminal = cdp.page_state(&alice).await?;
    if terminal.revision != 159 || checkpoints.len() != 6 {
        eprintln!(
            "poche browser terminal evidence mismatch revision={} checkpoints={} labels={:?}",
            terminal.revision,
            checkpoints.len(),
            captured
        );
        return Err(browser_contract(
            "the browser game did not produce the complete terminal checkpoint set",
        ));
    }

    let chat_round_trip = cdp
        .send_chat(&bob, &alice, "browser puppet chat round trip")
        .await
        .map_err(|_| browser_contract("the browser chat round trip did not complete"))?;
    let disconnect_resume_reconnect = cdp
        .disconnect_resume_reconnect(&alice)
        .await
        .map_err(|_| browser_contract("the browser resume and reconnect flow did not complete"))?;
    let final_state = cdp
        .page_state(&alice)
        .await
        .map_err(|_| browser_contract("the browser final state could not be observed"))?;
    let public_history_events = cdp
        .eval_u64(
            &alice,
            "document.querySelectorAll('.public-event-log > li').length",
        )
        .await
        .and_then(|value| usize::try_from(value).map_err(|_| browser_protocol()))
        .map_err(|_| browser_contract("the browser public history count could not be observed"))?;
    if !cdp.diagnostics.console_errors.is_empty() || !cdp.diagnostics.network_failures.is_empty() {
        eprintln!(
            "poche browser diagnostics console={} network={}",
            serde_json::to_string(&cdp.diagnostics.console_errors)
                .unwrap_or_else(|_| "<unserializable>".to_owned()),
            serde_json::to_string(&cdp.diagnostics.network_failures)
                .unwrap_or_else(|_| "<unserializable>".to_owned())
        );
    }
    Ok(BrowserHarnessResult {
        checkpoints,
        summary: BrowserRunSummary {
            schema: "poche.browser-puppet.run.v1".to_owned(),
            browser_contexts: 3,
            player_choices: choices,
            terminal_revision: terminal.revision,
            post_validation_revision: final_state.revision,
            public_history_events,
            chat_round_trip,
            disconnect_resume_reconnect,
            console_errors: cdp.diagnostics.console_errors.len(),
            network_failures: cdp.diagnostics.network_failures.len(),
        },
    })
}

async fn maybe_capture(
    cdp: &mut Cdp,
    page: &BrowserPage,
    seed: u64,
    captured: &mut BTreeSet<&'static str>,
    output: &mut Vec<RawCaptureBundle>,
) -> Result<(), PuppetError> {
    let state = cdp.page_state(page).await?;
    let label = if state.room_phase == "PostGame" {
        Some("terminal")
    } else if state.game_phase == "Bidding" && state.completed_rounds == 0 {
        Some("bidding")
    } else if state.game_phase == "Playing"
        && state.played_cards == 0
        && state.completed_rounds == 0
    {
        Some("card-selection")
    } else if state.played_cards > 0 && state.completed_rounds == 0 {
        Some("trick-in-progress")
    } else if state.visible_tricks > 0 {
        Some("trick-resolved")
    } else if state.completed_rounds > 0 {
        Some("score-sheet")
    } else {
        None
    };
    if let Some(label) = label
        && captured.insert(label)
    {
        output.push(cdp.capture_bundle(page, seed, label, &state).await?);
    }
    Ok(())
}

async fn create_room(cdp: &mut Cdp, page: &BrowserPage, name: &str) -> Result<(), PuppetError> {
    cdp.set_input(page, "#player-name", name).await?;
    cdp.click_selector(page, "button.create").await?;
    cdp.wait_path_prefix(page, "/game/").await
}

async fn join_room(
    cdp: &mut Cdp,
    page: &BrowserPage,
    name: &str,
    room_code: &str,
) -> Result<(), PuppetError> {
    cdp.set_input(page, "#player-name", name).await?;
    cdp.eval_value(page, "document.querySelector('details').open = true; true")
        .await?;
    cdp.set_input(page, "#room-code", room_code).await?;
    cdp.click_selector(page, "button.join").await?;
    cdp.wait_path_prefix(page, "/game/").await
}

#[derive(Clone, Debug)]
struct PageState {
    revision: u64,
    room_phase: String,
    game_phase: String,
    played_cards: u64,
    completed_rounds: u64,
    visible_tricks: u64,
    projection_hash: SemanticHash,
    scene_hash: SemanticHash,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageStateWire {
    revision: u64,
    room_phase: String,
    game_phase: String,
    played_cards: u64,
    completed_rounds: u64,
    visible_tricks: u64,
    projection_hash: String,
    scene_hash: String,
}

#[derive(Default)]
struct CdpDiagnostics {
    console_errors: Vec<Value>,
    network_failures: Vec<Value>,
}

struct Cdp {
    socket: WebSocketStream<ConnectStream>,
    next_id: u64,
    diagnostics: CdpDiagnostics,
}

#[derive(Clone)]
struct BrowserPage {
    session_id: String,
}

impl Cdp {
    const fn new(socket: WebSocketStream<ConnectStream>) -> Self {
        Self {
            socket,
            next_id: 1,
            diagnostics: CdpDiagnostics {
                console_errors: Vec::new(),
                network_failures: Vec::new(),
            },
        }
    }

    async fn new_page(&mut self, origin: &str) -> Result<BrowserPage, PuppetError> {
        let context = self
            .command(None, "Target.createBrowserContext", json!({}))
            .await?["browserContextId"]
            .as_str()
            .ok_or_else(browser_protocol)?
            .to_owned();
        let target = self
            .command(
                None,
                "Target.createTarget",
                json!({"url":"about:blank", "browserContextId":context}),
            )
            .await?["targetId"]
            .as_str()
            .ok_or_else(browser_protocol)?
            .to_owned();
        let session_id = self
            .command(
                None,
                "Target.attachToTarget",
                json!({"targetId":target, "flatten":true}),
            )
            .await?["sessionId"]
            .as_str()
            .ok_or_else(browser_protocol)?
            .to_owned();
        let page = BrowserPage { session_id };
        for domain in [
            "Page.enable",
            "Runtime.enable",
            "Network.enable",
            "Log.enable",
        ] {
            self.command(Some(&page), domain, json!({})).await?;
        }
        self.set_viewport(&page, WIDE_WIDTH, WIDE_HEIGHT).await?;
        self.command(
            Some(&page),
            "Page.navigate",
            json!({"url":format!("{origin}/")}),
        )
        .await?;
        self.wait_bool(&page, "document.readyState === 'complete'")
            .await?;
        Ok(page)
    }

    async fn command(
        &mut self,
        page: Option<&BrowserPage>,
        method: &str,
        params: Value,
    ) -> Result<Value, PuppetError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut request = json!({"id":id, "method":method, "params":params});
        if let Some(page) = page {
            request["sessionId"] = Value::String(page.session_id.clone());
        }
        self.socket
            .send(Message::text(request.to_string()))
            .await
            .map_err(|_| browser_protocol())?;
        let response = tokio::time::timeout(CDP_TIMEOUT, async {
            loop {
                let message = self
                    .socket
                    .next()
                    .await
                    .ok_or_else(browser_protocol)?
                    .map_err(|_| browser_protocol())?;
                match message {
                    Message::Text(text) => {
                        let value: Value =
                            serde_json::from_str(text.as_str()).map_err(|_| browser_protocol())?;
                        if value["id"].as_u64() == Some(id) {
                            if value.get("error").is_some() {
                                return Err(browser_protocol());
                            }
                            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                        }
                        self.observe_event(&value);
                    }
                    Message::Ping(bytes) => {
                        self.socket
                            .send(Message::Pong(bytes))
                            .await
                            .map_err(|_| browser_protocol())?;
                    }
                    Message::Close(_) => return Err(browser_protocol()),
                    Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
                }
            }
        })
        .await
        .map_err(|_| browser_protocol())??;
        Ok(response)
    }

    fn observe_event(&mut self, value: &Value) {
        match value["method"].as_str() {
            Some("Runtime.exceptionThrown") => {
                self.diagnostics.console_errors.push(value.clone());
            }
            Some("Log.entryAdded")
                if matches!(
                    value["params"]["entry"]["level"].as_str(),
                    Some("error" | "warning")
                ) =>
            {
                self.diagnostics.console_errors.push(value.clone());
            }
            Some("Network.loadingFailed")
                if value["params"]["canceled"].as_bool() != Some(true)
                    && value["params"]["errorText"].as_str() != Some("net::ERR_ABORTED") =>
            {
                self.diagnostics.network_failures.push(value.clone());
            }
            _ => {}
        }
    }

    async fn eval_value(
        &mut self,
        page: &BrowserPage,
        expression: &str,
    ) -> Result<Value, PuppetError> {
        let result = self
            .command(
                Some(page),
                "Runtime.evaluate",
                json!({
                    "expression":expression,
                    "returnByValue":true,
                    "awaitPromise":true
                }),
            )
            .await?;
        if result.get("exceptionDetails").is_some() {
            return Err(browser_protocol());
        }
        Ok(result["result"]
            .get("value")
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn eval_string(
        &mut self,
        page: &BrowserPage,
        expression: &str,
    ) -> Result<String, PuppetError> {
        self.eval_value(page, expression)
            .await?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(browser_protocol)
    }

    async fn eval_u64(&mut self, page: &BrowserPage, expression: &str) -> Result<u64, PuppetError> {
        self.eval_value(page, expression)
            .await?
            .as_u64()
            .ok_or_else(browser_protocol)
    }

    async fn wait_bool(&mut self, page: &BrowserPage, expression: &str) -> Result<(), PuppetError> {
        let started = Instant::now();
        while started.elapsed() < CDP_TIMEOUT {
            if self.eval_value(page, expression).await?.as_bool() == Some(true) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(browser_protocol())
    }

    async fn set_input(
        &mut self,
        page: &BrowserPage,
        selector: &str,
        value: &str,
    ) -> Result<(), PuppetError> {
        let selector = js(selector)?;
        let value = js(value)?;
        self.eval_value(
            page,
            &format!(
                "(()=>{{const input=document.querySelector({selector});if(!input)return false;input.value={value};input.dispatchEvent(new Event('input',{{bubbles:true}}));input.dispatchEvent(new Event('change',{{bubbles:true}}));return true}})()"
            ),
        )
        .await?
        .as_bool()
        .filter(|value| *value)
        .map(|_| ())
        .ok_or_else(browser_protocol)
    }

    async fn click_selector(
        &mut self,
        page: &BrowserPage,
        selector: &str,
    ) -> Result<(), PuppetError> {
        let selector = js(selector)?;
        self.eval_value(
            page,
            &format!(
                "(()=>{{const element=document.querySelector({selector});if(!element)return false;element.click();return true}})()"
            ),
        )
        .await?
        .as_bool()
        .filter(|value| *value)
        .map(|_| ())
        .ok_or_else(browser_protocol)
    }

    async fn click_command(
        &mut self,
        page: &BrowserPage,
        command_id: &str,
    ) -> Result<(), PuppetError> {
        let selector = format!(".action-dock button[data-command-id={}]", js(command_id)?);
        let before = self.page_state(page).await?.revision;
        self.click_selector(page, &selector).await?;
        self.wait_after_revision(page, before).await
    }

    async fn next_game_action(
        &mut self,
        page: &BrowserPage,
    ) -> Result<Option<String>, PuppetError> {
        let value = self
            .eval_value(
                page,
                "[...document.querySelectorAll('.action-dock button[data-command-id]')].map(button=>button.textContent.trim()).find(label=>/^Bid \\d+ Trick/.test(label)||/^Play /.test(label)) ?? null",
            )
            .await?;
        Ok(value.as_str().map(str::to_owned))
    }

    async fn click_action_label(
        &mut self,
        page: &BrowserPage,
        label: &str,
    ) -> Result<(), PuppetError> {
        let label = js(label)?;
        self.eval_value(
            page,
            &format!(
                "(()=>{{const button=[...document.querySelectorAll('.action-dock button[data-command-id]')].find(candidate=>candidate.textContent.trim()==={label});if(!button)return false;button.click();return true}})()"
            ),
        )
        .await?
        .as_bool()
        .filter(|value| *value)
        .map(|_| ())
        .ok_or_else(browser_protocol)
    }

    async fn wait_path_prefix(
        &mut self,
        page: &BrowserPage,
        prefix: &str,
    ) -> Result<(), PuppetError> {
        self.wait_bool(
            page,
            &format!("location.pathname.startsWith({})", js(prefix)?),
        )
        .await
    }

    async fn wait_exact_path(&mut self, page: &BrowserPage, path: &str) -> Result<(), PuppetError> {
        self.wait_bool(page, &format!("location.pathname === {}", js(path)?))
            .await
    }

    async fn wait_revision(
        &mut self,
        page: &BrowserPage,
        revision: u64,
    ) -> Result<(), PuppetError> {
        self.wait_bool(
            page,
            &format!(
                "Number(document.querySelector('#game-shell')?.dataset.authorityRevision ?? -1) === {revision}"
            ),
        )
        .await
    }

    async fn wait_after_revision(
        &mut self,
        page: &BrowserPage,
        revision: u64,
    ) -> Result<(), PuppetError> {
        self.wait_at_least_revision(page, revision.saturating_add(1))
            .await
    }

    async fn wait_at_least_revision(
        &mut self,
        page: &BrowserPage,
        revision: u64,
    ) -> Result<(), PuppetError> {
        self.wait_bool(
            page,
            &format!(
                "Number(document.querySelector('#game-shell')?.dataset.authorityRevision ?? -1) >= {revision}"
            ),
        )
        .await
    }

    async fn page_state(&mut self, page: &BrowserPage) -> Result<PageState, PuppetError> {
        let value = self
            .eval_value(
                page,
                "(()=>{const root=document.querySelector('#game-shell');const phase=document.querySelector('.turn-banner > span')?.textContent?.split('·')[0]?.trim()??'';return {revision:Number(root?.dataset.authorityRevision??0),roomPhase:root?.dataset.roomPhase??'',gamePhase:phase,playedCards:document.querySelectorAll('.trick-zone .table-card').length,completedRounds:document.querySelectorAll('.score-sheet tbody tr:not(.active-round)').length,visibleTricks:[...document.querySelectorAll('.table-player small')].filter(node=>/[1-9]\\d* tricks/.test(node.textContent)).length,projectionHash:root?.dataset.projectionHash??'',sceneHash:root?.dataset.sceneHash??''}})()",
            )
            .await?;
        let wire: PageStateWire = serde_json::from_value(value).map_err(|_| browser_protocol())?;
        Ok(PageState {
            revision: wire.revision,
            room_phase: wire.room_phase,
            game_phase: wire.game_phase,
            played_cards: wire.played_cards,
            completed_rounds: wire.completed_rounds,
            visible_tricks: wire.visible_tricks,
            projection_hash: parse_hash(&wire.projection_hash)?,
            scene_hash: parse_hash(&wire.scene_hash)?,
        })
    }

    async fn set_viewport(
        &mut self,
        page: &BrowserPage,
        width: u32,
        height: u32,
    ) -> Result<(), PuppetError> {
        self.command(
            Some(page),
            "Emulation.setDeviceMetricsOverride",
            json!({"width":width,"height":height,"deviceScaleFactor":1,"mobile":false}),
        )
        .await?;
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one atomic browser checkpoint collects and binds the four required renderer representations plus both layout viewports"
    )]
    async fn capture_bundle(
        &mut self,
        page: &BrowserPage,
        seed: u64,
        label: &str,
        state: &PageState,
    ) -> Result<RawCaptureBundle, PuppetError> {
        self.set_viewport(page, WIDE_WIDTH, WIDE_HEIGHT).await?;
        tokio::time::sleep(Duration::from_millis(40)).await;
        let screenshot = self
            .command(
                Some(page),
                "Page.captureScreenshot",
                json!({"format":"png","captureBeyondViewport":false,"fromSurface":true}),
            )
            .await?["data"]
            .as_str()
            .ok_or_else(browser_protocol)
            .and_then(|encoded| BASE64.decode(encoded).map_err(|_| browser_protocol()))?;
        let html = self
            .eval_string(
                page,
                "document.querySelector('#game-shell')?.outerHTML ?? ''",
            )
            .await?
            .into_bytes();
        let accessibility = self
            .command(Some(page), "Accessibility.getFullAXTree", json!({}))
            .await?;
        let wide = self.layout_snapshot(page).await?;
        self.set_viewport(page, NARROW_WIDTH, NARROW_HEIGHT).await?;
        tokio::time::sleep(Duration::from_millis(40)).await;
        let narrow = self.layout_snapshot(page).await?;
        self.set_viewport(page, WIDE_WIDTH, WIDE_HEIGHT).await?;
        if wide["collisionCount"].as_u64().unwrap_or(1) != 0
            || narrow["collisionCount"].as_u64().unwrap_or(1) != 0
        {
            eprintln!(
                "poche browser layout collision label={label} revision={} wide={} narrow={} wide_ids={} narrow_ids={}",
                state.revision,
                wide["collisionCount"],
                narrow["collisionCount"],
                wide["collisions"],
                narrow["collisions"],
            );
            return Err(browser_protocol());
        }
        let layout = serde_json::to_vec_pretty(&json!({
            "schema":"poche.browser-layout-evidence.v1",
            "wide":wide,
            "narrow":narrow,
            "consoleErrors":self.diagnostics.console_errors,
            "networkFailures":self.diagnostics.network_failures,
        }))
        .map_err(|_| browser_protocol())?;
        let accessibility =
            serde_json::to_vec_pretty(&accessibility).map_err(|_| browser_protocol())?;
        let token = format!("browser-{seed}-{label}-{}", state.revision);
        let figure = artifact_token(&token);
        let artifact = |suffix: &str,
                        representation: CaptureRepresentationWire,
                        media_type: &str,
                        bytes: Vec<u8>|
         -> Result<RawCaptureArtifact, PuppetError> {
            Ok(RawCaptureArtifact {
                artifact_id: CaptureArtifactId::new(format!("{figure}-{suffix}"))
                    .map_err(|_| browser_protocol())?,
                representation,
                media_type: media_type.to_owned(),
                expected_source_hash: Some(SemanticHash(*blake3::hash(&bytes).as_bytes())),
                bytes,
            })
        };
        let artifacts = vec![
            artifact(
                "png",
                CaptureRepresentationWire::Png,
                "image/png",
                screenshot,
            )?,
            artifact(
                "html",
                CaptureRepresentationWire::SemanticHtml,
                "text/html; charset=utf-8",
                html,
            )?,
            artifact(
                "accessibility",
                CaptureRepresentationWire::AccessibilityTreeJson,
                "application/json",
                accessibility,
            )?,
            artifact(
                "layout",
                CaptureRepresentationWire::LayoutJson,
                "application/json",
                layout,
            )?,
        ];
        Ok(RawCaptureBundle {
            figure_id: figure,
            caption: format!("{label} browser player view"),
            captured_revision: state.revision,
            projection_hash: state.projection_hash,
            scene_hash: Some(state.scene_hash),
            surface: CaptureSurfaceMetadata {
                provider_kind: CaptureProviderKindWire::BrowserHarness,
                viewport: CaptureViewportWire {
                    width_pixels: WIDE_WIDTH,
                    height_pixels: WIDE_HEIGHT,
                },
                framebuffer_width: WIDE_WIDTH,
                framebuffer_height: WIDE_HEIGHT,
                scale_milli: 1_000,
                camera: None,
            },
            qualification: CaptureQualification::BrowserHarness,
            cancelled: false,
            artifacts,
        })
    }

    async fn layout_snapshot(&mut self, page: &BrowserPage) -> Result<Value, PuppetError> {
        self.eval_value(
            page,
            "(()=>{const root=document.querySelector('#game-shell');const nodes=[...document.querySelectorAll('.table-surface [data-layout-footprint],.action-dock button')].filter(node=>{const r=node.getBoundingClientRect();return r.width>0&&r.height>0}).map(node=>{const r=node.getBoundingClientRect();return {id:node.dataset.layoutFootprint??node.dataset.commandId??node.textContent.trim(),left:Math.round(r.left),top:Math.round(r.top),right:Math.round(r.right),bottom:Math.round(r.bottom),width:Math.round(r.width),height:Math.round(r.height)}});return {viewport:{width:innerWidth,height:innerHeight,devicePixelRatio},document:{width:document.documentElement.scrollWidth,height:document.documentElement.scrollHeight},authorityRevision:Number(root?.dataset.authorityRevision??0),projectionHash:root?.dataset.projectionHash??'',sceneHash:root?.dataset.sceneHash??'',collisionCount:Number(document.querySelector('.table-surface')?.dataset.layoutCollisionCount??0),collisions:(document.querySelector('.table-surface')?.dataset.layoutCollisions??'').split(',').filter(Boolean),elements:nodes}})()",
        )
        .await
    }

    async fn send_chat(
        &mut self,
        sender: &BrowserPage,
        receiver: &BrowserPage,
        text: &str,
    ) -> Result<bool, PuppetError> {
        self.eval_value(
            sender,
            "(()=>{const panel=document.querySelector('.chat-panel');if(!panel)return false;panel.open=true;return true})()",
        )
        .await?;
        self.set_input(sender, ".chat-composer input[name=text]", text)
            .await?;
        self.click_selector(sender, ".chat-composer button[type=submit]")
            .await?;
        self.wait_bool(
            receiver,
            &format!("document.body.textContent.includes({})", js(text)?),
        )
        .await?;
        Ok(true)
    }

    async fn disconnect_resume_reconnect(
        &mut self,
        page: &BrowserPage,
    ) -> Result<bool, PuppetError> {
        self.click_selector(page, ".action-dock button[data-exit-table]")
            .await?;
        self.wait_exact_path(page, "/").await?;
        self.click_selector(page, "#resume-table").await?;
        self.wait_path_prefix(page, "/game/").await?;
        self.wait_bool(
            page,
            "document.querySelector('.action-dock button[data-command-id=reconnect]') !== null",
        )
        .await?;
        self.click_command(page, "reconnect").await?;
        self.wait_bool(
            page,
            "document.querySelector('.authority-chip small')?.textContent.includes('Connected') === true",
        )
        .await?;
        Ok(true)
    }
}

struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct BrowserGuard(Child);

impl Drop for BrowserGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn launch_browser(profile: &Path) -> Result<Child, PuppetError> {
    let executable = browser_executable().ok_or_else(browser_unavailable)?;
    let mut command = Command::new(executable);
    command
        .arg("--headless=new")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--metrics-recording-only",
            "--mute-audio",
            "about:blank",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map_err(|_| browser_unavailable())
}

fn browser_executable() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("POCHE_BROWSER_EXECUTABLE") {
        let configured = PathBuf::from(configured);
        if configured.is_file() {
            return Some(configured);
        }
    }
    [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        "/usr/bin/microsoft-edge",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|candidate| candidate.is_file())
}

async fn wait_for_devtools(profile: &Path) -> Result<String, PuppetError> {
    let marker = profile.join("DevToolsActivePort");
    let started = Instant::now();
    while started.elapsed() < CDP_TIMEOUT {
        if let Ok(contents) = std::fs::read_to_string(&marker) {
            let mut lines = contents.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next())
                && port.parse::<u16>().is_ok()
                && path.starts_with("/devtools/browser/")
            {
                return Ok(format!("ws://127.0.0.1:{port}{path}"));
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(browser_unavailable())
}

fn parse_hash(value: &str) -> Result<SemanticHash, PuppetError> {
    if value.len() != 64 {
        return Err(browser_protocol());
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| browser_protocol())?;
    }
    Ok(SemanticHash(bytes))
}

fn artifact_token(value: &str) -> String {
    let hash = blake3::hash(value.as_bytes());
    hash.as_bytes()[..8]
        .iter()
        .fold(String::from("browser-"), |mut output, byte| {
            use std::fmt::Write as _;

            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

fn js(value: &str) -> Result<String, PuppetError> {
    serde_json::to_string(value).map_err(|_| browser_protocol())
}

const fn browser_unavailable() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::BrowserUnavailable,
        "a supported headless browser could not be launched",
    )
}

const fn browser_protocol() -> PuppetError {
    browser_contract("the browser puppet violated its bounded control or evidence contract")
}

const fn browser_contract(message: &'static str) -> PuppetError {
    PuppetError::new(PuppetErrorCode::DeviceProtocol, message)
}
