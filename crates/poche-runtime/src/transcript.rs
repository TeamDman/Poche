use std::collections::BTreeMap;
use std::fmt::Write;

use poche_protocol::{
    ChanceWire, CommandEnvelope, CommandId, CommandPayload, CorrelationId, CountdownToken,
    DenyReason, GameActionWire, GamePublicStateWire, InviteProof, PROTOCOL_VERSION_V1, PrincipalId,
    ProjectionPayload, PublicGamePhase, PublicTurnWire, RoomId, SIGNATURE_DOMAIN_V1, SemanticHash,
    SignatureAlgorithm, SignatureBytes, SignatureIntent, SnapshotPayload, UnsignedCommandEnvelope,
    decode_command_line, encode_command_line, protocol_schema_hash, verified_command_semantic_hash,
};
use poche_session::{
    AuthorizedCommand, ConnectionState, GameTransition, GameTurn, HandCapabilityExpiry,
    InviteRecord, PolicyDecision, ProjectionError, SessionError, SessionEvent, SessionEventKind,
    SessionGame, SessionPhase, SessionState, apply, authorize, decide, decide_transport_disconnect,
    project_viewer,
};
use serde::{Deserialize, Serialize};

const FIXTURE_SCHEMA_VERSION: u16 = 1;
const FIXTURE_ID: &str = "micro-session-v1";
const BUILTIN_SNAPSHOT_AFTER_STEP: usize = 23;
const ALICE_INVITE: &str = "runtime-only-alice-invite";
const BOB_INVITE: &str = "runtime-only-bob-invite";

/// Compact deterministic game used to exercise the protocol/session recovery
/// surface without duplicating the full Poche oracle fixture suite.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptGame {
    turn: GameTurn,
    hands: [Vec<u8>; 2],
}

impl SessionGame for TranscriptGame {
    type Error = &'static str;

    fn start(seats: &[(u8, PrincipalId)]) -> Result<Self, Self::Error> {
        if seats.len() != 2 || seats[0].0 != 0 || seats[1].0 != 1 {
            return Err("transcript game requires canonical seats zero and one");
        }
        Ok(Self {
            turn: GameTurn::Chance,
            hands: [vec![0, 1], vec![2, 3]],
        })
    }

    fn turn(&self) -> GameTurn {
        self.turn
    }

    fn public_projection(&self) -> Result<GamePublicStateWire, Self::Error> {
        Ok(GamePublicStateWire {
            schema_version: 1,
            phase: if self.turn == GameTurn::Finished {
                PublicGamePhase::Finished
            } else {
                PublicGamePhase::Playing
            },
            dealer: Some(0),
            actor: match self.turn {
                GameTurn::Chance => PublicTurnWire::Chance,
                GameTurn::Player(seat) => PublicTurnWire::Player(seat),
                GameTurn::Environment => PublicTurnWire::Environment,
                GameTurn::Finished => PublicTurnWire::Finished,
            },
            round_index: 0,
            hand_size: 2,
            hand_counts: vec![2, 2],
            trump: Some(51),
            current_trick: Vec::new(),
            bids: vec![None, None],
            tricks_won: vec![0, 0],
            scores: vec![0, 0],
            pot_cents: 50,
        })
    }

    fn private_hand(&self, seat: u8) -> Result<Vec<u8>, Self::Error> {
        self.hands
            .get(usize::from(seat))
            .cloned()
            .ok_or("invalid transcript seat")
    }

    fn player_transition(
        &self,
        seat: u8,
        _action: &GameActionWire,
    ) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Player(seat) {
            return Err("wrong transcript actor");
        }
        Ok(GameTransition {
            game: Self {
                turn: if seat == 0 {
                    GameTurn::Player(1)
                } else {
                    GameTurn::Environment
                },
                hands: self.hands.clone(),
            },
            round_scores: None,
            terminal: false,
        })
    }

    fn chance_transition(&self, chance: &ChanceWire) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Chance || chance.cards != (0_u8..52).collect::<Vec<_>>() {
            return Err("transcript chance must be the canonical standard deck");
        }
        Ok(GameTransition {
            game: Self {
                turn: GameTurn::Player(0),
                hands: self.hands.clone(),
            },
            round_scores: None,
            terminal: false,
        })
    }

    fn settle(&self) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Environment {
            return Err("transcript settlement unavailable");
        }
        Ok(GameTransition {
            game: Self {
                turn: GameTurn::Finished,
                hands: self.hands.clone(),
            },
            round_scores: Some(vec![7, -7]),
            terminal: true,
        })
    }
}

/// How a fixture binds one command to the authority revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureRevision {
    Current,
    Previous(u64),
}

/// Secret-free operation vocabulary stored in golden fixtures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureOperation {
    CreateRoom,
    RedeemInviteRef {
        invite_ref: String,
    },
    TakeSeat {
        seat: u8,
    },
    Ready,
    ArmCountdown {
        deadline_tick: u64,
        token: String,
    },
    AbortCountdown,
    CountdownExpired {
        token: String,
    },
    ApplyStandardChance,
    Pause,
    Unpause,
    GameBid {
        tricks: u8,
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
    DenyHand {
        request_id: String,
        player: String,
        recipient: String,
    },
    RevokeHand {
        player: String,
        recipient: String,
        grant_epoch: u64,
    },
    Reconnect,
    ResetLobby,
    CloseRoom,
}

/// One exact fixture input. Invite references resolve only inside the runner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureInput {
    Command {
        principal: String,
        command_id: String,
        revision: FixtureRevision,
        operation: FixtureOperation,
    },
    DuplicateCommand {
        command_id: String,
    },
    TransportDisconnect {
        principal: String,
        observation_id: String,
    },
}

/// Recorded result of one input, including only hashes of scoped projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenStep {
    pub input: FixtureInput,
    pub command_hash: Option<String>,
    pub authorization: String,
    pub outcome: String,
    pub events: Vec<String>,
    pub state_hash: String,
    pub projection_hashes: BTreeMap<String, String>,
    pub scoped_projections: BTreeMap<String, ProjectionPayload>,
}

