use core::fmt;
use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ids::{
    CommandId, CorrelationId, CountdownToken, EventId, PolicyId, PrincipalId, ProjectionId, RoomId,
    SnapshotId,
};

/// The only currently accepted protocol version.
pub const PROTOCOL_VERSION_V1: u16 = 1;
/// The only currently accepted canonical signature-domain version.
pub const SIGNATURE_DOMAIN_V1: u16 = 1;
/// Maximum UTF-8 bytes in one chat message.
pub const MAX_CHAT_BYTES: usize = 2_048;
/// Maximum cards accepted by the Poche chance wire shape.
pub const MAX_DECK_CARDS: usize = 52;
const MAX_CARD_CODE_EXCLUSIVE: u8 = 52;

/// Stable 256-bit semantic identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SemanticHash(pub [u8; 32]);

/// Supported application signature algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum SignatureAlgorithm {
    /// Ed25519 over the versioned canonical signing domain.
    Ed25519,
}

/// Lowercase hexadecimal Ed25519 signature bytes.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureBytes(String);

impl SignatureBytes {
    /// Validate and construct a 64-byte lowercase hexadecimal signature.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeValidationError::InvalidSignature`] for another shape.
    pub fn new(value: impl Into<String>) -> Result<Self, EnvelopeValidationError> {
        let value = value.into();
        let candidate = Self(value);
        candidate.validate()?;
        Ok(candidate)
    }

    /// Return the public wire representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn validate(&self) -> Result<(), EnvelopeValidationError> {
        if self.0.len() == 128
            && self
                .0
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(())
        } else {
            Err(EnvelopeValidationError::InvalidSignature)
        }
    }
}

/// Invite verifier carried only to the room authority.
#[derive(Clone, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InviteProof(String);

impl InviteProof {
    /// Construct a bounded nonempty invite proof.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeValidationError::InvalidPayload`] for an invalid bound.
    pub fn new(value: impl Into<String>) -> Result<Self, EnvelopeValidationError> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 {
            Err(EnvelopeValidationError::InvalidPayload)
        } else {
            Ok(Self(value))
        }
    }

    /// Explicitly expose the verifier only to the authority's invite checker.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InviteProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InviteProof(<redacted>)")
    }
}

/// Signature intent included in canonical bytes before a signature exists.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureIntent {
    /// Canonical signature-domain version.
    pub domain_version: u16,
    /// Application signature algorithm.
    pub algorithm: SignatureAlgorithm,
    /// Stable application key identifier.
    pub key_id: PrincipalId,
}

/// Signature metadata attached after canonical bytes are signed.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureMetadata {
    /// Canonical signature-domain version.
    pub domain_version: u16,
    /// Application signature algorithm.
    pub algorithm: SignatureAlgorithm,
    /// Stable application key identifier.
    pub key_id: PrincipalId,
    /// Lowercase hexadecimal signature.
    pub signature: SignatureBytes,
}

impl SignatureMetadata {
    /// Return the signature-independent intent.
    #[must_use]
    pub fn intent(&self) -> SignatureIntent {
        SignatureIntent {
            domain_version: self.domain_version,
            algorithm: self.algorithm,
            key_id: self.key_id.clone(),
        }
    }
}

/// Stable room lifecycle projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum RoomPhase {
    /// Accepting membership, seats, and readiness changes.
    Lobby,
    /// Waiting for one authority-owned logical expiry token.
    Countdown,
    /// Game transitions may advance.
    Running,
    /// Game state is frozen while session-side operations continue.
    Paused,
    /// Terminal game result remains available.
    PostGame,
    /// Absorbing room state.
    Closed,
}

/// Poche player action carried by the inspectable protocol.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GameActionWire {
    /// Announce a whole-number trick bid.
    Bid {
        /// Number of tricks bid.
        tricks: u8,
    },
    /// Play one canonical standard-deck card code.
    Play {
        /// Dense card code in `0..52`.
        card: u8,
    },
}

/// Explicit replayable chance input.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChanceWire {
    /// Canonical card codes in explicit deck order.
    pub cards: Vec<u8>,
    /// Optional deterministic derivation seed retained as provenance.
    pub seed: Option<u64>,
    /// Optional deal ordinal paired with the seed.
    pub deal_ordinal: Option<u32>,
}

