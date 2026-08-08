// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{collections::BTreeMap, fmt::Write};

use poche_protocol::{
    ChanceWire, CommandPayload, CountdownToken, InviteProof, MemberProjection, PrincipalId,
    ProjectionPayload, ProtocolFrame, RoomId, RoomPhase, encode_frame_line,
};
use poche_runtime::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    OracleSessionGame, ScriptedClient,
};
use poche_session::{
    ConnectionState, GameTurn, InviteRecord, SessionGame, SessionPhase, SessionState,
};
use poche_ui::{
    ChatPresentation, ConnectionPresentation, CountdownPresentation, HandGrantPresentation,
    HandRequestPresentation, LiveClientInput, LiveClientPresentation, NoticePresentation,
    PresentationInput, PresentationModel, RoomInvitePresentation,
    render_semantic_html_with_root_id,
};

const ALICE: &str = "alice";
const BOB: &str = "bob";
const SPECTATOR: &str = "spectator";
const GAME: &str = "system-game";
const CLOCK: &str = "system-clock";
const ROOM: &str = "datastar-live-room";
const ALICE_CODE: &str = "POCHE-LAB-ALICE";
const BOB_CODE: &str = "POCHE-LAB-BOB";
const SPECTATOR_CODE: &str = "POCHE-LAB-SPECTATOR";
/// Stateful local authority used to exercise every live-client surface.
pub struct LiveDemo {
    authority: InProcessAuthority<OracleSessionGame<2>>,
    authority_surface: String,
    authority_incarnation: u64,
    clients: BTreeMap<String, ScriptedClient>,
    display_names: BTreeMap<String, String>,
    latest: BTreeMap<String, ProjectionPayload>,
    projection_history: BTreeMap<String, Vec<ProjectionPayload>>,
    frame_history: BTreeMap<String, Vec<String>>,
    notices: BTreeMap<String, Vec<NoticePresentation>>,
    next_command: u64,
}

impl LiveDemo {
    /// Construct an independently named authority surface for diagnostics.
    pub fn named(surface: &str) -> Result<Self, String> {
        Self::named_at(surface, 0)
    }

    fn named_at(surface: &str, incarnation: u64) -> Result<Self, String> {
        let mut state = SessionState::pending(room(ROOM)?, principal(CLOCK)?, principal(GAME)?);
        for code in [ALICE_CODE, BOB_CODE, SPECTATOR_CODE] {
            state
                .invites
                .push(InviteRecord::new(code, u64::MAX).map_err(debug_error)?);
        }
        let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let mut clients = BTreeMap::new();
        for name in [ALICE, BOB, SPECTATOR, GAME] {
            let client = transport.connect(principal(name)?).map_err(debug_error)?;
            clients.insert(name.to_owned(), client);
        }
        let display_names = [
            (ALICE.to_owned(), "Alice".to_owned()),
            (BOB.to_owned(), "Bob".to_owned()),
            (SPECTATOR.to_owned(), "Spectator".to_owned()),
        ]
        .into_iter()
        .collect();
        Ok(Self {
            authority: InProcessAuthority::new(state, transport),
            authority_surface: surface.to_owned(),
            authority_incarnation: incarnation,
            clients,
            display_names,
            latest: BTreeMap::new(),
            projection_history: BTreeMap::new(),
            frame_history: BTreeMap::new(),
            notices: BTreeMap::new(),
            next_command: 0,
        })
    }

    /// Construct an empty player-facing room. Peers are registered dynamically.
    pub fn dynamic(surface: &str) -> Result<Self, String> {
        let state = SessionState::pending(room(ROOM)?, principal(CLOCK)?, principal(GAME)?);
        let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let game = transport.connect(principal(GAME)?).map_err(debug_error)?;
        Ok(Self {
            authority: InProcessAuthority::new(state, transport),
            authority_surface: surface.to_owned(),
            authority_incarnation: 0,
            clients: [(GAME.to_owned(), game)].into_iter().collect(),
            display_names: BTreeMap::new(),
            latest: BTreeMap::new(),
            projection_history: BTreeMap::new(),
            frame_history: BTreeMap::new(),
            notices: BTreeMap::new(),
            next_command: 0,
        })
    }

    /// Register a new opaque principal and human-facing player name.
    pub fn register_peer(&mut self, viewer: &str, display_name: &str) -> Result<(), String> {
        if self.clients.contains_key(viewer) || matches!(viewer, GAME | CLOCK) {
            return Err("player principal is already registered".to_owned());
        }
        let client = self
            .authority
            .transport
            .connect(principal(viewer)?)
            .map_err(debug_error)?;
        self.clients.insert(viewer.to_owned(), client);
        self.display_names
            .insert(viewer.to_owned(), display_name.to_owned());
        Ok(())
    }