/// Evidence for secret-free persisted prefix restore plus tail replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEvidence {
    pub after_step: usize,
    pub prefix_hash: String,
    pub snapshot_payload_hash: String,
    pub state_hash: String,
    pub final_state_hash_after_tail: String,
    pub final_projection_hashes_after_tail: BTreeMap<String, String>,
}

/// Complete deterministic golden transcript.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenTranscript {
    pub schema_version: u16,
    pub fixture_id: String,
    pub steps: Vec<GoldenStep>,
    pub snapshot: SnapshotEvidence,
    pub final_state_hash: String,
}

/// Successful fixture verification summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptVerification {
    pub fixture_id: String,
    pub steps: usize,
    pub final_state_hash: String,
}

/// Canonical output record for one replayed script.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum TranscriptOutputRecord {
    Step {
        index: usize,
        step: Box<GoldenStep>,
    },
    Summary {
        fixture_id: String,
        steps: usize,
        final_state_hash: String,
    },
}

/// All presentations derived from one typed script replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptReplay {
    pub transcript: GoldenTranscript,
    pub output_ndjson: String,
    pub text: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum ControlledReplayDefect {
    None,
    DropFirstEvent { command_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayCodec {
    Typed,
    CanonicalNdjson,
}

struct ReplayContext {
    state: SessionState<TranscriptGame>,
    commands: BTreeMap<String, CommandEnvelope>,
}

/// Render the canonical built-in transcript for deliberate fixture updates.
///
/// # Errors
///
/// Returns a precise runner or JSON serialization failure.
pub fn render_builtin_transcript() -> Result<String, String> {
    let transcript = generate_transcript(
        &builtin_inputs(),
        BUILTIN_SNAPSHOT_AFTER_STEP,
        &ControlledReplayDefect::None,
        ReplayCodec::Typed,
    )?;
    serde_json::to_string(&transcript).map_err(|error| error.to_string())
}

/// Render the built-in secret-free input script as canonical NDJSON.
///
/// # Errors
///
/// Returns a serialization failure without exposing runtime invite material.
pub fn render_builtin_script_ndjson() -> Result<String, String> {
    render_fixture_script_ndjson(&builtin_inputs())
}

/// Replay a canonical fixture script through typed and canonical-NDJSON ingress.
///
/// # Errors
///
/// Rejects ambiguous/noncanonical script framing or the first semantic/codec
/// disagreement.
pub fn replay_fixture_script_ndjson(script: &str) -> Result<ScriptReplay, String> {
    let inputs = parse_fixture_script_ndjson(script)?;
    if inputs.len() <= BUILTIN_SNAPSHOT_AFTER_STEP {
        return Err(format!(
            "fixture script requires at least {} inputs for the pinned snapshot checkpoint",
            BUILTIN_SNAPSHOT_AFTER_STEP + 1
        ));
    }
    let typed = generate_transcript(
        &inputs,
        BUILTIN_SNAPSHOT_AFTER_STEP,
        &ControlledReplayDefect::None,
        ReplayCodec::Typed,
    )?;
    let ndjson = generate_transcript(
        &inputs,
        BUILTIN_SNAPSHOT_AFTER_STEP,
        &ControlledReplayDefect::None,
        ReplayCodec::CanonicalNdjson,
    )?;
    compare_transcripts(&typed, &ndjson).map_err(|error| format!("typed/NDJSON {error}"))?;
    let output_ndjson = render_transcript_output_ndjson(&typed)?;
    let text = crate::render_transcript_text(&typed);
    Ok(ScriptReplay {
        transcript: typed,
        output_ndjson,
        text,
    })
}

/// Verify a checked-in script against a checked-in full transcript.
///
/// # Errors
///
/// Returns the first script, codec, or expected-transcript divergence.
pub fn verify_fixture_script_against_transcript(
    script: &str,
    expected_transcript: &str,
) -> Result<TranscriptVerification, String> {
    let replay = replay_fixture_script_ndjson(script)?;
    let expected: GoldenTranscript = serde_json::from_str(expected_transcript)
        .map_err(|error| format!("fixture JSON: {error}"))?;
    compare_transcripts(&expected, &replay.transcript)?;
    Ok(TranscriptVerification {
        fixture_id: replay.transcript.fixture_id,
        steps: replay.transcript.steps.len(),
        final_state_hash: replay.transcript.final_state_hash,
    })
}

fn render_fixture_script_ndjson(inputs: &[FixtureInput]) -> Result<String, String> {
    let mut output = String::new();
    for input in inputs {
        output.push_str(&serde_json::to_string(input).map_err(|error| error.to_string())?);
        output.push('\n');
    }
    Ok(output)
}

fn parse_fixture_script_ndjson(script: &str) -> Result<Vec<FixtureInput>, String> {
    if script.is_empty() || !script.ends_with('\n') || script.contains('\r') {
        return Err("fixture script requires nonempty LF-terminated NDJSON".to_owned());
    }
    let mut inputs = Vec::new();
    for (index, line) in script[..script.len() - 1].split('\n').enumerate() {
        if line.is_empty() {
            return Err(format!("fixture script line {} is empty", index + 1));
        }
        let input: FixtureInput = serde_json::from_str(line)
            .map_err(|_| format!("fixture script line {} is invalid JSON", index + 1))?;
        let canonical = serde_json::to_string(&input).map_err(|error| error.to_string())?;
        if canonical != line {
            return Err(format!(
                "fixture script line {} is not canonical JSON",
                index + 1
            ));
        }
        inputs.push(input);
    }
    Ok(inputs)
}

/// Render every transcript step plus one terminal summary as canonical NDJSON.
///
/// # Errors
///
/// Returns a serialization failure.
pub fn render_transcript_output_ndjson(transcript: &GoldenTranscript) -> Result<String, String> {
    let mut output = String::new();
    for (index, step) in transcript.steps.iter().enumerate() {
        let record = TranscriptOutputRecord::Step {
            index,
            step: Box::new(step.clone()),
        };
        output.push_str(&serde_json::to_string(&record).map_err(|error| error.to_string())?);
        output.push('\n');
    }
    let summary = TranscriptOutputRecord::Summary {
        fixture_id: transcript.fixture_id.clone(),
        steps: transcript.steps.len(),
        final_state_hash: transcript.final_state_hash.clone(),
    };
    output.push_str(&serde_json::to_string(&summary).map_err(|error| error.to_string())?);
    output.push('\n');
    Ok(output)
}

/// Replay and verify one checked-in transcript.
///
/// # Errors
///
/// Returns the first field-level divergence with its step index.
pub fn verify_transcript(text: &str) -> Result<TranscriptVerification, String> {
    let expected: GoldenTranscript =
        serde_json::from_str(text).map_err(|error| format!("fixture JSON: {error}"))?;
    if expected.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "fixture schema: expected {FIXTURE_SCHEMA_VERSION}, found {}",
            expected.schema_version
        ));
    }
    let inputs: Vec<_> = expected
        .steps
        .iter()
        .map(|step| step.input.clone())
        .collect();
    let actual = generate_transcript(
        &inputs,
        expected.snapshot.after_step,
        &ControlledReplayDefect::None,
        ReplayCodec::Typed,
    )?;
    compare_transcripts(&expected, &actual)?;
    Ok(TranscriptVerification {
        fixture_id: actual.fixture_id,
        steps: actual.steps.len(),
        final_state_hash: actual.final_state_hash,
    })
}