/// Typed command payload. Variant tags are part of the v1 schema identity.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CommandPayload {
    /// Create a new host-authoritative room.
    CreateRoom,
    /// Redeem a one-time/expiring room invite.
    RedeemInvite { invite: InviteProof },
    /// Occupy one lobby seat.
    TakeSeat { seat: u8 },
    /// Release the caller's current seat.
    ReleaseSeat,
    /// Mark the caller's seat ready.
    Ready,
    /// Mark the caller unready and cancel a countdown.
    Unready,
    /// Arm a logical countdown.
    ArmCountdown {
        deadline_tick: u64,
        countdown_token: CountdownToken,
    },
    /// Abort the current countdown.
    AbortCountdown,
    /// Deliver an authority-clock expiry token.
    CountdownExpired { countdown_token: CountdownToken },
    /// Freeze game transitions.
    Pause,
    /// Resume game transitions.
    Unpause,
    /// Apply one player-owned game action.
    GameAction { action: GameActionWire },
    /// Apply explicit chance owned by the environment.
    ApplyChance { chance: ChanceWire },
    /// Apply deterministic environment settlement.
    Settle,
    /// Send bounded ephemeral room chat.
    Chat { text: String },
    /// Request a viewer grant from one active player.
    RequestHand { player: PrincipalId },
    /// Grant the exact pending viewer request.
    GrantHand {
        request_id: CommandId,
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    /// Revoke an exact viewer grant.
    RevokeHand {
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    /// Rebind a durable membership to a new transport.
    Reconnect,
    /// Voluntarily revoke the caller's membership.
    Leave,
    /// Revoke another member as host.
    RemoveMember { target: PrincipalId },
    /// Archive a post-game result and return to an unready lobby.
    ResetLobby,
    /// Close the room permanently.
    CloseRoom,
}

/// Stable semantic command classification used by authorization policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    CreateRoom,
    RedeemInvite,
    TakeSeat,
    ReleaseSeat,
    Ready,
    Unready,
    ArmCountdown,
    AbortCountdown,
    CountdownExpired,
    Pause,
    Unpause,
    GameAction,
    ApplyChance,
    Settle,
    Chat,
    RequestHand,
    GrantHand,
    RevokeHand,
    Reconnect,
    Leave,
    RemoveMember,
    ResetLobby,
    CloseRoom,
}

impl CommandPayload {
    /// Return the stable authorization-policy classification.
    #[must_use]
    pub const fn kind(&self) -> CommandKind {
        match self {
            Self::CreateRoom => CommandKind::CreateRoom,
            Self::RedeemInvite { .. } => CommandKind::RedeemInvite,
            Self::TakeSeat { .. } => CommandKind::TakeSeat,
            Self::ReleaseSeat => CommandKind::ReleaseSeat,
            Self::Ready => CommandKind::Ready,
            Self::Unready => CommandKind::Unready,
            Self::ArmCountdown { .. } => CommandKind::ArmCountdown,
            Self::AbortCountdown => CommandKind::AbortCountdown,
            Self::CountdownExpired { .. } => CommandKind::CountdownExpired,
            Self::Pause => CommandKind::Pause,
            Self::Unpause => CommandKind::Unpause,
            Self::GameAction { .. } => CommandKind::GameAction,
            Self::ApplyChance { .. } => CommandKind::ApplyChance,
            Self::Settle => CommandKind::Settle,
            Self::Chat { .. } => CommandKind::Chat,
            Self::RequestHand { .. } => CommandKind::RequestHand,
            Self::GrantHand { .. } => CommandKind::GrantHand,
            Self::RevokeHand { .. } => CommandKind::RevokeHand,
            Self::Reconnect => CommandKind::Reconnect,
            Self::Leave => CommandKind::Leave,
            Self::RemoveMember { .. } => CommandKind::RemoveMember,
            Self::ResetLobby => CommandKind::ResetLobby,
            Self::CloseRoom => CommandKind::CloseRoom,
        }
    }
}

/// Command fields that exist before a signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedCommandEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub command_id: CommandId,
    pub principal_id: PrincipalId,
    pub expected_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub payload: CommandPayload,
    pub signature_intent: SignatureIntent,
}

