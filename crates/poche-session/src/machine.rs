use core::marker::PhantomData;

use poche_protocol::{
    CommandEnvelope, CommandId, CommandKind, CommandPayload, CorrelationId, DenyReason, PolicyId,
    PrincipalId, SemanticHash, verified_command_semantic_hash,
};

use crate::{
    AuthorizedCommand, CHAT_MESSAGES_PER_WINDOW, CHAT_WINDOW_REVISIONS, ConnectionState,
    EventProvenance, GameTransition, GameTurn, HandCapabilityExpiry, HandViewGrant,
    HandViewRequest, MAX_HAND_GRANTS, MAX_HAND_REQUESTS, MAX_MEMBERS, MemberState, PolicyDecision,
    PolicyEffect, PolicyResult, ProcessedCommand, PublicGameChange, PureSessionMachine,
    SessionError, SessionEvent, SessionEventKind, SessionGame, SessionInvariantError, SessionPhase,
    SessionState, command_kind,
};

const MIN_PLAYERS: usize = 2;

/// Concrete pure session machine for one game implementation.
pub struct SessionMachine<G>(PhantomData<fn() -> G>);

impl<G> PureSessionMachine for SessionMachine<G>
where
    G: SessionGame,
{
    type State = SessionState<G>;
    type Event = SessionEvent<G>;
    type Error = SessionError<G::Error>;

    fn authorize(state: &Self::State, command: &CommandEnvelope) -> PolicyDecision {
        authorize(state, command)
    }

    fn decide(
        state: &Self::State,
        command: &AuthorizedCommand,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        decide(state, command)
    }

    fn apply(state: &Self::State, event: &Self::Event) -> Result<Self::State, Self::Error> {
        apply(state, event)
    }
}

/// Evaluate structural checks, derived capabilities, deny override, and audit policy.
#[must_use]
pub fn authorize<G>(state: &SessionState<G>, command: &CommandEnvelope) -> PolicyDecision {
    let Ok(command_hash) = verified_command_semantic_hash(command) else {
        return structural_deny(DenyReason::Malformed);
    };
    let command_kind = command_kind(&command.payload);
    let principal_kind = state.principal_kind(&command.principal_id);
    if let Some(decision) =
        structural_decision(state, command, command_hash, command_kind, principal_kind)
    {
        return decision;
    }

    let mut results = Vec::new();
    let baseline_allowed = baseline_allows(state, command);
    results.push(PolicyResult {
        policy_id: policy_id(if baseline_allowed {
            "P-BASELINE-ALLOW"
        } else {
            "P-DEFAULT-DENY"
        }),
        priority: i32::MIN,
        allowed: baseline_allowed,
        reason: (!baseline_allowed).then_some(DenyReason::MissingCapability),
        audit_only: false,
    });

    let mut enforce_allow = baseline_allowed;
    let mut enforce_deny = None;
    let mut applicable = state
        .policies
        .iter()
        .filter(|rule| rule.applies(&command.principal_id, principal_kind, command_kind))
        .collect::<Vec<_>>();
    applicable.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.policy_id.cmp(&right.policy_id))
    });
    for rule in applicable {
        let (allowed, reason) = match rule.effect {
            PolicyEffect::Allow => (true, None),
            PolicyEffect::Deny(reason) => (false, Some(reason)),
        };
        results.push(PolicyResult {
            policy_id: rule.policy_id.clone(),
            priority: rule.priority,
            allowed,
            reason,
            audit_only: rule.audit_only,
        });
        if !rule.audit_only {
            match rule.effect {
                PolicyEffect::Allow => enforce_allow = true,
                PolicyEffect::Deny(reason) => {
                    enforce_deny.get_or_insert((rule.policy_id.clone(), reason));
                }
            }
        }
    }

    if let Some((policy_id, reason)) = enforce_deny {
        PolicyDecision::Deny {
            reason,
            policy_id: Some(policy_id),
            results,
        }
    } else if enforce_allow {
        let policy_id = results
            .iter()
            .filter(|result| !result.audit_only && result.allowed)
            .max_by(|left, right| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| right.policy_id.cmp(&left.policy_id))
            })
            .map_or_else(
                || policy_id("P-BASELINE-ALLOW"),
                |result| result.policy_id.clone(),
            );
        PolicyDecision::Allow { policy_id, results }
    } else {
        PolicyDecision::Deny {
            reason: DenyReason::MissingCapability,
            policy_id: Some(policy_id("P-DEFAULT-DENY")),
            results,
        }
    }
}

