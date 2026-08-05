// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic explicit-state checking for the bounded session abstraction.

use std::collections::{HashMap, VecDeque};

/// Stable scope used by the independent session oracles.
pub const SESSION_SCOPE_ID: &str =
    "session-micro-host-2p-1spec-1countdown-1pause-1grant-1chat-1game-step";

/// Enabling assumptions needed to turn existential terminal reachability into
/// the same conditional progress claim checked by the `NuSMV` oracle.
pub const SESSION_LIVENESS_ASSUMPTIONS: [&str; 5] = [
    "eventual-readiness-and-arm",
    "eventual-countdown-expiry",
    "eventual-player-action",
    "eventual-resume",
    "eventual-reconnect-or-stable-delivery",
];

/// Fixed player seat in the micro room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionSeat {
    /// Host's permanent seat.
    Host,
    /// Other player's permanent seat.
    PlayerOne,
}

impl SessionSeat {
    const ALL: [Self; 2] = [Self::Host, Self::PlayerOne];

    const fn index(self) -> usize {
        match self {
            Self::Host => 0,
            Self::PlayerOne => 1,
        }
    }
}

/// Bounded actor identities, including environment authorities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionPrincipal {
    /// Room host and seat zero.
    Host,
    /// Seat one.
    PlayerOne,
    /// Unseated room member.
    Spectator,
    /// Unknown/nonmember principal used by default-deny checks.
    Outsider,
    /// Logical countdown authority.
    Clock,
}

impl SessionPrincipal {
    const MEMBERS: [Self; 3] = [Self::Host, Self::PlayerOne, Self::Spectator];
    const ATTEMPT_ACTORS: [Self; 4] =
        [Self::Host, Self::PlayerOne, Self::Spectator, Self::Outsider];

    const fn seat(self) -> Option<SessionSeat> {
        match self {
            Self::Host => Some(SessionSeat::Host),
            Self::PlayerOne => Some(SessionSeat::PlayerOne),
            Self::Spectator | Self::Outsider | Self::Clock => None,
        }
    }
}

/// Room lifecycle projection checked independently from concrete cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionPhase {
    /// Membership/readiness phase.
    Lobby,
    /// Armed logical countdown.
    Countdown,
    /// Game actions are enabled.
    Running,
    /// Game actions are disabled until a player resumes.
    Paused,
    /// Abstract game terminated.
    PostGame,
    /// Absorbing room closure.
    Closed,
}

impl SessionPhase {
    const fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    const fn is_terminal(self) -> bool {
        matches!(self, Self::PostGame | Self::Closed)
    }
}

/// Small contract substituted for the concrete Poche environment.
pub trait SessionGamePort {
    /// Finite game state retained by the session graph.
    type State: Copy + Eq + std::hash::Hash;

    /// Initial game value installed when countdown expiry starts once.
    fn initial() -> Self::State;
    /// Apply the sole abstract game action.
    fn act(state: Self::State) -> Option<Self::State>;
    /// Whether the environment reports terminal.
    fn is_terminal(state: Self::State) -> bool;
}

/// Two-value implementation: one accepted action reaches terminal.
pub struct SingleStepGamePort;

/// State of [`SingleStepGamePort`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbstractGameState {
    /// Waiting for the current actor.
    AwaitingAction,
    /// Environment terminal output.
    Complete,
}

impl SessionGamePort for SingleStepGamePort {
    type State = AbstractGameState;

    fn initial() -> Self::State {
        AbstractGameState::AwaitingAction
    }

    fn act(state: Self::State) -> Option<Self::State> {
        match state {
            AbstractGameState::AwaitingAction => Some(AbstractGameState::Complete),
            AbstractGameState::Complete => None,
        }
    }

    fn is_terminal(state: Self::State) -> bool {
        state == AbstractGameState::Complete
    }
}