/// Prove that direct typed and strict canonical NDJSON ingress have identical
/// semantic results for one checked transcript.
///
/// # Errors
///
/// Returns the first field-level divergence or codec failure.
pub fn verify_transcript_codec_parity(text: &str) -> Result<TranscriptVerification, String> {
    let expected: GoldenTranscript =
        serde_json::from_str(text).map_err(|error| format!("fixture JSON: {error}"))?;
    let inputs: Vec<_> = expected
        .steps
        .iter()
        .map(|step| step.input.clone())
        .collect();
    let typed = generate_transcript(
        &inputs,
        expected.snapshot.after_step,
        &ControlledReplayDefect::None,
        ReplayCodec::Typed,
    )?;
    let ndjson = generate_transcript(
        &inputs,
        expected.snapshot.after_step,
        &ControlledReplayDefect::None,
        ReplayCodec::CanonicalNdjson,
    )?;
    compare_transcripts(&expected, &typed)?;
    compare_transcripts(&typed, &ndjson).map_err(|error| format!("typed/NDJSON {error}"))?;
    Ok(TranscriptVerification {
        fixture_id: typed.fixture_id,
        steps: typed.steps.len(),
        final_state_hash: typed.final_state_hash,
    })
}

fn generate_transcript(
    inputs: &[FixtureInput],
    snapshot_after_step: usize,
    defect: &ControlledReplayDefect,
    codec: ReplayCodec,
) -> Result<GoldenTranscript, String> {
    if inputs.is_empty() || snapshot_after_step >= inputs.len() {
        return Err("snapshot step must select a non-empty transcript prefix".to_owned());
    }
    let (full_context, steps) = run_inputs(new_context()?, inputs, defect, codec)?;
    let final_state_hash = state_hash(&full_context.state)?;

    let prefix = &inputs[..=snapshot_after_step];
    let tail = &inputs[snapshot_after_step + 1..];
    let prefix_bytes = serde_json::to_vec(prefix).map_err(|error| error.to_string())?;
    let prefix_hash = hash_bytes(&prefix_bytes);
    let restored_inputs: Vec<FixtureInput> =
        serde_json::from_slice(&prefix_bytes).map_err(|error| error.to_string())?;
    let (restored, _) = run_inputs(
        new_context()?,
        &restored_inputs,
        &ControlledReplayDefect::None,
        codec,
    )?;
    let snapshot_state_hash = state_hash(&restored.state)?;
    let payload = snapshot_payload(&restored.state, prefix_bytes)?;
    let payload_bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    let snapshot_payload_hash = hash_bytes(&payload_bytes);
    let (tail_replayed, _) = run_inputs(restored, tail, &ControlledReplayDefect::None, codec)?;
    let tail_hash = state_hash(&tail_replayed.state)?;
    if tail_hash != final_state_hash {
        return Err(format!(
            "snapshot tail final state: expected {final_state_hash}, found {tail_hash}"
        ));
    }
    let full_projection_hashes = projection_hashes(&full_context.state)?;
    let tail_projection_hashes = projection_hashes(&tail_replayed.state)?;
    if tail_projection_hashes != full_projection_hashes {
        return Err("snapshot tail final viewer projections differ from genesis replay".to_owned());
    }

    Ok(GoldenTranscript {
        schema_version: FIXTURE_SCHEMA_VERSION,
        fixture_id: FIXTURE_ID.to_owned(),
        steps,
        snapshot: SnapshotEvidence {
            after_step: snapshot_after_step,
            prefix_hash,
            snapshot_payload_hash,
            state_hash: snapshot_state_hash,
            final_state_hash_after_tail: tail_hash,
            final_projection_hashes_after_tail: tail_projection_hashes,
        },
        final_state_hash,
    })
}

fn new_context() -> Result<ReplayContext, String> {
    let mut state = SessionState::pending(
        room("transcript-room")?,
        principal("system-clock")?,
        principal("system-game")?,
    );
    state
        .invites
        .push(InviteRecord::new(ALICE_INVITE, u64::MAX).map_err(|error| format!("{error:?}"))?);
    state
        .invites
        .push(InviteRecord::new(BOB_INVITE, u64::MAX).map_err(|error| format!("{error:?}"))?);
    Ok(ReplayContext {
        state,
        commands: BTreeMap::new(),
    })
}

