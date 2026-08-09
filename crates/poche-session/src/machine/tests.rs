use poche_protocol::{
    ChanceWire, CommandId, CommandPayload, CorrelationId, CountdownToken, GameActionWire,
    GamePublicStateWire, InviteProof, PROTOCOL_VERSION_V1, PrincipalId, PublicGamePhase,
    PublicTurnWire, RoomId, SIGNATURE_DOMAIN_V1, SignatureAlgorithm, SignatureBytes,
    SignatureIntent, UnsignedCommandEnvelope,
};

use super::*;
use crate::{
    ConnectionState, InviteRecord, PolicyRule, PrincipalKind, PrincipalSelector, ProjectionError,
    SessionGame, SessionInvariantError, local_host_diagnostic_capability,
    project_local_host_diagnostics, project_viewer,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestGame {
    turn: GameTurn,
    steps: u8,
    hands: [Vec<u8>; 2],
}

impl SessionGame for TestGame {
    type Error = &'static str;

    fn start(seats: &[(u8, PrincipalId)]) -> Result<Self, Self::Error> {
        if seats.len() < 2 {
            return Err("two seats required");
        }
        Ok(Self {
            turn: GameTurn::Chance,
            steps: 0,
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
            hand_counts: self
                .hands
                .iter()
                .map(|hand| u8::try_from(hand.len()).unwrap())
                .collect(),
            trump: Some(51),
            current_trick: Vec::new(),
            bids: vec![Some(0), Some(0)],
            tricks_won: vec![0, 0],
            scores: vec![0, 0],
            pot_cents: 50,
        })
    }

    fn private_hand(&self, seat: u8) -> Result<Vec<u8>, Self::Error> {
        self.hands
            .get(usize::from(seat))
            .cloned()
            .ok_or("invalid seat")
    }

    fn player_transition(
        &self,
        seat: u8,
        _action: &GameActionWire,
    ) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Player(seat) {
            return Err("wrong actor");
        }
        let turn = if seat == 0 {
            GameTurn::Player(1)
        } else {
            GameTurn::Environment
        };
        Ok(GameTransition {
            game: Self {
                turn,
                steps: self.steps + 1,
                hands: self.hands.clone(),
            },
            round_scores: None,
            terminal: false,
        })
    }

    fn chance_transition(&self, _chance: &ChanceWire) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Chance {
            return Err("chance unavailable");
        }
        Ok(GameTransition {
            game: Self {
                turn: GameTurn::Player(0),
                steps: self.steps + 1,
                hands: self.hands.clone(),
            },
            round_scores: None,
            terminal: false,
        })
    }

    fn settle(&self) -> Result<GameTransition<Self>, Self::Error> {
        if self.turn != GameTurn::Environment {
            return Err("settlement unavailable");
        }
        Ok(GameTransition {
            game: Self {
                turn: GameTurn::Finished,
                steps: self.steps + 1,
                hands: self.hands.clone(),
            },
            round_scores: Some(vec![7, -7]),
            terminal: true,
        })
    }
}

fn id<T>(
    value: &str,
    constructor: impl FnOnce(String) -> Result<T, poche_protocol::IdentifierError>,
) -> T {
    constructor(value.to_owned()).unwrap()
}

fn principal(value: &str) -> PrincipalId {
    id(value, PrincipalId::new)
}

fn command_id(value: &str) -> CommandId {
    id(value, CommandId::new)
}

fn correlation(value: &str) -> CorrelationId {
    id(value, CorrelationId::new)
}

fn room(value: &str) -> RoomId {
    id(value, RoomId::new)
}

fn pending() -> SessionState<TestGame> {
    SessionState::pending(
        room("room-test"),
        principal("system-clock"),
        principal("system-game"),
    )
}

fn signed(
    state: &SessionState<TestGame>,
    principal_id: PrincipalId,
    command_id: &str,
    payload: CommandPayload,
) -> CommandEnvelope {
    UnsignedCommandEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id: state.room_id.clone(),
        session_epoch: state.session_epoch,
        command_id: tests_command_id(command_id),
        principal_id: principal_id.clone(),
        expected_revision: state.revision,
        correlation_id: correlation(&format!("cor-{command_id}")),
        causation_id: None,
        payload,
        signature_intent: SignatureIntent {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: principal_id,
        },
    }
    .attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
}

fn execute(
    state: &SessionState<TestGame>,
    command: &CommandEnvelope,
) -> (SessionState<TestGame>, Vec<SessionEvent<TestGame>>) {
    let decision = authorize(state, command);
    assert!(
        decision.is_allowed(),
        "unexpected denial for {:?} by {:?}: {decision:?}",
        command.payload.kind(),
        state.principal_kind(&command.principal_id)
    );
    let authorized = AuthorizedCommand::from_decision(command.clone(), decision).unwrap();
    let events = decide(state, &authorized).unwrap();
    assert!(!events.is_empty());
    let mut next = state.clone();
    for event in &events {
        next = apply(&next, event).unwrap();
    }
    (next, events)
}

fn tests_command_id(value: &str) -> CommandId {
    command_id(value)
}