    /// Create this room as a dynamically registered peer.
    pub fn create_room_as(&mut self, viewer: &str) -> Result<String, String> {
        self.submit_payload(viewer, CommandPayload::CreateRoom, false)
    }

    /// Mint and consume a one-use reducer invite behind an adapter-level room code.
    pub fn join_room_as(&mut self, viewer: &str, invite: &str) -> Result<String, String> {
        self.authority
            .state
            .invites
            .push(InviteRecord::new(invite, u64::MAX).map_err(debug_error)?);
        self.submit_payload(
            viewer,
            CommandPayload::RedeemInvite {
                invite: InviteProof::new(invite).map_err(debug_error)?,
            },
            false,
        )
    }

    /// Build the exact current live presentation for one browser identity.
    pub fn view(&self, viewer: &str) -> Result<LiveClientPresentation, String> {
        if !self.clients.contains_key(viewer) || viewer == GAME {
            return Err("unknown live demo viewer".to_owned());
        }
        let viewer_id = principal(viewer)?;
        let member = self.authority.state.member(&viewer_id);
        let mut payload = self
            .latest
            .get(viewer)
            .cloned()
            .unwrap_or_else(|| self.synthetic_lobby_projection());
        if let Some(projected_viewer) = payload
            .members
            .iter_mut()
            .find(|candidate| candidate.principal_id == viewer_id)
            && let Some(current) = member
        {
            projected_viewer.connected = current.connection == ConnectionState::Connected;
        }
        let connection = match member.map(|member| member.connection) {
            Some(ConnectionState::Disconnected) => ConnectionPresentation::Reconnecting,
            Some(ConnectionState::Connected) | None => ConnectionPresentation::Connected,
        };
        let countdown = match &self.authority.state.phase {
            SessionPhase::Countdown { deadline_tick, .. } => Some(CountdownPresentation {
                logical_now: self.authority.clock.now(),
                deadline_tick: *deadline_tick,
            }),
            _ => None,
        };
        let legal_actions = self.legal_actions(&viewer_id);
        let chat = self
            .authority
            .chat_tail()
            .entries()
            .map(|entry| ChatPresentation {
                principal: self.display_name(entry.principal_id.as_str()).to_owned(),
                text: entry.text.clone(),
            })
            .collect();
        let mut presentation = PresentationModel::from_input(PresentationInput {
            viewer: viewer.to_owned(),
            projection: payload,
            legal_actions,
            connection,
            countdown,
            chat,
            notices: self.notices.get(viewer).cloned().unwrap_or_default(),
        });
        self.decorate_names(&mut presentation);
        let involved = member.is_some_and(|member| member.connection == ConnectionState::Connected);
        let (hand_requests, hand_grants) = self.hand_access(&viewer_id, involved);
        let (room_invites, join_proof) = room_access(
            viewer,
            member.is_some(),
            member.is_some_and(|member| member.host),
            self.authority.state.host.is_some(),
        )?;
        let countdown_token = CountdownToken::new(format!("ui-countdown-{}", self.next_command))
            .map_err(|_| "invalid countdown token".to_owned())?;
        Ok(LiveClientPresentation::from_input(
            presentation,
            LiveClientInput {
                room_id: ROOM.to_owned(),
                authority_instance: self.authority_instance(),
                authority_revision: self.authority.state.revision,
                room_invites,
                join_proof,
                seat_count: 2,
                chat_draft: Some(format!("hello from {viewer}")),
                countdown_command: Some((
                    self.authority.clock.now().saturating_add(3),
                    countdown_token,
                )),
                next_grant_epoch: self.authority.state.projection_epoch.saturating_add(1),
                hand_requests,
                hand_grants,
                transcript_href: Some(format!("/live/{viewer}/transcript.ndjson")),
                replay_href: Some(format!("/live/{viewer}/replay")),
            },
        ))
    }

    fn display_name<'a>(&'a self, principal: &'a str) -> &'a str {
        self.display_names
            .get(principal)
            .map_or(principal, String::as_str)
    }

    fn decorate_names(&self, presentation: &mut PresentationModel) {
        presentation.viewer_display_name = self.display_name(&presentation.viewer).to_owned();
        for member in &mut presentation.members {
            member.display_name = self.display_name(&member.principal).to_owned();
        }
    }