fn run_inputs(
    mut context: ReplayContext,
    inputs: &[FixtureInput],
    defect: &ControlledReplayDefect,
    codec: ReplayCodec,
) -> Result<(ReplayContext, Vec<GoldenStep>), String> {
    let mut steps = Vec::with_capacity(inputs.len());
    for input in inputs {
        let (command_hash, authorization, outcome, events) = match input {
            FixtureInput::Command {
                principal: actor,
                command_id,
                revision,
                operation,
            } => {
                let command =
                    signed_command(&context.state, actor, command_id, revision, operation)?;
                let command = ingress_command(command, codec)?;
                context.commands.insert(command_id.clone(), command.clone());
                execute_command(&mut context.state, &command, defect)?
            }
            FixtureInput::DuplicateCommand { command_id } => {
                let command =
                    context.commands.get(command_id).cloned().ok_or_else(|| {
                        format!("duplicate references unknown command {command_id}")
                    })?;
                execute_command(&mut context.state, &command, defect)?
            }
            FixtureInput::TransportDisconnect {
                principal: actor,
                observation_id,
            } => execute_disconnect(&mut context.state, actor, observation_id, defect)?,
        };
        steps.push(GoldenStep {
            input: input.clone(),
            command_hash,
            authorization,
            outcome,
            events,
            state_hash: state_hash(&context.state)?,
            projection_hashes: projection_hashes(&context.state)?,
            scoped_projections: scoped_projection_checkpoint(&context.state, input)?,
        });
    }
    Ok((context, steps))
}

fn ingress_command(
    command: CommandEnvelope,
    codec: ReplayCodec,
) -> Result<CommandEnvelope, String> {
    match codec {
        ReplayCodec::Typed => Ok(command),
        ReplayCodec::CanonicalNdjson => {
            let line =
                encode_command_line(&command).map_err(|error| format!("NDJSON encode: {error}"))?;
            decode_command_line(&line).map_err(|error| format!("NDJSON decode: {error}"))
        }
    }
}

type StepOutcome = (Option<String>, String, String, Vec<String>);

fn execute_command(
    state: &mut SessionState<TranscriptGame>,
    command: &CommandEnvelope,
    defect: &ControlledReplayDefect,
) -> Result<StepOutcome, String> {
    let command_hash = Some(hash_hex(
        &verified_command_semantic_hash(command).map_err(|error| error.to_string())?,
    ));
    let decision = authorize(state, command);
    let authorization = decision_label(&decision);
    if !decision.is_allowed() {
        return Ok((
            command_hash,
            authorization.clone(),
            authorization,
            Vec::new(),
        ));
    }
    let authorized = AuthorizedCommand::from_decision(command.clone(), decision)
        .map_err(|_| "allowed decision could not become AuthorizedCommand".to_owned())?;
    let events = match decide(state, &authorized) {
        Ok(events) => events,
        Err(SessionError::Denied(reason)) => {
            return Ok((
                command_hash,
                authorization,
                format!("deny:{}", deny_code(reason)?),
                Vec::new(),
            ));
        }
        Err(error) => return Err(format!("semantic command failure: {error:?}")),
    };
    let labels = events.iter().map(event_label).collect();
    apply_events(state, &events, defect, command.command_id.as_str())?;
    Ok((command_hash, authorization, "applied".to_owned(), labels))
}

fn execute_disconnect(
    state: &mut SessionState<TranscriptGame>,
    actor: &str,
    observation_id: &str,
    defect: &ControlledReplayDefect,
) -> Result<StepOutcome, String> {
    let target = principal(actor)?;
    let observation = command_id(observation_id)?;
    let observation_hash = SemanticHash(*blake3::hash(observation_id.as_bytes()).as_bytes());
    let correlation = correlation(&format!("cor-{observation_id}"))?;
    let events =
        decide_transport_disconnect(state, &target, &observation, observation_hash, &correlation)
            .map_err(|error| format!("disconnect failure: {error:?}"))?;
    let labels = events.iter().map(event_label).collect();
    apply_events(state, &events, defect, observation_id)?;
    Ok((
        Some(hash_hex(&observation_hash)),
        "allow:P-TRANSPORT-DISCONNECT".to_owned(),
        "applied".to_owned(),
        labels,
    ))
}

fn apply_events(
    state: &mut SessionState<TranscriptGame>,
    events: &[SessionEvent<TranscriptGame>],
    defect: &ControlledReplayDefect,
    input_id: &str,
) -> Result<(), String> {
    for (index, event) in events.iter().enumerate() {
        if matches!(
            defect,
            ControlledReplayDefect::DropFirstEvent { command_id }
                if command_id == input_id && index == 0
        ) {
            continue;
        }
        *state = apply(state, event).map_err(|error| format!("apply failure: {error:?}"))?;
    }
    Ok(())
}

fn signed_command(
    state: &SessionState<TranscriptGame>,
    actor: &str,
    command_name: &str,
    revision: &FixtureRevision,
    operation: &FixtureOperation,
) -> Result<CommandEnvelope, String> {
    let principal_id = principal(actor)?;
    let expected_revision = match revision {
        FixtureRevision::Current => state.revision,
        FixtureRevision::Previous(distance) => state.revision.saturating_sub(*distance),
    };
    let payload = materialize_operation(operation)?;
    Ok(UnsignedCommandEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id: state.room_id.clone(),
        session_epoch: state.session_epoch,
        command_id: command_id(command_name)?,
        principal_id: principal_id.clone(),
        expected_revision,
        correlation_id: correlation(&format!("cor-{command_name}"))?,
        causation_id: None,
        payload,
        signature_intent: SignatureIntent {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: principal_id,
        },
    }
    .attach_signature(SignatureBytes::new("0".repeat(128)).map_err(|error| format!("{error:?}"))?))
}

