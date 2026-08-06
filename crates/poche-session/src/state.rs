use core::fmt;

use facet::Facet;
use poche_protocol::{
    ChanceWire, CommandId, CommandKind, CommandPayload, CorrelationId, CountdownToken, DenyReason,
    GameActionWire, GamePublicStateWire, PolicyId, PrincipalId, PublicGameEventWire, RoomId,
    SemanticHash,
};

use crate::PolicyDecision;

/// Maximum seated or unseated memberships retained by the phase-2 engine.
pub const MAX_MEMBERS: usize = 8;
/// Maximum accepted chat messages per logical revision window.
pub const CHAT_MESSAGES_PER_WINDOW: u16 = 5;
/// Logical revision width of one chat-rate window.
pub const CHAT_WINDOW_REVISIONS: u64 = 20;
/// Maximum simultaneously retained spectator hand requests.
pub const MAX_HAND_REQUESTS: usize = 64;
/// Maximum simultaneously active spectator hand grants.
pub const MAX_HAND_GRANTS: usize = 64;

/// Authority-derived principal classification. Commands never supply this tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum PrincipalKind {
    Unknown,
    Host,
    Member,
    Player,
    Spectator,
    DisconnectedMember,
    AuthorityClock,
    GameEnvironment,
}

/// Current application membership connection state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ConnectionState {
    Connected,
    Disconnected,
}

/// Current member record. Seat ownership and connection are orthogonal.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct MemberState {
    pub principal_id: PrincipalId,
    pub membership_epoch: u64,
    pub connection: ConnectionState,
    pub seat: Option<u8>,
    pub ready: bool,
    pub host: bool,
    pub chat_window_start_revision: u64,
    pub chat_messages_in_window: u16,
}

/// One authority-known invite verifier. The raw invite never enters events.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteRecord {
    verifier: [u8; 32],
    pub expires_after_revision: u64,
    pub consumed: bool,
    pub revoked: bool,
}

impl fmt::Debug for InviteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InviteRecord")
            .field("verifier", &"<redacted>")
            .field("expires_after_revision", &self.expires_after_revision)
            .field("consumed", &self.consumed)
            .field("revoked", &self.revoked)
            .finish()
    }
}

impl InviteRecord {
    /// Construct a test/runtime-provisioned invite verifier.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized verifier text.
    pub fn new(
        verifier: impl Into<String>,
        expires_after_revision: u64,
    ) -> Result<Self, SessionInvariantError> {
        let candidate = verifier.into();
        if candidate.is_empty() || candidate.len() > 256 {
            return Err(SessionInvariantError::InvalidInvite);
        }
        Ok(Self {
            verifier: invite_verifier(candidate.as_bytes()),
            expires_after_revision,
            consumed: false,
            revoked: false,
        })
    }

    pub(crate) fn matches(&self, candidate: &str) -> bool {
        constant_time_equal(&self.verifier, &invite_verifier(candidate.as_bytes()))
    }
}

fn invite_verifier(candidate: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-session-invite-v1\0");
    hasher.update(candidate);
    *hasher.finalize().as_bytes()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let maximum = left.len().max(right.len());
    for index in 0..maximum {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

/// Custom policy match target. Every variant is explicit; there is no unknown-role wildcard.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PrincipalSelector {
    Exact(PrincipalId),
    Kind(PrincipalKind),
}

/// Enforce or audit-only policy effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PolicyEffect {
    Allow,
    Deny(DenyReason),
}

/// One immutable room policy entry.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct PolicyRule {
    pub policy_id: PolicyId,
    /// Higher priorities are evaluated first; policy ID breaks ties.
    pub priority: i32,
    pub principal: PrincipalSelector,
    pub command: CommandKind,
    pub effect: PolicyEffect,
    pub audit_only: bool,
}

impl PolicyRule {
    pub(crate) fn applies(
        &self,
        principal_id: &PrincipalId,
        kind: PrincipalKind,
        command: CommandKind,
    ) -> bool {
        self.command == command
            && match &self.principal {
                PrincipalSelector::Exact(candidate) => candidate == principal_id,
                PrincipalSelector::Kind(candidate) => *candidate == kind,
            }
    }
}

/// Current owner of game action selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameTurn {
    Chance,
    Player(u8),
    Environment,
    Finished,
}