fn structural_decision<G>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    command_hash: SemanticHash,
    command_kind: CommandKind,
    principal_kind: crate::PrincipalKind,
) -> Option<PolicyDecision> {
    if let Some(record) = state
        .processed_commands
        .iter()
        .find(|record| record.command_id == command.command_id)
    {
        return Some(if record.command_hash == command_hash {
            record.decision.clone()
        } else {
            structural_deny(DenyReason::Malformed)
        });
    }
    let reason = if command.room_id != state.room_id {
        Some(DenyReason::WrongRoom)
    } else if command.session_epoch != state.session_epoch {
        Some(DenyReason::StaleEpoch)
    } else if command.expected_revision != state.revision {
        Some(DenyReason::StaleRevision)
    } else if matches!(state.phase, SessionPhase::Closed) {
        Some(DenyReason::Closed)
    } else if principal_kind == crate::PrincipalKind::Unknown
        && !matches!(
            command_kind,
            CommandKind::CreateRoom | CommandKind::RedeemInvite
        )
    {
        Some(DenyReason::UnknownPrincipal)
    } else if matches!(
        command_kind,
        CommandKind::ReleaseSeat
            | CommandKind::Ready
            | CommandKind::Unready
            | CommandKind::AbortCountdown
            | CommandKind::Pause
            | CommandKind::Unpause
            | CommandKind::GameAction
    ) && state
        .member(&command.principal_id)
        .is_some_and(|member| member.seat.is_none())
    {
        Some(DenyReason::NotSeated)
    } else if command_kind != CommandKind::Reconnect
        && state.member(&command.principal_id).is_some_and(|member| {
            member.connection == ConnectionState::Disconnected
                && command_kind != CommandKind::CloseRoom
        })
    {
        Some(DenyReason::NotConnected)
    } else if (command_kind == CommandKind::CountdownExpired
        && principal_kind != crate::PrincipalKind::AuthorityClock)
        || (matches!(command_kind, CommandKind::ApplyChance | CommandKind::Settle)
            && principal_kind != crate::PrincipalKind::GameEnvironment)
    {
        Some(DenyReason::EnvironmentOnly)
    } else {
        None
    };
    reason.map(structural_deny)
}

fn baseline_allows<G>(state: &SessionState<G>, command: &CommandEnvelope) -> bool {
    let kind = state.principal_kind(&command.principal_id);
    match command.payload.kind() {
        CommandKind::CreateRoom => {
            matches!(state.phase, SessionPhase::Uninitialized)
                && state.host.is_none()
                && kind == crate::PrincipalKind::Unknown
        }
        CommandKind::RedeemInvite => {
            kind == crate::PrincipalKind::Unknown && !state.invites.is_empty()
        }
        CommandKind::TakeSeat | CommandKind::Chat | CommandKind::Leave => matches!(
            kind,
            crate::PrincipalKind::Host
                | crate::PrincipalKind::Member
                | crate::PrincipalKind::Player
                | crate::PrincipalKind::Spectator
        ),
        CommandKind::ReleaseSeat
        | CommandKind::Ready
        | CommandKind::Unready
        | CommandKind::AbortCountdown
        | CommandKind::Pause
        | CommandKind::Unpause
        | CommandKind::GameAction => state.member(&command.principal_id).is_some_and(|member| {
            member.connection == ConnectionState::Connected && member.seat.is_some()
        }),
        CommandKind::ArmCountdown
        | CommandKind::RemoveMember
        | CommandKind::ResetLobby
        | CommandKind::CloseRoom => kind == crate::PrincipalKind::Host,
        CommandKind::CountdownExpired => kind == crate::PrincipalKind::AuthorityClock,
        CommandKind::ApplyChance | CommandKind::Settle => {
            kind == crate::PrincipalKind::GameEnvironment
        }
        CommandKind::Reconnect => kind == crate::PrincipalKind::DisconnectedMember,
        CommandKind::RequestHand => kind == crate::PrincipalKind::Spectator,
        CommandKind::GrantHand | CommandKind::DenyHand | CommandKind::RevokeHand => {
            state.member(&command.principal_id).is_some_and(|member| {
                member.connection == ConnectionState::Connected && member.seat.is_some()
            })
        }
    }
}

/// Decide a deterministic event batch without mutating state.
///
/// # Errors
///
/// Returns stable lifecycle/game/invariant errors. Authorization denials cannot
/// be converted into [`AuthorizedCommand`].
pub fn decide<G: SessionGame>(
    state: &SessionState<G>,
    authorized: &AuthorizedCommand,
) -> Result<Vec<SessionEvent<G>>, SessionError<G::Error>> {
    state.validate()?;
    let command = authorized.command();
    let command_hash = verified_command_semantic_hash(command)
        .map_err(|_| SessionError::Denied(DenyReason::Malformed))?;

    if let Some(record) = state
        .processed_commands
        .iter()
        .find(|record| record.command_id == command.command_id)
    {
        return if record.command_hash == command_hash {
            Ok(record.events.clone())
        } else {
            Err(SessionError::ConflictingCommandId)
        };
    }

    if command.expected_revision != state.revision {
        return Err(SessionError::Denied(DenyReason::StaleRevision));
    }

    let mut kinds = match &command.payload {
        CommandPayload::CreateRoom => decide_create(state, command)?,
        CommandPayload::RedeemInvite { invite } => decide_join(state, command, invite.expose())?,
        CommandPayload::TakeSeat { seat } => decide_take_seat(state, command, *seat)?,
        CommandPayload::ReleaseSeat => decide_release_seat(state, command)?,
        CommandPayload::Ready => decide_ready(state, command, true)?,
        CommandPayload::Unready => decide_ready(state, command, false)?,
        CommandPayload::ArmCountdown {
            deadline_tick,
            countdown_token,
        } => decide_arm_countdown(state, *deadline_tick, countdown_token.clone())?,
        CommandPayload::AbortCountdown => decide_abort_countdown(state)?,
        CommandPayload::CountdownExpired { countdown_token } => {
            decide_countdown_expired(state, countdown_token, G::start)?
        }
        CommandPayload::Pause => decide_pause(state)?,
        CommandPayload::Unpause => decide_unpause(state)?,
        CommandPayload::GameAction { action } => decide_game_action(state, command, action)?,
        CommandPayload::ApplyChance { chance } => decide_chance(state, chance)?,
        CommandPayload::Settle => decide_settle(state)?,
        CommandPayload::Chat { text } => decide_chat(state, command, text)?,
        CommandPayload::Reconnect => decide_reconnect(state, command)?,
        CommandPayload::Leave => decide_leave(state, command)?,
        CommandPayload::RemoveMember { target } => decide_remove(state, command, target)?,
        CommandPayload::ResetLobby => decide_reset(state)?,
        CommandPayload::CloseRoom => decide_close(state)?,
        CommandPayload::RequestHand { player } => decide_request_hand(state, command, player)?,
        CommandPayload::GrantHand {
            request_id,
            player,
            recipient,
            grant_epoch,
        } => decide_grant_hand(state, command, request_id, player, recipient, *grant_epoch)?,
        CommandPayload::DenyHand {
            request_id,
            player,
            recipient,
        } => decide_deny_hand(state, command, request_id, player, recipient)?,
        CommandPayload::RevokeHand {
            player,
            recipient,
            grant_epoch,
        } => decide_revoke_hand(state, command, player, recipient, *grant_epoch)?,
    };

    let mut events = Vec::with_capacity(kinds.len());
    for (index, kind) in kinds.drain(..).enumerate() {
        let event_index = u16::try_from(index).map_err(|_| SessionError::EventOrder)?;
        events.push(SessionEvent {
            provenance: EventProvenance {
                command_id: command.command_id.clone(),
                command_hash,
                principal_id: command.principal_id.clone(),
                correlation_id: command.correlation_id.clone(),
                base_revision: state.revision,
                event_index,
                decision: authorized.decision().clone(),
            },
            kind,
        });
    }
    Ok(events)
}