    fn hand_access(
        &self,
        viewer: &PrincipalId,
        involved: bool,
    ) -> (Vec<HandRequestPresentation>, Vec<HandGrantPresentation>) {
        if !involved {
            return (Vec::new(), Vec::new());
        }
        let requests = self
            .authority
            .state
            .hand_requests
            .iter()
            .filter(|request| request.player == *viewer || request.recipient == *viewer)
            .map(|request| HandRequestPresentation {
                request_id: request.request_id.clone(),
                player: request.player.clone(),
                recipient: request.recipient.clone(),
            })
            .collect();
        let grants = self
            .authority
            .state
            .hand_grants
            .iter()
            .filter(|grant| grant.player == *viewer || grant.recipient == *viewer)
            .map(|grant| HandGrantPresentation {
                player: grant.player.clone(),
                recipient: grant.recipient.clone(),
                grant_epoch: grant.grant_epoch,
            })
            .collect();
        (requests, grants)
    }

    /// Resolve an opaque UI ID and submit its retained typed payload.
    pub fn control(&mut self, viewer: &str, control_id: &str) -> Result<String, String> {
        let Some(payload) = self.view(viewer)?.command(control_id) else {
            self.push_notice(
                viewer,
                "STALE-CONTROL",
                "That action is no longer available. The room changed in another client. This view is now up to date.",
            );
            println!(
                "poche-web-spike event=stale-control authority={} viewer={viewer} control={control_id} revision={}",
                self.authority_instance(),
                self.authority.state.revision,
            );
            return Err("unknown or stale UI control".to_owned());
        };
        self.submit_payload(viewer, payload, true)
    }

    /// Submit a typed payload supplied by a separately authenticated gateway
    /// device. Authentication and replay protection remain the gateway
    /// adapter's responsibility; this method crosses only the semantic bridge.
    pub fn submit_gateway_payload(
        &mut self,
        viewer: &str,
        payload: CommandPayload,
    ) -> Result<String, String> {
        self.submit_payload(viewer, payload, true)
    }

    /// Reset and prepare a deterministic lobby, countdown, or running game.
    pub fn setup(&mut self, stage: &str) -> Result<String, String> {
        let next_incarnation = self.authority_incarnation.saturating_add(1);
        *self = Self::named_at(&self.authority_surface.clone(), next_incarnation)?;
        println!(
            "poche-web-spike event=scenario-reset authority={} stage={stage} revision=0",
            self.authority_instance()
        );
        if stage == "pending" {
            return Ok("reset to pending".to_owned());
        }
        self.submit_payload(ALICE, CommandPayload::CreateRoom, false)?;
        for (viewer, code) in [(BOB, BOB_CODE), (SPECTATOR, SPECTATOR_CODE)] {
            self.submit_payload(
                viewer,
                CommandPayload::RedeemInvite {
                    invite: InviteProof::new(code).map_err(|_| "invalid demo invite".to_owned())?,
                },
                false,
            )?;
        }
        self.submit_payload(ALICE, CommandPayload::TakeSeat { seat: 0 }, false)?;
        self.submit_payload(BOB, CommandPayload::TakeSeat { seat: 1 }, false)?;
        if stage == "lobby" {
            return Ok("prepared lobby".to_owned());
        }
        self.submit_payload(ALICE, CommandPayload::Ready, false)?;
        self.submit_payload(BOB, CommandPayload::Ready, false)?;
        let token = CountdownToken::new("demo-countdown")
            .map_err(|_| "invalid demo countdown".to_owned())?;
        self.submit_payload(
            ALICE,
            CommandPayload::ArmCountdown {
                deadline_tick: 3,
                countdown_token: token,
            },
            false,
        )?;
        if stage == "countdown" {
            return Ok("prepared countdown".to_owned());
        }
        if stage != "running" && stage != "playing" {
            return Err("unknown demo setup stage".to_owned());
        }
        self.advance_clock()?;
        if stage == "playing" {
            self.advance_to_first_play()?;
            return Ok("prepared first playable trick".to_owned());
        }
        Ok("prepared running game".to_owned())
    }