/// Canonical state in the named bounded session scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct SessionCheckState {
    /// Explicit lifecycle.
    pub phase: SessionPhase,
    /// Readiness by fixed seat.
    pub ready: [bool; 2],
    /// Connectivity by fixed seat; identity/seat survive route loss.
    pub connected: [bool; 2],
    /// Spectator transport availability; membership survives route loss.
    pub spectator_connected: bool,
    /// Whether the one logical countdown token is live.
    pub countdown_live: bool,
    /// At-most-once start counter.
    pub start_count: u8,
    /// Records the readiness gate at the unique start.
    pub start_was_ready: bool,
    /// Whether the sole pause cycle has been consumed.
    pub pause_used: bool,
    /// One pending exact spectator request.
    pub pending_hand: Option<SessionSeat>,
    /// Current exact spectator entitlement.
    pub granted_hand: Option<SessionSeat>,
    /// One grant epoch may be minted in this scope.
    pub grant_epoch: u8,
    /// Bounded chat metadata; content is outside formal state.
    pub chat_count: u8,
    /// Abstract game environment value.
    pub game: AbstractGameState,
}

impl SessionCheckState {
    fn initial() -> Self {
        Self {
            phase: SessionPhase::Lobby,
            ready: [false; 2],
            connected: [true; 2],
            spectator_connected: true,
            countdown_live: false,
            start_count: 0,
            start_was_ready: false,
            pause_used: false,
            pending_hand: None,
            granted_hand: None,
            grant_epoch: 0,
            chat_count: 0,
            game: AbstractGameState::AwaitingAction,
        }
    }

    /// Current exact visibility for the single spectator.
    #[must_use]
    pub const fn spectator_can_see(self, owner: SessionSeat) -> bool {
        matches!(self.granted_hand, Some(granted) if granted as u8 == owner as u8)
    }

    fn all_ready_connected(self) -> bool {
        self.ready == [true; 2] && self.connected == [true; 2]
    }

    const fn principal_connected(self, actor: SessionPrincipal) -> bool {
        match actor {
            SessionPrincipal::Host => self.connected[SessionSeat::Host.index()],
            SessionPrincipal::PlayerOne => self.connected[SessionSeat::PlayerOne.index()],
            SessionPrincipal::Spectator => self.spectator_connected,
            SessionPrincipal::Outsider | SessionPrincipal::Clock => false,
        }
    }
}

/// Every command/environment choice enumerated from every reachable state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionCheckAction {
    /// Explicit scheduler stutter; demonstrates unconditional nontermination.
    Idle,
    /// Mark a seat ready.
    Ready(SessionSeat),
    /// Mark a seat unready and cancel a countdown.
    Unready(SessionSeat),
    /// Attempt to arm the countdown.
    ArmCountdown(SessionPrincipal),
    /// Attempt to abort the countdown.
    AbortCountdown(SessionPrincipal),
    /// Logical authority expiry.
    ExpireCountdown,
    /// Attempt one abstract game action.
    GameAction(SessionPrincipal),
    /// Attempt pause.
    Pause(SessionPrincipal),
    /// Attempt resume.
    Resume(SessionPrincipal),
    /// Lose one player's route.
    Disconnect(SessionSeat),
    /// Reconnect the same stable player.
    Reconnect(SessionSeat),
    /// Lose the spectator route without revoking membership.
    DisconnectSpectator,
    /// Reconnect the same stable spectator.
    ReconnectSpectator,
    /// Spectator requests one exact player's hand.
    RequestHand(SessionSeat),
    /// Target owner grants the sole spectator.
    GrantHand(SessionSeat),
    /// Target owner denies a pending request.
    DenyHand(SessionSeat),
    /// Target owner revokes future delivery.
    RevokeHand(SessionSeat),
    /// Attempt bounded chat.
    Chat(SessionPrincipal),
    /// Attempt room close.
    CloseRoom(SessionPrincipal),
}