fn created_room() -> SessionState<TestGame> {
    let pending = pending();
    let create = signed(
        &pending,
        principal("host"),
        "create",
        CommandPayload::CreateRoom,
    );
    execute(&pending, &create).0
}

fn two_player_lobby() -> SessionState<TestGame> {
    let mut state = created_room();
    state
        .invites
        .push(InviteRecord::new("invite-alice", 100).unwrap());
    let join = signed(
        &state,
        principal("alice"),
        "join-alice",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new("invite-alice").unwrap(),
        },
    );
    state = execute(&state, &join).0;
    let host_seat = signed(
        &state,
        principal("host"),
        "seat-host",
        CommandPayload::TakeSeat { seat: 0 },
    );
    state = execute(&state, &host_seat).0;
    let alice_seat = signed(
        &state,
        principal("alice"),
        "seat-alice",
        CommandPayload::TakeSeat { seat: 1 },
    );
    execute(&state, &alice_seat).0
}

fn ready_lobby() -> SessionState<TestGame> {
    let mut state = two_player_lobby();
    for (who, command_name) in [("host", "ready-host"), ("alice", "ready-alice")] {
        let ready = signed(&state, principal(who), command_name, CommandPayload::Ready);
        state = execute(&state, &ready).0;
    }
    state
}

fn deny_reason(decision: &PolicyDecision) -> DenyReason {
    let PolicyDecision::Deny { reason, .. } = decision else {
        panic!("expected denial")
    };
    *reason
}

#[test]
#[allow(clippy::too_many_lines)]
fn lifecycle_countdown_pause_game_and_reset_are_phase_typed() {
    let mut state = ready_lobby();
    let arm = signed(
        &state,
        principal("host"),
        "arm-first",
        CommandPayload::ArmCountdown {
            deadline_tick: 10,
            countdown_token: id("countdown-first", CountdownToken::new),
        },
    );
    state = execute(&state, &arm).0;
    assert!(matches!(state.phase, SessionPhase::Countdown { .. }));

    let abort = signed(
        &state,
        principal("alice"),
        "abort",
        CommandPayload::AbortCountdown,
    );
    state = execute(&state, &abort).0;
    assert!(matches!(state.phase, SessionPhase::Lobby));
    assert!(state.members.iter().all(|member| member.ready));

    let arm = signed(
        &state,
        principal("host"),
        "arm-race",
        CommandPayload::ArmCountdown {
            deadline_tick: 20,
            countdown_token: id("countdown-race", CountdownToken::new),
        },
    );
    state = execute(&state, &arm).0;
    let stale_expiry = signed(
        &state,
        principal("system-clock"),
        "expiry-race",
        CommandPayload::CountdownExpired {
            countdown_token: id("countdown-race", CountdownToken::new),
        },
    );
    let unready = signed(
        &state,
        principal("alice"),
        "unready-race",
        CommandPayload::Unready,
    );
    state = execute(&state, &unready).0;
    assert!(matches!(state.phase, SessionPhase::Lobby));
    assert_eq!(
        deny_reason(&authorize(&state, &stale_expiry)),
        DenyReason::StaleRevision
    );

    let ready = signed(
        &state,
        principal("alice"),
        "ready-again",
        CommandPayload::Ready,
    );
    state = execute(&state, &ready).0;
    let arm = signed(
        &state,
        principal("host"),
        "arm-final",
        CommandPayload::ArmCountdown {
            deadline_tick: 30,
            countdown_token: id("countdown-final", CountdownToken::new),
        },
    );
    state = execute(&state, &arm).0;
    let expire = signed(
        &state,
        principal("system-clock"),
        "expiry-final",
        CommandPayload::CountdownExpired {
            countdown_token: id("countdown-final", CountdownToken::new),
        },
    );
    state = execute(&state, &expire).0;
    assert!(matches!(state.phase, SessionPhase::Running { .. }));

    let leave_while_running = signed(
        &state,
        principal("alice"),
        "leave-while-running",
        CommandPayload::Leave,
    );
    let leave_decision = authorize(&state, &leave_while_running);
    let authorized_leave =
        AuthorizedCommand::from_decision(leave_while_running, leave_decision).unwrap();
    assert_eq!(
        decide(&state, &authorized_leave),
        Err(SessionError::Denied(DenyReason::WrongPhase))
    );
    assert!(state.member(&principal("alice")).is_some());

    let pause = signed(
        &state,
        principal("alice"),
        "pause-by-nonactor",
        CommandPayload::Pause,
    );
    state = execute(&state, &pause).0;
    assert!(matches!(state.phase, SessionPhase::Paused { .. }));
    let chance_while_paused = signed(
        &state,
        principal("system-game"),
        "chance-paused",
        CommandPayload::ApplyChance { chance: chance() },
    );
    let decision = authorize(&state, &chance_while_paused);
    let authorized = AuthorizedCommand::from_decision(chance_while_paused, decision).unwrap();
    assert_eq!(
        decide(&state, &authorized),
        Err(SessionError::Denied(DenyReason::Paused))
    );

    let unpause = signed(
        &state,
        principal("host"),
        "unpause-by-other",
        CommandPayload::Unpause,
    );
    state = execute(&state, &unpause).0;

    let chance_command = signed(
        &state,
        principal("system-game"),
        "chance",
        CommandPayload::ApplyChance { chance: chance() },
    );
    state = execute(&state, &chance_command).0;

    let wrong_actor = signed(
        &state,
        principal("alice"),
        "wrong-actor",
        CommandPayload::GameAction {
            action: GameActionWire::Bid { tricks: 0 },
        },
    );
    let decision = authorize(&state, &wrong_actor);
    let authorized = AuthorizedCommand::from_decision(wrong_actor, decision).unwrap();
    assert_eq!(
        decide(&state, &authorized),
        Err(SessionError::Denied(DenyReason::NotActor))
    );

    for (who, command_name) in [("host", "act-host"), ("alice", "act-alice")] {
        let action = signed(
            &state,
            principal(who),
            command_name,
            CommandPayload::GameAction {
                action: GameActionWire::Bid { tricks: 0 },
            },
        );
        state = execute(&state, &action).0;
    }
    let settle = signed(
        &state,
        principal("system-game"),
        "settle",
        CommandPayload::Settle,
    );
    state = execute(&state, &settle).0;
    assert!(matches!(state.phase, SessionPhase::PostGame { .. }));

    let reset = signed(
        &state,
        principal("host"),
        "reset",
        CommandPayload::ResetLobby,
    );
    state = execute(&state, &reset).0;
    assert!(matches!(state.phase, SessionPhase::Lobby));
    assert!(state.members.iter().all(|member| !member.ready));

    let close = signed(
        &state,
        principal("host"),
        "close",
        CommandPayload::CloseRoom,
    );
    state = execute(&state, &close).0;
    assert!(matches!(state.phase, SessionPhase::Closed));
    let after_close = signed(
        &state,
        principal("host"),
        "after-close",
        CommandPayload::Chat {
            text: "no".to_owned(),
        },
    );
    assert_eq!(
        deny_reason(&authorize(&state, &after_close)),
        DenyReason::Closed
    );
}

