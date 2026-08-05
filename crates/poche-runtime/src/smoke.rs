use std::collections::BTreeMap;

use poche_environment::{
    GameEnvironment, OracleChanceAction, OracleEnvironment, OraclePlayerAction,
};
use poche_oracle_rust::{Card, round_count};
use poche_protocol::{
    ChanceWire, CommandId, CommandPayload, CountdownToken, DenyReason, GameActionWire, InviteProof,
    PrincipalId, ProjectionPayload, RoomId,
};
use poche_session::{
    GameTurn, InviteRecord, ProjectionError, SessionGame, SessionPhase, SessionState,
    project_viewer,
};
use serde::{Deserialize, Serialize};

use crate::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    OracleSessionGame, ScriptedClient,
};

const PLAYER_INVITE: &str = "runtime-only-smoke-player-invite";
const SPECTATOR_INVITE: &str = "runtime-only-smoke-spectator-invite";
const TRANSCRIPT_SCHEMA_VERSION: u16 = 1;

/// Secret-free operation vocabulary for the deterministic loopback smoke run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SmokeOperation {
    CreateRoom,
    Join {
        invite_ref: String,
    },
    TakeSeat {
        seat: u8,
    },
    Ready,
    Unready,
    ArmCountdown {
        deadline_tick: u64,
        token: String,
    },
    AbortCountdown,
    AdvanceClock {
        tick: u64,
    },
    ApplySeededChance {
        deal_ordinal: u32,
    },
    Pause,
    Unpause,
    GameAction {
        action: GameActionWire,
    },
    Settle,
    Chat {
        text: String,
    },
    RequestHand {
        player: String,
    },
    GrantHand {
        request_id: String,
        player: String,
        recipient: String,
        grant_epoch: u64,
    },
    RevokeHand {
        player: String,
        recipient: String,
        grant_epoch: u64,
    },
    Disconnect,
    Reconnect,
    ResetLobby,
    CloseRoom,
}

/// Expected semantic disposition pinned beside each smoke input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmokeExpectation {
    Applied,
    DeniedPaused,
    Disconnected,
}

/// One exact, replayable smoke input without credentials or private state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeInput {
    pub actor: String,
    pub command_id: String,
    pub operation: SmokeOperation,
    pub expected: SmokeExpectation,
}

/// Normalized public result of one smoke input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeStepRecord {
    pub command_id: String,
    pub disposition: String,
    pub revision: u64,
    pub events: usize,
    pub phase: String,
    pub projection_hashes: BTreeMap<String, String>,
}

/// Generated transcript which can be replayed against a fresh authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeTranscript {
    pub schema_version: u16,
    pub seed: u64,
    pub inputs: Vec<SmokeInput>,
    pub records: Vec<SmokeStepRecord>,
}

/// Small stable acceptance summary suitable for checked evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InProcessSmokeReport {
    pub schema_version: u16,
    pub transport: String,
    pub seed: u64,
    pub inputs: usize,
    pub applied: usize,
    pub denied: usize,
    pub final_revision: u64,
    pub deals: u32,
    pub rounds: usize,
    pub final_scores: Vec<u16>,
    pub chat_messages: usize,
    pub verified: Vec<String>,
    pub transcript_hash: String,
    pub final_public_hash: String,
}

struct SmokeContext {
    authority: InProcessAuthority<OracleSessionGame<2>>,
    clients: BTreeMap<String, ScriptedClient>,
}

struct RecordedSmoke {
    transcript: SmokeTranscript,
    report: InProcessSmokeReport,
}