fn all_actions() -> Vec<SessionCheckAction> {
    let mut actions = vec![
        SessionCheckAction::Idle,
        SessionCheckAction::ExpireCountdown,
        SessionCheckAction::DisconnectSpectator,
        SessionCheckAction::ReconnectSpectator,
    ];
    for seat in SessionSeat::ALL {
        actions.extend([
            SessionCheckAction::Ready(seat),
            SessionCheckAction::Unready(seat),
            SessionCheckAction::Disconnect(seat),
            SessionCheckAction::Reconnect(seat),
            SessionCheckAction::RequestHand(seat),
            SessionCheckAction::GrantHand(seat),
            SessionCheckAction::DenyHand(seat),
            SessionCheckAction::RevokeHand(seat),
        ]);
    }
    for actor in SessionPrincipal::ATTEMPT_ACTORS {
        actions.extend([
            SessionCheckAction::ArmCountdown(actor),
            SessionCheckAction::AbortCountdown(actor),
            SessionCheckAction::GameAction(actor),
            SessionCheckAction::Pause(actor),
            SessionCheckAction::Resume(actor),
            SessionCheckAction::Chat(actor),
            SessionCheckAction::CloseRoom(actor),
        ]);
    }
    actions
}

/// Accepted rule identity or stable default-deny outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionCheckDecision {
    /// Semantic event accepted under the cited stable rule.
    Accepted(&'static str),
    /// Attempt did not satisfy an exact allow.
    Denied(&'static str),
}

impl SessionCheckDecision {
    const fn accepted(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

#[allow(clippy::too_many_lines)]
fn transition(
    state: SessionCheckState,
    action: SessionCheckAction,
) -> (SessionCheckState, SessionCheckDecision) {
    use SessionCheckAction as A;
    use SessionCheckDecision::{Accepted, Denied};
    use SessionPhase as P;

    let deny = || (state, Denied("S-AUTH-001"));
    match action {
        A::Idle => (state, Accepted("S-TIME-007")),
        A::Ready(seat) if state.phase == P::Lobby && state.connected[seat.index()] => {
            let mut next = state;
            next.ready[seat.index()] = true;
            (next, Accepted("S-ROOM-004"))
        }
        A::Unready(seat) if matches!(state.phase, P::Lobby | P::Countdown) => {
            let mut next = state;
            next.phase = P::Lobby;
            next.ready[seat.index()] = false;
            next.countdown_live = false;
            (next, Accepted("S-ROOM-005"))
        }
        A::ArmCountdown(SessionPrincipal::Host)
            if state.phase == P::Lobby
                && state.connected[SessionSeat::Host.index()]
                && state.all_ready_connected() =>
        {
            let mut next = state;
            next.phase = P::Countdown;
            next.countdown_live = true;
            (next, Accepted("S-ROOM-006"))
        }
        A::AbortCountdown(actor)
            if state.phase == P::Countdown
                && actor
                    .seat()
                    .is_some_and(|seat| state.connected[seat.index()]) =>
        {
            let mut next = state;
            next.phase = P::Lobby;
            next.countdown_live = false;
            (next, Accepted("S-ROOM-007"))
        }
        A::ExpireCountdown
            if state.phase == P::Countdown
                && state.countdown_live
                && state.start_count == 0
                && state.all_ready_connected() =>
        {
            let mut next = state;
            next.phase = P::Running;
            next.countdown_live = false;
            next.ready = [false; 2];
            next.start_count = 1;
            next.start_was_ready = true;
            next.game = SingleStepGamePort::initial();
            (next, Accepted("S-ROOM-008"))
        }
        A::GameAction(actor)
            if state.phase == P::Running
                && actor
                    .seat()
                    .is_some_and(|seat| state.connected[seat.index()]) =>
        {
            let Some(game) = SingleStepGamePort::act(state.game) else {
                return deny();
            };
            let mut next = state;
            next.game = game;
            if SingleStepGamePort::is_terminal(game) {
                next.phase = P::PostGame;
            }
            (next, Accepted("S-ROOM-010"))
        }
        A::Pause(actor)
            if state.phase == P::Running
                && !state.pause_used
                && actor
                    .seat()
                    .is_some_and(|seat| state.connected[seat.index()]) =>
        {
            let mut next = state;
            next.phase = P::Paused;
            next.pause_used = true;
            (next, Accepted("S-ROOM-011"))
        }
        A::Resume(actor)
            if state.phase == P::Paused
                && actor
                    .seat()
                    .is_some_and(|seat| state.connected[seat.index()]) =>
        {
            let mut next = state;
            next.phase = P::Running;
            (next, Accepted("S-ROOM-012"))
        }
        A::Disconnect(seat) if state.phase.is_open() && state.connected[seat.index()] => {
            let mut next = state;
            next.connected[seat.index()] = false;
            next.ready[seat.index()] = false;
            if state.phase == P::Countdown {
                next.phase = P::Lobby;
                next.countdown_live = false;
            }
            (next, Accepted("S-ROOM-019"))
        }
        A::Reconnect(seat) if state.phase.is_open() && !state.connected[seat.index()] => {
            let mut next = state;
            next.connected[seat.index()] = true;
            (next, Accepted("S-ROOM-020"))
        }
        A::DisconnectSpectator if state.phase.is_open() && state.spectator_connected => {
            let mut next = state;
            next.spectator_connected = false;
            (next, Accepted("S-ROOM-019"))
        }
        A::ReconnectSpectator if state.phase.is_open() && !state.spectator_connected => {
            let mut next = state;
            next.spectator_connected = true;
            (next, Accepted("S-ROOM-020"))
        }
        A::RequestHand(owner)
            if state.phase.is_open()
                && state.spectator_connected
                && state.pending_hand.is_none()
                && state.grant_epoch == 0 =>
        {
            let mut next = state;
            next.pending_hand = Some(owner);
            (next, Accepted("S-VIEW-003"))
        }
        A::GrantHand(owner)
            if state.phase.is_open()
                && state.pending_hand == Some(owner)
                && state.connected[owner.index()]
                && state.spectator_connected
                && state.grant_epoch == 0 =>
        {
            let mut next = state;
            next.pending_hand = None;
            next.granted_hand = Some(owner);
            next.grant_epoch = 1;
            (next, Accepted("S-VIEW-004"))
        }
        A::DenyHand(owner) if state.phase.is_open() && state.pending_hand == Some(owner) => {
            let mut next = state;
            next.pending_hand = None;
            (next, Accepted("S-VIEW-004"))
        }
        A::RevokeHand(owner) if state.phase.is_open() && state.granted_hand == Some(owner) => {
            let mut next = state;
            next.granted_hand = None;
            (next, Accepted("S-VIEW-005"))
        }
        A::Chat(actor)
            if state.phase.is_open()
                && SessionPrincipal::MEMBERS.contains(&actor)
                && state.principal_connected(actor)
                && state.chat_count == 0 =>
        {
            let mut next = state;
            next.chat_count = 1;
            (next, Accepted("S-CHAT-001"))
        }
        A::CloseRoom(SessionPrincipal::Host)
            if state.phase.is_open() && state.connected[SessionSeat::Host.index()] =>
        {
            let mut next = state;
            next.phase = P::Closed;
            next.ready = [false; 2];
            next.countdown_live = false;
            next.pending_hand = None;
            next.granted_hand = None;
            (next, Accepted("S-ROOM-022"))
        }
        _ => deny(),
    }
}

/// Dense graph-local ID for session states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionStateId(u32);

impl SessionStateId {
    /// Zero-based stable BFS identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn index(self) -> usize {
        usize::try_from(self.0).expect("u32 session state ID fits usize")
    }
}

/// One attempted command edge, including stable authorization outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionCheckEdge {
    /// Source state.
    pub from: SessionStateId,
    /// Attempted action.
    pub action: SessionCheckAction,
    /// Allow or default-deny result.
    pub decision: SessionCheckDecision,
    /// Result state; denials leave it unchanged.
    pub to: SessionStateId,
}

#[derive(Clone, Copy, Debug)]
struct SessionPredecessor {
    state: SessionStateId,
    action: SessionCheckAction,
}

/// Fixed-point exploration measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionExplorationStats {
    /// Distinct reachable states.
    pub states: usize,
    /// All action attempts from every state.
    pub transitions: usize,
    /// Greatest shortest-path depth.
    pub maximum_depth: u32,
    /// Accepted semantic edges.
    pub accepted_transitions: usize,
    /// Default-denied attempts.
    pub denied_transitions: usize,
    /// Reachable terminal states.
    pub terminal_states: usize,
}