/// Result of one pure game transition at the session boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameTransition<G> {
    pub game: G,
    pub round_scores: Option<Vec<i32>>,
    pub terminal: bool,
}

/// Public meaning attached to one game transition without exposing chance data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicGameChange {
    PlayerAction { seat: u8, action: GameActionWire },
    RoundScored { scores: Vec<i32>, terminal: bool },
}

/// A pending spectator request addressed to one seated player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandViewRequest {
    pub request_id: CommandId,
    pub player: PrincipalId,
    pub recipient: PrincipalId,
}

/// An active, exact spectator capability. No card data is stored here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandViewGrant {
    pub player: PrincipalId,
    pub recipient: PrincipalId,
    pub grant_epoch: u64,
}

/// Why pending/granted hand capabilities stop applying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandCapabilityExpiry {
    RoundBoundary,
    SeatRoleChanged { principal: PrincipalId },
    MembershipLost { principal: PrincipalId },
}

/// Pure game port used by the session reducer only after lifecycle gating.
pub trait SessionGame: Clone + Eq {
    type Error;

    /// Construct a game for seats in canonical seat order.
    ///
    /// # Errors
    ///
    /// Returns a game-specific invalid-composition error.
    fn start(seats: &[(u8, PrincipalId)]) -> Result<Self, Self::Error>;

    /// Return the current owner of action selection.
    fn turn(&self) -> GameTurn;

    /// Produce public game knowledge with every private hand absent.
    ///
    /// # Errors
    ///
    /// Returns a game-specific projection/conversion error.
    fn public_projection(&self) -> Result<GamePublicStateWire, Self::Error>;

    /// Produce exactly one seat's private hand as canonical card codes.
    ///
    /// # Errors
    ///
    /// Returns a game-specific seat/projection error.
    fn private_hand(&self, seat: u8) -> Result<Vec<u8>, Self::Error>;

    /// Apply one player-owned action.
    ///
    /// # Errors
    ///
    /// Returns a game-specific action/precondition error.
    fn player_transition(
        &self,
        seat: u8,
        action: &GameActionWire,
    ) -> Result<GameTransition<Self>, Self::Error>;

    /// Apply one explicit chance input.
    ///
    /// # Errors
    ///
    /// Returns a game-specific chance/precondition error.
    fn chance_transition(&self, chance: &ChanceWire) -> Result<GameTransition<Self>, Self::Error>;

    /// Apply deterministic settlement.
    ///
    /// # Errors
    ///
    /// Returns a game-specific settlement/precondition error.
    fn settle(&self) -> Result<GameTransition<Self>, Self::Error>;
}

/// Strong phase state: countdown/game data exists only in compatible variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionPhase<G> {
    Uninitialized,
    Lobby,
    Countdown {
        deadline_tick: u64,
        token: CountdownToken,
    },
    Running {
        game: G,
    },
    Paused {
        game: G,
    },
    PostGame {
        game: G,
    },
    Closed,
}

/// Provenance repeated on every event so apply remains pure and idempotent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventProvenance {
    pub command_id: CommandId,
    pub command_hash: SemanticHash,
    pub principal_id: PrincipalId,
    pub correlation_id: CorrelationId,
    pub base_revision: u64,
    pub event_index: u16,
    pub decision: PolicyDecision,
}

/// Pure session event kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEventKind<G> {
    RoomCreated {
        host: PrincipalId,
    },
    MemberJoined {
        principal: PrincipalId,
        invite_index: u16,
    },
    SeatTaken {
        principal: PrincipalId,
        seat: u8,
    },
    SeatReleased {
        principal: PrincipalId,
        seat: u8,
    },
    ReadyChanged {
        principal: PrincipalId,
        ready: bool,
    },
    CountdownArmed {
        deadline_tick: u64,
        token: CountdownToken,
    },
    CountdownCancelled {
        token: CountdownToken,
    },
    GameStarted {
        game: G,
    },
    GameAdvanced {
        game: G,
        round_scores: Option<Vec<i32>>,
        terminal: bool,
        public_change: Option<PublicGameChange>,
    },
    Paused,
    Unpaused,
    MemberDisconnected {
        principal: PrincipalId,
    },
    MemberReconnected {
        principal: PrincipalId,
    },
    MemberLeft {
        principal: PrincipalId,
    },
    MemberRemoved {
        principal: PrincipalId,
    },
    LobbyReset,
    ChatPosted {
        principal: PrincipalId,
        text: String,
    },
    HandViewRequested {
        request_id: CommandId,
        player: PrincipalId,
        recipient: PrincipalId,
    },
    HandViewGranted {
        request_id: CommandId,
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    HandViewDenied {
        request_id: CommandId,
        player: PrincipalId,
        recipient: PrincipalId,
    },
    HandViewRevoked {
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    HandCapabilitiesExpired {
        reason: HandCapabilityExpiry,
    },
    RoomClosed,
}

/// One replayable event with command provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEvent<G> {
    pub provenance: EventProvenance,
    pub kind: SessionEventKind<G>,
}