fn materialize_operation(operation: &FixtureOperation) -> Result<CommandPayload, String> {
    Ok(match operation {
        FixtureOperation::CreateRoom => CommandPayload::CreateRoom,
        FixtureOperation::RedeemInviteRef { invite_ref } => CommandPayload::RedeemInvite {
            invite: InviteProof::new(match invite_ref.as_str() {
                "alice" => ALICE_INVITE,
                "bob" => BOB_INVITE,
                _ => return Err(format!("unknown invite reference {invite_ref}")),
            })
            .map_err(|error| format!("{error:?}"))?,
        },
        FixtureOperation::TakeSeat { seat } => CommandPayload::TakeSeat { seat: *seat },
        FixtureOperation::Ready => CommandPayload::Ready,
        FixtureOperation::ArmCountdown {
            deadline_tick,
            token,
        } => CommandPayload::ArmCountdown {
            deadline_tick: *deadline_tick,
            countdown_token: countdown(token)?,
        },
        FixtureOperation::AbortCountdown => CommandPayload::AbortCountdown,
        FixtureOperation::CountdownExpired { token } => CommandPayload::CountdownExpired {
            countdown_token: countdown(token)?,
        },
        FixtureOperation::ApplyStandardChance => CommandPayload::ApplyChance {
            chance: ChanceWire {
                cards: (0_u8..52).collect(),
                seed: None,
                deal_ordinal: None,
            },
        },
        FixtureOperation::Pause => CommandPayload::Pause,
        FixtureOperation::Unpause => CommandPayload::Unpause,
        FixtureOperation::GameBid { tricks } => CommandPayload::GameAction {
            action: GameActionWire::Bid { tricks: *tricks },
        },
        FixtureOperation::Settle => CommandPayload::Settle,
        FixtureOperation::Chat { text } => CommandPayload::Chat { text: text.clone() },
        FixtureOperation::RequestHand { player } => CommandPayload::RequestHand {
            player: principal(player)?,
        },
        FixtureOperation::GrantHand {
            request_id,
            player,
            recipient,
            grant_epoch,
        } => CommandPayload::GrantHand {
            request_id: command_id(request_id)?,
            player: principal(player)?,
            recipient: principal(recipient)?,
            grant_epoch: *grant_epoch,
        },
        FixtureOperation::DenyHand {
            request_id,
            player,
            recipient,
        } => CommandPayload::DenyHand {
            request_id: command_id(request_id)?,
            player: principal(player)?,
            recipient: principal(recipient)?,
        },
        FixtureOperation::RevokeHand {
            player,
            recipient,
            grant_epoch,
        } => CommandPayload::RevokeHand {
            player: principal(player)?,
            recipient: principal(recipient)?,
            grant_epoch: *grant_epoch,
        },
        FixtureOperation::Reconnect => CommandPayload::Reconnect,
        FixtureOperation::ResetLobby => CommandPayload::ResetLobby,
        FixtureOperation::CloseRoom => CommandPayload::CloseRoom,
    })
}

#[allow(clippy::too_many_lines)]
fn builtin_inputs() -> Vec<FixtureInput> {
    vec![
        command("host", "create", FixtureOperation::CreateRoom),
        command(
            "alice",
            "join-alice",
            FixtureOperation::RedeemInviteRef {
                invite_ref: "alice".to_owned(),
            },
        ),
        command(
            "bob",
            "join-bob",
            FixtureOperation::RedeemInviteRef {
                invite_ref: "bob".to_owned(),
            },
        ),
        command("host", "seat-host", FixtureOperation::TakeSeat { seat: 0 }),
        command(
            "alice",
            "seat-alice",
            FixtureOperation::TakeSeat { seat: 1 },
        ),
        command("host", "ready-host", FixtureOperation::Ready),
        command("alice", "ready-alice", FixtureOperation::Ready),
        command(
            "host",
            "arm-abort",
            FixtureOperation::ArmCountdown {
                deadline_tick: 10,
                token: "countdown-abort".to_owned(),
            },
        ),
        command("alice", "abort", FixtureOperation::AbortCountdown),
        command(
            "host",
            "arm-final",
            FixtureOperation::ArmCountdown {
                deadline_tick: 20,
                token: "countdown-final".to_owned(),
            },
        ),
        command(
            "system-clock",
            "expire-final",
            FixtureOperation::CountdownExpired {
                token: "countdown-final".to_owned(),
            },
        ),
        command(
            "system-game",
            "deal-standard",
            FixtureOperation::ApplyStandardChance,
        ),
        command("alice", "pause", FixtureOperation::Pause),
        command(
            "host",
            "action-paused",
            FixtureOperation::GameBid { tricks: 0 },
        ),
        command("host", "unpause", FixtureOperation::Unpause),
        command(
            "bob",
            "chat-bob",
            FixtureOperation::Chat {
                text: "inspectable hello".to_owned(),
            },
        ),
        command(
            "bob",
            "request-denied",
            FixtureOperation::RequestHand {
                player: "host".to_owned(),
            },
        ),
        command(
            "host",
            "deny-request",
            FixtureOperation::DenyHand {
                request_id: "request-denied".to_owned(),
                player: "host".to_owned(),
                recipient: "bob".to_owned(),
            },
        ),
        command(
            "bob",
            "request-granted",
            FixtureOperation::RequestHand {
                player: "host".to_owned(),
            },
        ),
        command(
            "host",
            "grant-bob",
            FixtureOperation::GrantHand {
                request_id: "request-granted".to_owned(),
                player: "host".to_owned(),
                recipient: "bob".to_owned(),
                grant_epoch: 1,
            },
        ),
        FixtureInput::DuplicateCommand {
            command_id: "grant-bob".to_owned(),
        },
        command(
            "host",
            "revoke-bob",
            FixtureOperation::RevokeHand {
                player: "host".to_owned(),
                recipient: "bob".to_owned(),
                grant_epoch: 1,
            },
        ),
        command(
            "bob",
            "request-round",
            FixtureOperation::RequestHand {
                player: "host".to_owned(),
            },
        ),
        command(
            "host",
            "grant-round",
            FixtureOperation::GrantHand {
                request_id: "request-round".to_owned(),
                player: "host".to_owned(),
                recipient: "bob".to_owned(),
                grant_epoch: 3,
            },
        ),
        FixtureInput::TransportDisconnect {
            principal: "alice".to_owned(),
            observation_id: "disconnect-alice".to_owned(),
        },
        command("alice", "reconnect-alice", FixtureOperation::Reconnect),
        command(
            "host",
            "action-host",
            FixtureOperation::GameBid { tricks: 0 },
        ),
        command(
            "alice",
            "action-alice",
            FixtureOperation::GameBid { tricks: 0 },
        ),
        command("system-game", "settle", FixtureOperation::Settle),
        FixtureInput::Command {
            principal: "bob".to_owned(),
            command_id: "stale-chat".to_owned(),
            revision: FixtureRevision::Previous(1),
            operation: FixtureOperation::Chat {
                text: "must be stale".to_owned(),
            },
        },
        command("host", "reset", FixtureOperation::ResetLobby),
        command("host", "close", FixtureOperation::CloseRoom),
    ]
}