    fn advance_to_first_play(&mut self) -> Result<(), String> {
        for _ in 0..16 {
            self.drive_environment()?;
            let (actor, action) = match &self.authority.state.phase {
                SessionPhase::Running { game } => match game.turn() {
                    GameTurn::Player(seat) => {
                        let actions = game.legal_player_actions();
                        if actions.iter().any(|action| {
                            matches!(action, poche_protocol::GameActionWire::Play { .. })
                        }) {
                            return Ok(());
                        }
                        let actor = match seat {
                            0 => ALICE,
                            1 => BOB,
                            _ => {
                                return Err("demo actor is outside the two-seat fixture".to_owned());
                            }
                        };
                        let action = actions
                            .first()
                            .cloned()
                            .ok_or_else(|| "demo bidding actor has no legal action".to_owned())?;
                        (actor, action)
                    }
                    GameTurn::Chance | GameTurn::Environment => continue,
                    GameTurn::Finished => {
                        return Err("demo finished before the first play".to_owned());
                    }
                },
                _ => return Err("demo left running phase before the first play".to_owned()),
            };
            self.submit_payload(actor, CommandPayload::GameAction { action }, false)?;
        }
        Err("demo did not reach a playable trick within the bounded setup".to_owned())
    }

    /// Advance the authority clock far enough to expire the current countdown.
    pub fn advance_clock(&mut self) -> Result<String, String> {
        let tick = self.authority.clock.now().saturating_add(10);
        let outcomes = self.authority.advance_clock_to(tick).map_err(debug_error)?;
        self.drain_all()?;
        self.drive_environment()?;
        Ok(format!(
            "advanced authority clock to {tick}; outcomes {}",
            outcomes.len()
        ))
    }

    /// Advance one logical second only while a countdown is active.
    pub fn tick_countdown(&mut self) -> Result<bool, String> {
        if !matches!(self.authority.state.phase, SessionPhase::Countdown { .. }) {
            return Ok(false);
        }
        let tick = self.authority.clock.now().saturating_add(1);
        self.authority.advance_clock_to(tick).map_err(debug_error)?;
        self.drain_all()?;
        self.drive_environment()?;
        println!(
            "poche-web-spike event=countdown-tick authority={} tick={tick} revision={}",
            self.authority_instance(),
            self.authority.state.revision,
        );
        Ok(true)
    }

    /// Simulate loss, then bind a fresh transport so the typed reconnect can run.
    pub fn disconnect(&mut self, viewer: &str) -> Result<String, String> {
        if !self.clients.contains_key(viewer) || viewer == GAME {
            return Err("unknown live demo viewer".to_owned());
        }
        let old = self
            .clients
            .get(viewer)
            .cloned()
            .ok_or_else(|| "missing demo client".to_owned())?;
        self.authority
            .transport
            .disconnect(old.connection_id())
            .map_err(debug_error)?;
        self.authority.drive_all().map_err(debug_error)?;
        self.drain_all()?;
        let replacement = self
            .authority
            .transport
            .connect(principal(viewer)?)
            .map_err(debug_error)?;
        self.clients.insert(viewer.to_owned(), replacement);
        self.push_notice(
            viewer,
            "TRANSPORT",
            "transport lost; reconnect is available",
        );
        println!(
            "poche-web-spike event=transport-lost authority={} viewer={viewer} revision={}",
            self.authority_instance(),
            self.authority.state.revision
        );
        Ok(format!("disconnected {viewer}"))
    }

    /// Canonical exact-recipient projection/error stream; it contains no invite.
    #[must_use]
    pub fn transcript(&self, viewer: &str) -> Option<String> {
        self.frame_history.get(viewer).map(|lines| lines.concat())
    }