/// Deterministic complete bounded graph.
#[derive(Clone, Debug)]
pub struct SessionExplicitGraph {
    states: Vec<SessionCheckState>,
    edges: Vec<SessionCheckEdge>,
    predecessors: Vec<Option<SessionPredecessor>>,
    stats: SessionExplorationStats,
    semantic_hash: String,
}

impl SessionExplicitGraph {
    /// Reachable states in BFS discovery order.
    #[must_use]
    pub fn states(&self) -> &[SessionCheckState] {
        &self.states
    }

    /// All attempted action edges in state/action order.
    #[must_use]
    pub fn edges(&self) -> &[SessionCheckEdge] {
        &self.edges
    }

    /// Fixed-point counts.
    #[must_use]
    pub const fn stats(&self) -> SessionExplorationStats {
        self.stats
    }

    /// BLAKE3 of the explicit canonical state/edge table.
    #[must_use]
    pub fn semantic_hash(&self) -> &str {
        &self.semantic_hash
    }

    /// Canonical shortest action trace to `target`.
    #[must_use]
    pub fn shortest_trace(&self, target: SessionStateId) -> Option<Vec<SessionCheckAction>> {
        self.states.get(target.index())?;
        let mut cursor = target;
        let mut actions = Vec::new();
        while let Some(previous) = self.predecessors.get(cursor.index()).copied().flatten() {
            actions.push(previous.action);
            cursor = previous.state;
        }
        actions.reverse();
        Some(actions)
    }
}