/// Application-signed command accepted by the authority boundary.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub command_id: CommandId,
    pub principal_id: PrincipalId,
    pub expected_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub payload: CommandPayload,
    pub signature: SignatureMetadata,
}

impl UnsignedCommandEnvelope {
    /// Attach a signature without changing any signed field.
    #[must_use]
    pub fn attach_signature(self, signature: SignatureBytes) -> CommandEnvelope {
        CommandEnvelope {
            protocol_version: self.protocol_version,
            room_id: self.room_id,
            session_epoch: self.session_epoch,
            command_id: self.command_id,
            principal_id: self.principal_id,
            expected_revision: self.expected_revision,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            payload: self.payload,
            signature: SignatureMetadata {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        }
    }

    pub(crate) fn validate(&self) -> Result<(), EnvelopeValidationError> {
        validate_unsigned_common(
            self.protocol_version,
            &self.room_id,
            &self.principal_id,
            &self.correlation_id,
            &self.signature_intent,
        )?;
        if !self.command_id.validate()
            || self.causation_id.as_ref().is_some_and(|id| !id.validate())
        {
            return Err(EnvelopeValidationError::InvalidIdentifier);
        }
        validate_command_payload(&self.payload)
    }
}

/// Authoritative session event payload.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EventPayload {
    RoomCreated,
    MemberJoined {
        principal: PrincipalId,
    },
    MemberDisconnected {
        principal: PrincipalId,
    },
    MemberReconnected {
        principal: PrincipalId,
    },
    MemberLeft {
        principal: PrincipalId,
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
        countdown_token: CountdownToken,
    },
    CountdownAborted {
        countdown_token: CountdownToken,
    },
    PhaseChanged {
        phase: RoomPhase,
    },
    GameTransitioned {
        game_state_hash: SemanticHash,
    },
    RoundScored {
        scores: Vec<i32>,
    },
    ChatPosted {
        principal: PrincipalId,
        text: String,
    },
    HandRequested {
        request_id: CommandId,
        player: PrincipalId,
        recipient: PrincipalId,
    },
    HandGranted {
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    HandRevoked {
        player: PrincipalId,
        recipient: PrincipalId,
        grant_epoch: u64,
    },
    RoomClosed,
}

/// Event fields that exist before the host signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedEventEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub event_id: EventId,
    pub principal_id: PrincipalId,
    pub current_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: CommandId,
    pub payload: EventPayload,
    pub signature_intent: SignatureIntent,
}

/// Host-signed authoritative event.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub event_id: EventId,
    pub principal_id: PrincipalId,
    pub current_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: CommandId,
    pub payload: EventPayload,
    pub signature: SignatureMetadata,
}

impl UnsignedEventEnvelope {
    /// Attach a host signature without changing signed fields.
    #[must_use]
    pub fn attach_signature(self, signature: SignatureBytes) -> EventEnvelope {
        EventEnvelope {
            protocol_version: self.protocol_version,
            room_id: self.room_id,
            session_epoch: self.session_epoch,
            event_id: self.event_id,
            principal_id: self.principal_id,
            current_revision: self.current_revision,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            payload: self.payload,
            signature: SignatureMetadata {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        }
    }

    pub(crate) fn validate(&self) -> Result<(), EnvelopeValidationError> {
        validate_unsigned_common(
            self.protocol_version,
            &self.room_id,
            &self.principal_id,
            &self.correlation_id,
            &self.signature_intent,
        )?;
        require_ids(self.event_id.validate() && self.causation_id.validate())?;
        validate_event_payload(&self.payload)
    }
}

/// Public member projection without transport or secret material.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberProjection {
    pub principal_id: PrincipalId,
    pub connected: bool,
    pub seat: Option<u8>,
    pub ready: bool,
    pub host: bool,
}

/// Exact viewer-scoped private hand projection.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandProjection {
    pub player: PrincipalId,
    pub grant_epoch: u64,
    pub cards: Vec<u8>,
}