    /// Deterministically render every exact projection retained for one viewer.
    pub fn replay_html(&self, viewer: &str) -> Result<String, String> {
        if !self.clients.contains_key(viewer) || viewer == GAME {
            return Err("unknown live demo viewer".to_owned());
        }
        let mut html = String::from(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Poche exact projection replay</title></head><body><h1>Exact projection replay</h1>",
        );
        for (index, projection) in self
            .projection_history
            .get(viewer)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let _ = write!(html, "<h2>Checkpoint {}</h2>", index + 1);
            let mut model = PresentationModel::from_input(PresentationInput {
                viewer: viewer.to_owned(),
                projection: projection.clone(),
                legal_actions: Vec::new(),
                connection: ConnectionPresentation::Replay,
                countdown: None,
                chat: Vec::new(),
                notices: Vec::new(),
            });
            self.decorate_names(&mut model);
            html.push_str(&render_semantic_html_with_root_id(
                &model,
                &format!("checkpoint-{index}"),
            ));
        }
        html.push_str("</body></html>");
        Ok(html)
    }

    fn submit_payload(
        &mut self,
        viewer: &str,
        payload: CommandPayload,
        drive_environment: bool,
    ) -> Result<String, String> {
        let client = self
            .clients
            .get(viewer)
            .cloned()
            .ok_or_else(|| "missing demo client".to_owned())?;
        let command_id = format!("ui-{viewer}-{}", self.next_command);
        self.next_command = self.next_command.saturating_add(1);
        let command = client
            .command(&self.authority.state, &command_id, payload)
            .map_err(debug_error)?;
        client
            .submit(&mut self.authority.transport, command)
            .map_err(debug_error)?;
        let outcomes = self.authority.drive_all().map_err(debug_error)?;
        self.drain_all()?;
        let outcome = outcomes
            .last()
            .ok_or_else(|| "authority produced no outcome".to_owned())?;
        let status = match outcome.disposition {
            AuthorityDisposition::Applied => format!(
                "applied; revision {}; events {}",
                outcome.revision, outcome.events
            ),
            AuthorityDisposition::Denied(reason) => {
                format!(
                    "denied {}; revision {}",
                    deny_code(reason),
                    outcome.revision
                )
            }
            AuthorityDisposition::Disconnected => {
                format!("disconnected; revision {}", outcome.revision)
            }
        };
        println!(
            "poche-web-spike event=authority-decision authority={} viewer={viewer} command={command_id} outcome=\"{status}\"",
            self.authority_instance()
        );
        self.push_notice(viewer, "COMMAND", &status);
        if drive_environment {
            self.drive_environment()?;
        }
        Ok(status)
    }

    fn drive_environment(&mut self) -> Result<(), String> {
        for _ in 0..8 {
            let turn = match &self.authority.state.phase {
                SessionPhase::Running { game } => game.turn(),
                _ => break,
            };
            match turn {
                GameTurn::Chance => {
                    self.submit_payload(
                        GAME,
                        CommandPayload::ApplyChance {
                            chance: ChanceWire {
                                cards: (0_u8..52).collect(),
                                seed: None,
                                deal_ordinal: None,
                            },
                        },
                        false,
                    )?;
                }
                GameTurn::Environment => {
                    self.submit_payload(GAME, CommandPayload::Settle, false)?;
                }
                GameTurn::Player(_) | GameTurn::Finished => break,
            }
        }
        Ok(())
    }

    fn drain_all(&mut self) -> Result<(), String> {
        let clients = self
            .clients
            .iter()
            .filter(|(name, _)| name.as_str() != GAME)
            .map(|(name, client)| (name.clone(), client.clone()))
            .collect::<Vec<_>>();
        for (viewer, client) in clients {
            while let Some(frame) = client
                .receive(&mut self.authority.transport)
                .map_err(debug_error)?
            {
                let line = String::from_utf8(encode_frame_line(&frame).map_err(debug_error)?)
                    .map_err(|_| "protocol frame was not UTF-8".to_owned())?;
                self.frame_history
                    .entry(viewer.clone())
                    .or_default()
                    .push(line);
                match frame {
                    ProtocolFrame::Projection(envelope) => {
                        self.projection_history
                            .entry(viewer.clone())
                            .or_default()
                            .push(envelope.payload.clone());
                        self.latest.insert(viewer.clone(), envelope.payload);
                    }
                    ProtocolFrame::Error(envelope) => self.push_notice(
                        &viewer,
                        deny_code(envelope.payload.reason),
                        "authority denied the typed command",
                    ),
                    ProtocolFrame::Command(_)
                    | ProtocolFrame::Event(_)
                    | ProtocolFrame::Snapshot(_) => {}
                }
            }
        }
        Ok(())
    }

    fn legal_actions(&self, viewer: &PrincipalId) -> Vec<poche_protocol::GameActionWire> {
        let Some(member) = self.authority.state.member(viewer) else {
            return Vec::new();
        };
        let SessionPhase::Running { game } = &self.authority.state.phase else {
            return Vec::new();
        };
        match (member.seat, game.turn()) {
            (Some(viewer_seat), GameTurn::Player(actor)) if viewer_seat == actor => {
                game.legal_player_actions()
            }
            _ => Vec::new(),
        }
    }

    fn synthetic_lobby_projection(&self) -> ProjectionPayload {
        ProjectionPayload {
            phase: session_room_phase(&self.authority.state.phase),
            members: self
                .authority
                .state
                .members
                .iter()
                .map(|member| MemberProjection {
                    principal_id: member.principal_id.clone(),
                    connected: member.connection == ConnectionState::Connected,
                    seat: member.seat,
                    ready: member.ready,
                    host: member.host,
                })
                .collect(),
            public_game_state: None,
            own_hand: None,
            granted_hands: Vec::new(),
            public_history: self.authority.state.public_history.clone(),
        }
    }

    fn push_notice(&mut self, viewer: &str, reason_code: &str, message: &str) {
        let notices = self.notices.entry(viewer.to_owned()).or_default();
        notices.push(NoticePresentation {
            reason_code: reason_code.to_owned(),
            message: message.to_owned(),
        });
        if notices.len() > 8 {
            notices.remove(0);
        }
    }

    fn authority_instance(&self) -> String {
        format!(
            "{}@{}#{}",
            ROOM, self.authority_surface, self.authority_incarnation
        )
    }
}