/// Accepted command record used for exact duplicate replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedCommand<G> {
    pub command_id: CommandId,
    pub command_hash: SemanticHash,
    pub decision: PolicyDecision,
    pub events: Vec<SessionEvent<G>>,
}

/// Complete authoritative session state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionState<G> {
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub revision: u64,
    pub host: Option<PrincipalId>,
    pub authority_clock: PrincipalId,
    pub game_environment: PrincipalId,
    pub phase: SessionPhase<G>,
    pub members: Vec<MemberState>,
    pub invites: Vec<InviteRecord>,
    pub policies: Vec<PolicyRule>,
    pub processed_commands: Vec<ProcessedCommand<G>>,
    pub chat_count: u64,
    pub projection_epoch: u64,
    pub hand_requests: Vec<HandViewRequest>,
    pub hand_grants: Vec<HandViewGrant>,
    pub public_history: Vec<PublicGameEventWire>,
}

impl<G> SessionState<G> {
    /// Construct a room slot that accepts exactly one `CreateRoom` command.
    #[must_use]
    pub const fn pending(
        room_id: RoomId,
        authority_clock: PrincipalId,
        game_environment: PrincipalId,
    ) -> Self {
        Self {
            room_id,
            session_epoch: 0,
            revision: 0,
            host: None,
            authority_clock,
            game_environment,
            phase: SessionPhase::Uninitialized,
            members: Vec::new(),
            invites: Vec::new(),
            policies: Vec::new(),
            processed_commands: Vec::new(),
            chat_count: 0,
            projection_epoch: 0,
            hand_requests: Vec::new(),
            hand_grants: Vec::new(),
            public_history: Vec::new(),
        }
    }

    /// Borrow one current member by stable application principal.
    #[must_use]
    pub fn member(&self, principal: &PrincipalId) -> Option<&MemberState> {
        self.members
            .iter()
            .find(|member| member.principal_id == *principal)
    }

    pub(crate) fn member_mut(&mut self, principal: &PrincipalId) -> Option<&mut MemberState> {
        self.members
            .iter_mut()
            .find(|member| member.principal_id == *principal)
    }

    /// Return the member occupying a seat.
    #[must_use]
    pub fn seat_owner(&self, seat: u8) -> Option<&MemberState> {
        self.members.iter().find(|member| member.seat == Some(seat))
    }

    /// Derive a principal kind from authority state rather than a wire claim.
    #[must_use]
    pub fn principal_kind(&self, principal: &PrincipalId) -> PrincipalKind {
        if *principal == self.authority_clock {
            return PrincipalKind::AuthorityClock;
        }
        if *principal == self.game_environment {
            return PrincipalKind::GameEnvironment;
        }
        let Some(member) = self.member(principal) else {
            return PrincipalKind::Unknown;
        };
        if member.connection == ConnectionState::Disconnected {
            return PrincipalKind::DisconnectedMember;
        }
        if member.host {
            PrincipalKind::Host
        } else if member.seat.is_some() {
            PrincipalKind::Player
        } else {
            PrincipalKind::Spectator
        }
    }