fn command(actor: &str, command_id: &str, operation: FixtureOperation) -> FixtureInput {
    FixtureInput::Command {
        principal: actor.to_owned(),
        command_id: command_id.to_owned(),
        revision: FixtureRevision::Current,
        operation,
    }
}

#[derive(Serialize)]
struct SemanticStateView {
    room_id: String,
    session_epoch: u64,
    revision: u64,
    phase: String,
    host: Option<String>,
    members: Vec<SemanticMember>,
    invites: Vec<SemanticInvite>,
    processed: Vec<SemanticProcessed>,
    projection_epoch: u64,
    requests: Vec<(String, String, String)>,
    grants: Vec<(String, String, u64)>,
    public_history: Vec<poche_protocol::PublicGameEventWire>,
    public_game: Option<GamePublicStateWire>,
    private_hands: BTreeMap<String, Vec<u8>>,
    policies: usize,
    chat_count: u64,
}

#[derive(Serialize)]
struct SemanticMember {
    principal: String,
    membership_epoch: u64,
    connected: bool,
    seat: Option<u8>,
    ready: bool,
    host: bool,
}

#[derive(Serialize)]
struct SemanticInvite {
    expires_after_revision: u64,
    consumed: bool,
    revoked: bool,
}

#[derive(Serialize)]
struct SemanticProcessed {
    command_id: String,
    command_hash: String,
    events: usize,
}

fn state_hash(state: &SessionState<TranscriptGame>) -> Result<String, String> {
    let game = active_game(&state.phase);
    let public_game = game
        .map(SessionGame::public_projection)
        .transpose()
        .map_err(str::to_owned)?;
    let mut private_hands = BTreeMap::new();
    if let Some(game) = game {
        for member in state.members.iter().filter(|member| member.seat.is_some()) {
            let seat = member.seat.ok_or_else(|| "seat disappeared".to_owned())?;
            private_hands.insert(
                member.principal_id.as_str().to_owned(),
                game.private_hand(seat).map_err(str::to_owned)?,
            );
        }
    }
    let view = SemanticStateView {
        room_id: state.room_id.as_str().to_owned(),
        session_epoch: state.session_epoch,
        revision: state.revision,
        phase: phase_label(&state.phase).to_owned(),
        host: state.host.as_ref().map(|host| host.as_str().to_owned()),
        members: state
            .members
            .iter()
            .map(|member| SemanticMember {
                principal: member.principal_id.as_str().to_owned(),
                membership_epoch: member.membership_epoch,
                connected: member.connection == ConnectionState::Connected,
                seat: member.seat,
                ready: member.ready,
                host: member.host,
            })
            .collect(),
        invites: state
            .invites
            .iter()
            .map(|invite| SemanticInvite {
                expires_after_revision: invite.expires_after_revision,
                consumed: invite.consumed,
                revoked: invite.revoked,
            })
            .collect(),
        processed: state
            .processed_commands
            .iter()
            .map(|record| SemanticProcessed {
                command_id: record.command_id.as_str().to_owned(),
                command_hash: hash_hex(&record.command_hash),
                events: record.events.len(),
            })
            .collect(),
        projection_epoch: state.projection_epoch,
        requests: state
            .hand_requests
            .iter()
            .map(|request| {
                (
                    request.request_id.as_str().to_owned(),
                    request.player.as_str().to_owned(),
                    request.recipient.as_str().to_owned(),
                )
            })
            .collect(),
        grants: state
            .hand_grants
            .iter()
            .map(|grant| {
                (
                    grant.player.as_str().to_owned(),
                    grant.recipient.as_str().to_owned(),
                    grant.grant_epoch,
                )
            })
            .collect(),
        public_history: state.public_history.clone(),
        public_game,
        private_hands,
        policies: state.policies.len(),
        chat_count: state.chat_count,
    };
    let bytes = serde_json::to_vec(&view).map_err(|error| error.to_string())?;
    Ok(hash_bytes(&bytes))
}

fn projection_hashes(
    state: &SessionState<TranscriptGame>,
) -> Result<BTreeMap<String, String>, String> {
    let mut hashes = BTreeMap::new();
    for member in &state.members {
        let value = match project_viewer(state, &member.principal_id, state.projection_epoch) {
            Ok(projection) => {
                let bytes = serde_json::to_vec(&projection).map_err(|error| error.to_string())?;
                hash_bytes(&bytes)
            }
            Err(ProjectionError::NotConnected) => "error:not-connected".to_owned(),
            Err(ProjectionError::UnknownViewer) => "error:unknown-viewer".to_owned(),
            Err(ProjectionError::StaleProjectionEpoch) => "error:stale-epoch".to_owned(),
            Err(ProjectionError::Game(error)) => return Err(error.to_owned()),
        };
        hashes.insert(member.principal_id.as_str().to_owned(), value);
    }
    Ok(hashes)
}