/// Convert a trusted transport-loss observation into a pure replayable event batch.
///
/// This is not a player command and therefore has no wire command tag. The
/// runtime supplies a collision-resistant observation ID/hash and applies the
/// returned events through the same idempotent reducer.
///
/// # Errors
///
/// Returns `D-UNKNOWN-PRINCIPAL` or `D-NOT-CONNECTED` without mutation.
pub fn decide_transport_disconnect<G: SessionGame>(
    state: &SessionState<G>,
    target: &PrincipalId,
    observation_id: &CommandId,
    observation_hash: SemanticHash,
    correlation_id: &CorrelationId,
) -> Result<Vec<SessionEvent<G>>, SessionError<G::Error>> {
    state.validate()?;
    let member = require_member(state, target)?;
    if member.connection != ConnectionState::Connected {
        return denied(DenyReason::NotConnected);
    }
    let decision = PolicyDecision::Allow {
        policy_id: policy_id("P-TRANSPORT-DISCONNECT"),
        results: vec![PolicyResult {
            policy_id: policy_id("P-TRANSPORT-DISCONNECT"),
            priority: 0,
            allowed: true,
            reason: None,
            audit_only: false,
        }],
    };
    let mut kinds = if member.seat.is_some() {
        cancel_countdown_first(state)
    } else {
        Vec::new()
    };
    kinds.push(SessionEventKind::MemberDisconnected {
        principal: target.clone(),
    });
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            Ok(SessionEvent {
                provenance: EventProvenance {
                    command_id: observation_id.clone(),
                    command_hash: observation_hash,
                    principal_id: target.clone(),
                    correlation_id: correlation_id.clone(),
                    base_revision: state.revision,
                    event_index: u16::try_from(index).map_err(|_| SessionError::EventOrder)?,
                    decision: decision.clone(),
                },
                kind,
            })
        })
        .collect()
}

fn decide_create<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if !matches!(state.phase, SessionPhase::Uninitialized) || state.host.is_some() {
        return denied(DenyReason::WrongPhase);
    }
    Ok(vec![SessionEventKind::RoomCreated {
        host: command.principal_id.clone(),
    }])
}

fn decide_join<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    invite: &str,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if matches!(
        state.phase,
        SessionPhase::Uninitialized | SessionPhase::Closed
    ) {
        return denied(DenyReason::WrongPhase);
    }
    if state.member(&command.principal_id).is_some() || state.members.len() >= MAX_MEMBERS {
        return denied(DenyReason::DenyPolicy);
    }
    let Some((index, record)) = state
        .invites
        .iter()
        .enumerate()
        .find(|(_, record)| record.matches(invite))
    else {
        return denied(DenyReason::InviteInvalid);
    };
    if record.revoked || record.consumed {
        return denied(DenyReason::Revoked);
    }
    if state.revision > record.expires_after_revision {
        return denied(DenyReason::InviteExpired);
    }
    Ok(vec![SessionEventKind::MemberJoined {
        principal: command.principal_id.clone(),
        invite_index: u16::try_from(index).map_err(|_| SessionError::EventOrder)?,
    }])
}

fn decide_take_seat<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    seat: u8,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if !matches!(state.phase, SessionPhase::Lobby) {
        return denied(DenyReason::WrongPhase);
    }
    if usize::from(seat) >= MAX_MEMBERS {
        return denied(DenyReason::Malformed);
    }
    let member = require_member(state, &command.principal_id)?;
    if member.connection != ConnectionState::Connected {
        return denied(DenyReason::NotConnected);
    }
    if member.seat.is_some() {
        return denied(DenyReason::AlreadySeated);
    }
    if state.seat_owner(seat).is_some() {
        return denied(DenyReason::SeatOccupied);
    }
    let mut events = expire_capabilities_for(
        state,
        HandCapabilityExpiry::SeatRoleChanged {
            principal: command.principal_id.clone(),
        },
    );
    events.push(SessionEventKind::SeatTaken {
        principal: command.principal_id.clone(),
        seat,
    });
    Ok(events)
}