fn session_room_phase(phase: &SessionPhase<OracleSessionGame<2>>) -> RoomPhase {
    match phase {
        SessionPhase::Uninitialized | SessionPhase::Lobby => RoomPhase::Lobby,
        SessionPhase::Countdown { .. } => RoomPhase::Countdown,
        SessionPhase::Running { .. } => RoomPhase::Running,
        SessionPhase::Paused { .. } => RoomPhase::Paused,
        SessionPhase::PostGame { .. } => RoomPhase::PostGame,
        SessionPhase::Closed => RoomPhase::Closed,
    }
}

fn invite_for(viewer: &str) -> Option<&'static str> {
    match viewer {
        ALICE => Some(ALICE_CODE),
        BOB => Some(BOB_CODE),
        SPECTATOR => Some(SPECTATOR_CODE),
        _ => None,
    }
}

fn room_access(
    viewer: &str,
    is_member: bool,
    is_coordinator: bool,
    room_exists: bool,
) -> Result<(Vec<RoomInvitePresentation>, Option<InviteProof>), String> {
    let room_invites = if is_coordinator {
        [
            (ALICE, "Alice", ALICE_CODE),
            (BOB, "Bob", BOB_CODE),
            (SPECTATOR, "Spectator", SPECTATOR_CODE),
        ]
        .into_iter()
        .filter(|(candidate, _, _)| *candidate != viewer)
        .map(|(_, label, code)| room_invite(label, code))
        .collect()
    } else if !is_member && room_exists {
        invite_for(viewer)
            .map(|code| {
                let label = match viewer {
                    ALICE => "Alice",
                    BOB => "Bob",
                    SPECTATOR => "Spectator",
                    _ => "Peer",
                };
                vec![room_invite(label, code)]
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let join_proof = (!is_member && room_exists)
        .then(|| invite_for(viewer))
        .flatten()
        .map(InviteProof::new)
        .transpose()
        .map_err(|_| "invalid checked demo invite".to_owned())?;
    Ok((room_invites, join_proof))
}

fn room_invite(label: &str, code: &str) -> RoomInvitePresentation {
    RoomInvitePresentation {
        label: label.to_owned(),
        code: code.to_owned(),
    }
}

fn deny_code(reason: poche_protocol::DenyReason) -> &'static str {
    match reason {
        poche_protocol::DenyReason::Malformed => "D-MALFORMED",
        poche_protocol::DenyReason::Oversize => "D-OVERSIZE",
        poche_protocol::DenyReason::UnknownVersion => "D-UNKNOWN-VERSION",
        poche_protocol::DenyReason::UnknownCommand => "D-UNKNOWN-COMMAND",
        poche_protocol::DenyReason::UnknownRole => "D-UNKNOWN-ROLE",
        poche_protocol::DenyReason::UnknownPrincipal => "D-UNKNOWN-PRINCIPAL",
        poche_protocol::DenyReason::BadSignature => "D-BAD-SIGNATURE",
        poche_protocol::DenyReason::WrongRoom => "D-WRONG-ROOM",
        poche_protocol::DenyReason::StaleEpoch => "D-STALE-EPOCH",
        poche_protocol::DenyReason::StaleRevision => "D-STALE-REVISION",
        poche_protocol::DenyReason::Revoked => "D-REVOKED",
        poche_protocol::DenyReason::MissingCapability => "D-MISSING-CAPABILITY",
        poche_protocol::DenyReason::DenyPolicy => "D-DENY-POLICY",
        poche_protocol::DenyReason::WrongPhase => "D-WRONG-PHASE",
        poche_protocol::DenyReason::Closed => "D-CLOSED",
        poche_protocol::DenyReason::NotSeated => "D-NOT-SEATED",
        poche_protocol::DenyReason::SeatOccupied => "D-SEAT-OCCUPIED",
        poche_protocol::DenyReason::AlreadySeated => "D-ALREADY-SEATED",
        poche_protocol::DenyReason::NotConnected => "D-NOT-CONNECTED",
        poche_protocol::DenyReason::NotReady => "D-NOT-READY",
        poche_protocol::DenyReason::CountdownInactive => "D-COUNTDOWN-INACTIVE",
        poche_protocol::DenyReason::NotActor => "D-NOT-ACTOR",
        poche_protocol::DenyReason::Paused => "D-PAUSED",
        poche_protocol::DenyReason::NotPaused => "D-NOT-PAUSED",
        poche_protocol::DenyReason::AlreadyPaused => "D-ALREADY-PAUSED",
        poche_protocol::DenyReason::InviteInvalid => "D-INVITE-INVALID",
        poche_protocol::DenyReason::InviteExpired => "D-INVITE-EXPIRED",
        poche_protocol::DenyReason::GrantScope => "D-GRANT-SCOPE",
        poche_protocol::DenyReason::ChatSize => "D-CHAT-SIZE",
        poche_protocol::DenyReason::ChatRate => "D-CHAT-RATE",
        poche_protocol::DenyReason::EnvironmentOnly => "D-ENVIRONMENT-ONLY",
    }
}

fn principal(value: &str) -> Result<PrincipalId, String> {
    PrincipalId::new(value).map_err(|_| format!("invalid principal {value}"))
}

fn room(value: &str) -> Result<RoomId, String> {
    RoomId::new(value).map_err(|_| format!("invalid room {value}"))
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use poche_protocol::{CommandPayload, ProtocolFrame, RoomPhase, decode_frame_line};
    use poche_ui::render_live_semantic_html;

    use super::{ALICE, BOB, LiveDemo, SPECTATOR};

    #[test]
    fn full_live_demo_uses_typed_controls_and_exact_projection_histories() {
        let mut demo = LiveDemo::named("test-live").expect("demo");
        let pending = demo.view(ALICE).expect("pending view");
        assert_eq!(pending.projection.members.len(), 0);
        assert_eq!(pending.authority_revision, 0);
        assert!(demo.view(ALICE).unwrap().command("create-room").is_some());
        assert!(demo.view(BOB).unwrap().command("create-room").is_some());
        demo.setup("running").expect("running setup");
        let alice = demo.view(ALICE).expect("alice view");
        let spectator = demo.view(SPECTATOR).expect("spectator view");
        assert_ne!(alice.authority_instance, pending.authority_instance);
        assert!(alice.authority_revision > pending.authority_revision);
        assert_eq!(alice.projection.room_phase, RoomPhase::Running);
        assert!(alice.projection.own_hand.is_some());
        assert!(spectator.projection.own_hand.is_none());
        assert!(spectator.projection.granted_hands.is_empty());
        assert!(spectator.command("request-hand-0").is_some());
        assert!(!demo.transcript(SPECTATOR).unwrap().contains("POCHE-LAB"));
    }

    #[test]
    fn pause_chat_grant_revoke_and_reconnect_cross_real_authority() {
        let mut demo = LiveDemo::named("test-live").expect("demo");
        demo.setup("running").expect("running setup");
        demo.control(ALICE, "pause").expect("pause");
        assert_eq!(
            demo.view(ALICE).unwrap().projection.room_phase,
            RoomPhase::Paused
        );
        demo.control(BOB, "unpause").expect("other player unpause");
        demo.control(SPECTATOR, "send-chat").expect("chat");
        demo.control(SPECTATOR, "request-hand-0").expect("request");
        demo.control(ALICE, "grant-hand-0").expect("grant");
        assert_eq!(
            demo.view(SPECTATOR).unwrap().projection.granted_hands.len(),
            1
        );
        demo.control(ALICE, "revoke-hand-0").expect("revoke");
        assert!(
            demo.view(SPECTATOR)
                .unwrap()
                .projection
                .granted_hands
                .is_empty()
        );
        demo.disconnect(BOB).expect("disconnect");
        assert!(demo.view(BOB).unwrap().command("reconnect").is_some());
        demo.control(BOB, "reconnect").expect("reconnect");
    }

    #[test]
    fn countdown_abort_and_authority_policy_are_independent_of_control_visibility() {
        let mut demo = LiveDemo::named("test-live").expect("demo");
        demo.setup("countdown").expect("countdown setup");
        demo.control(ALICE, "abort-countdown").expect("abort");
        assert_eq!(
            demo.view(ALICE).unwrap().projection.room_phase,
            RoomPhase::Lobby
        );

        demo.setup("running").expect("running setup");
        assert!(demo.view(SPECTATOR).unwrap().command("pause").is_none());
        let denial = demo
            .submit_payload(SPECTATOR, CommandPayload::Pause, false)
            .expect("authority returns a denial disposition");
        assert!(denial.starts_with("denied D-NOT-SEATED; revision "));
        assert_eq!(
            demo.view(ALICE).unwrap().projection.room_phase,
            RoomPhase::Running
        );
    }

    #[test]
    fn countdown_ticks_one_step_at_a_time_and_starts_the_game() {
        let mut demo = LiveDemo::named("countdown-tick-test").expect("demo");
        demo.setup("countdown").expect("countdown setup");
        assert_eq!(
            demo.view(ALICE)
                .unwrap()
                .projection
                .countdown
                .unwrap()
                .remaining(),
            3
        );
        assert!(demo.tick_countdown().expect("first tick"));
        assert_eq!(
            demo.view(ALICE)
                .unwrap()
                .projection
                .countdown
                .unwrap()
                .remaining(),
            2
        );
        assert!(demo.tick_countdown().expect("second tick"));
        assert_eq!(
            demo.view(ALICE)
                .unwrap()
                .projection
                .countdown
                .unwrap()
                .remaining(),
            1
        );
        assert!(demo.tick_countdown().expect("deadline tick"));
        assert_eq!(
            demo.view(ALICE).unwrap().projection.room_phase,
            RoomPhase::Running
        );
        assert!(!demo.tick_countdown().expect("running does not tick"));
    }

    #[test]
    fn stale_control_refreshes_the_view_and_records_visible_feedback() {
        let mut demo = LiveDemo::named("stale-control-test").expect("demo");
        demo.control(ALICE, "create-room").expect("create room");
        demo.control(BOB, "join-room").expect("join room");
        assert!(demo.view(BOB).unwrap().command("take-seat-0").is_some());

        demo.control(ALICE, "take-seat-0")
            .expect("coordinator takes seat");
        assert!(demo.control(BOB, "take-seat-0").is_err());
        let refreshed = demo.view(BOB).expect("refreshed Bob view");
        assert!(refreshed.command("take-seat-0").is_none());
        let notice = refreshed
            .projection
            .notices
            .last()
            .expect("stale control notice");
        assert_eq!(notice.reason_code, "STALE-CONTROL");
        assert!(notice.message.contains("view is now up to date"));
    }

    #[test]
    fn spectator_network_history_is_exact_before_during_and_after_a_grant() {
        let mut demo = LiveDemo::named("test-live").expect("demo");
        demo.setup("running").expect("running setup");
        assert_all_projection_frames_hide_hands(&demo, SPECTATOR);
        let ungranted = demo.view(SPECTATOR).expect("ungranted view");
        let ungranted_html =
            render_live_semantic_html(&ungranted, "live-client", "/live/spectator/command");
        assert!(!ungranted_html.contains("Your hand"));
        assert!(!ungranted_html.contains("Granted spectator view"));

        demo.control(SPECTATOR, "request-hand-0").expect("request");
        demo.control(ALICE, "grant-hand-0").expect("grant");
        let granted = demo.view(SPECTATOR).expect("granted view");
        let alice_hand = demo
            .view(ALICE)
            .expect("alice view")
            .projection
            .own_hand
            .expect("alice hand");
        assert_eq!(granted.projection.granted_hands[0].cards, alice_hand.cards);
        let authorized_history_end = demo.frame_history[SPECTATOR].len();

        demo.control(ALICE, "revoke-hand-0").expect("revoke");
        let revoked = demo.view(SPECTATOR).expect("revoked view");
        assert!(revoked.projection.granted_hands.is_empty());
        let revoked_html =
            render_live_semantic_html(&revoked, "live-client", "/live/spectator/command");
        assert!(!revoked_html.contains("Granted spectator view"));
        for line in &demo.frame_history[SPECTATOR][authorized_history_end..] {
            if let ProtocolFrame::Projection(envelope) =
                decode_frame_line(line.as_bytes()).expect("canonical frame")
            {
                assert!(envelope.payload.own_hand.is_none());
                assert!(envelope.payload.granted_hands.is_empty());
            }
        }
        assert!(!demo.transcript(SPECTATOR).unwrap().contains("POCHE-LAB"));
    }

    fn assert_all_projection_frames_hide_hands(demo: &LiveDemo, viewer: &str) {
        for line in &demo.frame_history[viewer] {
            if let ProtocolFrame::Projection(envelope) =
                decode_frame_line(line.as_bytes()).expect("canonical frame")
            {
                assert!(envelope.payload.own_hand.is_none());
                assert!(envelope.payload.granted_hands.is_empty());
            }
        }
    }
}