fn snapshot_payload(
    state: &SessionState<TranscriptGame>,
    state_bytes: Vec<u8>,
) -> Result<SnapshotPayload, String> {
    let state_hash_bytes = hex_to_hash(&state_hash(state)?)?;
    let projection = state
        .members
        .iter()
        .find(|member| member.connection == ConnectionState::Connected)
        .map(|member| project_viewer(state, &member.principal_id, state.projection_epoch))
        .transpose()
        .map_err(|error| format!("snapshot projection: {error:?}"))?;
    Ok(SnapshotPayload {
        schema_hash: protocol_schema_hash(),
        state_hash: state_hash_bytes,
        event_tail_revision: state.revision,
        phase: projection
            .as_ref()
            .map_or(poche_protocol::RoomPhase::Closed, |value| value.phase),
        members: projection.map_or_else(Vec::new, |value| value.members),
        state: state_bytes,
    })
}

fn compare_transcripts(
    expected: &GoldenTranscript,
    actual: &GoldenTranscript,
) -> Result<(), String> {
    if expected.fixture_id != actual.fixture_id {
        return Err(format!(
            "fixture id: expected {}, found {}",
            expected.fixture_id, actual.fixture_id
        ));
    }
    if expected.steps.len() != actual.steps.len() {
        return Err(format!(
            "step count: expected {}, found {}",
            expected.steps.len(),
            actual.steps.len()
        ));
    }
    for (index, (left, right)) in expected.steps.iter().zip(&actual.steps).enumerate() {
        if left == right {
            continue;
        }
        let field = if left.input != right.input {
            "input"
        } else if left.command_hash != right.command_hash {
            "command_hash"
        } else if left.authorization != right.authorization {
            "authorization"
        } else if left.outcome != right.outcome {
            "outcome"
        } else if left.events != right.events {
            "events"
        } else if left.state_hash != right.state_hash {
            "state_hash"
        } else if left.scoped_projections != right.scoped_projections {
            "scoped_projections"
        } else {
            "projection_hashes"
        };
        return Err(format!("step {index} first divergence: {field}"));
    }
    if expected.snapshot != actual.snapshot {
        return Err("snapshot evidence first divergence".to_owned());
    }
    if expected.final_state_hash != actual.final_state_hash {
        return Err("final state hash first divergence".to_owned());
    }
    Ok(())
}

fn scoped_projection_checkpoint(
    state: &SessionState<TranscriptGame>,
    input: &FixtureInput,
) -> Result<BTreeMap<String, ProjectionPayload>, String> {
    let checkpoint = match input {
        FixtureInput::Command { command_id, .. } => matches!(
            command_id.as_str(),
            "expire-final"
                | "grant-bob"
                | "revoke-bob"
                | "grant-round"
                | "reconnect-alice"
                | "settle"
                | "close"
        ),
        FixtureInput::TransportDisconnect { .. } => true,
        FixtureInput::DuplicateCommand { .. } => false,
    };
    if !checkpoint {
        return Ok(BTreeMap::new());
    }
    let mut projections = BTreeMap::new();
    for member in state
        .members
        .iter()
        .filter(|member| member.connection == ConnectionState::Connected)
    {
        let projection = project_viewer(state, &member.principal_id, state.projection_epoch)
            .map_err(|error| format!("checkpoint projection: {error:?}"))?;
        projections.insert(member.principal_id.as_str().to_owned(), projection);
    }
    Ok(projections)
}

fn decision_label(decision: &PolicyDecision) -> String {
    match decision {
        PolicyDecision::Allow { policy_id, .. } => format!("allow:{}", policy_id.as_str()),
        PolicyDecision::Deny { reason, .. } => {
            format!(
                "deny:{}",
                deny_code(*reason).unwrap_or_else(|_| "D-INVALID".to_owned())
            )
        }
    }
}

fn deny_code(reason: DenyReason) -> Result<String, String> {
    serde_json::to_string(&reason)
        .map(|code| code.trim_matches('"').to_owned())
        .map_err(|error| error.to_string())
}

fn event_label(event: &SessionEvent<TranscriptGame>) -> String {
    match &event.kind {
        SessionEventKind::RoomCreated { .. } => "room-created",
        SessionEventKind::MemberJoined { .. } => "member-joined",
        SessionEventKind::SeatTaken { .. } => "seat-taken",
        SessionEventKind::SeatReleased { .. } => "seat-released",
        SessionEventKind::ReadyChanged { .. } => "ready-changed",
        SessionEventKind::CountdownArmed { .. } => "countdown-armed",
        SessionEventKind::CountdownCancelled { .. } => "countdown-cancelled",
        SessionEventKind::GameStarted { .. } => "game-started",
        SessionEventKind::GameAdvanced { .. } => "game-advanced",
        SessionEventKind::Paused => "paused",
        SessionEventKind::Unpaused => "unpaused",
        SessionEventKind::MemberDisconnected { .. } => "member-disconnected",
        SessionEventKind::MemberReconnected { .. } => "member-reconnected",
        SessionEventKind::MemberLeft { .. } => "member-left",
        SessionEventKind::MemberRemoved { .. } => "member-removed",
        SessionEventKind::LobbyReset => "lobby-reset",
        SessionEventKind::ChatPosted { .. } => "chat-posted",
        SessionEventKind::HandViewRequested { .. } => "hand-view-requested",
        SessionEventKind::HandViewGranted { .. } => "hand-view-granted",
        SessionEventKind::HandViewDenied { .. } => "hand-view-denied",
        SessionEventKind::HandViewRevoked { .. } => "hand-view-revoked",
        SessionEventKind::HandCapabilitiesExpired { reason } => match reason {
            HandCapabilityExpiry::RoundBoundary => "hand-capabilities-expired-round",
            HandCapabilityExpiry::SeatRoleChanged { .. } => "hand-capabilities-expired-seat-role",
            HandCapabilityExpiry::MembershipLost { .. } => "hand-capabilities-expired-membership",
        },
        SessionEventKind::RoomClosed => "room-closed",
    }
    .to_owned()
}