fn decide_release_seat<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if !matches!(
        state.phase,
        SessionPhase::Lobby | SessionPhase::Countdown { .. }
    ) {
        return denied(DenyReason::WrongPhase);
    }
    let member = require_member(state, &command.principal_id)?;
    let Some(seat) = member.seat else {
        return denied(DenyReason::NotSeated);
    };
    let mut events = cancel_countdown_first(state);
    events.extend(expire_capabilities_for(
        state,
        HandCapabilityExpiry::SeatRoleChanged {
            principal: command.principal_id.clone(),
        },
    ));
    events.push(SessionEventKind::SeatReleased {
        principal: command.principal_id.clone(),
        seat,
    });
    Ok(events)
}

fn decide_ready<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    ready: bool,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let phase_ok = if ready {
        matches!(state.phase, SessionPhase::Lobby)
    } else {
        matches!(
            state.phase,
            SessionPhase::Lobby | SessionPhase::Countdown { .. }
        )
    };
    if !phase_ok {
        return denied(DenyReason::WrongPhase);
    }
    let member = require_member(state, &command.principal_id)?;
    if member.seat.is_none() {
        return denied(DenyReason::NotSeated);
    }
    if ready && member.connection != ConnectionState::Connected {
        return denied(DenyReason::NotConnected);
    }
    let mut events = if ready {
        Vec::new()
    } else {
        cancel_countdown_first(state)
    };
    events.push(SessionEventKind::ReadyChanged {
        principal: command.principal_id.clone(),
        ready,
    });
    Ok(events)
}

fn decide_arm_countdown<G: SessionGame>(
    state: &SessionState<G>,
    deadline_tick: u64,
    token: poche_protocol::CountdownToken,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if !matches!(state.phase, SessionPhase::Lobby) {
        return denied(DenyReason::WrongPhase);
    }
    let seated: Vec<_> = state
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .collect();
    if seated.len() < MIN_PLAYERS
        || seated
            .iter()
            .any(|member| !member.ready || member.connection != ConnectionState::Connected)
    {
        return denied(DenyReason::NotReady);
    }
    Ok(vec![SessionEventKind::CountdownArmed {
        deadline_tick,
        token,
    }])
}

fn decide_abort_countdown<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let SessionPhase::Countdown { token, .. } = &state.phase else {
        return denied(DenyReason::CountdownInactive);
    };
    Ok(vec![SessionEventKind::CountdownCancelled {
        token: token.clone(),
    }])
}

fn decide_countdown_expired<G: SessionGame>(
    state: &SessionState<G>,
    token: &poche_protocol::CountdownToken,
    start: impl FnOnce(&[(u8, PrincipalId)]) -> Result<G, G::Error>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let SessionPhase::Countdown {
        token: active_token,
        ..
    } = &state.phase
    else {
        return denied(DenyReason::CountdownInactive);
    };
    if active_token != token {
        return denied(DenyReason::CountdownInactive);
    }
    let seats = canonical_seats(state)?;
    let game = start(&seats).map_err(SessionError::Game)?;
    Ok(vec![SessionEventKind::GameStarted { game }])
}

fn decide_pause<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    match state.phase {
        SessionPhase::Running { .. } => Ok(vec![SessionEventKind::Paused]),
        SessionPhase::Paused { .. } => denied(DenyReason::AlreadyPaused),
        _ => denied(DenyReason::WrongPhase),
    }
}

fn decide_unpause<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if matches!(state.phase, SessionPhase::Paused { .. }) {
        Ok(vec![SessionEventKind::Unpaused])
    } else {
        denied(DenyReason::NotPaused)
    }
}

fn decide_game_action<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    action: &poche_protocol::GameActionWire,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let SessionPhase::Running { game } = &state.phase else {
        return if matches!(state.phase, SessionPhase::Paused { .. }) {
            denied(DenyReason::Paused)
        } else {
            denied(DenyReason::WrongPhase)
        };
    };
    let member = require_member(state, &command.principal_id)?;
    let Some(seat) = member.seat else {
        return denied(DenyReason::NotSeated);
    };
    if game.turn() != GameTurn::Player(seat) {
        return denied(DenyReason::NotActor);
    }
    Ok(transition_event(
        state,
        game.player_transition(seat, action)
            .map_err(SessionError::Game)?,
        Some(PublicGameChange::PlayerAction {
            seat,
            action: action.clone(),
        }),
    ))
}

fn decide_chance<G: SessionGame>(
    state: &SessionState<G>,
    chance: &poche_protocol::ChanceWire,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let game = running_game(state)?;
    if game.turn() != GameTurn::Chance {
        return denied(DenyReason::EnvironmentOnly);
    }
    Ok(transition_event(
        state,
        game.chance_transition(chance).map_err(SessionError::Game)?,
        None,
    ))
}

fn decide_settle<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let game = running_game(state)?;
    if game.turn() != GameTurn::Environment {
        return denied(DenyReason::EnvironmentOnly);
    }
    let transition = game.settle().map_err(SessionError::Game)?;
    let public_change =
        transition
            .round_scores
            .as_ref()
            .map(|scores| PublicGameChange::RoundScored {
                scores: scores.clone(),
                terminal: transition.terminal,
            });
    Ok(transition_event(state, transition, public_change))
}