    /// Validate structural invariants after replay/application.
    ///
    /// # Errors
    ///
    /// Returns the first stable invariant category found.
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), SessionInvariantError> {
        if self.members.len() > MAX_MEMBERS {
            return Err(SessionInvariantError::TooManyMembers);
        }
        if self.hand_requests.len() > MAX_HAND_REQUESTS || self.hand_grants.len() > MAX_HAND_GRANTS
        {
            return Err(SessionInvariantError::TooManyHandCapabilities);
        }
        for (index, member) in self.members.iter().enumerate() {
            if member
                .seat
                .is_some_and(|seat| usize::from(seat) >= MAX_MEMBERS)
            {
                return Err(SessionInvariantError::InvalidSeat);
            }
            if member.ready
                && (member.connection != ConnectionState::Connected || member.seat.is_none())
            {
                return Err(SessionInvariantError::InvalidReadiness);
            }
            if self.members[index + 1..].iter().any(|other| {
                other.principal_id == member.principal_id
                    || (member.seat.is_some() && other.seat == member.seat)
            }) {
                return Err(SessionInvariantError::DuplicateMemberOrSeat);
            }
        }
        for (index, policy) in self.policies.iter().enumerate() {
            if self.policies[index + 1..]
                .iter()
                .any(|other| other.policy_id == policy.policy_id)
            {
                return Err(SessionInvariantError::InvalidPolicy);
            }
        }
        match &self.phase {
            SessionPhase::Uninitialized => {
                if self.host.is_some() || !self.members.is_empty() {
                    return Err(SessionInvariantError::InvalidPhaseData);
                }
            }
            SessionPhase::Countdown { .. } => {
                let seated: Vec<_> = self
                    .members
                    .iter()
                    .filter(|member| member.seat.is_some())
                    .collect();
                if seated.len() < 2
                    || seated.iter().any(|member| {
                        !member.ready || member.connection != ConnectionState::Connected
                    })
                {
                    return Err(SessionInvariantError::InvalidReadiness);
                }
            }
            _ => {}
        }
        if let Some(host) = &self.host {
            let Some(member) = self.member(host) else {
                return Err(SessionInvariantError::MissingHost);
            };
            if !member.host || self.members.iter().filter(|member| member.host).count() != 1 {
                return Err(SessionInvariantError::MissingHost);
            }
        }
        for (index, request) in self.hand_requests.iter().enumerate() {
            let valid_player = self
                .member(&request.player)
                .is_some_and(|member| member.seat.is_some());
            let valid_recipient = self
                .member(&request.recipient)
                .is_some_and(|member| member.seat.is_none());
            if !valid_player
                || !valid_recipient
                || request.player == request.recipient
                || self.hand_requests[index + 1..].iter().any(|other| {
                    other.request_id == request.request_id || other.recipient == request.recipient
                })
            {
                return Err(SessionInvariantError::InvalidHandCapability);
            }
        }
        for (index, grant) in self.hand_grants.iter().enumerate() {
            let valid_player = self
                .member(&grant.player)
                .is_some_and(|member| member.seat.is_some());
            let valid_recipient = self
                .member(&grant.recipient)
                .is_some_and(|member| member.seat.is_none());
            if !valid_player
                || !valid_recipient
                || grant.player == grant.recipient
                || grant.grant_epoch == 0
                || grant.grant_epoch > self.projection_epoch
                || self.hand_grants[index + 1..]
                    .iter()
                    .any(|other| other.recipient == grant.recipient)
            {
                return Err(SessionInvariantError::InvalidHandCapability);
            }
        }
        for (index, record) in self.processed_commands.iter().enumerate() {
            if record
                .events
                .iter()
                .enumerate()
                .any(|(event_index, event)| {
                    event.provenance.command_id != record.command_id
                        || event.provenance.command_hash != record.command_hash
                        || usize::from(event.provenance.event_index) != event_index
                })
                || self.processed_commands[index + 1..]
                    .iter()
                    .any(|other| other.command_id == record.command_id)
            {
                return Err(SessionInvariantError::InvalidCommandRecord);
            }
        }
        Ok(())
    }
}

/// Stable structural invariant category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionInvariantError {
    TooManyMembers,
    InvalidSeat,
    InvalidReadiness,
    DuplicateMemberOrSeat,
    InvalidPhaseData,
    MissingHost,
    InvalidCommandRecord,
    InvalidPolicy,
    InvalidInvite,
    TooManyHandCapabilities,
    InvalidHandCapability,
}

/// Stable semantic reducer error.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionError<E> {
    Denied(DenyReason),
    Game(E),
    Invariant(SessionInvariantError),
    ConflictingCommandId,
    EventOrder,
    UnsupportedInThisTask,
}

impl<E> From<SessionInvariantError> for SessionError<E> {
    fn from(value: SessionInvariantError) -> Self {
        Self::Invariant(value)
    }
}

/// Return the stable kind of one command payload.
pub(crate) const fn command_kind(payload: &CommandPayload) -> CommandKind {
    payload.kind()
}