#[test]
fn policy_is_default_deny_with_deny_override_and_audit_non_authority() {
    let mut state = created_room();
    let unknown = signed(
        &state,
        principal("stranger"),
        "unknown",
        CommandPayload::Chat {
            text: "hello".to_owned(),
        },
    );
    assert_eq!(
        deny_reason(&authorize(&state, &unknown)),
        DenyReason::UnknownPrincipal
    );

    state.policies.push(PolicyRule {
        policy_id: id("P-AUDIT-DENY", PolicyId::new),
        priority: 50,
        principal: PrincipalSelector::Exact(principal("host")),
        command: CommandKind::Chat,
        effect: PolicyEffect::Deny(DenyReason::DenyPolicy),
        audit_only: true,
    });
    let chat = signed(
        &state,
        principal("host"),
        "audit-chat",
        CommandPayload::Chat {
            text: "hello".to_owned(),
        },
    );
    let decision = authorize(&state, &chat);
    assert!(decision.is_allowed());
    let PolicyDecision::Allow { results, .. } = decision else {
        unreachable!()
    };
    assert!(
        results
            .iter()
            .any(|result| result.audit_only && !result.allowed)
    );

    state.policies.push(PolicyRule {
        policy_id: id("P-ENFORCE-DENY", PolicyId::new),
        priority: -50,
        principal: PrincipalSelector::Kind(PrincipalKind::Host),
        command: CommandKind::Chat,
        effect: PolicyEffect::Deny(DenyReason::DenyPolicy),
        audit_only: false,
    });
    let decision = authorize(&state, &chat);
    assert_eq!(deny_reason(&decision), DenyReason::DenyPolicy);
    let PolicyDecision::Deny {
        policy_id, results, ..
    } = decision
    else {
        unreachable!()
    };
    assert_eq!(policy_id, Some(id("P-ENFORCE-DENY", PolicyId::new)));
    let ordered_custom = results
        .iter()
        .filter(|result| result.priority != i32::MIN)
        .map(|result| (result.priority, result.policy_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        ordered_custom,
        vec![(50, "P-AUDIT-DENY"), (-50, "P-ENFORCE-DENY")]
    );
}

#[test]
fn duplicate_commands_replay_exact_events_and_apply_idempotently() {
    let state = created_room();
    let chat = signed(
        &state,
        principal("host"),
        "chat-once",
        CommandPayload::Chat {
            text: "once".to_owned(),
        },
    );
    let (after, events) = execute(&state, &chat);
    assert_eq!(after.chat_count, 1);

    let replay_decision = authorize(&after, &chat);
    assert!(replay_decision.is_allowed());
    let authorized = AuthorizedCommand::from_decision(chat.clone(), replay_decision).unwrap();
    assert_eq!(decide(&after, &authorized).unwrap(), events);
    let mut replayed = after.clone();
    for event in &events {
        replayed = apply(&replayed, event).unwrap();
    }
    assert_eq!(replayed, after);

    let conflicting = signed(
        &after,
        principal("host"),
        "chat-once",
        CommandPayload::Chat {
            text: "different".to_owned(),
        },
    );
    assert_eq!(
        deny_reason(&authorize(&after, &conflicting)),
        DenyReason::Malformed
    );
}

#[test]
fn transport_disconnect_cancels_countdown_and_stable_key_reconnects() {
    let mut state = ready_lobby();
    let arm = signed(
        &state,
        principal("host"),
        "arm-disconnect",
        CommandPayload::ArmCountdown {
            deadline_tick: 50,
            countdown_token: id("countdown-disconnect", CountdownToken::new),
        },
    );
    state = execute(&state, &arm).0;
    let events = decide_transport_disconnect(
        &state,
        &principal("alice"),
        &command_id("transport-loss"),
        SemanticHash([0xd1; 32]),
        &correlation("cor-transport-loss"),
    )
    .unwrap();
    assert_eq!(events.len(), 2);
    for event in &events {
        state = apply(&state, event).unwrap();
    }
    assert!(matches!(state.phase, SessionPhase::Lobby));
    let alice = state.member(&principal("alice")).unwrap();
    assert_eq!(alice.connection, ConnectionState::Disconnected);
    assert_eq!(alice.seat, Some(1));
    assert!(!alice.ready);
    assert_eq!(
        state.principal_kind(&principal("alice")),
        PrincipalKind::DisconnectedMember
    );

    let reconnect = signed(
        &state,
        principal("alice"),
        "reconnect",
        CommandPayload::Reconnect,
    );
    state = execute(&state, &reconnect).0;
    assert_eq!(
        state.member(&principal("alice")).unwrap().connection,
        ConnectionState::Connected
    );
}

#[test]
fn removed_member_cannot_reconnect_with_the_former_stable_principal() {
    let mut state = two_player_lobby();
    let disconnect = decide_transport_disconnect(
        &state,
        &principal("alice"),
        &command_id("disconnect-before-removal"),
        SemanticHash([0xd2; 32]),
        &correlation("disconnect-before-removal-correlation"),
    )
    .unwrap();
    for event in disconnect {
        state = apply(&state, &event).unwrap();
    }
    assert_eq!(
        state.member(&principal("alice")).unwrap().connection,
        ConnectionState::Disconnected
    );
    let remove = signed(
        &state,
        principal("host"),
        "remove-disconnected-alice",
        CommandPayload::RemoveMember {
            target: principal("alice"),
        },
    );
    state = execute(&state, &remove).0;
    assert!(state.member(&principal("alice")).is_none());

    let reconnect = signed(
        &state,
        principal("alice"),
        "removed-alice-reconnect",
        CommandPayload::Reconnect,
    );
    assert_eq!(
        deny_reason(&authorize(&state, &reconnect)),
        DenyReason::UnknownPrincipal
    );
}

#[test]
fn controlled_invalid_states_and_stale_events_are_rejected() {
    let state = two_player_lobby();
    let mut duplicate_seat = state.clone();
    duplicate_seat.member_mut(&principal("alice")).unwrap().seat = Some(0);
    assert_eq!(
        duplicate_seat.validate(),
        Err(SessionInvariantError::DuplicateMemberOrSeat)
    );

    let mut invalid_ready = state.clone();
    let alice = invalid_ready.member_mut(&principal("alice")).unwrap();
    alice.connection = ConnectionState::Disconnected;
    alice.ready = true;
    assert_eq!(
        invalid_ready.validate(),
        Err(SessionInvariantError::InvalidReadiness)
    );

    let chat = signed(
        &state,
        principal("host"),
        "ordered-chat",
        CommandPayload::Chat {
            text: "ordered".to_owned(),
        },
    );
    let (_, events) = execute(&state, &chat);
    let mut misordered_event = events[0].clone();
    misordered_event.provenance.base_revision += 1;
    assert_eq!(
        apply(&state, &misordered_event),
        Err(SessionError::EventOrder)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn command_specific_denials_are_stable_before_any_mutation() {
    let mut state = created_room();

    let ready_unseated = signed(
        &state,
        principal("host"),
        "ready-unseated",
        CommandPayload::Ready,
    );
    assert_eq!(
        deny_reason(&authorize(&state, &ready_unseated)),
        DenyReason::NotSeated
    );

    let arm_unready = signed(
        &state,
        principal("host"),
        "arm-unready",
        CommandPayload::ArmCountdown {
            deadline_tick: 1,
            countdown_token: id("countdown-unready", CountdownToken::new),
        },
    );
    assert_semantic_denial(&state, arm_unready, DenyReason::NotReady);

    state
        .invites
        .push(InviteRecord::new("expired-invite", 0).unwrap());
    state
        .invites
        .push(InviteRecord::new("revoked-invite", 100).unwrap());
    state.invites[1].revoked = true;
    for (name, proof, expected) in [
        ("invalid-invite", "wrong-invite", DenyReason::InviteInvalid),
        (
            "expired-invite",
            "expired-invite",
            DenyReason::InviteExpired,
        ),
        ("revoked-invite", "revoked-invite", DenyReason::Revoked),
    ] {
        let join = signed(
            &state,
            principal(name),
            name,
            CommandPayload::RedeemInvite {
                invite: InviteProof::new(proof).unwrap(),
            },
        );
        assert_semantic_denial(&state, join, expected);
    }

    state
        .invites
        .push(InviteRecord::new("valid-invite", 100).unwrap());
    let join = signed(
        &state,
        principal("alice"),
        "join-valid",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new("valid-invite").unwrap(),
        },
    );
    state = execute(&state, &join).0;
    let host_seat = signed(
        &state,
        principal("host"),
        "host-seat-denials",
        CommandPayload::TakeSeat { seat: 0 },
    );
    state = execute(&state, &host_seat).0;
    let host_again = signed(
        &state,
        principal("host"),
        "host-seat-again",
        CommandPayload::TakeSeat { seat: 2 },
    );
    assert_semantic_denial(&state, host_again, DenyReason::AlreadySeated);
    let occupied = signed(
        &state,
        principal("alice"),
        "alice-occupied",
        CommandPayload::TakeSeat { seat: 0 },
    );
    assert_semantic_denial(&state, occupied, DenyReason::SeatOccupied);

    for (name, payload, expected) in [
        (
            "abort-inactive",
            CommandPayload::AbortCountdown,
            DenyReason::CountdownInactive,
        ),
        ("pause-lobby", CommandPayload::Pause, DenyReason::WrongPhase),
        (
            "unpause-lobby",
            CommandPayload::Unpause,
            DenyReason::NotPaused,
        ),
        ("host-leave", CommandPayload::Leave, DenyReason::DenyPolicy),
        (
            "remove-unknown",
            CommandPayload::RemoveMember {
                target: principal("nobody"),
            },
            DenyReason::UnknownPrincipal,
        ),
    ] {
        let command = signed(&state, principal("host"), name, payload);
        assert_semantic_denial(&state, command, expected);
    }

    let false_expiry = signed(
        &state,
        principal("host"),
        "false-expiry",
        CommandPayload::CountdownExpired {
            countdown_token: id("countdown-false", CountdownToken::new),
        },
    );
    assert_eq!(
        deny_reason(&authorize(&state, &false_expiry)),
        DenyReason::EnvironmentOnly
    );
    let false_chance = signed(
        &state,
        principal("host"),
        "false-chance",
        CommandPayload::ApplyChance { chance: chance() },
    );
    assert_eq!(
        deny_reason(&authorize(&state, &false_chance)),
        DenyReason::EnvironmentOnly
    );

    let mut wrong_room = signed(
        &state,
        principal("host"),
        "wrong-room",
        CommandPayload::Chat {
            text: "x".to_owned(),
        },
    );
    wrong_room.room_id = room("other-room");
    assert_eq!(
        deny_reason(&authorize(&state, &wrong_room)),
        DenyReason::WrongRoom
    );
    let mut stale_epoch = signed(
        &state,
        principal("host"),
        "stale-epoch",
        CommandPayload::Chat {
            text: "x".to_owned(),
        },
    );
    stale_epoch.session_epoch += 1;
    assert_eq!(
        deny_reason(&authorize(&state, &stale_epoch)),
        DenyReason::StaleEpoch
    );
    let mut stale_revision = signed(
        &state,
        principal("host"),
        "stale-revision",
        CommandPayload::Chat {
            text: "x".to_owned(),
        },
    );
    stale_revision.expected_revision += 1;
    assert_eq!(
        deny_reason(&authorize(&state, &stale_revision)),
        DenyReason::StaleRevision
    );
}

#[test]
fn chat_rate_is_logical_bounded_and_invite_diagnostics_are_redacted() {
    let mut state = two_player_lobby();
    state
        .invites
        .push(InviteRecord::new("never-log-this", 100).unwrap());
    let diagnostic = format!("{state:?}");
    assert!(diagnostic.contains("<redacted>"));
    assert!(!diagnostic.contains("never-log-this"));

    for index in 0..usize::from(CHAT_MESSAGES_PER_WINDOW) {
        let command = signed(
            &state,
            principal("host"),
            &format!("chat-rate-{index}"),
            CommandPayload::Chat {
                text: format!("message {index}"),
            },
        );
        state = execute(&state, &command).0;
    }
    let limited = signed(
        &state,
        principal("host"),
        "chat-rate-limited",
        CommandPayload::Chat {
            text: "one too many".to_owned(),
        },
    );
    let before = state.clone();
    assert_semantic_denial(&state, limited.clone(), DenyReason::ChatRate);
    assert_eq!(state, before);

    for index in 0..usize::try_from(CHAT_WINDOW_REVISIONS).unwrap() {
        let payload = if index % 2 == 0 {
            CommandPayload::Ready
        } else {
            CommandPayload::Unready
        };
        let advance = signed(
            &state,
            principal("host"),
            &format!("advance-window-{index}"),
            payload,
        );
        state = execute(&state, &advance).0;
    }
    let after_window = signed(
        &state,
        principal("host"),
        "chat-after-window",
        CommandPayload::Chat {
            text: "allowed in next window".to_owned(),
        },
    );
    state = execute(&state, &after_window).0;
    assert_eq!(state.chat_count, u64::from(CHAT_MESSAGES_PER_WINDOW) + 1);
}

#[test]
fn seat_release_leave_remove_and_host_disconnect_preserve_authority_rules() {
    let mut state = two_player_lobby();
    let release = signed(
        &state,
        principal("alice"),
        "release-alice",
        CommandPayload::ReleaseSeat,
    );
    state = execute(&state, &release).0;
    assert_eq!(state.member(&principal("alice")).unwrap().seat, None);
    let leave = signed(
        &state,
        principal("alice"),
        "leave-alice",
        CommandPayload::Leave,
    );
    state = execute(&state, &leave).0;
    assert!(state.member(&principal("alice")).is_none());

    let disconnect = decide_transport_disconnect(
        &state,
        &principal("host"),
        &command_id("host-transport-loss"),
        SemanticHash([0xe1; 32]),
        &correlation("cor-host-transport-loss"),
    )
    .unwrap();
    for event in &disconnect {
        state = apply(&state, event).unwrap();
    }
    assert_eq!(state.host, Some(principal("host")));
    assert_eq!(
        state.principal_kind(&principal("host")),
        PrincipalKind::DisconnectedMember
    );
    assert_eq!(state.members.iter().filter(|member| member.host).count(), 1);
    let host_command = signed(
        &state,
        principal("host"),
        "disconnected-host-command",
        CommandPayload::CloseRoom,
    );
    assert_eq!(
        deny_reason(&authorize(&state, &host_command)),
        DenyReason::MissingCapability
    );
}

#[test]
fn every_registered_deny_reason_survives_policy_evidence() {
    let mut state = created_room();
    for (index, reason) in DenyReason::ALL.into_iter().enumerate() {
        state.policies.clear();
        state.policies.push(PolicyRule {
            policy_id: id(&format!("P-DENY-{index}"), PolicyId::new),
            priority: i32::try_from(index).unwrap(),
            principal: PrincipalSelector::Exact(principal("host")),
            command: CommandKind::Chat,
            effect: PolicyEffect::Deny(reason),
            audit_only: false,
        });
        let command = signed(
            &state,
            principal("host"),
            &format!("deny-{index}"),
            CommandPayload::Chat {
                text: "policy probe".to_owned(),
            },
        );
        assert_eq!(deny_reason(&authorize(&state, &command)), reason);
    }
}

#[test]
fn controlled_readiness_start_pause_and_unknown_role_defects_fail_closed() {
    let mut invalid_readiness = ready_lobby();
    invalid_readiness
        .member_mut(&principal("alice"))
        .unwrap()
        .ready = false;
    invalid_readiness.phase = SessionPhase::Countdown {
        deadline_tick: 1,
        token: id("invalid-countdown", CountdownToken::new),
    };
    assert_eq!(
        invalid_readiness.validate(),
        Err(SessionInvariantError::InvalidReadiness)
    );

    let running = running_via_expiry();
    let duplicate_start = synthetic_event(
        &running,
        "duplicate-start",
        SessionEventKind::GameStarted {
            game: TestGame {
                turn: GameTurn::Chance,
                steps: 0,
                hands: [vec![0, 1], vec![2, 3]],
            },
        },
    );
    assert_eq!(
        apply(&running, &duplicate_start),
        Err(SessionError::Denied(DenyReason::CountdownInactive))
    );

    let pause = signed(
        &running,
        principal("host"),
        "controlled-pause",
        CommandPayload::Pause,
    );
    let paused = execute(&running, &pause).0;
    let advance_while_paused = synthetic_event(
        &paused,
        "advance-while-paused",
        SessionEventKind::GameAdvanced {
            game: TestGame {
                turn: GameTurn::Player(0),
                steps: 1,
                hands: [vec![0, 1], vec![2, 3]],
            },
            round_scores: None,
            terminal: false,
            public_change: None,
        },
    );
    assert_eq!(
        apply(&paused, &advance_while_paused),
        Err(SessionError::Denied(DenyReason::Paused))
    );

    let mut unknown_allow = created_room();
    unknown_allow.policies.push(PolicyRule {
        policy_id: id("P-UNKNOWN-ALLOW", PolicyId::new),
        priority: 100,
        principal: PrincipalSelector::Kind(PrincipalKind::Unknown),
        command: CommandKind::Chat,
        effect: PolicyEffect::Allow,
        audit_only: false,
    });
    let stranger = signed(
        &unknown_allow,
        principal("stranger"),
        "unknown-role-allow",
        CommandPayload::Chat {
            text: "still denied".to_owned(),
        },
    );
    assert_eq!(
        deny_reason(&authorize(&unknown_allow, &stranger)),
        DenyReason::UnknownPrincipal
    );
}

#[test]
fn projections_enforce_pairwise_noninterference_for_every_viewer_kind() {
    let mut state = running_with_spectators();
    state = grant_host_hand_to(&state, "bob", "pairwise");
    let epoch = state.projection_epoch;
    let viewers = ["host", "alice", "bob", "carol"];
    let before: Vec<_> = viewers
        .iter()
        .map(|viewer| project_viewer(&state, &principal(viewer), epoch).unwrap())
        .collect();

    let mut changed = state.clone();
    let SessionPhase::Running { game } = &mut changed.phase else {
        panic!("fixture must be running");
    };
    game.hands[0] = vec![48, 49];
    let after: Vec<_> = viewers
        .iter()
        .map(|viewer| project_viewer(&changed, &principal(viewer), epoch).unwrap())
        .collect();

    assert_ne!(before[0], after[0], "player zero sees their own change");
    assert_eq!(before[1], after[1], "other player is noninterfering");
    assert_ne!(before[2], after[2], "exact granted spectator sees it");
    assert_eq!(before[3], after[3], "other spectator is noninterfering");
    assert!(before[0].granted_hands.is_empty());
    assert_eq!(
        before[0].own_hand.as_ref().unwrap().player,
        principal("host")
    );
    assert!(before[3].own_hand.is_none());
    assert!(before[3].granted_hands.is_empty());

    let capability = local_host_diagnostic_capability(&state, &principal("host")).unwrap();
    let diagnostics = project_local_host_diagnostics(&state, &capability).unwrap();
    assert_eq!(diagnostics.hands.len(), 2);

    let carol_json = serde_json::to_string(&after[3]).unwrap();
    assert!(!carol_json.contains("48"));
    assert!(!carol_json.contains("49"));
}

#[test]
fn grant_revoke_and_round_boundary_change_only_future_projection() {
    let mut state = running_with_spectators();
    let initial = project_viewer(&state, &principal("bob"), 0).unwrap();
    assert!(initial.granted_hands.is_empty());

    state = grant_host_hand_to(&state, "bob", "lifetime");
    assert_eq!(state.projection_epoch, 1);
    assert_eq!(
        project_viewer(&state, &principal("bob"), 0),
        Err(ProjectionError::StaleProjectionEpoch)
    );
    let granted = project_viewer(&state, &principal("bob"), 1).unwrap();
    assert_eq!(granted.granted_hands.len(), 1);
    assert!(initial.granted_hands.is_empty(), "past value is immutable");

    let revoke = signed(
        &state,
        principal("host"),
        "lifetime-revoke",
        CommandPayload::RevokeHand {
            player: principal("host"),
            recipient: principal("bob"),
            grant_epoch: 1,
        },
    );
    state = execute(&state, &revoke).0;
    assert_eq!(state.projection_epoch, 2);
    assert!(
        project_viewer(&state, &principal("bob"), 2)
            .unwrap()
            .granted_hands
            .is_empty()
    );
    assert_eq!(granted.granted_hands.len(), 1, "revocation is future-only");

    state = grant_host_hand_to(&state, "bob", "round");
    let chance_command = signed(
        &state,
        principal("system-game"),
        "round-chance",
        CommandPayload::ApplyChance { chance: chance() },
    );
    state = execute(&state, &chance_command).0;
    for (who, name) in [
        ("host", "round-action-host"),
        ("alice", "round-action-alice"),
    ] {
        let action = signed(
            &state,
            principal(who),
            name,
            CommandPayload::GameAction {
                action: GameActionWire::Bid { tricks: 0 },
            },
        );
        state = execute(&state, &action).0;
    }
    let settle = signed(
        &state,
        principal("system-game"),
        "round-settle",
        CommandPayload::Settle,
    );
    let (settled, events) = execute(&state, &settle);
    assert!(matches!(
        events.first().map(|event| &event.kind),
        Some(SessionEventKind::HandCapabilitiesExpired {
            reason: HandCapabilityExpiry::RoundBoundary
        })
    ));
    assert!(settled.hand_grants.is_empty());
    assert_eq!(settled.public_history.len(), 4);
    assert!(matches!(
        settled.public_history.last(),
        Some(poche_protocol::PublicGameEventWire::RoundScored { .. })
    ));
}

#[test]
fn hand_owner_can_explicitly_deny_only_the_exact_pending_request() {
    let state = running_with_spectators();
    let request = signed(
        &state,
        principal("bob"),
        "deny-request-bob",
        CommandPayload::RequestHand {
            player: principal("host"),
        },
    );
    let requested = execute(&state, &request).0;
    let wrong_owner = signed(
        &requested,
        principal("alice"),
        "deny-by-wrong-owner",
        CommandPayload::DenyHand {
            request_id: command_id("deny-request-bob"),
            player: principal("host"),
            recipient: principal("bob"),
        },
    );
    let decision = authorize(&requested, &wrong_owner);
    let authorized = AuthorizedCommand::from_decision(wrong_owner, decision).unwrap();
    assert_eq!(
        decide(&requested, &authorized),
        Err(SessionError::Denied(DenyReason::GrantScope))
    );

    let deny = signed(
        &requested,
        principal("host"),
        "deny-by-owner",
        CommandPayload::DenyHand {
            request_id: command_id("deny-request-bob"),
            player: principal("host"),
            recipient: principal("bob"),
        },
    );
    let (denied_state, events) = execute(&requested, &deny);
    assert!(denied_state.hand_requests.is_empty());
    assert_eq!(denied_state.projection_epoch, requested.projection_epoch);
    assert!(matches!(
        events.as_slice(),
        [SessionEvent {
            kind: SessionEventKind::HandViewDenied { .. },
            ..
        }]
    ));
}

#[test]
fn reconnect_restores_public_prefix_and_only_the_players_own_private_state() {
    let mut state = running_with_spectators();
    let chance_command = signed(
        &state,
        principal("system-game"),
        "reconnect-chance",
        CommandPayload::ApplyChance { chance: chance() },
    );
    state = execute(&state, &chance_command).0;
    let action = signed(
        &state,
        principal("host"),
        "reconnect-action",
        CommandPayload::GameAction {
            action: GameActionWire::Bid { tricks: 0 },
        },
    );
    state = execute(&state, &action).0;
    let before = project_viewer(&state, &principal("alice"), state.projection_epoch).unwrap();

    let disconnect = decide_transport_disconnect(
        &state,
        &principal("alice"),
        &command_id("transport-loss-alice"),
        SemanticHash([0x44; 32]),
        &correlation("cor-transport-loss-alice"),
    )
    .unwrap();
    for event in disconnect {
        state = apply(&state, &event).unwrap();
    }
    assert_eq!(
        project_viewer(&state, &principal("alice"), state.projection_epoch),
        Err(ProjectionError::NotConnected)
    );
    let reconnect = signed(
        &state,
        principal("alice"),
        "reconnect-alice-projection",
        CommandPayload::Reconnect,
    );
    state = execute(&state, &reconnect).0;
    let after = project_viewer(&state, &principal("alice"), state.projection_epoch).unwrap();
    assert_eq!(after.public_game_state, before.public_game_state);
    assert_eq!(after.public_history, before.public_history);
    assert_eq!(after.own_hand, before.own_hand);
    assert!(after.granted_hands.is_empty());

    let spectator = project_viewer(&state, &principal("carol"), state.projection_epoch).unwrap();
    assert_eq!(spectator.public_game_state, after.public_game_state);
    assert_eq!(spectator.public_history, after.public_history);
    assert!(spectator.own_hand.is_none());
    assert!(spectator.granted_hands.is_empty());
}

fn assert_semantic_denial(
    state: &SessionState<TestGame>,
    command: CommandEnvelope,
    expected: DenyReason,
) {
    let decision = authorize(state, &command);
    assert!(
        decision.is_allowed(),
        "authorization unexpectedly denied: {decision:?}"
    );
    let authorized = AuthorizedCommand::from_decision(command, decision).unwrap();
    assert_eq!(
        decide(state, &authorized),
        Err(SessionError::Denied(expected))
    );
}

fn running_via_expiry() -> SessionState<TestGame> {
    let mut state = ready_lobby();
    let arm = signed(
        &state,
        principal("host"),
        "controlled-arm",
        CommandPayload::ArmCountdown {
            deadline_tick: 10,
            countdown_token: id("controlled-countdown", CountdownToken::new),
        },
    );
    state = execute(&state, &arm).0;
    let expiry = signed(
        &state,
        principal("system-clock"),
        "controlled-expiry",
        CommandPayload::CountdownExpired {
            countdown_token: id("controlled-countdown", CountdownToken::new),
        },
    );
    execute(&state, &expiry).0
}

fn running_with_spectators() -> SessionState<TestGame> {
    let mut state = running_via_expiry();
    for spectator in ["bob", "carol"] {
        let invite_text = format!("invite-{spectator}");
        state
            .invites
            .push(InviteRecord::new(invite_text.clone(), u64::MAX).unwrap());
        let join = signed(
            &state,
            principal(spectator),
            &format!("join-{spectator}-running"),
            CommandPayload::RedeemInvite {
                invite: InviteProof::new(invite_text).unwrap(),
            },
        );
        state = execute(&state, &join).0;
    }
    state
}

fn grant_host_hand_to(
    state: &SessionState<TestGame>,
    spectator: &str,
    suffix: &str,
) -> SessionState<TestGame> {
    let request_name = format!("{suffix}-request-{spectator}");
    let request = signed(
        state,
        principal(spectator),
        &request_name,
        CommandPayload::RequestHand {
            player: principal("host"),
        },
    );
    let requested = execute(state, &request).0;
    let grant = signed(
        &requested,
        principal("host"),
        &format!("{suffix}-grant-{spectator}"),
        CommandPayload::GrantHand {
            request_id: command_id(&request_name),
            player: principal("host"),
            recipient: principal(spectator),
            grant_epoch: requested.projection_epoch + 1,
        },
    );
    execute(&requested, &grant).0
}

fn synthetic_event(
    state: &SessionState<TestGame>,
    name: &str,
    kind: SessionEventKind<TestGame>,
) -> SessionEvent<TestGame> {
    let policy_id = id("P-CONTROLLED-DEFECT", PolicyId::new);
    SessionEvent {
        provenance: EventProvenance {
            command_id: command_id(name),
            command_hash: SemanticHash([0xf0; 32]),
            principal_id: principal("system-game"),
            correlation_id: correlation(&format!("cor-{name}")),
            base_revision: state.revision,
            event_index: 0,
            decision: PolicyDecision::Allow {
                policy_id: policy_id.clone(),
                results: vec![PolicyResult {
                    policy_id,
                    priority: 0,
                    allowed: true,
                    reason: None,
                    audit_only: false,
                }],
            },
        },
        kind,
    }
}

fn chance() -> ChanceWire {
    ChanceWire {
        cards: (0_u8..52).collect(),
        seed: Some(7),
        deal_ordinal: Some(0),
    }
}