/// Record and independently replay the complete in-process acceptance scenario.
///
/// # Errors
///
/// Returns the first transport, reducer, projection, replay, or evidence error.
pub fn run_in_process_smoke(seed: u64) -> Result<InProcessSmokeReport, String> {
    let recorded = record_smoke(seed)?;
    let replayed = replay_smoke(&recorded.transcript)?;
    if replayed.transcript.records != recorded.transcript.records {
        return Err("smoke replay records diverged from the recorded transcript".to_owned());
    }
    let mut report = recorded.report;
    if replayed.report.final_public_hash != report.final_public_hash
        || replayed.report.final_scores != report.final_scores
        || replayed.report.final_revision != report.final_revision
    {
        return Err("smoke replay summary diverged from the recorded run".to_owned());
    }
    report.verified.push("transcript-replay".to_owned());
    Ok(report)
}

#[expect(
    clippy::too_many_lines,
    reason = "the acceptance transcript is intentionally chronological and inspectable"
)]
fn record_smoke(seed: u64) -> Result<RecordedSmoke, String> {
    let mut context = SmokeContext::new()?;
    let mut inputs = Vec::new();
    let mut records = Vec::new();
    let mut push = |context: &mut SmokeContext, input: SmokeInput| -> Result<(), String> {
        records.push(context.execute(&input, seed)?);
        inputs.push(input);
        Ok(())
    };
    let applied = |actor: &str, command_id: &str, operation| SmokeInput {
        actor: actor.to_owned(),
        command_id: command_id.to_owned(),
        operation,
        expected: SmokeExpectation::Applied,
    };

    push(
        &mut context,
        applied("host", "create", SmokeOperation::CreateRoom),
    )?;
    push(
        &mut context,
        applied(
            "player",
            "join-player",
            SmokeOperation::Join {
                invite_ref: "player".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied(
            "spectator",
            "join-spectator",
            SmokeOperation::Join {
                invite_ref: "spectator".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied(
            "spectator",
            "chat-lobby",
            SmokeOperation::Chat {
                text: "lobby hello".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied("host", "seat-host", SmokeOperation::TakeSeat { seat: 0 }),
    )?;
    push(
        &mut context,
        applied(
            "player",
            "seat-player",
            SmokeOperation::TakeSeat { seat: 1 },
        ),
    )?;
    push(
        &mut context,
        applied("host", "ready-host", SmokeOperation::Ready),
    )?;
    push(
        &mut context,
        applied("player", "ready-player", SmokeOperation::Ready),
    )?;
    push(
        &mut context,
        applied("player", "unready-player", SmokeOperation::Unready),
    )?;
    push(
        &mut context,
        applied("player", "reready-player", SmokeOperation::Ready),
    )?;
    push(
        &mut context,
        applied(
            "host",
            "arm-abort",
            SmokeOperation::ArmCountdown {
                deadline_tick: 10,
                token: "smoke-abort".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied("player", "abort-countdown", SmokeOperation::AbortCountdown),
    )?;
    push(
        &mut context,
        applied(
            "host",
            "arm-final",
            SmokeOperation::ArmCountdown {
                deadline_tick: 20,
                token: "smoke-final".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied(
            "player",
            "chat-countdown",
            SmokeOperation::Chat {
                text: "countdown hello".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied(
            "clock",
            "advance-start-clock",
            SmokeOperation::AdvanceClock { tick: 20 },
        ),
    )?;
    push(
        &mut context,
        applied(
            "game",
            "deal-0",
            SmokeOperation::ApplySeededChance { deal_ordinal: 0 },
        ),
    )?;
    push(
        &mut context,
        applied(
            "host",
            "chat-running",
            SmokeOperation::Chat {
                text: "running hello".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied("player", "pause", SmokeOperation::Pause),
    )?;
    push(
        &mut context,
        applied(
            "spectator",
            "chat-paused",
            SmokeOperation::Chat {
                text: "paused hello".to_owned(),
            },
        ),
    )?;
    let paused_action = first_legal_action(&context.authority.state)?;
    push(
        &mut context,
        SmokeInput {
            actor: "host".to_owned(),
            command_id: "action-while-paused".to_owned(),
            operation: SmokeOperation::GameAction {
                action: paused_action,
            },
            expected: SmokeExpectation::DeniedPaused,
        },
    )?;
    push(
        &mut context,
        applied("host", "unpause", SmokeOperation::Unpause),
    )?;
    push(
        &mut context,
        applied(
            "spectator",
            "request-host-hand",
            SmokeOperation::RequestHand {
                player: "host".to_owned(),
            },
        ),
    )?;
    let grant_epoch = context.authority.state.projection_epoch.saturating_add(1);
    push(
        &mut context,
        applied(
            "host",
            "grant-host-hand",
            SmokeOperation::GrantHand {
                request_id: "request-host-hand".to_owned(),
                player: "host".to_owned(),
                recipient: "spectator".to_owned(),
                grant_epoch,
            },
        ),
    )?;
    let spectator_grant_verified = context.verify_spectator_grant(true)?;
    push(
        &mut context,
        applied(
            "host",
            "revoke-host-hand",
            SmokeOperation::RevokeHand {
                player: "host".to_owned(),
                recipient: "spectator".to_owned(),
                grant_epoch,
            },
        ),
    )?;
    let spectator_revocation_verified = context.verify_spectator_grant(false)?;
    push(
        &mut context,
        SmokeInput {
            actor: "player".to_owned(),
            command_id: "disconnect-player".to_owned(),
            operation: SmokeOperation::Disconnect,
            expected: SmokeExpectation::Disconnected,
        },
    )?;
    push(
        &mut context,
        applied("player", "reconnect-player", SmokeOperation::Reconnect),
    )?;
    let reconnect_verified = context
        .authority
        .state
        .member(&principal("player")?)
        .is_some_and(|member| member.connection == poche_session::ConnectionState::Connected);

    let mut deal_ordinal = 1_u32;
    let mut game_sequence = 0_u32;
    while !matches!(context.authority.state.phase, SessionPhase::PostGame { .. }) {
        if game_sequence >= 1_000 {
            return Err("real Poche smoke game exceeded 1,000 transitions".to_owned());
        }
        let turn = context
            .active_game()
            .ok_or_else(|| "smoke game left running phase unexpectedly".to_owned())?
            .turn();
        let (actor, operation) = match turn {
            GameTurn::Chance => {
                let operation = SmokeOperation::ApplySeededChance { deal_ordinal };
                deal_ordinal = deal_ordinal.saturating_add(1);
                ("game", operation)
            }
            GameTurn::Player(seat) => {
                let actor = if seat == 0 { "host" } else { "player" };
                let action = first_legal_action(&context.authority.state)?;
                (actor, SmokeOperation::GameAction { action })
            }
            GameTurn::Environment => ("game", SmokeOperation::Settle),
            GameTurn::Finished => {
                return Err("terminal game did not move the session to post-game".to_owned());
            }
        };
        push(
            &mut context,
            applied(actor, &format!("game-{game_sequence}"), operation),
        )?;
        game_sequence = game_sequence.saturating_add(1);
    }
    let post_game = context.projection("host")?;
    let final_scores = post_game
        .public_game_state
        .as_ref()
        .ok_or_else(|| "post-game projection omitted public game state".to_owned())?
        .scores
        .clone();
    let final_public_hash = hash_json(&post_game)?;
    push(
        &mut context,
        applied(
            "host",
            "chat-post-game",
            SmokeOperation::Chat {
                text: "post-game hello".to_owned(),
            },
        ),
    )?;
    push(
        &mut context,
        applied("host", "reset-lobby", SmokeOperation::ResetLobby),
    )?;
    push(
        &mut context,
        applied("host", "close-room", SmokeOperation::CloseRoom),
    )?;

    let mut transcript = SmokeTranscript {
        schema_version: TRANSCRIPT_SCHEMA_VERSION,
        seed,
        inputs,
        records,
    };
    let transcript_hash = transcript_hash(&transcript)?;
    let applied = transcript
        .records
        .iter()
        .filter(|record| record.disposition == "applied")
        .count();
    let denied = transcript
        .records
        .iter()
        .filter(|record| record.disposition.starts_with("denied:"))
        .count();
    if !spectator_grant_verified || !spectator_revocation_verified || !reconnect_verified {
        return Err("smoke feature verification did not pass".to_owned());
    }
    let report = InProcessSmokeReport {
        schema_version: TRANSCRIPT_SCHEMA_VERSION,
        transport: "in-process-canonical-ndjson".to_owned(),
        seed,
        inputs: transcript.inputs.len(),
        applied,
        denied,
        final_revision: context.authority.state.revision,
        deals: deal_ordinal,
        rounds: round_count(2),
        final_scores,
        chat_messages: context.authority.chat_tail().len(),
        verified: vec![
            "spectator-grant".to_owned(),
            "spectator-revocation".to_owned(),
            "reconnect".to_owned(),
        ],
        transcript_hash,
        final_public_hash,
    };
    // Make accidental secret persistence a hard acceptance failure.
    let serialized = serde_json::to_string(&transcript).map_err(|error| error.to_string())?;
    if serialized.contains(PLAYER_INVITE) || serialized.contains(SPECTATOR_INVITE) {
        return Err("smoke transcript serialized an invite secret".to_owned());
    }
    // Normalize capacity so replay cannot retain implementation-only allocation state.
    transcript.inputs.shrink_to_fit();
    transcript.records.shrink_to_fit();
    Ok(RecordedSmoke { transcript, report })
}

fn replay_smoke(transcript: &SmokeTranscript) -> Result<RecordedSmoke, String> {
    if transcript.schema_version != TRANSCRIPT_SCHEMA_VERSION {
        return Err("unsupported smoke transcript schema".to_owned());
    }
    let mut context = SmokeContext::new()?;
    let mut records = Vec::with_capacity(transcript.inputs.len());
    let mut final_scores = Vec::new();
    let mut final_public_hash = String::new();
    let mut spectator_grant_verified = false;
    let mut spectator_revocation_verified = false;
    let mut reconnect_verified = false;
    let mut deals = 0_u32;
    for input in &transcript.inputs {
        records.push(context.execute(input, transcript.seed)?);
        match &input.operation {
            SmokeOperation::ApplySeededChance { deal_ordinal } => {
                deals = deals.max(deal_ordinal.saturating_add(1));
            }
            SmokeOperation::GrantHand { .. } => {
                spectator_grant_verified = context.verify_spectator_grant(true)?;
            }
            SmokeOperation::RevokeHand { .. } => {
                spectator_revocation_verified = context.verify_spectator_grant(false)?;
            }
            SmokeOperation::Reconnect => {
                reconnect_verified = true;
            }
            SmokeOperation::Chat { .. }
                if matches!(context.authority.state.phase, SessionPhase::PostGame { .. }) =>
            {
                let projection = context.projection("host")?;
                final_scores = projection
                    .public_game_state
                    .as_ref()
                    .map_or_else(Vec::new, |game| game.scores.clone());
                final_public_hash = hash_json(&projection)?;
            }
            _ => {}
        }
    }
    let replayed = SmokeTranscript {
        schema_version: transcript.schema_version,
        seed: transcript.seed,
        inputs: transcript.inputs.clone(),
        records,
    };
    if !spectator_grant_verified || !spectator_revocation_verified || !reconnect_verified {
        return Err("replayed smoke feature verification did not pass".to_owned());
    }
    let report = InProcessSmokeReport {
        schema_version: TRANSCRIPT_SCHEMA_VERSION,
        transport: "in-process-canonical-ndjson".to_owned(),
        seed: transcript.seed,
        inputs: replayed.inputs.len(),
        applied: replayed
            .records
            .iter()
            .filter(|record| record.disposition == "applied")
            .count(),
        denied: replayed
            .records
            .iter()
            .filter(|record| record.disposition.starts_with("denied:"))
            .count(),
        final_revision: context.authority.state.revision,
        deals,
        rounds: round_count(2),
        final_scores,
        chat_messages: context.authority.chat_tail().len(),
        verified: vec![
            "spectator-grant".to_owned(),
            "spectator-revocation".to_owned(),
            "reconnect".to_owned(),
        ],
        transcript_hash: transcript_hash(&replayed)?,
        final_public_hash,
    };
    Ok(RecordedSmoke {
        transcript: replayed,
        report,
    })
}

impl SmokeContext {
    fn new() -> Result<Self, String> {
        let clock = principal("clock")?;
        let game = principal("game")?;
        let mut state =
            SessionState::pending(RoomId::new("smoke-room").map_err(debug)?, clock, game);
        state
            .invites
            .push(InviteRecord::new(PLAYER_INVITE, u64::MAX).map_err(debug)?);
        state
            .invites
            .push(InviteRecord::new(SPECTATOR_INVITE, u64::MAX).map_err(debug)?);
        let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let mut clients = BTreeMap::new();
        for name in ["host", "player", "spectator", "game"] {
            clients.insert(
                name.to_owned(),
                transport.connect(principal(name)?).map_err(debug)?,
            );
        }
        Ok(Self {
            authority: InProcessAuthority::new(state, transport),
            clients,
        })
    }

    fn execute(&mut self, input: &SmokeInput, seed: u64) -> Result<SmokeStepRecord, String> {
        let outcome = match &input.operation {
            SmokeOperation::AdvanceClock { tick } => self
                .authority
                .advance_clock_to(*tick)
                .map_err(debug)?
                .into_iter()
                .next()
                .ok_or_else(|| "clock advance produced no expiry".to_owned())?,
            SmokeOperation::Disconnect => {
                let client = self.client(&input.actor)?;
                self.authority
                    .transport
                    .disconnect(client.connection_id())
                    .map_err(debug)?;
                self.authority
                    .drive_all()
                    .map_err(debug)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| "disconnect produced no semantic observation".to_owned())?
            }
            operation => {
                if matches!(operation, SmokeOperation::Reconnect) {
                    let client = self
                        .authority
                        .transport
                        .connect(principal(&input.actor)?)
                        .map_err(debug)?;
                    self.clients.insert(input.actor.clone(), client);
                }
                let payload = payload(operation, seed)?;
                let client = self.client(&input.actor)?;
                let command = client
                    .command(&self.authority.state, &input.command_id, payload)
                    .map_err(debug)?;
                client
                    .submit(&mut self.authority.transport, command)
                    .map_err(debug)?;
                self.authority
                    .drive_all()
                    .map_err(debug)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| "command produced no authority outcome".to_owned())?
            }
        };
        let disposition = match &outcome.disposition {
            AuthorityDisposition::Applied => "applied".to_owned(),
            AuthorityDisposition::Denied(reason) => format!("denied:{reason:?}"),
            AuthorityDisposition::Disconnected => "disconnected".to_owned(),
        };
        let expected = match input.expected {
            SmokeExpectation::Applied => AuthorityDisposition::Applied,
            SmokeExpectation::DeniedPaused => AuthorityDisposition::Denied(DenyReason::Paused),
            SmokeExpectation::Disconnected => AuthorityDisposition::Disconnected,
        };
        if outcome.disposition != expected {
            return Err(format!(
                "{} expected {:?}, observed {:?}",
                input.command_id, expected, outcome.disposition
            ));
        }
        Ok(SmokeStepRecord {
            command_id: input.command_id.clone(),
            disposition,
            revision: outcome.revision,
            events: outcome.events,
            phase: phase_label(&self.authority.state.phase).to_owned(),
            projection_hashes: self.projection_hashes()?,
        })
    }

    fn client(&self, name: &str) -> Result<ScriptedClient, String> {
        self.clients
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown smoke actor {name}"))
    }

    fn active_game(&self) -> Option<&OracleSessionGame<2>> {
        match &self.authority.state.phase {
            SessionPhase::Running { game } | SessionPhase::Paused { game } => Some(game),
            _ => None,
        }
    }

    fn projection(&self, viewer: &str) -> Result<ProjectionPayload, String> {
        project_viewer(
            &self.authority.state,
            &principal(viewer)?,
            self.authority.state.projection_epoch,
        )
        .map_err(debug)
    }

    fn projection_hashes(&self) -> Result<BTreeMap<String, String>, String> {
        ["host", "player", "spectator"]
            .into_iter()
            .map(|viewer| {
                let viewer_id = principal(viewer)?;
                let hash = match project_viewer(
                    &self.authority.state,
                    &viewer_id,
                    self.authority.state.projection_epoch,
                ) {
                    Ok(projection) => hash_json(&projection)?,
                    Err(ProjectionError::NotConnected) => "error:not-connected".to_owned(),
                    Err(ProjectionError::UnknownViewer) => "error:unknown-viewer".to_owned(),
                    Err(ProjectionError::StaleProjectionEpoch) => "error:stale-epoch".to_owned(),
                    Err(ProjectionError::Game(error)) => return Err(debug(error)),
                };
                Ok((viewer.to_owned(), hash))
            })
            .collect()
    }

    fn verify_spectator_grant(&self, expected: bool) -> Result<bool, String> {
        let spectator = self.projection("spectator")?;
        if !expected {
            return Ok(spectator.granted_hands.is_empty());
        }
        let host = self.projection("host")?;
        let host_hand = host
            .own_hand
            .ok_or_else(|| "host projection omitted its own hand".to_owned())?;
        Ok(spectator.granted_hands.len() == 1
            && spectator.granted_hands[0].player == principal("host")?
            && spectator.granted_hands[0].cards == host_hand.cards)
    }
}

fn payload(operation: &SmokeOperation, seed: u64) -> Result<CommandPayload, String> {
    Ok(match operation {
        SmokeOperation::CreateRoom => CommandPayload::CreateRoom,
        SmokeOperation::Join { invite_ref } => CommandPayload::RedeemInvite {
            invite: InviteProof::new(match invite_ref.as_str() {
                "player" => PLAYER_INVITE,
                "spectator" => SPECTATOR_INVITE,
                _ => return Err(format!("unknown smoke invite ref {invite_ref}")),
            })
            .map_err(debug)?,
        },
        SmokeOperation::TakeSeat { seat } => CommandPayload::TakeSeat { seat: *seat },
        SmokeOperation::Ready => CommandPayload::Ready,
        SmokeOperation::Unready => CommandPayload::Unready,
        SmokeOperation::ArmCountdown {
            deadline_tick,
            token,
        } => CommandPayload::ArmCountdown {
            deadline_tick: *deadline_tick,
            countdown_token: CountdownToken::new(token).map_err(debug)?,
        },
        SmokeOperation::AbortCountdown => CommandPayload::AbortCountdown,
        SmokeOperation::ApplySeededChance { deal_ordinal } => CommandPayload::ApplyChance {
            chance: chance_wire(seed, *deal_ordinal)?,
        },
        SmokeOperation::Pause => CommandPayload::Pause,
        SmokeOperation::Unpause => CommandPayload::Unpause,
        SmokeOperation::GameAction { action } => CommandPayload::GameAction {
            action: action.clone(),
        },
        SmokeOperation::Settle => CommandPayload::Settle,
        SmokeOperation::Chat { text } => CommandPayload::Chat { text: text.clone() },
        SmokeOperation::RequestHand { player } => CommandPayload::RequestHand {
            player: principal(player)?,
        },
        SmokeOperation::GrantHand {
            request_id,
            player,
            recipient,
            grant_epoch,
        } => CommandPayload::GrantHand {
            request_id: CommandId::new(request_id).map_err(debug)?,
            player: principal(player)?,
            recipient: principal(recipient)?,
            grant_epoch: *grant_epoch,
        },
        SmokeOperation::RevokeHand {
            player,
            recipient,
            grant_epoch,
        } => CommandPayload::RevokeHand {
            player: principal(player)?,
            recipient: principal(recipient)?,
            grant_epoch: *grant_epoch,
        },
        SmokeOperation::Reconnect => CommandPayload::Reconnect,
        SmokeOperation::ResetLobby => CommandPayload::ResetLobby,
        SmokeOperation::CloseRoom => CommandPayload::CloseRoom,
        SmokeOperation::AdvanceClock { .. } | SmokeOperation::Disconnect => {
            return Err("runtime-only smoke operation has no command payload".to_owned());
        }
    })
}

fn first_legal_action(
    state: &SessionState<OracleSessionGame<2>>,
) -> Result<GameActionWire, String> {
    let (SessionPhase::Running { game } | SessionPhase::Paused { game }) = &state.phase else {
        return Err("legal action requested outside an active game".to_owned());
    };
    let action = OracleEnvironment::<2>::legal_actions(&game.game)
        .into_iter()
        .next()
        .ok_or_else(|| "acting player has no legal action".to_owned())?;
    Ok(match action {
        OraclePlayerAction::Bid { tricks, .. } => GameActionWire::Bid { tricks },
        OraclePlayerAction::Play { card, .. } => GameActionWire::Play {
            card: Card::standard_deck()
                .iter()
                .position(|candidate| *candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| "legal card has no canonical code".to_owned())?,
        },
    })
}

fn chance_wire(seed: u64, deal_ordinal: u32) -> Result<ChanceWire, String> {
    let chance = OracleChanceAction::seeded(seed, deal_ordinal);
    let standard = Card::standard_deck();
    let cards = chance
        .deck
        .cards()
        .iter()
        .map(|card| {
            standard
                .iter()
                .position(|candidate| candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| "seeded card has no canonical code".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChanceWire {
        cards,
        seed: Some(seed),
        deal_ordinal: Some(deal_ordinal),
    })
}

fn transcript_hash(transcript: &SmokeTranscript) -> Result<String, String> {
    hash_json(transcript)
}

fn hash_json(value: &impl Serialize) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn principal(value: &str) -> Result<PrincipalId, String> {
    PrincipalId::new(value).map_err(debug)
}

fn phase_label<G>(phase: &SessionPhase<G>) -> &'static str {
    match phase {
        SessionPhase::Uninitialized => "uninitialized",
        SessionPhase::Lobby => "lobby",
        SessionPhase::Countdown { .. } => "countdown",
        SessionPhase::Running { .. } => "running",
        SessionPhase::Paused { .. } => "paused",
        SessionPhase::PostGame { .. } => "post_game",
        SessionPhase::Closed => "closed",
    }
}

fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_scenario_replays_every_input_and_finishes_the_real_game() {
        let report = run_in_process_smoke(0x5eed).unwrap();
        assert!(report.verified.contains(&"transcript-replay".to_owned()));
        assert!(report.verified.contains(&"spectator-grant".to_owned()));
        assert!(report.verified.contains(&"spectator-revocation".to_owned()));
        assert!(report.verified.contains(&"reconnect".to_owned()));
        assert_eq!(report.deals, u32::try_from(round_count(2)).unwrap());
        assert_eq!(report.chat_messages, 5);
        assert_eq!(report.denied, 1);
    }

    #[test]
    fn different_seeds_produce_distinct_transcript_and_public_hashes() {
        let first = run_in_process_smoke(1).unwrap();
        let second = run_in_process_smoke(2).unwrap();
        assert_ne!(first.transcript_hash, second.transcript_hash);
        assert_ne!(first.final_public_hash, second.final_public_hash);
    }
}