/// Viewer projection payload. Unauthorized hands are absent from the object graph.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionPayload {
    pub phase: RoomPhase,
    pub members: Vec<MemberProjection>,
    pub public_game_state: Vec<u8>,
    pub own_hand: Option<HandProjection>,
    pub granted_hands: Vec<HandProjection>,
    pub public_history: Vec<EventId>,
}

/// One exact viewer projection.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub projection_id: ProjectionId,
    pub principal_id: PrincipalId,
    pub current_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: EventId,
    pub payload: ProjectionPayload,
    pub signature: SignatureMetadata,
}

/// Snapshot payload committed to schema and semantic state hashes.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPayload {
    pub schema_hash: SemanticHash,
    pub state_hash: SemanticHash,
    pub event_tail_revision: u64,
    pub phase: RoomPhase,
    pub members: Vec<MemberProjection>,
    pub state: Vec<u8>,
}

/// Host-signed recovery snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub snapshot_id: SnapshotId,
    pub principal_id: PrincipalId,
    pub current_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: EventId,
    pub payload: SnapshotPayload,
    pub signature: SignatureMetadata,
}

/// Stable public denial/error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum DenyReason {
    #[serde(rename = "D-MALFORMED")]
    Malformed,
    #[serde(rename = "D-OVERSIZE")]
    Oversize,
    #[serde(rename = "D-UNKNOWN-VERSION")]
    UnknownVersion,
    #[serde(rename = "D-UNKNOWN-COMMAND")]
    UnknownCommand,
    #[serde(rename = "D-UNKNOWN-ROLE")]
    UnknownRole,
    #[serde(rename = "D-UNKNOWN-PRINCIPAL")]
    UnknownPrincipal,
    #[serde(rename = "D-BAD-SIGNATURE")]
    BadSignature,
    #[serde(rename = "D-WRONG-ROOM")]
    WrongRoom,
    #[serde(rename = "D-STALE-EPOCH")]
    StaleEpoch,
    #[serde(rename = "D-STALE-REVISION")]
    StaleRevision,
    #[serde(rename = "D-REVOKED")]
    Revoked,
    #[serde(rename = "D-MISSING-CAPABILITY")]
    MissingCapability,
    #[serde(rename = "D-DENY-POLICY")]
    DenyPolicy,
    #[serde(rename = "D-WRONG-PHASE")]
    WrongPhase,
    #[serde(rename = "D-CLOSED")]
    Closed,
    #[serde(rename = "D-NOT-SEATED")]
    NotSeated,
    #[serde(rename = "D-SEAT-OCCUPIED")]
    SeatOccupied,
    #[serde(rename = "D-ALREADY-SEATED")]
    AlreadySeated,
    #[serde(rename = "D-NOT-CONNECTED")]
    NotConnected,
    #[serde(rename = "D-NOT-READY")]
    NotReady,
    #[serde(rename = "D-COUNTDOWN-INACTIVE")]
    CountdownInactive,
    #[serde(rename = "D-NOT-ACTOR")]
    NotActor,
    #[serde(rename = "D-PAUSED")]
    Paused,
    #[serde(rename = "D-NOT-PAUSED")]
    NotPaused,
    #[serde(rename = "D-ALREADY-PAUSED")]
    AlreadyPaused,
    #[serde(rename = "D-INVITE-INVALID")]
    InviteInvalid,
    #[serde(rename = "D-INVITE-EXPIRED")]
    InviteExpired,
    #[serde(rename = "D-GRANT-SCOPE")]
    GrantScope,
    #[serde(rename = "D-CHAT-SIZE")]
    ChatSize,
    #[serde(rename = "D-CHAT-RATE")]
    ChatRate,
    #[serde(rename = "D-ENVIRONMENT-ONLY")]
    EnvironmentOnly,
}

impl DenyReason {
    /// Every registered stable denial code in protocol-v1 order.
    pub const ALL: [Self; 31] = [
        Self::Malformed,
        Self::Oversize,
        Self::UnknownVersion,
        Self::UnknownCommand,
        Self::UnknownRole,
        Self::UnknownPrincipal,
        Self::BadSignature,
        Self::WrongRoom,
        Self::StaleEpoch,
        Self::StaleRevision,
        Self::Revoked,
        Self::MissingCapability,
        Self::DenyPolicy,
        Self::WrongPhase,
        Self::Closed,
        Self::NotSeated,
        Self::SeatOccupied,
        Self::AlreadySeated,
        Self::NotConnected,
        Self::NotReady,
        Self::CountdownInactive,
        Self::NotActor,
        Self::Paused,
        Self::NotPaused,
        Self::AlreadyPaused,
        Self::InviteInvalid,
        Self::InviteExpired,
        Self::GrantScope,
        Self::ChatSize,
        Self::ChatRate,
        Self::EnvironmentOnly,
    ];
}