fn transition_event<G>(
    state: &SessionState<G>,
    transition: GameTransition<G>,
    public_change: Option<PublicGameChange>,
) -> Vec<SessionEventKind<G>> {
    let mut events = if transition.round_scores.is_some() {
        expire_capabilities_for(state, HandCapabilityExpiry::RoundBoundary)
    } else {
        Vec::new()
    };
    events.push(SessionEventKind::GameAdvanced {
        game: transition.game,
        round_scores: transition.round_scores,
        terminal: transition.terminal,
        public_change,
    });
    events
}

fn decide_chat<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    text: &str,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if matches!(
        state.phase,
        SessionPhase::Closed | SessionPhase::Uninitialized
    ) {
        return denied(DenyReason::Closed);
    }
    let member = require_member(state, &command.principal_id)?;
    let in_current_window = state
        .revision
        .saturating_sub(member.chat_window_start_revision)
        < CHAT_WINDOW_REVISIONS;
    if in_current_window && member.chat_messages_in_window >= CHAT_MESSAGES_PER_WINDOW {
        return denied(DenyReason::ChatRate);
    }
    Ok(vec![SessionEventKind::ChatPosted {
        principal: command.principal_id.clone(),
        text: text.to_owned(),
    }])
}

fn decide_request_hand<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    player: &PrincipalId,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if !matches!(
        state.phase,
        SessionPhase::Running { .. } | SessionPhase::Paused { .. }
    ) {
        return denied(DenyReason::WrongPhase);
    }
    let recipient = require_member(state, &command.principal_id)?;
    if recipient.host || recipient.seat.is_some() {
        return denied(DenyReason::GrantScope);
    }
    let target = require_member(state, player)?;
    if target.connection != ConnectionState::Connected || target.seat.is_none() {
        return denied(DenyReason::GrantScope);
    }
    if state.hand_requests.len() >= MAX_HAND_REQUESTS
        || state
            .hand_requests
            .iter()
            .any(|request| request.recipient == command.principal_id)
        || state
            .hand_grants
            .iter()
            .any(|grant| grant.recipient == command.principal_id)
    {
        return denied(DenyReason::GrantScope);
    }
    Ok(vec![SessionEventKind::HandViewRequested {
        request_id: command.command_id.clone(),
        player: player.clone(),
        recipient: command.principal_id.clone(),
    }])
}

fn decide_grant_hand<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    request_id: &CommandId,
    player: &PrincipalId,
    recipient: &PrincipalId,
    grant_epoch: u64,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if command.principal_id != *player
        || state.hand_grants.len() >= MAX_HAND_GRANTS
        || grant_epoch != state.projection_epoch.saturating_add(1)
        || !state.hand_requests.iter().any(|request| {
            request.request_id == *request_id
                && request.player == *player
                && request.recipient == *recipient
        })
    {
        return denied(DenyReason::GrantScope);
    }
    let owner = require_member(state, player)?;
    let viewer = require_member(state, recipient)?;
    if owner.connection != ConnectionState::Connected
        || owner.seat.is_none()
        || viewer.connection != ConnectionState::Connected
        || viewer.seat.is_some()
        || viewer.host
    {
        return denied(DenyReason::GrantScope);
    }
    Ok(vec![SessionEventKind::HandViewGranted {
        request_id: request_id.clone(),
        player: player.clone(),
        recipient: recipient.clone(),
        grant_epoch,
    }])
}

fn decide_revoke_hand<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    player: &PrincipalId,
    recipient: &PrincipalId,
    grant_epoch: u64,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if command.principal_id != *player
        || !state.hand_grants.iter().any(|grant| {
            grant.player == *player
                && grant.recipient == *recipient
                && grant.grant_epoch == grant_epoch
        })
    {
        return denied(DenyReason::GrantScope);
    }
    Ok(vec![SessionEventKind::HandViewRevoked {
        player: player.clone(),
        recipient: recipient.clone(),
        grant_epoch,
    }])
}

fn decide_deny_hand<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    request_id: &CommandId,
    player: &PrincipalId,
    recipient: &PrincipalId,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if command.principal_id != *player
        || !state.hand_requests.iter().any(|request| {
            request.request_id == *request_id
                && request.player == *player
                && request.recipient == *recipient
        })
    {
        return denied(DenyReason::GrantScope);
    }
    Ok(vec![SessionEventKind::HandViewDenied {
        request_id: request_id.clone(),
        player: player.clone(),
        recipient: recipient.clone(),
    }])
}

fn decide_reconnect<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let member = require_member(state, &command.principal_id)?;
    if member.connection != ConnectionState::Disconnected {
        return denied(DenyReason::NotConnected);
    }
    Ok(vec![SessionEventKind::MemberReconnected {
        principal: command.principal_id.clone(),
    }])
}

fn decide_leave<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let member = require_member(state, &command.principal_id)?;
    if member.host {
        return denied(DenyReason::DenyPolicy);
    }
    // A spectator owns no hand, turn, or occupied game seat, so ending that
    // membership cannot strand the active game. A seated player is different:
    // until the game has an explicit dropout/substitution transition, retain
    // that membership and use Disconnect / Reconnect for a recoverable exit.
    if member.seat.is_some()
        && matches!(
            state.phase,
            SessionPhase::Running { .. }
                | SessionPhase::Paused { .. }
                | SessionPhase::PostGame { .. }
        )
    {
        return denied(DenyReason::WrongPhase);
    }
    let mut events = if member.seat.is_some() {
        cancel_countdown_first(state)
    } else {
        Vec::new()
    };
    events.extend(expire_capabilities_for(
        state,
        HandCapabilityExpiry::MembershipLost {
            principal: command.principal_id.clone(),
        },
    ));
    events.push(SessionEventKind::MemberLeft {
        principal: command.principal_id.clone(),
    });
    Ok(events)
}