/// Drain the deterministic BFS queue to a reachable-state fixed point.
///
/// # Panics
///
/// Panics only if this deliberately small scope exceeds `u32::MAX` states.
#[must_use]
pub fn explore_session() -> SessionExplicitGraph {
    let actions = all_actions();
    let initial = SessionCheckState::initial();
    let mut ids = HashMap::from([(initial, SessionStateId(0))]);
    let mut states = vec![initial];
    let mut depths = vec![0_u32];
    let mut predecessors = vec![None];
    let mut edges = Vec::new();
    let mut queue = VecDeque::from([SessionStateId(0)]);

    while let Some(from) = queue.pop_front() {
        let state = states[from.index()];
        for action in &actions {
            let (next, decision) = transition(state, *action);
            let to = if let Some(existing) = ids.get(&next).copied() {
                existing
            } else {
                let id = SessionStateId(
                    u32::try_from(states.len()).expect("session scope has fewer than u32 states"),
                );
                ids.insert(next, id);
                states.push(next);
                depths.push(depths[from.index()] + 1);
                predecessors.push(Some(SessionPredecessor {
                    state: from,
                    action: *action,
                }));
                queue.push_back(id);
                id
            };
            edges.push(SessionCheckEdge {
                from,
                action: *action,
                decision,
                to,
            });
        }
    }

    let accepted_transitions = edges.iter().filter(|edge| edge.decision.accepted()).count();
    let measurements = SessionExplorationStats {
        states: states.len(),
        transitions: edges.len(),
        maximum_depth: depths.iter().copied().max().unwrap_or(0),
        accepted_transitions,
        denied_transitions: edges.len() - accepted_transitions,
        terminal_states: states
            .iter()
            .filter(|state| state.phase.is_terminal())
            .count(),
    };
    let semantic_hash = hash_graph(&states, &edges);
    SessionExplicitGraph {
        states,
        edges,
        predecessors,
        stats: measurements,
        semantic_hash,
    }
}

fn hash_graph(states: &[SessionCheckState], edges: &[SessionCheckEdge]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SESSION_SCOPE_ID.as_bytes());
    for (index, state) in states.iter().enumerate() {
        hasher.update(format!("\nS{index}:{state:?}").as_bytes());
    }
    for edge in edges {
        hasher.update(
            format!(
                "\nE{}:{:?}:{:?}:{}",
                edge.from.get(),
                edge.action,
                edge.decision,
                edge.to.get()
            )
            .as_bytes(),
        );
    }
    hasher.finalize().to_hex().to_string()
}