/// Structured redacted error payload.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorPayload {
    pub reason: DenyReason,
    pub policy_id: Option<PolicyId>,
    pub public_detail: Option<String>,
}

/// Redacted protocol error response.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorEnvelope {
    pub protocol_version: u16,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub event_id: EventId,
    pub principal_id: PrincipalId,
    pub current_revision: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: CommandId,
    pub payload: ErrorPayload,
    pub signature: SignatureMetadata,
}

/// One complete NDJSON frame.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "frame",
    content = "envelope",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ProtocolFrame {
    Command(CommandEnvelope),
    Event(EventEnvelope),
    Snapshot(SnapshotEnvelope),
    Projection(ProjectionEnvelope),
    Error(ErrorEnvelope),
}

/// Semantic validation failure after structural JSON decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeValidationError {
    UnknownVersion,
    InvalidIdentifier,
    PrincipalKeyMismatch,
    InvalidSignature,
    InvalidPayload,
    PrivateProjectionOverlap,
}

impl CommandEnvelope {
    pub(crate) fn validate(&self) -> Result<(), EnvelopeValidationError> {
        validate_common(
            self.protocol_version,
            &self.room_id,
            &self.principal_id,
            &self.correlation_id,
            &self.signature,
        )?;
        if !self.command_id.validate()
            || self.causation_id.as_ref().is_some_and(|id| !id.validate())
        {
            return Err(EnvelopeValidationError::InvalidIdentifier);
        }
        validate_command_payload(&self.payload)
    }
}

impl ProtocolFrame {
    pub(crate) fn validate(&self) -> Result<(), EnvelopeValidationError> {
        match self {
            Self::Command(envelope) => envelope.validate(),
            Self::Event(envelope) => {
                validate_common(
                    envelope.protocol_version,
                    &envelope.room_id,
                    &envelope.principal_id,
                    &envelope.correlation_id,
                    &envelope.signature,
                )?;
                require_ids(envelope.event_id.validate() && envelope.causation_id.validate())?;
                validate_event_payload(&envelope.payload)
            }
            Self::Snapshot(envelope) => {
                validate_common(
                    envelope.protocol_version,
                    &envelope.room_id,
                    &envelope.principal_id,
                    &envelope.correlation_id,
                    &envelope.signature,
                )?;
                require_ids(envelope.snapshot_id.validate() && envelope.causation_id.validate())?;
                validate_members(&envelope.payload.members)
            }
            Self::Projection(envelope) => {
                validate_common(
                    envelope.protocol_version,
                    &envelope.room_id,
                    &envelope.principal_id,
                    &envelope.correlation_id,
                    &envelope.signature,
                )?;
                require_ids(envelope.projection_id.validate() && envelope.causation_id.validate())?;
                validate_projection(&envelope.payload)
            }
            Self::Error(envelope) => {
                validate_common(
                    envelope.protocol_version,
                    &envelope.room_id,
                    &envelope.principal_id,
                    &envelope.correlation_id,
                    &envelope.signature,
                )?;
                require_ids(envelope.event_id.validate() && envelope.causation_id.validate())?;
                if envelope
                    .payload
                    .policy_id
                    .as_ref()
                    .is_some_and(|policy_id| !policy_id.validate())
                {
                    return Err(EnvelopeValidationError::InvalidIdentifier);
                }
                if envelope
                    .payload
                    .public_detail
                    .as_ref()
                    .is_some_and(|detail| detail.len() > 1_024)
                {
                    return Err(EnvelopeValidationError::InvalidPayload);
                }
                Ok(())
            }
        }
    }
}