fn decide_remove<G: SessionGame>(
    state: &SessionState<G>,
    command: &CommandEnvelope,
    target: &PrincipalId,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    let target_member = require_member(state, target)?;
    if target_member.host || *target == command.principal_id {
        return denied(DenyReason::DenyPolicy);
    }
    let mut events = if target_member.seat.is_some() {
        cancel_countdown_first(state)
    } else {
        Vec::new()
    };
    events.extend(expire_capabilities_for(
        state,
        HandCapabilityExpiry::MembershipLost {
            principal: target.clone(),
        },
    ));
    events.push(SessionEventKind::MemberRemoved {
        principal: target.clone(),
    });
    Ok(events)
}

fn decide_reset<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if matches!(state.phase, SessionPhase::PostGame { .. }) {
        Ok(vec![SessionEventKind::LobbyReset])
    } else {
        denied(DenyReason::WrongPhase)
    }
}

fn decide_close<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<SessionEventKind<G>>, SessionError<G::Error>> {
    if matches!(state.phase, SessionPhase::Closed) {
        denied(DenyReason::Closed)
    } else {
        Ok(vec![SessionEventKind::RoomClosed])
    }
}

fn running_game<G: SessionGame>(state: &SessionState<G>) -> Result<&G, SessionError<G::Error>> {
    match &state.phase {
        SessionPhase::Running { game } => Ok(game),
        SessionPhase::Paused { .. } => denied(DenyReason::Paused),
        _ => denied(DenyReason::WrongPhase),
    }
}

fn canonical_seats<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<Vec<(u8, PrincipalId)>, SessionError<G::Error>> {
    let mut seats: Vec<_> = state
        .members
        .iter()
        .filter_map(|member| {
            member.seat.map(|seat| {
                (
                    seat,
                    member.principal_id.clone(),
                    member.ready,
                    member.connection,
                )
            })
        })
        .collect();
    seats.sort_by_key(|(seat, ..)| *seat);
    if seats.len() < MIN_PLAYERS
        || seats
            .iter()
            .any(|(_, _, ready, connection)| !ready || *connection != ConnectionState::Connected)
    {
        return denied(DenyReason::NotReady);
    }
    Ok(seats
        .into_iter()
        .map(|(seat, principal, _, _)| (seat, principal))
        .collect())
}

fn cancel_countdown_first<G>(state: &SessionState<G>) -> Vec<SessionEventKind<G>> {
    match &state.phase {
        SessionPhase::Countdown { token, .. } => vec![SessionEventKind::CountdownCancelled {
            token: token.clone(),
        }],
        _ => Vec::new(),
    }
}

fn expire_capabilities_for<G>(
    state: &SessionState<G>,
    reason: HandCapabilityExpiry,
) -> Vec<SessionEventKind<G>> {
    let affects_pair = |player: &PrincipalId, recipient: &PrincipalId| match &reason {
        HandCapabilityExpiry::RoundBoundary => true,
        HandCapabilityExpiry::SeatRoleChanged { principal }
        | HandCapabilityExpiry::MembershipLost { principal } => {
            player == principal || recipient == principal
        }
    };
    if state
        .hand_requests
        .iter()
        .any(|request| affects_pair(&request.player, &request.recipient))
        || state
            .hand_grants
            .iter()
            .any(|grant| affects_pair(&grant.player, &grant.recipient))
    {
        vec![SessionEventKind::HandCapabilitiesExpired { reason }]
    } else {
        Vec::new()
    }
}

fn require_member<'a, G: SessionGame>(
    state: &'a SessionState<G>,
    principal: &PrincipalId,
) -> Result<&'a MemberState, SessionError<G::Error>> {
    state
        .member(principal)
        .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))
}

fn denied<T, E>(reason: DenyReason) -> Result<T, SessionError<E>> {
    Err(SessionError::Denied(reason))
}

/// Apply one event atomically, enforcing gap-free order and idempotency.
///
/// # Errors
///
/// Returns an invariant, conflict, or event-order error without changing the
/// input state.
pub fn apply<G: SessionGame>(
    state: &SessionState<G>,
    event: &SessionEvent<G>,
) -> Result<SessionState<G>, SessionError<G::Error>> {
    state.validate()?;

    let record_index = state
        .processed_commands
        .iter()
        .position(|record| record.command_id == event.provenance.command_id);
    if let Some(index) = record_index {
        let record = &state.processed_commands[index];
        if record.command_hash != event.provenance.command_hash {
            return Err(SessionError::ConflictingCommandId);
        }
        if let Some(existing) = record.events.get(usize::from(event.provenance.event_index)) {
            return if existing == event {
                Ok(state.clone())
            } else {
                Err(SessionError::ConflictingCommandId)
            };
        }
        if record.events.len() != usize::from(event.provenance.event_index)
            || record.decision != event.provenance.decision
        {
            return Err(SessionError::EventOrder);
        }
    } else if event.provenance.event_index != 0 {
        return Err(SessionError::EventOrder);
    }

    let expected_revision = event
        .provenance
        .base_revision
        .checked_add(u64::from(event.provenance.event_index))
        .ok_or(SessionError::EventOrder)?;
    if state.revision != expected_revision {
        return Err(SessionError::EventOrder);
    }

    let mut next = state.clone();
    apply_kind(&mut next, &event.kind, &event.provenance)?;
    next.revision = next
        .revision
        .checked_add(1)
        .ok_or(SessionError::EventOrder)?;
    if let Some(index) = record_index {
        next.processed_commands[index].events.push(event.clone());
    } else {
        next.processed_commands.push(ProcessedCommand {
            command_id: event.provenance.command_id.clone(),
            command_hash: event.provenance.command_hash,
            decision: event.provenance.decision.clone(),
            events: vec![event.clone()],
        });
    }
    next.validate()?;
    Ok(next)
}