/// Shortest witness for one deliberately false claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFalseInvariant {
    /// Stable claim name.
    pub name: &'static str,
    /// Minimal BFS action prefix to a violating state.
    pub actions: Vec<SessionCheckAction>,
}

/// Safety and qualified-liveness evidence over the fixed-point graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionCheckReport {
    /// Exact named scope.
    pub scope: &'static str,
    /// Deterministic graph measurements.
    pub stats: SessionExplorationStats,
    /// Stable graph semantic hash.
    pub semantic_hash: String,
    /// Number of independently evaluated safety obligations.
    pub safety_properties: usize,
    /// Unconditional termination is expected false because idle/pause/partition may persist.
    pub unconditional_termination: bool,
    /// Every state has some terminal path when enabling actions eventually occur.
    pub every_state_has_terminal_path: bool,
    /// Named fairness/enabling obligations; no unconditional claim is made.
    pub liveness_assumptions: [&'static str; 5],
    /// Minimal traces for known-false claims.
    pub false_invariants: Vec<SessionFalseInvariant>,
}

type SessionClaimPredicate = fn(&SessionCheckState) -> bool;

/// Check security invariants, progress qualification, and controlled false claims.
///
/// # Panics
///
/// Panics only when the checker itself violates the declared bounded contract.
#[must_use]
pub fn check_session() -> SessionCheckReport {
    let graph = explore_session();
    let safety_properties = check_session_safety(&graph);
    let every_state_has_terminal_path = every_state_reaches_terminal(&graph);
    assert!(every_state_has_terminal_path);

    let known_false_claims: [(&str, SessionClaimPredicate); 3] = [
        (
            "all-reachable-states-are-terminal",
            |state: &SessionCheckState| !state.phase.is_terminal(),
        ),
        ("pause-is-unreachable", |state: &SessionCheckState| {
            state.phase == SessionPhase::Paused
        }),
        (
            "spectator-never-sees-a-hand",
            |state: &SessionCheckState| state.granted_hand.is_some(),
        ),
    ];
    let false_invariants = known_false_claims
        .into_iter()
        .map(|(name, violates)| {
            let (index, _) = graph
                .states
                .iter()
                .enumerate()
                .find(|(_, state)| violates(state))
                .expect("known-false session claim must have a witness");
            let id = SessionStateId(u32::try_from(index).expect("state index fits u32"));
            SessionFalseInvariant {
                name,
                actions: graph
                    .shortest_trace(id)
                    .expect("discovered state has a shortest trace"),
            }
        })
        .collect();

    SessionCheckReport {
        scope: SESSION_SCOPE_ID,
        stats: graph.stats,
        semantic_hash: graph.semantic_hash.clone(),
        safety_properties,
        unconditional_termination: false,
        every_state_has_terminal_path,
        liveness_assumptions: SESSION_LIVENESS_ASSUMPTIONS,
        false_invariants,
    }
}

fn check_session_safety(graph: &SessionExplicitGraph) -> usize {
    for state in &graph.states {
        assert!(state.start_count <= 1, "start occurs at most once");
        assert!(state.grant_epoch <= 1, "grant epoch is bounded");
        assert!(state.chat_count <= 1, "chat metadata is bounded");
        if state.start_count == 1 {
            assert!(state.start_was_ready, "start records the readiness gate");
        }
        if state.phase == SessionPhase::Countdown {
            assert!(state.countdown_live && state.all_ready_connected());
        }
        if matches!(
            state.phase,
            SessionPhase::Running | SessionPhase::Paused | SessionPhase::PostGame
        ) {
            assert_eq!(state.start_count, 1);
        }
        for owner in SessionSeat::ALL {
            assert_eq!(
                state.spectator_can_see(owner),
                state.granted_hand == Some(owner),
                "spectator knowledge is exactly grant-scoped"
            );
        }
    }
    for edge in &graph.edges {
        let before = graph.states[edge.from.index()];
        let after = graph.states[edge.to.index()];
        if action_actor(edge.action) == Some(SessionPrincipal::Outsider) {
            assert!(!edge.decision.accepted(), "outsider is default denied");
        }
        if before.phase == SessionPhase::Paused
            && matches!(edge.action, SessionCheckAction::GameAction(_))
        {
            assert!(!edge.decision.accepted());
            assert_eq!(before.game, after.game, "paused game cannot advance");
        }
        if !edge.decision.accepted() {
            assert_eq!(before, after, "denials are atomic no-ops");
        }
        if before.phase == SessionPhase::Closed {
            assert_eq!(after.phase, SessionPhase::Closed, "close is absorbing");
        }
    }
    10
}