fn validate_common(
    protocol_version: u16,
    room_id: &RoomId,
    principal_id: &PrincipalId,
    correlation_id: &CorrelationId,
    signature: &SignatureMetadata,
) -> Result<(), EnvelopeValidationError> {
    if protocol_version != PROTOCOL_VERSION_V1 || signature.domain_version != SIGNATURE_DOMAIN_V1 {
        return Err(EnvelopeValidationError::UnknownVersion);
    }
    require_ids(room_id.validate() && principal_id.validate() && correlation_id.validate())?;
    if !signature.key_id.validate() {
        return Err(EnvelopeValidationError::InvalidIdentifier);
    }
    if signature.key_id != *principal_id {
        return Err(EnvelopeValidationError::PrincipalKeyMismatch);
    }
    signature.signature.validate()
}

fn validate_unsigned_common(
    protocol_version: u16,
    room_id: &RoomId,
    principal_id: &PrincipalId,
    correlation_id: &CorrelationId,
    signature: &SignatureIntent,
) -> Result<(), EnvelopeValidationError> {
    if protocol_version != PROTOCOL_VERSION_V1 || signature.domain_version != SIGNATURE_DOMAIN_V1 {
        return Err(EnvelopeValidationError::UnknownVersion);
    }
    require_ids(
        room_id.validate()
            && principal_id.validate()
            && correlation_id.validate()
            && signature.key_id.validate(),
    )?;
    if signature.key_id != *principal_id {
        return Err(EnvelopeValidationError::PrincipalKeyMismatch);
    }
    Ok(())
}

fn require_ids(valid: bool) -> Result<(), EnvelopeValidationError> {
    if valid {
        Ok(())
    } else {
        Err(EnvelopeValidationError::InvalidIdentifier)
    }
}

fn validate_command_payload(payload: &CommandPayload) -> Result<(), EnvelopeValidationError> {
    match payload {
        CommandPayload::Chat { text } if text.is_empty() || text.len() > MAX_CHAT_BYTES => {
            Err(EnvelopeValidationError::InvalidPayload)
        }
        CommandPayload::RedeemInvite { invite }
            if invite.expose().is_empty() || invite.expose().len() > 256 =>
        {
            Err(EnvelopeValidationError::InvalidPayload)
        }
        CommandPayload::ApplyChance { chance }
            if chance.cards.len() != MAX_DECK_CARDS
                || chance
                    .cards
                    .iter()
                    .any(|card| *card >= MAX_CARD_CODE_EXCLUSIVE)
                || chance.seed.is_some() != chance.deal_ordinal.is_some() =>
        {
            Err(EnvelopeValidationError::InvalidPayload)
        }
        CommandPayload::GameAction {
            action: GameActionWire::Play { card },
        } if *card >= MAX_CARD_CODE_EXCLUSIVE => Err(EnvelopeValidationError::InvalidPayload),
        CommandPayload::RequestHand { player } if !player.validate() => {
            Err(EnvelopeValidationError::InvalidIdentifier)
        }
        CommandPayload::GrantHand {
            request_id,
            player,
            recipient,
            ..
        } if !request_id.validate() || !player.validate() || !recipient.validate() => {
            Err(EnvelopeValidationError::InvalidIdentifier)
        }
        CommandPayload::RevokeHand {
            player, recipient, ..
        } if !player.validate() || !recipient.validate() => {
            Err(EnvelopeValidationError::InvalidIdentifier)
        }
        CommandPayload::RemoveMember { target } if !target.validate() => {
            Err(EnvelopeValidationError::InvalidIdentifier)
        }
        CommandPayload::ArmCountdown {
            countdown_token, ..
        }
        | CommandPayload::CountdownExpired { countdown_token }
            if !countdown_token.validate() =>
        {
            Err(EnvelopeValidationError::InvalidIdentifier)
        }
        _ => Ok(()),
    }
}