fn active_game<G>(phase: &SessionPhase<G>) -> Option<&G> {
    match phase {
        SessionPhase::Running { game }
        | SessionPhase::Paused { game }
        | SessionPhase::PostGame { game } => Some(game),
        SessionPhase::Uninitialized
        | SessionPhase::Lobby
        | SessionPhase::Countdown { .. }
        | SessionPhase::Closed => None,
    }
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

fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn hash_hex(hash: &SemanticHash) -> String {
    hash.0
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("writing to a String is infallible");
            hex
        })
}

fn hex_to_hash(value: &str) -> Result<SemanticHash, String> {
    if value.len() != 64 {
        return Err("semantic hash must contain 64 lowercase hex characters".to_owned());
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = core::str::from_utf8(chunk).map_err(|error| error.to_string())?;
        bytes[index] = u8::from_str_radix(text, 16).map_err(|error| error.to_string())?;
    }
    Ok(SemanticHash(bytes))
}

fn principal(value: &str) -> Result<PrincipalId, String> {
    PrincipalId::new(value).map_err(|error| error.to_string())
}

fn command_id(value: &str) -> Result<CommandId, String> {
    CommandId::new(value).map_err(|error| error.to_string())
}

fn correlation(value: &str) -> Result<CorrelationId, String> {
    CorrelationId::new(value).map_err(|error| error.to_string())
}

fn countdown(value: &str) -> Result<CountdownToken, String> {
    CountdownToken::new(value).map_err(|error| error.to_string())
}

fn room(value: &str) -> Result<RoomId, String> {
    RoomId::new(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_transcript_replays_from_genesis_and_snapshot_tail() {
        let rendered = render_builtin_transcript().unwrap();
        let verification = verify_transcript(&rendered).unwrap();
        assert_eq!(verification.fixture_id, FIXTURE_ID);
        assert_eq!(verification.steps, 32);
    }

    #[test]
    fn deletion_reorder_and_controlled_apply_defect_report_first_divergence() {
        let expected = generate_transcript(
            &builtin_inputs(),
            23,
            &ControlledReplayDefect::None,
            ReplayCodec::Typed,
        )
        .unwrap();

        let mut deleted = expected.clone();
        deleted.steps.remove(15);
        let error = verify_transcript(&serde_json::to_string(&deleted).unwrap()).unwrap_err();
        assert!(error.starts_with("step 15 first divergence:"), "{error}");

        let mut reordered = expected.clone();
        reordered.steps.swap(16, 17);
        let error = verify_transcript(&serde_json::to_string(&reordered).unwrap()).unwrap_err();
        assert!(error.starts_with("step 16 first divergence:"), "{error}");

        let (_, defective_steps) = run_inputs(
            new_context().unwrap(),
            &builtin_inputs(),
            &ControlledReplayDefect::DropFirstEvent {
                command_id: "revoke-bob".to_owned(),
            },
            ReplayCodec::Typed,
        )
        .unwrap();
        let mut defective = expected.clone();
        defective.steps = defective_steps;
        let error = compare_transcripts(&expected, &defective).unwrap_err();
        assert_eq!(error, "step 21 first divergence: state_hash");
    }

    #[test]
    fn persisted_fixture_vocabulary_contains_no_invite_secret() {
        let rendered = render_builtin_transcript().unwrap();
        assert!(!rendered.contains(ALICE_INVITE));
        assert!(!rendered.contains(BOB_INVITE));
        assert!(rendered.contains("invite_ref"));
    }

    #[test]
    fn checked_fixture_has_exact_typed_and_canonical_ndjson_semantic_parity() {
        let fixture = include_str!("../../../tests/fixtures/protocol/session-micro-v1.json");
        let verification = verify_transcript_codec_parity(fixture).unwrap();
        assert_eq!(verification.steps, 32);
        let script =
            include_str!("../../../tests/fixtures/protocol/session-micro-v1.script.ndjson");
        let script_verification =
            verify_fixture_script_against_transcript(script, fixture).unwrap();
        assert_eq!(script_verification, verification);
    }

    #[test]
    fn canonical_script_replay_is_complete_inspectable_and_secret_free() {
        let script = render_builtin_script_ndjson().unwrap();
        let replay = replay_fixture_script_ndjson(&script).unwrap();
        assert_eq!(script.lines().count(), 32);
        assert_eq!(replay.output_ndjson.lines().count(), 33);
        assert!(
            replay
                .output_ndjson
                .lines()
                .all(|line| serde_json::from_str::<TranscriptOutputRecord>(line).is_ok())
        );
        for marker in [
            "actor=host command=create",
            "actor=alice command=abort",
            "actor=host command=arm-final",
            "action=pause",
            "outcome: deny:D-PAUSED",
            "action=unpause",
            "action=chat:inspectable hello",
            "action=grant-hand",
            "action=revoke-hand",
            "events: hand-capabilities-expired-round, game-advanced",
            "final-state:",
        ] {
            assert!(replay.text.contains(marker), "missing text marker {marker}");
        }
        let persisted = format!("{script}{}{}", replay.output_ndjson, replay.text);
        assert!(!persisted.contains(ALICE_INVITE));
        assert!(!persisted.contains(BOB_INVITE));
    }

    #[test]
    fn fixture_script_framing_fails_closed() {
        let script = render_builtin_script_ndjson().unwrap();
        assert!(replay_fixture_script_ndjson(script.trim_end()).is_err());
        assert!(replay_fixture_script_ndjson(&script.replace('\n', "\r\n")).is_err());
        let first = script.lines().next().unwrap();
        let noncanonical = format!(" {first}\n");
        assert!(replay_fixture_script_ndjson(&noncanonical).is_err());
    }
}
