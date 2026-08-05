use std::fmt::Write;

use poche_protocol::{
    GameActionWire, ProjectionPayload, PublicGameEventWire, PublicTurnWire, RoomPhase,
};

use crate::{FixtureInput, FixtureOperation, FixtureRevision, GoldenTranscript};

/// Pure text presentation of one exact viewer projection and supplied legal
/// action list. It cannot access authoritative hidden state.
#[must_use]
pub fn render_projection_text(
    viewer: &str,
    projection: &ProjectionPayload,
    legal_actions: &[GameActionWire],
) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "viewer: {viewer}");
    let _ = writeln!(output, "phase: {}", phase_name(projection.phase));
    let members = projection
        .members
        .iter()
        .map(|member| {
            let role = if member.host {
                "host"
            } else if member.seat.is_some() {
                "player"
            } else {
                "spectator"
            };
            format!(
                "{}({role},seat={},ready={},connected={})",
                member.principal_id.as_str(),
                member
                    .seat
                    .map_or_else(|| "-".to_owned(), |seat| seat.to_string()),
                member.ready,
                member.connected
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(output, "members: {members}");

    if let Some(game) = &projection.public_game_state {
        let _ = writeln!(
            output,
            "game: phase={:?} round={} hand-size={} actor={} scores={:?} pot-cents={}",
            game.phase,
            game.round_index,
            game.hand_size,
            turn_name(game.actor),
            game.scores,
            game.pot_cents
        );
        let _ = writeln!(output, "hand-counts: {:?}", game.hand_counts);
    }
    if let Some(hand) = &projection.own_hand {
        let _ = writeln!(output, "own-hand: {:?}", hand.cards);
    }
    for hand in &projection.granted_hands {
        let _ = writeln!(
            output,
            "granted-hand: player={} epoch={} cards={:?}",
            hand.player.as_str(),
            hand.grant_epoch,
            hand.cards
        );
    }
    let actions = legal_actions
        .iter()
        .map(action_name)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        output,
        "legal-actions: {}",
        if actions.is_empty() { "-" } else { &actions }
    );
    if !projection.public_history.is_empty() {
        let _ = writeln!(
            output,
            "history: {}",
            projection
                .public_history
                .iter()
                .map(history_name)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    output
}

/// Render attributed chat as escaped data on one physical line.
#[must_use]
pub fn render_chat_text(principal: &str, text: &str) -> String {
    format!("chat {principal}: {}\n", text.escape_default())
}

/// Render a runtime-supplied logical countdown estimate without reading a
/// clock inside presentation code.
#[must_use]
pub fn render_countdown_text(logical_now: u64, deadline_tick: u64) -> String {
    format!(
        "countdown: now={logical_now} deadline={deadline_tick} remaining={}\n",
        deadline_tick.saturating_sub(logical_now)
    )
}

/// Render every script action, authorization decision, outcome, event, state
/// hash, and viewer hash from an already verified typed transcript.
#[must_use]
pub fn render_transcript_text(transcript: &GoldenTranscript) -> String {
    let mut output = format!(
        "fixture: {}\nsteps: {}\n\n",
        transcript.fixture_id,
        transcript.steps.len()
    );
    for (index, step) in transcript.steps.iter().enumerate() {
        let _ = writeln!(output, "[{index:02}] {}", fixture_input_name(&step.input));
        let _ = writeln!(output, "  decision: {}", step.authorization);
        let _ = writeln!(output, "  outcome: {}", step.outcome);
        let _ = writeln!(
            output,
            "  events: {}",
            if step.events.is_empty() {
                "-".to_owned()
            } else {
                step.events.join(", ")
            }
        );
        let _ = writeln!(output, "  state: {}", step.state_hash);
        let _ = writeln!(
            output,
            "  views: {}",
            step.projection_hashes
                .iter()
                .map(|(viewer, hash)| format!("{viewer}={hash}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(output, "\nfinal-state: {}", transcript.final_state_hash);
    output
}

const fn phase_name(phase: RoomPhase) -> &'static str {
    match phase {
        RoomPhase::Lobby => "lobby",
        RoomPhase::Countdown => "countdown",
        RoomPhase::Running => "running",
        RoomPhase::Paused => "paused",
        RoomPhase::PostGame => "post-game",
        RoomPhase::Closed => "closed",
    }
}

fn turn_name(turn: PublicTurnWire) -> String {
    match turn {
        PublicTurnWire::Chance => "chance".to_owned(),
        PublicTurnWire::Player(seat) => format!("player-{seat}"),
        PublicTurnWire::Environment => "environment".to_owned(),
        PublicTurnWire::Finished => "finished".to_owned(),
    }
}

fn action_name(action: &GameActionWire) -> String {
    match action {
        GameActionWire::Bid { tricks } => format!("bid:{tricks}"),
        GameActionWire::Play { card } => format!("play:{card}"),
    }
}

fn history_name(event: &PublicGameEventWire) -> String {
    match event {
        PublicGameEventWire::GameStarted { command_id } => {
            format!("game-started@{}", command_id.as_str())
        }
        PublicGameEventWire::PlayerAction {
            command_id,
            seat,
            action,
            ..
        } => format!(
            "player-action@{}:seat-{seat}:{}",
            command_id.as_str(),
            action_name(action)
        ),
        PublicGameEventWire::RoundScored {
            command_id,
            scores,
            terminal,
            ..
        } => format!(
            "round-scored@{}:{scores:?}:terminal={terminal}",
            command_id.as_str()
        ),
    }
}

fn fixture_input_name(input: &FixtureInput) -> String {
    match input {
        FixtureInput::Command {
            principal,
            command_id,
            revision,
            operation,
        } => format!(
            "actor={principal} command={command_id} revision={} action={}",
            revision_name(revision),
            operation_name(operation)
        ),
        FixtureInput::DuplicateCommand { command_id } => {
            format!("transport duplicate command={command_id}")
        }
        FixtureInput::TransportDisconnect {
            principal,
            observation_id,
        } => format!("transport disconnect principal={principal} observation={observation_id}"),
    }
}

fn revision_name(revision: &FixtureRevision) -> String {
    match revision {
        FixtureRevision::Current => "current".to_owned(),
        FixtureRevision::Previous(distance) => format!("previous-{distance}"),
    }
}

fn operation_name(operation: &FixtureOperation) -> String {
    match operation {
        FixtureOperation::CreateRoom => "create-room".to_owned(),
        FixtureOperation::RedeemInviteRef { invite_ref } => {
            format!("redeem-invite-ref:{invite_ref}")
        }
        FixtureOperation::TakeSeat { seat } => format!("take-seat:{seat}"),
        FixtureOperation::Ready => "ready".to_owned(),
        FixtureOperation::ArmCountdown {
            deadline_tick,
            token,
        } => format!("arm-countdown:{deadline_tick}:{token}"),
        FixtureOperation::AbortCountdown => "abort-countdown".to_owned(),
        FixtureOperation::CountdownExpired { token } => {
            format!("countdown-expired:{token}")
        }
        FixtureOperation::ApplyStandardChance => "apply-standard-chance".to_owned(),
        FixtureOperation::Pause => "pause".to_owned(),
        FixtureOperation::Unpause => "unpause".to_owned(),
        FixtureOperation::GameBid { tricks } => format!("game-bid:{tricks}"),
        FixtureOperation::Settle => "settle".to_owned(),
        FixtureOperation::Chat { text } => format!("chat:{}", text.escape_default()),
        FixtureOperation::RequestHand { player } => format!("request-hand:{player}"),
        FixtureOperation::GrantHand {
            request_id,
            player,
            recipient,
            grant_epoch,
        } => format!(
            "grant-hand:request={request_id}:player={player}:recipient={recipient}:epoch={grant_epoch}"
        ),
        FixtureOperation::DenyHand {
            request_id,
            player,
            recipient,
        } => format!("deny-hand:request={request_id}:player={player}:recipient={recipient}"),
        FixtureOperation::RevokeHand {
            player,
            recipient,
            grant_epoch,
        } => format!("revoke-hand:player={player}:recipient={recipient}:epoch={grant_epoch}"),
        FixtureOperation::Reconnect => "reconnect".to_owned(),
        FixtureOperation::ResetLobby => "reset-lobby".to_owned(),
        FixtureOperation::CloseRoom => "close-room".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        GamePublicStateWire, HandProjection, MemberProjection, PrincipalId, PublicGamePhase,
    };

    use super::*;

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).unwrap()
    }

    fn projection(phase: RoomPhase) -> ProjectionPayload {
        ProjectionPayload {
            phase,
            members: vec![
                MemberProjection {
                    principal_id: principal("alice"),
                    connected: true,
                    seat: Some(0),
                    ready: true,
                    host: true,
                },
                MemberProjection {
                    principal_id: principal("spectator"),
                    connected: true,
                    seat: None,
                    ready: false,
                    host: false,
                },
            ],
            public_game_state: None,
            own_hand: None,
            granted_hands: Vec::new(),
            public_history: Vec::new(),
        }
    }

    fn game(phase: PublicGamePhase) -> GamePublicStateWire {
        GamePublicStateWire {
            schema_version: 1,
            phase,
            dealer: Some(0),
            actor: if phase == PublicGamePhase::Finished {
                PublicTurnWire::Finished
            } else {
                PublicTurnWire::Player(0)
            },
            round_index: 2,
            hand_size: 3,
            hand_counts: vec![3, 3],
            trump: Some(10),
            current_trick: Vec::new(),
            bids: vec![Some(1), Some(2)],
            tricks_won: vec![0, 1],
            scores: vec![7, 4],
            pot_cents: 50,
        }
    }

    #[test]
    fn text_projection_covers_every_user_visible_state_shape() {
        let lobby = render_projection_text("alice", &projection(RoomPhase::Lobby), &[]);
        assert!(lobby.contains("phase: lobby"));
        assert!(lobby.contains("spectator(spectator"));

        let countdown = render_projection_text("alice", &projection(RoomPhase::Countdown), &[]);
        assert!(countdown.contains("phase: countdown"));

        let mut running = projection(RoomPhase::Running);
        running.public_game_state = Some(game(PublicGamePhase::Playing));
        running.own_hand = Some(HandProjection {
            player: principal("alice"),
            grant_epoch: 1,
            cards: vec![1, 2, 3],
        });
        let running = render_projection_text(
            "alice",
            &running,
            &[
                GameActionWire::Bid { tricks: 1 },
                GameActionWire::Play { card: 2 },
            ],
        );
        assert!(running.contains("phase: running"));
        assert!(running.contains("actor=player-0"));
        assert!(running.contains("own-hand: [1, 2, 3]"));
        assert!(running.contains("legal-actions: bid:1, play:2"));

        let paused = render_projection_text("alice", &projection(RoomPhase::Paused), &[]);
        assert!(paused.contains("phase: paused"));

        let mut granted = projection(RoomPhase::Running);
        granted.granted_hands.push(HandProjection {
            player: principal("alice"),
            grant_epoch: 3,
            cards: vec![4, 5],
        });
        let granted = render_projection_text("spectator", &granted, &[]);
        assert!(granted.contains("granted-hand: player=alice epoch=3 cards=[4, 5]"));

        let mut result = projection(RoomPhase::PostGame);
        result.public_game_state = Some(game(PublicGamePhase::Finished));
        let result = render_projection_text("alice", &result, &[]);
        assert!(result.contains("phase: post-game"));
        assert!(result.contains("scores=[7, 4]"));

        assert_eq!(
            render_chat_text("alice", "hello\nworld"),
            "chat alice: hello\\nworld\n"
        );
        assert_eq!(
            render_countdown_text(7, 10),
            "countdown: now=7 deadline=10 remaining=3\n"
        );
    }
}