fn validate_event_payload(payload: &EventPayload) -> Result<(), EnvelopeValidationError> {
    let identifiers_valid = match payload {
        EventPayload::MemberJoined { principal }
        | EventPayload::MemberDisconnected { principal }
        | EventPayload::MemberReconnected { principal }
        | EventPayload::MemberLeft { principal }
        | EventPayload::SeatTaken { principal, .. }
        | EventPayload::SeatReleased { principal, .. }
        | EventPayload::ReadyChanged { principal, .. }
        | EventPayload::ChatPosted { principal, .. } => principal.validate(),
        EventPayload::HandRequested {
            request_id,
            player,
            recipient,
        } => request_id.validate() && player.validate() && recipient.validate(),
        EventPayload::HandGranted {
            player, recipient, ..
        }
        | EventPayload::HandRevoked {
            player, recipient, ..
        } => player.validate() && recipient.validate(),
        EventPayload::CountdownArmed {
            countdown_token, ..
        }
        | EventPayload::CountdownAborted { countdown_token } => countdown_token.validate(),
        _ => true,
    };
    if !identifiers_valid {
        return Err(EnvelopeValidationError::InvalidIdentifier);
    }
    match payload {
        EventPayload::ChatPosted { text, .. } if text.is_empty() || text.len() > MAX_CHAT_BYTES => {
            Err(EnvelopeValidationError::InvalidPayload)
        }
        EventPayload::RoundScored { scores } if scores.is_empty() || scores.len() > 8 => {
            Err(EnvelopeValidationError::InvalidPayload)
        }
        _ => Ok(()),
    }
}

fn validate_members(members: &[MemberProjection]) -> Result<(), EnvelopeValidationError> {
    if members.iter().any(|member| !member.principal_id.validate()) {
        Err(EnvelopeValidationError::InvalidIdentifier)
    } else {
        Ok(())
    }
}

fn validate_projection(payload: &ProjectionPayload) -> Result<(), EnvelopeValidationError> {
    validate_members(&payload.members)?;
    if payload
        .public_history
        .iter()
        .any(|event_id| !event_id.validate())
    {
        return Err(EnvelopeValidationError::InvalidIdentifier);
    }
    if payload
        .own_hand
        .iter()
        .chain(&payload.granted_hands)
        .any(|hand| {
            !hand.player.validate()
                || hand.cards.len() > MAX_DECK_CARDS
                || hand
                    .cards
                    .iter()
                    .any(|card| *card >= MAX_CARD_CODE_EXCLUSIVE)
        })
    {
        return Err(EnvelopeValidationError::InvalidPayload);
    }
    if let Some(own) = &payload.own_hand
        && payload
            .granted_hands
            .iter()
            .any(|granted| granted.player == own.player)
    {
        return Err(EnvelopeValidationError::PrivateProjectionOverlap);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_reason_registry_uses_exact_stable_codes() {
        let expected = [
            "D-MALFORMED",
            "D-OVERSIZE",
            "D-UNKNOWN-VERSION",
            "D-UNKNOWN-COMMAND",
            "D-UNKNOWN-ROLE",
            "D-UNKNOWN-PRINCIPAL",
            "D-BAD-SIGNATURE",
            "D-WRONG-ROOM",
            "D-STALE-EPOCH",
            "D-STALE-REVISION",
            "D-REVOKED",
            "D-MISSING-CAPABILITY",
            "D-DENY-POLICY",
            "D-WRONG-PHASE",
            "D-CLOSED",
            "D-NOT-SEATED",
            "D-SEAT-OCCUPIED",
            "D-ALREADY-SEATED",
            "D-NOT-CONNECTED",
            "D-NOT-READY",
            "D-COUNTDOWN-INACTIVE",
            "D-NOT-ACTOR",
            "D-PAUSED",
            "D-NOT-PAUSED",
            "D-ALREADY-PAUSED",
            "D-INVITE-INVALID",
            "D-INVITE-EXPIRED",
            "D-GRANT-SCOPE",
            "D-CHAT-SIZE",
            "D-CHAT-RATE",
            "D-ENVIRONMENT-ONLY",
        ];
        let actual: Vec<_> = DenyReason::ALL
            .iter()
            .map(|reason| serde_json::to_string(reason).unwrap())
            .collect();
        assert_eq!(
            actual,
            expected.map(|reason| format!("\"{reason}\"")).to_vec()
        );
    }

    #[test]
    fn invite_debug_output_is_redacted() {
        let invite = InviteProof::new("secret-room-verifier").unwrap();
        let diagnostic = format!("{invite:?}");
        assert!(diagnostic.contains("<redacted>"));
        assert!(!diagnostic.contains("secret-room-verifier"));
    }
}