#[allow(clippy::too_many_lines)]
fn apply_kind<G: SessionGame>(
    state: &mut SessionState<G>,
    kind: &SessionEventKind<G>,
    provenance: &EventProvenance,
) -> Result<(), SessionError<G::Error>> {
    match kind {
        SessionEventKind::RoomCreated { host } => {
            if !matches!(state.phase, SessionPhase::Uninitialized) {
                return denied(DenyReason::WrongPhase);
            }
            state.session_epoch = 1;
            state.host = Some(host.clone());
            state.members.push(MemberState {
                principal_id: host.clone(),
                membership_epoch: state.session_epoch,
                connection: ConnectionState::Connected,
                seat: None,
                ready: false,
                host: true,
                chat_window_start_revision: state.revision,
                chat_messages_in_window: 0,
            });
            state.phase = SessionPhase::Lobby;
        }
        SessionEventKind::MemberJoined {
            principal,
            invite_index,
        } => {
            let invite = state.invites.get_mut(usize::from(*invite_index)).ok_or(
                SessionError::Invariant(SessionInvariantError::InvalidInvite),
            )?;
            if invite.consumed || invite.revoked {
                return denied(DenyReason::Revoked);
            }
            invite.consumed = true;
            state.members.push(MemberState {
                principal_id: principal.clone(),
                membership_epoch: state.session_epoch,
                connection: ConnectionState::Connected,
                seat: None,
                ready: false,
                host: false,
                chat_window_start_revision: state.revision,
                chat_messages_in_window: 0,
            });
        }
        SessionEventKind::SeatTaken { principal, seat } => {
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            member.seat = Some(*seat);
            member.ready = false;
        }
        SessionEventKind::SeatReleased { principal, seat } => {
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            if member.seat != Some(*seat) {
                return denied(DenyReason::NotSeated);
            }
            member.seat = None;
            member.ready = false;
        }
        SessionEventKind::ReadyChanged { principal, ready } => {
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            member.ready = *ready;
        }
        SessionEventKind::CountdownArmed {
            deadline_tick,
            token,
        } => {
            state.phase = SessionPhase::Countdown {
                deadline_tick: *deadline_tick,
                token: token.clone(),
            };
        }
        SessionEventKind::CountdownCancelled { token } => {
            let SessionPhase::Countdown {
                token: active_token,
                ..
            } = &state.phase
            else {
                return denied(DenyReason::CountdownInactive);
            };
            if active_token != token {
                return denied(DenyReason::CountdownInactive);
            }
            state.phase = SessionPhase::Lobby;
        }
        SessionEventKind::GameStarted { game } => {
            if !matches!(state.phase, SessionPhase::Countdown { .. }) {
                return denied(DenyReason::CountdownInactive);
            }
            for member in &mut state.members {
                member.ready = false;
            }
            state.public_history.clear();
            state
                .public_history
                .push(poche_protocol::PublicGameEventWire::GameStarted {
                    command_id: provenance.command_id.clone(),
                });
            state.phase = SessionPhase::Running { game: game.clone() };
        }
        SessionEventKind::GameAdvanced {
            game,
            round_scores: _,
            terminal,
            public_change,
        } => {
            if !matches!(state.phase, SessionPhase::Running { .. }) {
                return denied(if matches!(state.phase, SessionPhase::Paused { .. }) {
                    DenyReason::Paused
                } else {
                    DenyReason::WrongPhase
                });
            }
            state.phase = if *terminal {
                SessionPhase::PostGame { game: game.clone() }
            } else {
                SessionPhase::Running { game: game.clone() }
            };
            if let Some(change) = public_change {
                let public_event = match change {
                    PublicGameChange::PlayerAction { seat, action } => {
                        poche_protocol::PublicGameEventWire::PlayerAction {
                            command_id: provenance.command_id.clone(),
                            event_index: provenance.event_index,
                            seat: *seat,
                            action: action.clone(),
                        }
                    }
                    PublicGameChange::RoundScored { scores, terminal } => {
                        poche_protocol::PublicGameEventWire::RoundScored {
                            command_id: provenance.command_id.clone(),
                            event_index: provenance.event_index,
                            scores: scores.clone(),
                            terminal: *terminal,
                        }
                    }
                };
                state.public_history.push(public_event);
            }
        }
        SessionEventKind::Paused => {
            let previous = core::mem::replace(&mut state.phase, SessionPhase::Closed);
            let SessionPhase::Running { game } = previous else {
                state.phase = previous;
                return denied(DenyReason::WrongPhase);
            };
            state.phase = SessionPhase::Paused { game };
        }
        SessionEventKind::Unpaused => {
            let previous = core::mem::replace(&mut state.phase, SessionPhase::Closed);
            let SessionPhase::Paused { game } = previous else {
                state.phase = previous;
                return denied(DenyReason::NotPaused);
            };
            state.phase = SessionPhase::Running { game };
        }
        SessionEventKind::MemberDisconnected { principal } => {
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            member.connection = ConnectionState::Disconnected;
            member.ready = false;
        }
        SessionEventKind::MemberReconnected { principal } => {
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            member.connection = ConnectionState::Connected;
        }
        SessionEventKind::MemberLeft { principal }
        | SessionEventKind::MemberRemoved { principal } => {
            let index = state
                .members
                .iter()
                .position(|member| member.principal_id == *principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            if state.members[index].host {
                return denied(DenyReason::DenyPolicy);
            }
            state.members.remove(index);
        }
        SessionEventKind::LobbyReset => {
            if !matches!(state.phase, SessionPhase::PostGame { .. }) {
                return denied(DenyReason::WrongPhase);
            }
            for member in &mut state.members {
                member.ready = false;
            }
            state.phase = SessionPhase::Lobby;
        }
        SessionEventKind::ChatPosted { principal, text: _ } => {
            let revision = state.revision;
            let member = state
                .member_mut(principal)
                .ok_or(SessionError::Denied(DenyReason::UnknownPrincipal))?;
            if revision.saturating_sub(member.chat_window_start_revision) >= CHAT_WINDOW_REVISIONS {
                member.chat_window_start_revision = revision;
                member.chat_messages_in_window = 0;
            } else if member.chat_messages_in_window >= CHAT_MESSAGES_PER_WINDOW {
                return denied(DenyReason::ChatRate);
            }
            member.chat_messages_in_window = member
                .chat_messages_in_window
                .checked_add(1)
                .ok_or(SessionError::EventOrder)?;
            state.chat_count = state
                .chat_count
                .checked_add(1)
                .ok_or(SessionError::EventOrder)?;
        }
        SessionEventKind::HandViewRequested {
            request_id,
            player,
            recipient,
        } => {
            if state.hand_requests.len() >= MAX_HAND_REQUESTS {
                return Err(SessionError::Invariant(
                    SessionInvariantError::TooManyHandCapabilities,
                ));
            }
            state.hand_requests.push(HandViewRequest {
                request_id: request_id.clone(),
                player: player.clone(),
                recipient: recipient.clone(),
            });
        }
        SessionEventKind::HandViewGranted {
            request_id,
            player,
            recipient,
            grant_epoch,
        } => {
            if *grant_epoch != state.projection_epoch.saturating_add(1)
                || state.hand_grants.len() >= MAX_HAND_GRANTS
            {
                return denied(DenyReason::GrantScope);
            }
            let request_index = state
                .hand_requests
                .iter()
                .position(|request| {
                    request.request_id == *request_id
                        && request.player == *player
                        && request.recipient == *recipient
                })
                .ok_or(SessionError::Denied(DenyReason::GrantScope))?;
            state.hand_requests.remove(request_index);
            state.hand_grants.push(HandViewGrant {
                player: player.clone(),
                recipient: recipient.clone(),
                grant_epoch: *grant_epoch,
            });
            state.projection_epoch = *grant_epoch;
        }
        SessionEventKind::HandViewDenied {
            request_id,
            player,
            recipient,
        } => {
            let request_index = state
                .hand_requests
                .iter()
                .position(|request| {
                    request.request_id == *request_id
                        && request.player == *player
                        && request.recipient == *recipient
                })
                .ok_or(SessionError::Denied(DenyReason::GrantScope))?;
            state.hand_requests.remove(request_index);
        }
        SessionEventKind::HandViewRevoked {
            player,
            recipient,
            grant_epoch,
        } => {
            let grant_index = state
                .hand_grants
                .iter()
                .position(|grant| {
                    grant.player == *player
                        && grant.recipient == *recipient
                        && grant.grant_epoch == *grant_epoch
                })
                .ok_or(SessionError::Denied(DenyReason::GrantScope))?;
            state.hand_grants.remove(grant_index);
            state.projection_epoch = state
                .projection_epoch
                .checked_add(1)
                .ok_or(SessionError::EventOrder)?;
        }
        SessionEventKind::HandCapabilitiesExpired { reason } => {
            let affects_pair = |player: &PrincipalId, recipient: &PrincipalId| match reason {
                HandCapabilityExpiry::RoundBoundary => true,
                HandCapabilityExpiry::SeatRoleChanged { principal }
                | HandCapabilityExpiry::MembershipLost { principal } => {
                    player == principal || recipient == principal
                }
            };
            state
                .hand_requests
                .retain(|request| !affects_pair(&request.player, &request.recipient));
            let before = state.hand_grants.len();
            state
                .hand_grants
                .retain(|grant| !affects_pair(&grant.player, &grant.recipient));
            if before != state.hand_grants.len() {
                state.projection_epoch = state
                    .projection_epoch
                    .checked_add(1)
                    .ok_or(SessionError::EventOrder)?;
            }
        }
        SessionEventKind::RoomClosed => {
            state.phase = SessionPhase::Closed;
            for member in &mut state.members {
                member.ready = false;
            }
        }
    }
    Ok(())
}

fn structural_deny(reason: DenyReason) -> PolicyDecision {
    PolicyDecision::Deny {
        reason,
        policy_id: None,
        results: Vec::new(),
    }
}

fn policy_id(value: &str) -> PolicyId {
    PolicyId::new(value).expect("built-in policy identifiers are canonical")
}

#[cfg(test)]
mod tests;