const fn action_actor(action: SessionCheckAction) -> Option<SessionPrincipal> {
    match action {
        SessionCheckAction::ArmCountdown(actor)
        | SessionCheckAction::AbortCountdown(actor)
        | SessionCheckAction::GameAction(actor)
        | SessionCheckAction::Pause(actor)
        | SessionCheckAction::Resume(actor)
        | SessionCheckAction::Chat(actor)
        | SessionCheckAction::CloseRoom(actor) => Some(actor),
        SessionCheckAction::Ready(_)
        | SessionCheckAction::Unready(_)
        | SessionCheckAction::Disconnect(_)
        | SessionCheckAction::Reconnect(_)
        | SessionCheckAction::DisconnectSpectator
        | SessionCheckAction::ReconnectSpectator
        | SessionCheckAction::RequestHand(_)
        | SessionCheckAction::GrantHand(_)
        | SessionCheckAction::DenyHand(_)
        | SessionCheckAction::RevokeHand(_)
        | SessionCheckAction::Idle
        | SessionCheckAction::ExpireCountdown => None,
    }
}

fn every_state_reaches_terminal(graph: &SessionExplicitGraph) -> bool {
    let mut reaches = vec![false; graph.states.len()];
    let mut queue = VecDeque::new();
    for (index, state) in graph.states.iter().enumerate() {
        if state.phase.is_terminal() {
            reaches[index] = true;
            queue.push_back(SessionStateId(
                u32::try_from(index).expect("state index fits u32"),
            ));
        }
    }
    let mut incoming = vec![Vec::new(); graph.states.len()];
    for edge in &graph.edges {
        if edge.decision.accepted() && edge.from != edge.to {
            incoming[edge.to.index()].push(edge.from);
        }
    }
    while let Some(target) = queue.pop_front() {
        for source in &incoming[target.index()] {
            if !reaches[source.index()] {
                reaches[source.index()] = true;
                queue.push_back(*source);
            }
        }
    }
    reaches.into_iter().all(std::convert::identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_scope_reaches_fixed_point_and_checks_catalog() {
        let report = check_session();
        assert_eq!(report.scope, SESSION_SCOPE_ID);
        assert_eq!(report.safety_properties, 10);
        assert!(!report.unconditional_termination);
        assert!(report.every_state_has_terminal_path);
        assert_eq!(report.false_invariants.len(), 3);
        assert_eq!(report.false_invariants[0].actions.len(), 0);
        assert!(!report.semantic_hash.is_empty());
    }

    #[test]
    fn grant_and_revoke_change_only_future_spectator_knowledge() {
        let initial = SessionCheckState::initial();
        let (requested, request) =
            transition(initial, SessionCheckAction::RequestHand(SessionSeat::Host));
        assert!(request.accepted());
        let (granted, grant) =
            transition(requested, SessionCheckAction::GrantHand(SessionSeat::Host));
        assert!(grant.accepted());
        assert!(granted.spectator_can_see(SessionSeat::Host));
        let (revoked, revoke) =
            transition(granted, SessionCheckAction::RevokeHand(SessionSeat::Host));
        assert!(revoke.accepted());
        assert!(!revoked.spectator_can_see(SessionSeat::Host));
    }
}
