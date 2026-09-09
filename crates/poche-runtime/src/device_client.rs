// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Real reducer-backed loopback adapter for the shared player-device client.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use poche_player_client::{
    AdvertisedAction, AdvertisedActionParameter, AdvertisedActionTemplate, DeviceActionRequest,
    DeviceActionResult, DeviceChatEntry, DeviceClientError, DeviceCooperationRequest,
    DeviceCooperationResult, DeviceObservation, DeviceProfile, LoopbackDeviceAuthority,
};
use poche_protocol::{
    CaptureProviderAdvertisementWire, CommandPayload, CorrelationId, CountdownToken,
    DeviceActionWire, DeviceCertificateWire, DeviceId, DeviceObservationRequestWire,
    DeviceRouteOperationWire, DeviceRouteRequestWire, DeviceRouteResultWire, EventId,
    GameActionWire, InviteProof, MemberProjection, PROTOCOL_VERSION_V1, PrincipalId,
    ProjectionEnvelope, ProjectionId, ProjectionPayload, RoomId, RoomPhase, SIGNATURE_DOMAIN_V1,
    SemanticHash, SignatureAlgorithm, SignatureBytes, SignatureMetadata,
    canonical_capture_provider_advertisement_bytes, canonical_capture_request_bytes,
    canonical_capture_response_bytes, canonical_device_action_bytes,
    canonical_device_certificate_bytes, canonical_device_observation_request_bytes,
    canonical_device_route_request_bytes,
};
use poche_session::{
    CaptureAuthorizationContext, CaptureReplayWindow, ConnectionState, GameTurn, SessionGame,
    SessionPhase, SessionState, authorize_capture_request_with_provider,
    authorize_capture_response, project_viewer,
};

use crate::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    ScriptedClient, signature::verify_ed25519_hex,
};

/// Derive the opaque action set for one exact projection. Production adapters
/// use the same policy/UI action derivation as their renderer; test sources may
/// expose a deliberately smaller subset but cannot bypass the reducer.
pub trait AdvertisedActionSource<G: SessionGame>: Send + Sync + 'static {
    /// Return the complete action set offered for this exact viewer projection.
    ///
    /// # Errors
    ///
    /// Returns a stable client error when policy cannot derive a trustworthy
    /// action set from the supplied state and projection.
    fn actions(
        &self,
        state: &SessionState<G>,
        projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedAction>, DeviceClientError>;

    /// Return the bounded value-bearing action families offered for this exact
    /// viewer projection. Concrete values are validated against the same
    /// current template again during invocation.
    ///
    /// # Errors
    ///
    /// Returns a stable client error when the source cannot derive a complete
    /// template set for the supplied state and exact projection.
    fn templates(
        &self,
        _state: &SessionState<G>,
        _projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedActionTemplate>, DeviceClientError> {
        Ok(Vec::new())
    }
}

/// Complete player/environment game-action derivation for the Rust Poche
/// oracle. Room, chat, and governance controls remain separate action-source
/// layers. Chance uses an explicit deterministic seed retained on the source.
#[derive(Clone, Copy, Debug)]
pub struct OracleGameActionSource {
    seed: u64,
}

impl OracleGameActionSource {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }
}

impl Default for OracleGameActionSource {
    fn default() -> Self {
        Self::new(0x5eed)
    }
}

impl<const PLAYERS: usize> AdvertisedActionSource<crate::OracleSessionGame<PLAYERS>>
    for OracleGameActionSource
{
    fn actions(
        &self,
        state: &SessionState<crate::OracleSessionGame<PLAYERS>>,
        projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedAction>, DeviceClientError> {
        let SessionPhase::Running { game } = &state.phase else {
            return Ok(Vec::new());
        };
        if projection.principal_id == state.game_environment {
            return match game.turn() {
                GameTurn::Chance => {
                    let round = game
                        .public_projection()
                        .map_err(|_| DeviceClientError::ProtocolViolation)?
                        .round_index;
                    Ok(vec![AdvertisedAction {
                        id: format!("game-deal-{round}"),
                        label: format!("Deal round {round}"),
                        payload: CommandPayload::ApplyChance {
                            chance: crate::OracleSessionGame::<PLAYERS>::seeded_chance(
                                self.seed,
                                u32::from(round),
                            )
                            .map_err(|_| DeviceClientError::ProtocolViolation)?,
                        },
                    }])
                }
                GameTurn::Environment => Ok(vec![AdvertisedAction {
                    id: "game-settle".to_owned(),
                    label: "Settle round".to_owned(),
                    payload: CommandPayload::Settle,
                }]),
                GameTurn::Player(_) | GameTurn::Finished => Ok(Vec::new()),
            };
        }
        let Some(member) = state.member(&projection.principal_id) else {
            return Ok(Vec::new());
        };
        let (Some(viewer_seat), GameTurn::Player(actor)) = (member.seat, game.turn()) else {
            return Ok(Vec::new());
        };
        if viewer_seat != actor {
            return Ok(Vec::new());
        }
        Ok(game
            .legal_player_actions()
            .into_iter()
            .map(|action| {
                let (id, label) = game_action_identity(&action);
                AdvertisedAction {
                    id,
                    label,
                    payload: CommandPayload::GameAction { action },
                }
            })
            .collect())
    }
}

/// Concrete lifecycle controls around the Oracle game actions. Client-local
/// parameterized actions such as an arbitrary chat draft remain supplements;
/// every action returned here is complete and ready for exact invocation.
#[derive(Clone)]
pub struct OracleRoomActionSource {
    game: OracleGameActionSource,
    seat_count: u8,
    invite: InviteProof,
    countdown_deadline_tick: u64,
    countdown_token: CountdownToken,
    allow_hand_sharing: bool,
}

impl OracleRoomActionSource {
    /// Create a bounded room action source.
    ///
    /// # Errors
    ///
    /// Rejects zero/more-than-eight seats, a zero deadline, or malformed
    /// invite/countdown text.
    pub fn new(
        seed: u64,
        seat_count: u8,
        invite: impl Into<String>,
        countdown_deadline_tick: u64,
        countdown_token: impl Into<String>,
    ) -> Result<Self, DeviceClientError> {
        if seat_count == 0 || seat_count > 8 || countdown_deadline_tick == 0 {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(Self {
            game: OracleGameActionSource::new(seed),
            seat_count,
            invite: InviteProof::new(invite.into())
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
            countdown_deadline_tick,
            countdown_token: CountdownToken::new(countdown_token.into())
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
            allow_hand_sharing: true,
        })
    }

    /// Strict Poche desktop policy: unplayed hands cannot be requested/granted.
    /// Existing experimental callers retain their explicit sharing surface.
    /// Configure this before creating the room, not as a way to revoke existing grants.
    #[must_use]
    pub const fn without_hand_sharing(mut self) -> Self {
        self.allow_hand_sharing = false;
        self
    }

    fn advertised(
        id: impl Into<String>,
        label: impl Into<String>,
        payload: CommandPayload,
    ) -> AdvertisedAction {
        AdvertisedAction {
            id: id.into(),
            label: label.into(),
            payload,
        }
    }
}

impl<const PLAYERS: usize> AdvertisedActionSource<crate::OracleSessionGame<PLAYERS>>
    for OracleRoomActionSource
{
    #[allow(
        clippy::too_many_lines,
        reason = "the exhaustive lifecycle mapping keeps every concrete room action visible in one source"
    )]
    fn actions(
        &self,
        state: &SessionState<crate::OracleSessionGame<PLAYERS>>,
        projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedAction>, DeviceClientError> {
        let principal = &projection.principal_id;
        if principal == &state.authority_clock {
            return Ok(match &state.phase {
                SessionPhase::Countdown { token, .. } => vec![Self::advertised(
                    "countdown-expire",
                    "Expire countdown",
                    CommandPayload::CountdownExpired {
                        countdown_token: token.clone(),
                    },
                )],
                _ => Vec::new(),
            });
        }
        if principal == &state.game_environment {
            return self.game.actions(state, projection);
        }
        if matches!(&state.phase, SessionPhase::Uninitialized) {
            return Ok(vec![Self::advertised(
                "room-create",
                "Create room",
                CommandPayload::CreateRoom,
            )]);
        }
        let Some(member) = state.member(principal) else {
            return Ok(vec![Self::advertised(
                "room-join",
                "Join room",
                CommandPayload::RedeemInvite {
                    invite: self.invite.clone(),
                },
            )]);
        };
        if member.connection == ConnectionState::Disconnected {
            return Ok(if matches!(state.phase, SessionPhase::Closed) {
                Vec::new()
            } else {
                vec![Self::advertised(
                    "room-reconnect",
                    "Reconnect",
                    CommandPayload::Reconnect,
                )]
            });
        }
        let mut actions = Vec::new();
        match &state.phase {
            SessionPhase::Lobby => {
                if member.seat.is_some() {
                    actions.push(Self::advertised(
                        "room-release-seat",
                        "Become spectator",
                        CommandPayload::ReleaseSeat,
                    ));
                    actions.push(Self::advertised(
                        if member.ready {
                            "room-unready"
                        } else {
                            "room-ready"
                        },
                        if member.ready { "Not ready" } else { "Ready" },
                        if member.ready {
                            CommandPayload::Unready
                        } else {
                            CommandPayload::Ready
                        },
                    ));
                } else {
                    for seat in 0..self.seat_count {
                        if state.seat_owner(seat).is_none() {
                            actions.push(Self::advertised(
                                format!("room-take-seat-{seat}"),
                                format!("Take seat {seat}"),
                                CommandPayload::TakeSeat { seat },
                            ));
                        }
                    }
                }
                let seated = state
                    .members
                    .iter()
                    .filter(|candidate| candidate.seat.is_some())
                    .collect::<Vec<_>>();
                if member.host
                    && seated.len() == usize::from(self.seat_count)
                    && seated.iter().all(|candidate| candidate.ready)
                {
                    actions.push(Self::advertised(
                        "countdown-arm",
                        "Start countdown",
                        CommandPayload::ArmCountdown {
                            deadline_tick: self.countdown_deadline_tick,
                            countdown_token: self.countdown_token.clone(),
                        },
                    ));
                }
            }
            SessionPhase::Countdown { .. } if member.seat.is_some() => {
                actions.push(Self::advertised(
                    "countdown-abort",
                    "Abort countdown",
                    CommandPayload::AbortCountdown,
                ));
            }
            SessionPhase::Running { .. } if member.seat.is_some() => {
                actions.push(Self::advertised(
                    "room-pause",
                    "Pause game",
                    CommandPayload::Pause,
                ));
                actions.extend(self.game.actions(state, projection)?);
            }
            SessionPhase::Paused { .. } if member.seat.is_some() => actions.push(Self::advertised(
                "room-unpause",
                "Resume game",
                CommandPayload::Unpause,
            )),
            SessionPhase::Running { .. } | SessionPhase::Paused { .. } => {
                let already_scoped = state
                    .hand_requests
                    .iter()
                    .any(|request| request.recipient == *principal)
                    || state
                        .hand_grants
                        .iter()
                        .any(|grant| grant.recipient == *principal);
                if !already_scoped {
                    for (index, player) in state
                        .members
                        .iter()
                        .filter(|candidate| candidate.seat.is_some())
                        .enumerate()
                    {
                        actions.push(Self::advertised(
                            format!("hand-request-{index}"),
                            "Request hand view",
                            CommandPayload::RequestHand {
                                player: player.principal_id.clone(),
                            },
                        ));
                    }
                }
            }
            SessionPhase::PostGame { .. } if member.host => actions.push(Self::advertised(
                "room-reset-lobby",
                "Return to lobby",
                CommandPayload::ResetLobby,
            )),
            SessionPhase::Uninitialized
            | SessionPhase::Countdown { .. }
            | SessionPhase::PostGame { .. }
            | SessionPhase::Closed => {}
        }
        for (index, request) in state
            .hand_requests
            .iter()
            .filter(|request| request.player == *principal)
            .enumerate()
        {
            actions.push(Self::advertised(
                format!("hand-grant-{index}"),
                "Grant hand view",
                CommandPayload::GrantHand {
                    request_id: request.request_id.clone(),
                    player: request.player.clone(),
                    recipient: request.recipient.clone(),
                    grant_epoch: state.projection_epoch.saturating_add(1),
                },
            ));
            actions.push(Self::advertised(
                format!("hand-deny-{index}"),
                "Deny hand view",
                CommandPayload::DenyHand {
                    request_id: request.request_id.clone(),
                    player: request.player.clone(),
                    recipient: request.recipient.clone(),
                },
            ));
        }
        for (index, grant) in state
            .hand_grants
            .iter()
            .filter(|grant| grant.player == *principal)
            .enumerate()
        {
            actions.push(Self::advertised(
                format!("hand-revoke-{index}"),
                "Revoke hand view",
                CommandPayload::RevokeHand {
                    player: grant.player.clone(),
                    recipient: grant.recipient.clone(),
                    grant_epoch: grant.grant_epoch,
                },
            ));
        }
        if member.host && !matches!(&state.phase, SessionPhase::Closed) {
            actions.push(Self::advertised(
                "room-close",
                "Close room",
                CommandPayload::CloseRoom,
            ));
        } else if !member.host
            && (member.seat.is_none()
                || matches!(
                    &state.phase,
                    SessionPhase::Lobby | SessionPhase::Countdown { .. }
                ))
        {
            actions.push(Self::advertised(
                "room-leave",
                "Leave room",
                CommandPayload::Leave,
            ));
        }
        if !self.allow_hand_sharing {
            actions.retain(|action| {
                !matches!(
                    action.payload,
                    CommandPayload::RequestHand { .. } | CommandPayload::GrantHand { .. }
                )
            });
        }
        Ok(actions)
    }

    fn templates(
        &self,
        state: &SessionState<crate::OracleSessionGame<PLAYERS>>,
        projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedActionTemplate>, DeviceClientError> {
        let can_chat = state
            .member(&projection.principal_id)
            .is_some_and(|member| {
                member.connection == poche_session::ConnectionState::Connected
                    && !matches!(state.phase, SessionPhase::Closed)
            });
        Ok(if can_chat {
            vec![AdvertisedActionTemplate {
                id: "chat-send".to_owned(),
                label: "Send chat".to_owned(),
                parameter: AdvertisedActionParameter::ChatText,
            }]
        } else {
            Vec::new()
        })
    }
}

fn game_action_identity(action: &GameActionWire) -> (String, String) {
    match action {
        GameActionWire::Bid { tricks } => {
            (format!("game-bid-{tricks}"), format!("Bid {tricks} tricks"))
        }
        GameActionWire::Play { card } => (format!("game-play-{card}"), format!("Play card {card}")),
    }
}

struct SharedLoopbackState<G: SessionGame, A> {
    authority: InProcessAuthority<G>,
    clients: BTreeMap<DeviceId, ScriptedClient>,
    profiles: BTreeMap<DeviceId, DeviceProfile>,
    capture_providers: BTreeMap<DeviceId, RegisteredCaptureProvider>,
    capture_replays: CaptureReplayWindow,
    cooperation_now_unix_ms: u64,
    action_source: A,
    next_projection: u64,
    physical_secret: Option<[u8; 32]>,
    physical_poses: BTreeMap<String, poche_player_client::PhysicalPoseState>,
    physical_pose_epoch: Option<u64>,
}

struct RegisteredCaptureProvider {
    advertisement: CaptureProviderAdvertisementWire,
    handler: Arc<Mutex<Box<dyn RuntimeDeviceCooperationHandler>>>,
}

impl Clone for RegisteredCaptureProvider {
    fn clone(&self) -> Self {
        Self {
            advertisement: self.advertisement.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

/// Exact-target non-authoritative device handler used by the deterministic
/// loopback adapter. The transport authorizes signed capture messages before
/// invoking this port; handlers never receive the room reducer.
pub trait RuntimeDeviceCooperationHandler: Send + 'static {
    /// Answer one already-authorized exact-target request without access to
    /// the authoritative room reducer.
    ///
    /// # Errors
    ///
    /// Returns a stable provider/transport failure; the adapter validates the
    /// signed response again before delivering it to the requester.
    fn cooperate(
        &mut self,
        context: &RuntimeDeviceCooperationContext,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError>;
}

/// Exact public certificate context supplied only after the adapter has
/// authenticated both sides of a cooperation exchange. Providers need the
/// requester encryption key to seal artifact keys, but receive no reducer or
/// private projection through this context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDeviceCooperationContext {
    pub requester_certificate: DeviceCertificateWire,
    pub provider_certificate: DeviceCertificateWire,
}

/// Cloneable device adapter whose clones share one actual in-process authority.
/// Every invoke enters `InProcessAuthority::drive_all`; no test-only state
/// mutation path exists.
pub struct RuntimeLoopbackDeviceAdapter<G: SessionGame, A> {
    shared: Arc<Mutex<SharedLoopbackState<G, A>>>,
}

/// Authority-only in-memory checkpoint; contains private game/identity data.
/// No Debug or serialization: protected durable encoding remains separate.
pub struct RuntimeDeviceCheckpoint<G: SessionGame, A> {
    authority: crate::AuthorityCheckpoint<G>,
    profiles: Vec<(DeviceProfile, bool)>,
    action_source: A,
    next_projection: u64,
    physical_secret: Option<[u8; 32]>,
    physical_poses: BTreeMap<String, poche_player_client::PhysicalPoseState>,
    physical_pose_epoch: Option<u64>,
    cooperation_now_unix_ms: u64,
}

impl<G: SessionGame, A> Clone for RuntimeLoopbackDeviceAdapter<G, A> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<G: SessionGame, A: AdvertisedActionSource<G>> RuntimeLoopbackDeviceAdapter<G, A> {
    /// Capture the adapter without retaining transport routes or capture handlers.
    pub fn checkpoint(&self) -> Result<RuntimeDeviceCheckpoint<G, A>, DeviceClientError>
    where A: Clone {
        let shared = self.shared.lock().map_err(|_| DeviceClientError::TransportUnavailable)?;
        Ok(RuntimeDeviceCheckpoint {
            authority: shared.authority.checkpoint(), profiles: shared.profiles.values().map(|profile| (profile.clone(), shared.clients.get(&profile.device_id).is_some_and(|client| client.route_connected(&shared.authority.transport)))).collect(),
            action_source: shared.action_source.clone(), next_projection: shared.next_projection,
            physical_secret: shared.physical_secret, physical_poses: shared.physical_poses.clone(),
            physical_pose_epoch: shared.physical_pose_epoch, cooperation_now_unix_ms: shared.cooperation_now_unix_ms,
        })
    }

    /// Restore internal state with freshly enrolled routes. Capture providers
    /// must register again; this does not restore a certified service's caches.
    pub fn from_checkpoint(checkpoint: RuntimeDeviceCheckpoint<G, A>, codec: LoopbackCodec) -> Result<Self, DeviceClientError> {
        let adapter = Self { shared: Arc::new(Mutex::new(SharedLoopbackState {
            authority: InProcessAuthority::from_checkpoint(checkpoint.authority, InProcessTransport::new(codec)),
            clients: BTreeMap::new(), profiles: BTreeMap::new(), capture_providers: BTreeMap::new(),
            capture_replays: CaptureReplayWindow::new(256), cooperation_now_unix_ms: checkpoint.cooperation_now_unix_ms,
            action_source: checkpoint.action_source, next_projection: checkpoint.next_projection,
            physical_secret: checkpoint.physical_secret, physical_poses: checkpoint.physical_poses,
            physical_pose_epoch: checkpoint.physical_pose_epoch,
        })) };
        {
            let mut shared = adapter.shared.lock().map_err(|_| DeviceClientError::TransportUnavailable)?;
            for (profile, connected) in checkpoint.profiles {
                profile.validate()?;
                let client = shared.authority.transport.restore_route(profile.player_id.clone(), connected).map_err(|_| DeviceClientError::TransportUnavailable)?;
                shared.clients.insert(profile.device_id.clone(), client);
                shared.profiles.insert(profile.device_id.clone(), profile);
            }
        }
        Ok(adapter)
    }

    #[must_use]
    pub fn new(state: SessionState<G>, action_source: A, codec: LoopbackCodec) -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedLoopbackState {
                authority: InProcessAuthority::new(state, InProcessTransport::new(codec)),
                clients: BTreeMap::new(),
                profiles: BTreeMap::new(),
                capture_providers: BTreeMap::new(),
                capture_replays: CaptureReplayWindow::new(256),
                cooperation_now_unix_ms: 1,
                action_source,
                next_projection: 0,
                physical_secret: None,
                physical_poses: BTreeMap::new(),
                physical_pose_epoch: None,
            })),
        }
    }

    /// Enable round-stable physical hand identities before any client enrolls.
    /// Supply independent cryptographic entropy, never the rules/deal seed.
    ///
    /// # Errors
    /// Rejects mutation after enrollment or a poisoned room lock.
    pub fn enable_physical_identities(&self, secret: [u8; 32]) -> Result<(), DeviceClientError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if !shared.clients.is_empty() || shared.physical_secret.is_some() {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        shared.physical_secret = Some(secret);
        Ok(())
    }

    /// Connect one independently certified profile to the shared loopback room.
    ///
    /// # Errors
    ///
    /// Rejects an invalid profile, duplicate device, or duplicate connected
    /// player route.
    pub fn enroll(&self, profile: &DeviceProfile) -> Result<(), DeviceClientError> {
        profile.validate()?;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if shared.clients.contains_key(&profile.device_id) {
            return Err(DeviceClientError::InvalidProfile);
        }
        let client = shared
            .authority
            .transport
            .connect(profile.player_id.clone())
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        shared.clients.insert(profile.device_id.clone(), client);
        shared
            .profiles
            .insert(profile.device_id.clone(), profile.clone());
        Ok(())
    }

    /// Disconnect or rebind one enrolled device's transport route. A final
    /// route disconnect enters the trusted transport-loss reducer boundary;
    /// rebind only restores delivery so the ordinary advertised `Reconnect`
    /// command can recover durable membership.
    ///
    /// # Errors
    ///
    /// Rejects unknown devices, duplicate transitions, wrong rooms, or a
    /// transport/reducer failure.
    pub fn change_route(
        &self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        profile.validate()?;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if shared.authority.state.room_id != *room_id
            || shared.profiles.get(&profile.device_id) != Some(profile)
            || shared.authority.state.member(&profile.player_id).is_none()
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        match operation {
            DeviceRouteOperationWire::Disconnect => {
                let client = shared
                    .clients
                    .remove(&profile.device_id)
                    .ok_or(DeviceClientError::TransportUnavailable)?;
                shared
                    .authority
                    .transport
                    .disconnect(client.connection_id())
                    .map_err(|_| DeviceClientError::TransportUnavailable)?;
                shared
                    .authority
                    .drive_all()
                    .map_err(|_| DeviceClientError::ProtocolViolation)?;
            }
            DeviceRouteOperationWire::Rebind => {
                if shared.clients.contains_key(&profile.device_id) {
                    return Err(DeviceClientError::ProtocolViolation);
                }
                let client = shared
                    .authority
                    .transport
                    .connect(profile.player_id.clone())
                    .map_err(|_| DeviceClientError::TransportUnavailable)?;
                shared.clients.insert(profile.device_id.clone(), client);
            }
        }
        let member_connected = shared
            .authority
            .state
            .member(&profile.player_id)
            .is_some_and(|member| member.connection == ConnectionState::Connected);
        Ok(DeviceRouteResultWire {
            schema_version: poche_protocol::DEVICE_ACTION_SCHEMA_VERSION_V1,
            room_id: room_id.clone(),
            session_epoch: shared.authority.state.session_epoch,
            player_id: profile.player_id.clone(),
            device_id: profile.device_id.clone(),
            operation,
            authoritative_revision: shared.authority.state.revision,
            route_connected: matches!(operation, DeviceRouteOperationWire::Rebind),
            member_connected,
        })
    }

    /// Register an enrolled target device's signed capture advertisement and
    /// non-authoritative provider handler.
    ///
    /// # Errors
    ///
    /// Rejects an unenrolled/mismatched device, invalid certificate or
    /// advertisement signature, or a stale provider registration. A newer
    /// signed advertisement for the same device atomically replaces its old
    /// transport route so crashed/restarted graphical workers can recover.
    pub fn register_capture_provider<H>(
        &self,
        profile: &DeviceProfile,
        advertisement: CaptureProviderAdvertisementWire,
        handler: H,
    ) -> Result<(), DeviceClientError>
    where
        H: RuntimeDeviceCooperationHandler,
    {
        profile.validate()?;
        verify_device_certificate(&profile.certificate)?;
        verify_capture_advertisement(&advertisement, &profile.certificate)?;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if shared.profiles.get(&profile.device_id) != Some(profile)
            || advertisement.provider_device_id != profile.device_id
            || shared
                .capture_providers
                .get(&profile.device_id)
                .is_some_and(|existing| {
                    existing.advertisement.advertisement_sequence
                        >= advertisement.advertisement_sequence
                })
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        shared.capture_providers.insert(
            profile.device_id.clone(),
            RegisteredCaptureProvider {
                advertisement,
                handler: Arc::new(Mutex::new(Box::new(handler))),
            },
        );
        Ok(())
    }

    /// Retire one provider route after its transport bearer capability has
    /// been authenticated by the owning gateway. Prepared in-flight routes
    /// remain independently owned and can finish.
    ///
    /// # Errors
    ///
    /// Returns a transport error if shared provider state is unavailable.
    pub fn unregister_capture_provider(
        &self,
        device_id: &DeviceId,
    ) -> Result<bool, DeviceClientError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        Ok(shared.capture_providers.remove(device_id).is_some())
    }

    /// Set deterministic wall-clock evidence used only by non-authoritative
    /// device-cooperation expiry checks.
    ///
    /// # Errors
    ///
    /// Returns a transport error if the shared adapter is unavailable.
    pub fn set_cooperation_now_unix_ms(&self, now_unix_ms: u64) -> Result<(), DeviceClientError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        shared.cooperation_now_unix_ms = now_unix_ms;
        Ok(())
    }

    #[must_use]
    pub fn revision(&self) -> Option<u64> {
        self.shared
            .lock()
            .ok()
            .map(|shared| shared.authority.state.revision)
    }

    #[must_use]
    pub fn session_epoch(&self) -> Option<u64> {
        self.shared
            .lock()
            .ok()
            .map(|shared| shared.authority.state.session_epoch)
    }

    #[must_use]
    pub fn room_id(&self) -> Option<RoomId> {
        self.shared
            .lock()
            .ok()
            .map(|shared| shared.authority.state.room_id.clone())
    }

    /// Check membership by stable player root without projecting private state.
    ///
    /// # Errors
    ///
    /// Returns transport unavailable if the shared authority lock is poisoned.
    pub fn player_is_member(&self, player: &PrincipalId) -> Result<bool, DeviceClientError> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        Ok(shared.authority.state.member(player).is_some())
    }

    /// Check a bearer proof against current hashed room invite records without
    /// exposing a verifier.
    ///
    /// # Errors
    ///
    /// Returns transport unavailable if the shared authority lock is poisoned.
    pub fn accepts_invite(&self, invite: &InviteProof) -> Result<bool, DeviceClientError> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        Ok(shared.authority.state.accepts_invite(invite))
    }

    /// Return the two configured authority-owned service principals.
    ///
    /// # Errors
    ///
    /// Returns transport unavailable if the shared authority lock is poisoned.
    pub fn authority_principals(&self) -> Result<(PrincipalId, PrincipalId), DeviceClientError> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        Ok((
            shared.authority.state.authority_clock.clone(),
            shared.authority.state.game_environment.clone(),
        ))
    }
}

#[derive(Clone)]
struct CachedRemoteObservation {
    request: DeviceObservationRequestWire,
    observation: DeviceObservation,
}

#[derive(Clone)]
struct CachedRemoteAction {
    action: DeviceActionWire,
    result: DeviceActionResult,
}

#[derive(Clone)]
struct CachedRemoteRoute {
    request: DeviceRouteRequestWire,
    result: DeviceRouteResultWire,
}

/// Process-external certified-device boundary around one real reducer-backed
/// room adapter. HTTP and Veilid servers may share this service rather than
/// reimplementing certificate enrollment, signature verification, or replay.
pub struct CertifiedDeviceRoom<G: SessionGame, A> {
    adapter: RuntimeLoopbackDeviceAdapter<G, A>,
    profiles: BTreeMap<DeviceId, DeviceProfile>,
    observation_cache: BTreeMap<(DeviceId, String), CachedRemoteObservation>,
    action_cache: BTreeMap<(DeviceId, String), CachedRemoteAction>,
    route_cache: BTreeMap<(DeviceId, String), CachedRemoteRoute>,
    service_profiles: Vec<DeviceProfile>,
    next_service_command: u64,
    countdown_started: Option<(u64, std::time::Duration)>,
}

/// Internal authority-only checkpoint including exact signed-request replay
/// caches. No serialization or Debug; not a player-visible snapshot.
pub struct CertifiedRoomCheckpoint<G: SessionGame, A> {
    adapter: RuntimeDeviceCheckpoint<G, A>,
    profiles: BTreeMap<DeviceId, DeviceProfile>,
    observation_cache: BTreeMap<(DeviceId, String), CachedRemoteObservation>,
    action_cache: BTreeMap<(DeviceId, String), CachedRemoteAction>,
    route_cache: BTreeMap<(DeviceId, String), CachedRemoteRoute>,
    service_profiles: Vec<DeviceProfile>,
    next_service_command: u64,
}

/// Already-authenticated cooperation route that can execute without holding
/// the certified-room enrollment/replay-cache lock. A remote graphical
/// provider may therefore observe the room while answering the request.
pub struct PreparedCertifiedCooperation<G: SessionGame, A> {
    adapter: RuntimeLoopbackDeviceAdapter<G, A>,
    profile: DeviceProfile,
    target_device: DeviceId,
    request: DeviceCooperationRequest,
}

impl<G, A> PreparedCertifiedCooperation<G, A>
where
    G: SessionGame + Send + 'static,
    G::Error: Send,
    A: AdvertisedActionSource<G>,
{
    /// Execute the authorized route through the ordinary adapter.
    ///
    /// # Errors
    ///
    /// Returns the provider adapter's stable cooperation failure.
    pub fn execute(self) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.adapter
            .clone()
            .cooperate(&self.profile, &self.target_device, self.request)
    }
}

impl<G, A> CertifiedDeviceRoom<G, A>
where
    G: SessionGame + Send + 'static,
    G::Error: Send,
    A: AdvertisedActionSource<G>,
{
    /// Capture while the caller holds exclusive access to this certified room.
    pub fn checkpoint(&self) -> Result<CertifiedRoomCheckpoint<G, A>, DeviceClientError>
    where A: Clone {
        Ok(CertifiedRoomCheckpoint {
            adapter: self.adapter.checkpoint()?, profiles: self.profiles.clone(),
            observation_cache: self.observation_cache.clone(), action_cache: self.action_cache.clone(),
            route_cache: self.route_cache.clone(), service_profiles: self.service_profiles.clone(),
            next_service_command: self.next_service_command,
        })
    }

    /// Restore in-memory checkpoint state; countdown observation restarts its
    /// local elapsed-time anchor instead of reusing another process's clock.
    pub fn from_checkpoint(checkpoint: CertifiedRoomCheckpoint<G, A>, codec: LoopbackCodec) -> Result<Self, DeviceClientError> {
        Ok(Self {
            adapter: RuntimeLoopbackDeviceAdapter::from_checkpoint(checkpoint.adapter, codec)?,
            profiles: checkpoint.profiles, observation_cache: checkpoint.observation_cache,
            action_cache: checkpoint.action_cache, route_cache: checkpoint.route_cache,
            service_profiles: checkpoint.service_profiles, next_service_command: checkpoint.next_service_command,
            countdown_started: None,
        })
    }

    #[must_use]
    pub fn new(adapter: RuntimeLoopbackDeviceAdapter<G, A>) -> Self {
        Self {
            adapter,
            profiles: BTreeMap::new(),
            observation_cache: BTreeMap::new(),
            action_cache: BTreeMap::new(),
            route_cache: BTreeMap::new(),
            service_profiles: Vec::new(),
            next_service_command: 0,
            countdown_started: None,
        }
    }

    /// Enroll one root-certified local authority service. Player devices never
    /// enter this list; the profile root must equal the room's configured
    /// authority-clock or game-environment principal.
    ///
    /// # Errors
    ///
    /// Rejects an ordinary player, invalid certificate, or duplicate device.
    pub fn enroll_authority_service(
        &mut self,
        profile: &DeviceProfile,
    ) -> Result<(), DeviceClientError> {
        let (clock, environment) = self.adapter.authority_principals()?;
        if profile.player_id != clock && profile.player_id != environment {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let enrolled = self.ensure_enrolled(&profile.certificate)?;
        if self
            .service_profiles
            .iter()
            .any(|item| item.device_id == enrolled.device_id)
        {
            return Err(DeviceClientError::InvalidProfile);
        }
        self.service_profiles.push(enrolled);
        self.service_profiles
            .sort_by(|left, right| left.device_id.cmp(&right.device_id));
        Ok(())
    }

    /// Accept hand motion without changing any logical game/session revision.
    /// Claims use compare-and-swap lease generations, so sibling devices cannot
    /// silently overwrite each other's active drag. No play is implied.
    pub fn physical_pose(
        &mut self,
        signed: &poche_player_client::SignedPhysicalPose,
    ) -> Result<poche_player_client::PhysicalPoseState, DeviceClientError> {
        let request = &signed.request;
        request
            .certificate
            .validate()
            .map_err(|_| DeviceClientError::InvalidProfile)?;
        verify_device_certificate(&request.certificate)?;
        if !verify_ed25519_hex(
            &request.certificate.device_signing_public_key,
            signed.signature.as_str(),
            &request.canonical_bytes()?,
        ) {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        if !request
            .certificate
            .capabilities
            .contains(&poche_protocol::DeviceCapabilityWire::Propose)
            || request.session_epoch < request.certificate.valid_from_membership_epoch
            || request
                .certificate
                .valid_through_membership_epoch
                .is_some_and(|end| request.session_epoch > end)
            || !poche_spatial::Point3Mm::new(
                request.position_mm[0],
                request.position_mm[1],
                request.position_mm[2],
            )
            .is_within_table_bounds()
            || request
                .rotation_millidegrees
                .iter()
                .any(|angle| !(0..360_000).contains(angle))
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let profile = self.ensure_enrolled(&request.certificate)?;
        let mut shared = self
            .adapter
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if !shared
            .clients
            .get(&profile.device_id)
            .is_some_and(|client| client.route_connected(&shared.authority.transport))
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        if request.room_id != shared.authority.state.room_id
            || request.session_epoch != shared.authority.state.session_epoch
        {
            return Err(DeviceClientError::StaleRevision);
        }
        let view = observation(&mut shared, &profile, &request.room_id)?;
        if !view
            .physical_hands
            .iter()
            .any(|card| card.id == request.card_id && card.face.is_some())
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let round = view.projection.payload.public_game_state.as_ref()
            .ok_or(DeviceClientError::InvalidObservation)?.round_index;
        let pose_epoch = shared.authority.latest_game_start_revision()
            .ok_or(DeviceClientError::InvalidObservation)?
            .checked_mul(65_536).and_then(|epoch| epoch.checked_add(u64::from(round)))
            .ok_or(DeviceClientError::ProtocolViolation)?;
        let previous = shared.physical_poses.get(&request.card_id)
            .filter(|_| shared.physical_pose_epoch == Some(pose_epoch));
        let current_generation = previous.map_or(0, |pose| pose.generation);
        if request.generation != current_generation {
            return Err(DeviceClientError::StaleRevision);
        }
        let generation = if request.claim {
            if request.sequence != 1 {
                return Err(DeviceClientError::ProtocolViolation);
            }
            current_generation
                .checked_add(1)
                .ok_or(DeviceClientError::ProtocolViolation)?
        } else {
            let previous = previous.ok_or(DeviceClientError::AuthorizationDenied)?;
            if previous.device != profile.device_id {
                return Err(DeviceClientError::AuthorizationDenied);
            }
            if request.sequence <= previous.sequence {
                return Err(DeviceClientError::StaleRevision);
            }
            current_generation
        };
        let accepted = poche_player_client::PhysicalPoseState {
            device: profile.device_id,
            generation,
            sequence: request.sequence,
            position_mm: request.position_mm,
            rotation_millidegrees: request.rotation_millidegrees,
        };
        // Preserve accepted poses when cards leave hands. Only a new deal
        // identity epoch invalidates the old deck's poses (at most 52 entries).
        // Publication of played-card poses remains a separate privacy boundary.
        let SharedLoopbackState { physical_poses, physical_pose_epoch, .. } = &mut *shared;
        retain_accepted_pose(physical_poses, physical_pose_epoch, pose_epoch, request.card_id.clone(), accepted.clone());
        Ok(accepted)
    }

    /// Advance deterministic authority-owned transitions through the same
    /// advertised-action and reducer path used by external devices. This is a
    /// bounded scheduler tick, not a wall-clock read inside the reducer.
    ///
    /// # Errors
    ///
    /// Returns a stable transport/protocol error if a certified service cannot
    /// observe or commit its currently advertised transition.
    pub fn drive_authority_services(
        &mut self,
        max_actions: usize,
    ) -> Result<usize, DeviceClientError> {
        self.drive_authority_services_gated(max_actions, true)
    }

    /// Scheduler-supplied monotonic elapsed time; no wall clock enters the reducer.
    /// A fresh countdown (even with a reused token) receives the full duration.
    /// The first scheduler observation starts the delay, so scheduling latency
    /// may lengthen but never shorten the opportunity to abort.
    ///
    /// # Errors
    /// Returns a transport error for poisoned state or the ordinary service
    /// dispatch error if an enrolled service cannot commit its advertised action.
    pub fn drive_authority_services_elapsed(
        &mut self,
        max_actions: usize,
        now: std::time::Duration,
        countdown_duration: std::time::Duration,
    ) -> Result<usize, DeviceClientError> {
        let generation = self
            .adapter
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?
            .authority
            .current_countdown_revision();
        let ready = match generation {
            None => {
                self.countdown_started = None;
                false
            }
            Some(revision) => {
                if self
                    .countdown_started
                    .is_none_or(|(previous, _)| previous != revision)
                {
                    self.countdown_started = Some((revision, now));
                }
                let (_, started) = self.countdown_started.expect("countdown initialized");
                now.checked_sub(started)
                    .is_some_and(|elapsed| elapsed >= countdown_duration)
            }
        };
        self.drive_authority_services_gated(max_actions, ready)
    }

    fn drive_authority_services_gated(
        &mut self,
        max_actions: usize,
        expire_countdown: bool,
    ) -> Result<usize, DeviceClientError> {
        let room_id = self
            .adapter
            .room_id()
            .ok_or(DeviceClientError::TransportUnavailable)?;
        let mut committed = 0;
        while committed < max_actions {
            let mut progressed = false;
            for profile in self.service_profiles.clone() {
                if committed >= max_actions {
                    break;
                }
                let mut adapter = self.adapter.clone();
                let observation = adapter.observe(&profile, &room_id)?;
                let Some(action) = observation.actions.first() else {
                    continue;
                };
                if matches!(action.payload, CommandPayload::CountdownExpired { .. })
                    && !expire_countdown
                {
                    continue;
                }
                let sequence = self.next_service_command;
                self.next_service_command = self.next_service_command.saturating_add(1);
                let request = DeviceActionRequest {
                    room_id: room_id.clone(),
                    session_epoch: observation.projection.session_epoch,
                    command_id: poche_protocol::CommandId::new(format!(
                        "authority-service-{sequence}"
                    ))
                    .map_err(|_| DeviceClientError::ProtocolViolation)?,
                    player_id: profile.player_id.clone(),
                    device_id: profile.device_id.clone(),
                    expected_revision: observation.projection.current_revision,
                    expected_projection_hash: observation.projection_hash,
                    action_id: action.id.clone(),
                    payload: action.payload.clone(),
                };
                if !matches!(
                    adapter.invoke(&profile, request)?,
                    DeviceActionResult::Committed { .. }
                ) {
                    return Err(DeviceClientError::ProtocolViolation);
                }
                committed += 1;
                progressed = true;
            }
            if !progressed {
                break;
            }
        }
        Ok(committed)
    }

    /// Route one independently signed, non-authoritative cooperation request
    /// from an external certified device to its exact registered target.
    ///
    /// # Errors
    ///
    /// Rejects invalid requester certification, request/profile mismatch,
    /// unknown targets, replay, or provider protocol failures.
    pub fn cooperate(
        &mut self,
        certificate: &DeviceCertificateWire,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.prepare_cooperation(certificate, target_device, request)?
            .execute()
    }

    /// Authenticate/enroll the requester and detach a non-authoritative route
    /// from the certified-room lock before a remote provider does rendering or
    /// artifact I/O.
    ///
    /// # Errors
    ///
    /// Fails closed when the requester certificate, membership, target route,
    /// or signed cooperation request is invalid.
    pub fn prepare_cooperation(
        &mut self,
        certificate: &DeviceCertificateWire,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<PreparedCertifiedCooperation<G, A>, DeviceClientError> {
        let profile = self.ensure_enrolled(certificate)?;
        Ok(PreparedCertifiedCooperation {
            adapter: self.adapter.clone(),
            profile,
            target_device: target_device.clone(),
            request,
        })
    }

    /// Register an external provider through the same certificate and
    /// advertisement checks used by in-process graphical devices.
    ///
    /// # Errors
    ///
    /// Fails closed for invalid, unauthorized, duplicate, expired, or
    /// mismatched provider advertisements.
    pub fn register_capture_provider<H>(
        &mut self,
        certificate: &DeviceCertificateWire,
        advertisement: CaptureProviderAdvertisementWire,
        handler: H,
    ) -> Result<(), DeviceClientError>
    where
        H: RuntimeDeviceCooperationHandler,
    {
        let profile = self.ensure_enrolled(certificate)?;
        self.adapter
            .register_capture_provider(&profile, advertisement, handler)
    }

    /// Retire a gateway-authenticated external provider route without
    /// changing player membership or authoritative game history.
    ///
    /// # Errors
    ///
    /// Returns a transport error if the shared adapter is unavailable.
    pub fn unregister_capture_provider(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<bool, DeviceClientError> {
        self.adapter.unregister_capture_provider(device_id)
    }

    /// Verify, enroll, and answer one signed exact-recipient snapshot or wait.
    /// Successful request IDs are replayed as the exact original observation,
    /// never reinterpreted against newer private state.
    ///
    /// # Errors
    ///
    /// Returns stable device-client categories for bad signatures, room/epoch
    /// mismatch, conflicting replay, no wait progress, or adapter failure.
    pub fn observe(
        &mut self,
        request: DeviceObservationRequestWire,
    ) -> Result<DeviceObservation, DeviceClientError> {
        verify_signed_device_observation_request(&request)?;
        let key = (
            request.device_id.clone(),
            request.request_id.as_str().to_owned(),
        );
        if let Some(cached) = self.observation_cache.get(&key) {
            return if cached.request == request {
                Ok(cached.observation.clone())
            } else {
                Err(DeviceClientError::ProtocolViolation)
            };
        }
        self.validate_room_epoch(&request.room_id, request.session_epoch)?;
        let is_member = self.adapter.player_is_member(&request.player_id)?;
        match (&request.join_invite, is_member) {
            (Some(_), true) => return Err(DeviceClientError::ProtocolViolation),
            (Some(invite), false) if !self.adapter.accepts_invite(invite)? => {
                return Err(DeviceClientError::AuthorizationDenied);
            }
            _ => {}
        }
        let profile = self.ensure_enrolled(&request.certificate)?;
        let mut adapter = self.adapter.clone();
        let mut observation = match request.mode {
            poche_protocol::DeviceObservationModeWire::Snapshot => {
                adapter.observe(&profile, &request.room_id)?
            }
            poche_protocol::DeviceObservationModeWire::Wait { after_revision } => {
                adapter.wait(&profile, &request.room_id, after_revision)?
            }
        };
        if let Some(invite) = &request.join_invite {
            observation.actions.retain(|action| {
                !matches!(&action.payload, CommandPayload::RedeemInvite { .. })
                    || matches!(
                        &action.payload,
                        CommandPayload::RedeemInvite { invite: advertised }
                            if advertised == invite
                    )
            });
            if !observation
                .actions
                .iter()
                .any(|action| matches!(&action.payload, CommandPayload::RedeemInvite { .. }))
            {
                return Err(DeviceClientError::ProtocolViolation);
            }
        } else {
            observation
                .actions
                .retain(|action| !matches!(&action.payload, CommandPayload::RedeemInvite { .. }));
        }
        if self.observation_cache.len() >= 256 {
            let oldest = self.observation_cache.keys().next().cloned();
            if let Some(oldest) = oldest {
                self.observation_cache.remove(&oldest);
            }
        }
        self.observation_cache.insert(
            key,
            CachedRemoteObservation {
                request,
                observation: observation.clone(),
            },
        );
        Ok(observation)
    }

    /// Verify, enroll, and submit one signed action through the ordinary
    /// advertised-action/reducer path.
    ///
    /// # Errors
    ///
    /// Returns a stable category for signature, room/epoch, stale action,
    /// authorization, or adapter failure.
    pub fn invoke(
        &mut self,
        action: &DeviceActionWire,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        let request = verify_signed_device_action(action)?;
        let key = (
            action.device_id.clone(),
            action.command_id.as_str().to_owned(),
        );
        if let Some(cached) = self.action_cache.get(&key) {
            return if cached.action == *action {
                Ok(cached.result.clone())
            } else {
                Err(DeviceClientError::ProtocolViolation)
            };
        }
        self.validate_room_epoch(&request.room_id, request.session_epoch)?;
        let profile = self.ensure_enrolled(&action.certificate)?;
        let result = self.adapter.clone().invoke(&profile, request)?;
        if self.action_cache.len() >= 256 {
            let oldest = self.action_cache.keys().next().cloned();
            if let Some(oldest) = oldest {
                self.action_cache.remove(&oldest);
            }
        }
        self.action_cache.insert(
            key,
            CachedRemoteAction {
                action: action.clone(),
                result: result.clone(),
            },
        );
        Ok(result)
    }

    /// Verify and apply one signed transport-route disconnect or rebind. The
    /// operation has its own replay cache and signature domain and cannot be
    /// confused with an ordinary observation or authoritative room command.
    ///
    /// # Errors
    ///
    /// Returns stable categories for invalid signatures, unknown enrollment,
    /// stale room/epoch, conflicting replay, or invalid route state.
    pub fn change_route(
        &mut self,
        request: DeviceRouteRequestWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        verify_signed_device_route_request(&request)?;
        let key = (
            request.device_id.clone(),
            request.request_id.as_str().to_owned(),
        );
        if let Some(cached) = self.route_cache.get(&key) {
            return if cached.request == request {
                Ok(cached.result.clone())
            } else {
                Err(DeviceClientError::ProtocolViolation)
            };
        }
        self.validate_room_epoch(&request.room_id, request.session_epoch)?;
        let profile = self
            .profiles
            .get(&request.device_id)
            .filter(|profile| profile.certificate == request.certificate)
            .cloned()
            .ok_or(DeviceClientError::AuthorizationDenied)?;
        let result = self
            .adapter
            .change_route(&profile, &request.room_id, request.operation)?;
        if self.route_cache.len() >= 256 {
            let oldest = self.route_cache.keys().next().cloned();
            if let Some(oldest) = oldest {
                self.route_cache.remove(&oldest);
            }
        }
        self.route_cache.insert(
            key,
            CachedRemoteRoute {
                request,
                result: result.clone(),
            },
        );
        Ok(result)
    }

    fn validate_room_epoch(
        &self,
        room_id: &RoomId,
        session_epoch: u64,
    ) -> Result<(), DeviceClientError> {
        if self.adapter.room_id().as_ref() == Some(room_id)
            && self.adapter.session_epoch() == Some(session_epoch)
        {
            Ok(())
        } else {
            Err(DeviceClientError::TransportUnavailable)
        }
    }

    fn ensure_enrolled(
        &mut self,
        certificate: &DeviceCertificateWire,
    ) -> Result<DeviceProfile, DeviceClientError> {
        verify_device_certificate(certificate)?;
        if let Some(profile) = self.profiles.get(&certificate.device_id) {
            return if profile.certificate == *certificate {
                Ok(profile.clone())
            } else {
                Err(DeviceClientError::InvalidProfile)
            };
        }
        let device_label = certificate
            .device_id
            .as_str()
            .get(..48)
            .ok_or(DeviceClientError::InvalidProfile)?;
        let profile = DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: format!("remote-{device_label}"),
            player_id: certificate.player_id.clone(),
            device_id: certificate.device_id.clone(),
            certificate: certificate.clone(),
            signing_key_handle: format!("remote-certified:{}", certificate.device_id.as_str()),
        };
        profile.validate()?;
        self.adapter.enroll(&profile)?;
        self.profiles
            .insert(profile.device_id.clone(), profile.clone());
        Ok(profile)
    }
}

impl<G, A> LoopbackDeviceAuthority for RuntimeLoopbackDeviceAdapter<G, A>
where
    G: SessionGame + Send + 'static,
    G::Error: Send,
    A: AdvertisedActionSource<G>,
{
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        observation(&mut shared, profile, room_id)
    }

    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let current_room = shared.authority.state.room_id.clone();
        let current = observation(&mut shared, profile, &current_room)?;
        if request.room_id != current.projection.room_id
            || request.session_epoch != current.projection.session_epoch
            || request.player_id != profile.player_id
            || request.device_id != profile.device_id
            || request.expected_revision != current.projection.current_revision
            || request.expected_projection_hash != current.projection_hash
        {
            return Err(DeviceClientError::StaleRevision);
        }
        let action = current
            .action(&request.action_id)
            .map(|action| action.payload == request.payload)
            .or_else(|| {
                current
                    .action_templates
                    .binary_search_by(|template| template.id.as_str().cmp(&request.action_id))
                    .ok()
                    .and_then(|index| current.action_templates.get(index))
                    .map(|template| template.accepts(&request.payload))
            })
            .ok_or(DeviceClientError::UnknownAction)?;
        if !action {
            return Err(DeviceClientError::ProtocolViolation);
        }
        let client = shared
            .clients
            .get(&profile.device_id)
            .cloned()
            .ok_or(DeviceClientError::TransportUnavailable)?;
        let command = client
            .command(
                &shared.authority.state,
                request.command_id.as_str(),
                request.payload,
            )
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        client
            .submit(&mut shared.authority.transport, command)
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let outcomes = shared
            .authority
            .drive_all()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let outcome = outcomes
            .into_iter()
            .find(|outcome| outcome.command_id == request.command_id)
            .ok_or(DeviceClientError::ProtocolViolation)?;
        Ok(match outcome.disposition {
            AuthorityDisposition::Applied => DeviceActionResult::Committed {
                command_id: outcome.command_id,
                revision: outcome.revision,
            },
            AuthorityDisposition::Denied(reason) => DeviceActionResult::Denied {
                command_id: outcome.command_id,
                code: denial_code(reason),
            },
            AuthorityDisposition::Disconnected => {
                return Err(DeviceClientError::TransportUnavailable);
            }
        })
    }

    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        let observation = self.observe(profile, room_id)?;
        if observation.projection.current_revision <= after_revision {
            Err(DeviceClientError::NoProgress)
        } else {
            Ok(observation)
        }
    }

    fn route(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        operation: DeviceRouteOperationWire,
    ) -> Result<DeviceRouteResultWire, DeviceClientError> {
        self.change_route(profile, room_id, operation)
    }

    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        let DeviceCooperationRequest::Capture(capture_request) = &request else {
            return Err(DeviceClientError::TransportUnavailable);
        };
        let capture_request = capture_request.clone();
        let (registered, context_snapshot, revision_before, history_before) = {
            let mut shared = self
                .shared
                .lock()
                .map_err(|_| DeviceClientError::TransportUnavailable)?;
            let requester = shared
                .profiles
                .get(&profile.device_id)
                .filter(|enrolled| *enrolled == profile)
                .cloned()
                .ok_or(DeviceClientError::AuthorizationDenied)?;
            let provider = shared
                .profiles
                .get(target_device)
                .cloned()
                .ok_or(DeviceClientError::AuthorizationDenied)?;
            if !shared.clients.contains_key(&requester.device_id)
                || !shared.clients.contains_key(&provider.device_id)
            {
                return Err(DeviceClientError::TransportUnavailable);
            }
            let registered = shared
                .capture_providers
                .get(target_device)
                .cloned()
                .ok_or(DeviceClientError::TransportUnavailable)?;
            verify_device_certificate(&requester.certificate)?;
            verify_device_certificate(&provider.certificate)?;
            verify_capture_request(&capture_request, &requester.certificate)?;
            let membership_epoch = shared
                .authority
                .state
                .member(&capture_request.player_id)
                .map(|member| member.membership_epoch)
                .ok_or(DeviceClientError::AuthorizationDenied)?;
            let snapshot = CaptureContextSnapshot {
                room_id: shared.authority.state.room_id.clone(),
                membership_epoch,
                current_revision: shared.authority.state.revision,
                now_unix_ms: shared.cooperation_now_unix_ms,
                requester_certificate: requester.certificate,
                provider_certificate: provider.certificate,
            };
            let context = snapshot.context();
            authorize_capture_request_with_provider(
                &registered.advertisement,
                &capture_request,
                &context,
            )
            .map_err(|_| DeviceClientError::AuthorizationDenied)?;
            shared
                .capture_replays
                .authorize_once(&capture_request, &context)
                .map_err(|_| DeviceClientError::AuthorizationDenied)?;
            (
                registered,
                snapshot,
                shared.authority.state.revision,
                shared.authority.state.public_history.len(),
            )
        };

        let result = registered
            .handler
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?
            .cooperate(
                &RuntimeDeviceCooperationContext {
                    requester_certificate: context_snapshot.requester_certificate.clone(),
                    provider_certificate: context_snapshot.provider_certificate.clone(),
                },
                request,
            )?;
        let DeviceCooperationResult::Capture(response) = &result;
        verify_capture_response(response, &context_snapshot.provider_certificate)?;
        authorize_capture_response(&capture_request, response, &context_snapshot.context())
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let shared = self
            .shared
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if shared.authority.state.revision != revision_before
            || shared.authority.state.public_history.len() != history_before
        {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(result)
    }
}

struct CaptureContextSnapshot {
    room_id: RoomId,
    membership_epoch: u64,
    current_revision: u64,
    now_unix_ms: u64,
    requester_certificate: DeviceCertificateWire,
    provider_certificate: DeviceCertificateWire,
}

impl CaptureContextSnapshot {
    const fn context(&self) -> CaptureAuthorizationContext<'_> {
        CaptureAuthorizationContext {
            room_id: &self.room_id,
            membership_epoch: self.membership_epoch,
            current_revision: self.current_revision,
            now_unix_ms: self.now_unix_ms,
            requester_certificate: &self.requester_certificate,
            provider_certificate: &self.provider_certificate,
            requester_revoked: false,
            provider_revoked: false,
        }
    }
}

fn verify_device_certificate(certificate: &DeviceCertificateWire) -> Result<(), DeviceClientError> {
    let bytes = canonical_device_certificate_bytes(&certificate.unsigned())
        .map_err(|_| DeviceClientError::InvalidProfile)?;
    verify_ed25519_hex(
        certificate.player_id.as_str(),
        certificate.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::InvalidProfile)
}

/// Verify a root-certified, device-signed external action and recover the
/// transport-neutral request consumed by an ordinary device adapter.
///
/// Structural validation covers capability, epoch, identity, action ID, and
/// signature intent. This function additionally verifies both Ed25519 layers;
/// stale revision/action-set checks remain the receiving authority's job.
///
/// # Errors
///
/// Returns a stable authorization or protocol category without retaining
/// rejected bytes or signature material.
pub fn verify_signed_device_action(
    action: &DeviceActionWire,
) -> Result<DeviceActionRequest, DeviceClientError> {
    action
        .validate()
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_device_certificate(&action.certificate)?;
    let bytes = canonical_device_action_bytes(&action.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    if !verify_ed25519_hex(
        &action.certificate.device_signing_public_key,
        action.signature.signature.as_str(),
        &bytes,
    ) {
        return Err(DeviceClientError::AuthorizationDenied);
    }
    Ok(DeviceActionRequest {
        room_id: action.room_id.clone(),
        session_epoch: action.session_epoch,
        command_id: action.command_id.clone(),
        player_id: action.player_id.clone(),
        device_id: action.device_id.clone(),
        expected_revision: action.expected_revision,
        expected_projection_hash: action.expected_projection_hash,
        action_id: action.action_id.clone(),
        payload: action.payload.clone(),
    })
}

/// Verify both certificate and device signatures on an exact-recipient read.
///
/// # Errors
///
/// Returns a stable category for malformed, wrong-root, or wrong-device input.
pub fn verify_signed_device_observation_request(
    request: &DeviceObservationRequestWire,
) -> Result<(), DeviceClientError> {
    request
        .validate()
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_device_certificate(&request.certificate)?;
    let bytes = canonical_device_observation_request_bytes(&request.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_ed25519_hex(
        &request.certificate.device_signing_public_key,
        request.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::AuthorizationDenied)
}

/// Verify both certificate and exact-device signatures on a transport-route
/// transition request.
///
/// # Errors
///
/// Returns a stable authorization or protocol category without retaining
/// rejected bytes, keys, or route metadata.
pub fn verify_signed_device_route_request(
    request: &DeviceRouteRequestWire,
) -> Result<(), DeviceClientError> {
    request
        .validate()
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_device_certificate(&request.certificate)?;
    let bytes = canonical_device_route_request_bytes(&request.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_ed25519_hex(
        &request.certificate.device_signing_public_key,
        request.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::AuthorizationDenied)
}

fn verify_capture_advertisement(
    advertisement: &CaptureProviderAdvertisementWire,
    certificate: &DeviceCertificateWire,
) -> Result<(), DeviceClientError> {
    let bytes = canonical_capture_provider_advertisement_bytes(&advertisement.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_ed25519_hex(
        &certificate.device_signing_public_key,
        advertisement.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::AuthorizationDenied)
}

fn verify_capture_request(
    request: &poche_protocol::CaptureRequestWire,
    certificate: &DeviceCertificateWire,
) -> Result<(), DeviceClientError> {
    let bytes = canonical_capture_request_bytes(&request.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_ed25519_hex(
        &certificate.device_signing_public_key,
        request.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::AuthorizationDenied)
}

fn verify_capture_response(
    response: &poche_protocol::CaptureResponseWire,
    certificate: &DeviceCertificateWire,
) -> Result<(), DeviceClientError> {
    let bytes = canonical_capture_response_bytes(&response.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    verify_ed25519_hex(
        &certificate.device_signing_public_key,
        response.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::ProtocolViolation)
}

fn observation<G, A>(
    shared: &mut SharedLoopbackState<G, A>,
    profile: &DeviceProfile,
    room_id: &RoomId,
) -> Result<DeviceObservation, DeviceClientError>
where
    G: SessionGame,
    A: AdvertisedActionSource<G>,
{
    if &shared.authority.state.room_id != room_id
        || !shared.clients.contains_key(&profile.device_id)
    {
        return Err(DeviceClientError::TransportUnavailable);
    }
    let client = shared
        .clients
        .get(&profile.device_id)
        .cloned()
        .ok_or(DeviceClientError::TransportUnavailable)?;
    while client
        .receive(&mut shared.authority.transport)
        .map_err(|_| DeviceClientError::TransportUnavailable)?
        .is_some()
    {}
    let connected_member = shared
        .authority
        .state
        .member(&profile.player_id)
        .is_some_and(|member| member.connection == ConnectionState::Connected);
    let payload = if profile.player_id == shared.authority.state.game_environment
        || profile.player_id == shared.authority.state.authority_clock
        || !connected_member
    {
        public_unprivileged_projection(&shared.authority.state)?
    } else {
        project_viewer(
            &shared.authority.state,
            &profile.player_id,
            shared.authority.state.projection_epoch,
        )
        .map_err(|_| DeviceClientError::InvalidObservation)?
    };
    let sequence = shared.next_projection;
    shared.next_projection = shared.next_projection.saturating_add(1);
    let projection = ProjectionEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id: room_id.clone(),
        session_epoch: shared.authority.state.session_epoch,
        projection_id: ProjectionId::new(format!("device-projection-{sequence}"))
            .map_err(|_| DeviceClientError::ProtocolViolation)?,
        principal_id: profile.player_id.clone(),
        current_revision: shared.authority.state.revision,
        projection_epoch: shared.authority.state.projection_epoch,
        correlation_id: CorrelationId::new(format!("device-observe-{sequence}"))
            .map_err(|_| DeviceClientError::ProtocolViolation)?,
        causation_id: EventId::new(format!("device-view-{sequence}"))
            .map_err(|_| DeviceClientError::ProtocolViolation)?,
        payload,
        signature: SignatureMetadata {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: profile.player_id.clone(),
            signature: SignatureBytes::new("0".repeat(128))
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
        },
    };
    // Delivery identifiers and the envelope signature change each time a
    // device observes the same state. Bind actions to the semantic projection
    // instead, so an unchanged authority view has one stable identity.
    let projection_hash = SemanticHash(
        *blake3::hash(
            &serde_json::to_vec(&(
                &projection.room_id,
                projection.session_epoch,
                &projection.principal_id,
                projection.current_revision,
                projection.projection_epoch,
                &projection.payload,
            ))
            .map_err(|_| DeviceClientError::ProtocolViolation)?,
        )
        .as_bytes(),
    );
    let mut actions = shared
        .action_source
        .actions(&shared.authority.state, &projection)?;
    actions.sort_by(|left, right| left.id.cmp(&right.id));
    if !actions.windows(2).all(|pair| pair[0].id < pair[1].id) {
        return Err(DeviceClientError::ProtocolViolation);
    }
    let (action_templates, chat_tail) = observation_supplements(shared, &projection, &actions)?;
    let mut capture_providers = shared
        .capture_providers
        .values()
        .filter(|provider| {
            provider.advertisement.player_id == projection.principal_id
                && provider.advertisement.room_id == projection.room_id
                && provider.advertisement.membership_epoch == projection.session_epoch
                && provider.advertisement.expires_at_unix_ms >= shared.cooperation_now_unix_ms
        })
        .map(|provider| provider.advertisement.clone())
        .collect::<Vec<_>>();
    capture_providers.sort_by(|left, right| left.provider_device_id.cmp(&right.provider_device_id));
    Ok(DeviceObservation {
        physical_hands: physical_hands(shared, &projection)?,
        physical_public: physical_public(shared, &projection)?,
        projection,
        projection_hash,
        actions,
        action_templates,
        chat_tail,
        capture_providers,
    })
}

fn retain_accepted_pose(
    poses: &mut BTreeMap<String, poche_player_client::PhysicalPoseState>,
    current_epoch: &mut Option<u64>,
    epoch: u64,
    card: String,
    pose: poche_player_client::PhysicalPoseState,
) {
    if *current_epoch != Some(epoch) {
        poses.clear();
        *current_epoch = Some(epoch);
    }
    poses.insert(card, pose);
}

fn physical_hands<G: SessionGame, A>(
    shared: &SharedLoopbackState<G, A>,
    projection: &ProjectionEnvelope,
) -> Result<Vec<poche_player_client::PhysicalHandCard>, DeviceClientError> {
    let Some(secret) = &shared.physical_secret else {
        return Ok(Vec::new());
    };
    let game = match &shared.authority.state.phase {
        SessionPhase::Running { game } | SessionPhase::Paused { game } => game,
        _ => return Ok(Vec::new()),
    };
    let public = game
        .public_projection()
        .map_err(|_| DeviceClientError::InvalidObservation)?;
    let epoch = shared
        .authority
        .latest_game_start_revision()
        .ok_or(DeviceClientError::InvalidObservation)?
        .checked_mul(65_536)
        .and_then(|epoch| epoch.checked_add(u64::from(public.round_index)))
        .ok_or(DeviceClientError::ProtocolViolation)?;
    let identities = poche_spatial::PhysicalDeckIdentity::new(secret, epoch);
    let mut cards = Vec::new();
    for member in &shared.authority.state.members {
        let Some(seat) = member.seat else {
            continue;
        };
        for face in game
            .private_hand(seat)
            .map_err(|_| DeviceClientError::InvalidObservation)?
        {
            let card =
                poche_spatial::CardFace::new(face).ok_or(DeviceClientError::InvalidObservation)?;
            let id = identities
                .card(card)
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let own = member.principal_id == projection.principal_id
                && member.connection == ConnectionState::Connected;
            cards.push(poche_player_client::PhysicalHandCard {
                pose: shared.physical_poses.get(&id).cloned(),
                id,
                seat,
                face: own.then_some(face),
            });
        }
    }
    cards.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(cards)
}

fn physical_public<G: SessionGame, A>(shared: &SharedLoopbackState<G, A>, projection: &ProjectionEnvelope) -> Result<Vec<poche_player_client::PhysicalPublicCard>, DeviceClientError> {
    let (Some(secret), Some(game), Some(epoch)) = (&shared.physical_secret, &projection.payload.public_game_state, shared.physical_pose_epoch) else { return Ok(Vec::new()); };
    let current_epoch = shared.authority.latest_game_start_revision().and_then(|revision| revision.checked_mul(65_536)).and_then(|value| value.checked_add(u64::from(game.round_index)));
    if current_epoch != Some(epoch) { return Ok(Vec::new()); }
    let identities = poche_spatial::PhysicalDeckIdentity::new(secret, epoch);
    let mut cards = Vec::new();
    for played in &game.current_trick {
        let face = poche_spatial::CardFace::new(played.card).ok_or(DeviceClientError::InvalidObservation)?;
        let id = identities.card(face).as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        if let Some(pose) = shared.physical_poses.get(&id) {
            cards.push(poche_player_client::PhysicalPublicCard { id, face: played.card, pose: pose.clone() });
        }
    }
    cards.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cards)
}

fn observation_supplements<G, A>(
    shared: &SharedLoopbackState<G, A>,
    projection: &ProjectionEnvelope,
    actions: &[AdvertisedAction],
) -> Result<(Vec<AdvertisedActionTemplate>, Vec<DeviceChatEntry>), DeviceClientError>
where
    G: SessionGame,
    A: AdvertisedActionSource<G>,
{
    let mut templates = shared
        .action_source
        .templates(&shared.authority.state, projection)?;
    templates.sort_by(|left, right| left.id.cmp(&right.id));
    if !templates.windows(2).all(|pair| pair[0].id < pair[1].id)
        || actions.iter().any(|action| {
            templates
                .binary_search_by(|template| template.id.as_str().cmp(&action.id))
                .is_ok()
        })
    {
        return Err(DeviceClientError::ProtocolViolation);
    }
    let chat = shared
        .authority
        .chat_tail()
        .entries()
        .map(|entry| DeviceChatEntry {
            revision: entry.revision,
            principal_id: entry.principal_id.clone(),
            text: entry.text.clone(),
        })
        .collect();
    Ok((templates, chat))
}

/// Public-only projection for authority services and not-yet-enrolled room
/// principals. It deliberately carries no private hand or spectator grant.
fn public_unprivileged_projection<G: SessionGame>(
    state: &SessionState<G>,
) -> Result<ProjectionPayload, DeviceClientError> {
    let public_game_state = match &state.phase {
        SessionPhase::Running { game }
        | SessionPhase::Paused { game }
        | SessionPhase::PostGame { game } => Some(
            game.public_projection()
                .map_err(|_| DeviceClientError::InvalidObservation)?,
        ),
        SessionPhase::Uninitialized
        | SessionPhase::Lobby
        | SessionPhase::Countdown { .. }
        | SessionPhase::Closed => None,
    };
    Ok(ProjectionPayload {
        phase: match &state.phase {
            SessionPhase::Uninitialized | SessionPhase::Lobby => RoomPhase::Lobby,
            SessionPhase::Countdown { .. } => RoomPhase::Countdown,
            SessionPhase::Running { .. } => RoomPhase::Running,
            SessionPhase::Paused { .. } => RoomPhase::Paused,
            SessionPhase::PostGame { .. } => RoomPhase::PostGame,
            SessionPhase::Closed => RoomPhase::Closed,
        },
        members: state
            .members
            .iter()
            .map(|member| MemberProjection {
                principal_id: member.principal_id.clone(),
                connected: member.connection == poche_session::ConnectionState::Connected,
                seat: member.seat,
                ready: member.ready,
                host: member.host,
            })
            .collect(),
        public_game_state,
        own_hand: None,
        granted_hands: Vec::new(),
        public_history: state.public_history.clone(),
    })
}

fn denial_code(reason: poche_protocol::DenyReason) -> String {
    serde_json::to_string(&reason)
        .unwrap_or_else(|_| "\"D-UNKNOWN\"".to_owned())
        .trim_matches('"')
        .to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepted_pose_retention_is_deal_scoped_not_hand_scoped() {
        let pose = poche_player_client::PhysicalPoseState {
            device: poche_protocol::DeviceId::new("pose-device").unwrap(),
            generation: 1, sequence: 1,
            position_mm: [100, 200, 300], rotation_millidegrees: [10, 20, 30],
        };
        let mut poses = std::collections::BTreeMap::new();
        let mut epoch = None;
        super::retain_accepted_pose(&mut poses, &mut epoch, 7, "played-card".to_owned(), pose.clone());
        super::retain_accepted_pose(&mut poses, &mut epoch, 7, "hand-card".to_owned(), pose.clone());
        assert_eq!(poses.len(), 2);
        assert_eq!(poses.get("played-card"), Some(&pose));
        super::retain_accepted_pose(&mut poses, &mut epoch, 8, "new-deal-card".to_owned(), pose.clone());
        assert_eq!(poses.len(), 1);
        assert!(!poses.contains_key("played-card"));
        assert_eq!(epoch, Some(8));
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    use ed25519_dalek::{Signer, SigningKey};
    use poche_player_client::{
        AdvertisedActionPolicy, DeviceProfile, DeviceSigner, LoopbackDeviceTransport,
        PlayerDeviceClient, PolicyScope, sign_observation_request, sign_route_request,
    };
    use poche_protocol::{
        CaptureArtifactDescriptorWire, CaptureArtifactId, CaptureConsentPolicyWire,
        CapturePrivacyWire, CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
        CaptureResponseOutcomeWire, CaptureTransferDescriptorWire, CaptureTransferId,
        CaptureViewportWire, CertificateId, CommandId, CommandPayload,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        DeviceCapabilityWire, DeviceCustodyWire, DeviceObservationModeWire,
        DeviceRouteOperationWire, DeviceSignatureIntentWire, PrincipalId, PublicGamePhase,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureIntent,
        UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
        UnsignedCaptureResponseWire, UnsignedDeviceCertificateWire, capture_request_hash,
    };

    use super::*;
    use crate::OracleSessionGame;
    use poche_session::InviteRecord;

    #[derive(Clone)]
    struct CreateRoomActions;

    #[test]
    fn adapter_checkpoint_retains_physical_identity_and_projection_sequence() {
        let state: SessionState<OracleSessionGame<2>> = SessionState::pending(RoomId::new("checkpoint-room").unwrap(), PrincipalId::new("clock").unwrap(), PrincipalId::new("game").unwrap());
        let adapter = RuntimeLoopbackDeviceAdapter::new(state, CreateRoomActions, LoopbackCodec::Typed);
        adapter.enable_physical_identities([73; 32]).unwrap();
        let profile = device_profile("checkpoint-device", "22");
        adapter.enroll(&profile).unwrap();
        adapter.shared.lock().unwrap().next_projection = 19;
        let restored = RuntimeLoopbackDeviceAdapter::from_checkpoint(adapter.checkpoint().unwrap(), LoopbackCodec::Typed).unwrap();
        let shared = restored.shared.lock().unwrap();
        assert_eq!(shared.physical_secret, Some([73; 32]));
        assert_eq!(shared.next_projection, 19);
        assert!(shared.profiles.contains_key(&profile.device_id));
        assert!(shared.capture_providers.is_empty());
    }

    impl AdvertisedActionSource<OracleSessionGame<2>> for CreateRoomActions {
        fn actions(
            &self,
            state: &SessionState<OracleSessionGame<2>>,
            _projection: &ProjectionEnvelope,
        ) -> Result<Vec<AdvertisedAction>, DeviceClientError> {
            Ok(if state.members.is_empty() {
                vec![AdvertisedAction {
                    id: "create-room".to_owned(),
                    label: "Create room".to_owned(),
                    payload: CommandPayload::CreateRoom,
                }]
            } else {
                Vec::new()
            })
        }
    }

    fn device_profile(label: &str, device_octet: &str) -> DeviceProfile {
        let player_key = "11".repeat(32);
        let device_key = device_octet.repeat(32);
        let player = PrincipalId::new(player_key).unwrap();
        let device = DeviceId::new(device_key.clone()).unwrap();
        let certificate = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("runtime-loopback-{label}")).unwrap(),
            player_id: player.clone(),
            device_id: device.clone(),
            device_signing_public_key: device_key,
            device_encryption_public_key: "ee".repeat(32),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![DeviceCapabilityWire::Propose],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player.clone(),
            },
        }
        .attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
        .unwrap();
        DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: label.to_owned(),
            player_id: player,
            device_id: device,
            certificate,
            signing_key_handle: "memory:test-only-handle".to_owned(),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").unwrap();
            output
        })
    }

    fn wire_signature(bytes: &[u8], key: &SigningKey) -> SignatureBytes {
        SignatureBytes::new(hex(&key.sign(bytes).to_bytes())).unwrap()
    }

    struct TestDeviceSigner(SigningKey);

    impl DeviceSigner for TestDeviceSigner {
        fn sign_device_bytes(
            &self,
            _profile: &DeviceProfile,
            canonical_bytes: &[u8],
        ) -> Result<SignatureBytes, DeviceClientError> {
            Ok(wire_signature(canonical_bytes, &self.0))
        }
    }

    fn signed_profile(
        label: &str,
        root_key: &SigningKey,
        device_key: &SigningKey,
        capabilities: Vec<DeviceCapabilityWire>,
    ) -> DeviceProfile {
        let player_public = hex(&root_key.verifying_key().to_bytes());
        let device_public = hex(&device_key.verifying_key().to_bytes());
        let player_id = PrincipalId::new(player_public).unwrap();
        let device_id = DeviceId::new(device_public.clone()).unwrap();
        let unsigned = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("signed-{label}")).unwrap(),
            player_id: player_id.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_public,
            device_encryption_public_key: "ee".repeat(32),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities,
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player_id.clone(),
            },
        };
        let signature = wire_signature(
            &canonical_device_certificate_bytes(&unsigned).unwrap(),
            root_key,
        );
        let certificate = unsigned.attach_signature(signature).unwrap();
        DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: label.to_owned(),
            player_id,
            device_id,
            certificate,
            signing_key_handle: format!("test-protected:{label}"),
        }
    }

    struct AcceptingCaptureHandler {
        key: SigningKey,
        calls: Arc<AtomicUsize>,
    }

    impl RuntimeDeviceCooperationHandler for AcceptingCaptureHandler {
        fn cooperate(
            &mut self,
            _context: &RuntimeDeviceCooperationContext,
            request: DeviceCooperationRequest,
        ) -> Result<DeviceCooperationResult, DeviceClientError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let DeviceCooperationRequest::Capture(request) = request else {
                return Err(DeviceClientError::ProtocolViolation);
            };
            let unsigned = UnsignedCaptureResponseWire {
                schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
                request_id: request.request_id.clone(),
                request_hash: capture_request_hash(&request.unsigned())
                    .map_err(|_| DeviceClientError::ProtocolViolation)?,
                room_id: request.room_id.clone(),
                membership_epoch: request.membership_epoch,
                player_id: request.player_id.clone(),
                requester_device_id: request.requester_device_id.clone(),
                provider_device_id: request.provider_device_id.clone(),
                outcome: CaptureResponseOutcomeWire::Accepted {
                    artifacts: vec![CaptureArtifactDescriptorWire {
                        artifact_id: CaptureArtifactId::new("runtime-synthetic-png")
                            .map_err(|_| DeviceClientError::ProtocolViolation)?,
                        representation: CaptureRepresentationWire::Png,
                        provider_kind: CaptureProviderKindWire::NativeBevy,
                        captured_revision: request.observed_revision,
                        projection_hash: SemanticHash([7; 32]),
                        scene_hash: Some(SemanticHash([8; 32])),
                        viewport: Some(CaptureViewportWire {
                            width_pixels: 1,
                            height_pixels: 1,
                        }),
                        transfer: CaptureTransferDescriptorWire {
                            transfer_id: CaptureTransferId::new("runtime-synthetic-transfer")
                                .map_err(|_| DeviceClientError::ProtocolViolation)?,
                            byte_length: 1,
                            chunk_bytes: 1,
                            chunk_count: 1,
                            content_hash: SemanticHash(*blake3::hash(&[0]).as_bytes()),
                        },
                    }],
                },
                signature_intent: DeviceSignatureIntentWire {
                    domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: request.provider_device_id,
                },
            };
            let signature = wire_signature(
                &canonical_capture_response_bytes(&unsigned)
                    .map_err(|_| DeviceClientError::ProtocolViolation)?,
                &self.key,
            );
            let response = unsigned
                .attach_signature(signature)
                .map_err(|_| DeviceClientError::ProtocolViolation)?;
            Ok(DeviceCooperationResult::Capture(response))
        }
    }

    #[test]
    fn shared_client_invokes_the_real_reducer_over_loopback() {
        let state: SessionState<OracleSessionGame<2>> = SessionState::pending(
            RoomId::new("device-room").unwrap(),
            PrincipalId::new("clock").unwrap(),
            PrincipalId::new("game").unwrap(),
        );
        let adapter = RuntimeLoopbackDeviceAdapter::new(
            state,
            CreateRoomActions,
            LoopbackCodec::CanonicalNdjson,
        );
        let profile = device_profile("alice-desktop", "22");
        let companion_profile = device_profile("alice-cli", "33");
        adapter.enroll(&profile).unwrap();
        adapter.enroll(&companion_profile).unwrap();
        let transport = LoopbackDeviceTransport::new(adapter.clone());
        let mut client = PlayerDeviceClient::new(profile, transport).unwrap();
        let companion_transport = LoopbackDeviceTransport::new(adapter.clone());
        let mut companion =
            PlayerDeviceClient::new(companion_profile, companion_transport).unwrap();
        let room = RoomId::new("device-room").unwrap();
        let observed = client.observe(&room).unwrap();
        let repeated = client.observe(&room).unwrap();
        assert_eq!(observed.projection.current_revision, 0);
        assert_ne!(
            observed.projection.projection_id,
            repeated.projection.projection_id
        );
        assert_eq!(observed.projection_hash, repeated.projection_hash);
        assert_eq!(observed.actions[0].id, "create-room");
        let result = client
            .invoke(
                &observed,
                "create-room",
                CommandId::new("device-create").unwrap(),
            )
            .unwrap();
        assert_eq!(
            result,
            DeviceActionResult::Committed {
                command_id: CommandId::new("device-create").unwrap(),
                revision: 1,
            }
        );
        assert_eq!(adapter.revision(), Some(1));
        assert!(client.actions(&room).unwrap().is_empty());
        assert_eq!(
            companion
                .observe(&room)
                .unwrap()
                .projection
                .current_revision,
            1
        );
    }

    #[test]
    fn entry_point_parity_commits_identical_successors_and_denials() {
        #[derive(Clone, Copy)]
        enum Entry {
            GuiAction,
            CliPayload,
            Policy,
        }

        let room = RoomId::new("entry-parity-room").unwrap();
        let mut evidence = Vec::new();
        for entry in [Entry::GuiAction, Entry::CliPayload, Entry::Policy] {
            let state: SessionState<OracleSessionGame<2>> = SessionState::pending(
                room.clone(),
                PrincipalId::new("entry-parity-clock").unwrap(),
                PrincipalId::new("entry-parity-game").unwrap(),
            );
            let adapter = RuntimeLoopbackDeviceAdapter::new(
                state,
                CreateRoomActions,
                LoopbackCodec::CanonicalNdjson,
            );
            let profile = device_profile("entry-parity-device", "77");
            adapter.enroll(&profile).unwrap();
            let mut client =
                PlayerDeviceClient::new(profile, LoopbackDeviceTransport::new(adapter.clone()))
                    .unwrap();
            let observation = client.observe(&room).unwrap();
            let command = CommandId::new("entry-parity-create").unwrap();
            let action_id = match entry {
                Entry::GuiAction | Entry::CliPayload => "create-room".to_owned(),
                Entry::Policy => AdvertisedActionPolicy::FirstLegal
                    .select(&observation, PolicyScope::AllAdvertised)
                    .unwrap()
                    .id
                    .clone(),
            };
            let prepared = match entry {
                Entry::CliPayload => client.prepare_payload(
                    &observation,
                    &CommandPayload::CreateRoom,
                    command.clone(),
                ),
                Entry::GuiAction | Entry::Policy => {
                    client.prepare(&observation, &action_id, command.clone())
                }
            }
            .unwrap();
            let result = match entry {
                Entry::CliPayload => client.invoke_payload(
                    &observation,
                    &CommandPayload::CreateRoom,
                    command.clone(),
                ),
                Entry::GuiAction | Entry::Policy => {
                    client.invoke(&observation, &action_id, command.clone())
                }
            }
            .unwrap();
            let successor = client.observe(&room).unwrap();
            let stale_result = match entry {
                Entry::CliPayload => client.invoke_payload(
                    &observation,
                    &CommandPayload::CreateRoom,
                    CommandId::new("entry-parity-stale").unwrap(),
                ),
                Entry::GuiAction | Entry::Policy => client.invoke(
                    &observation,
                    &action_id,
                    CommandId::new("entry-parity-stale").unwrap(),
                ),
            };
            evidence.push((
                serde_json::to_vec(&prepared).unwrap(),
                result,
                successor.projection_hash,
                successor.projection.payload,
                stale_result,
                adapter.revision(),
            ));
        }

        assert!(evidence.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(
            evidence[0].1,
            DeviceActionResult::Committed {
                command_id: CommandId::new("entry-parity-create").unwrap(),
                revision: 1,
            }
        );
        assert_eq!(evidence[0].4, Err(DeviceClientError::StaleRevision));
        assert_eq!(evidence[0].5, Some(1));
    }

    #[test]
    fn external_device_action_verifies_both_root_and_device_signatures() {
        let root_key = SigningKey::from_bytes(&[21; 32]);
        let device_key = SigningKey::from_bytes(&[22; 32]);
        let profile = signed_profile(
            "external-cli",
            &root_key,
            &device_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
        );
        let request = DeviceActionRequest {
            room_id: RoomId::new("external-room").unwrap(),
            session_epoch: 1,
            command_id: CommandId::new("external-command").unwrap(),
            player_id: profile.player_id.clone(),
            device_id: profile.device_id.clone(),
            expected_revision: 7,
            expected_projection_hash: SemanticHash([7; 32]),
            action_id: "game-bid-1".to_owned(),
            payload: CommandPayload::GameAction {
                action: GameActionWire::Bid { tricks: 1 },
            },
        };
        let signed = request
            .sign(&profile, &TestDeviceSigner(device_key))
            .unwrap();
        assert_eq!(verify_signed_device_action(&signed).unwrap(), request);

        let mut changed_revision = signed.clone();
        changed_revision.expected_revision += 1;
        assert_eq!(
            verify_signed_device_action(&changed_revision),
            Err(DeviceClientError::AuthorizationDenied)
        );

        let mut changed_certificate = signed;
        changed_certificate.certificate.signature.signature =
            SignatureBytes::new("0".repeat(128)).unwrap();
        assert_eq!(
            verify_signed_device_action(&changed_certificate),
            Err(DeviceClientError::InvalidProfile)
        );

        let observation_request = sign_observation_request(
            &profile,
            &RoomId::new("external-room").unwrap(),
            1,
            CorrelationId::new("external-observe").unwrap(),
            DeviceObservationModeWire::Wait { after_revision: 7 },
            &TestDeviceSigner(SigningKey::from_bytes(&[22; 32])),
        )
        .unwrap();
        assert_eq!(
            verify_signed_device_observation_request(&observation_request),
            Ok(())
        );
        let mut changed_wait = observation_request;
        changed_wait.mode = DeviceObservationModeWire::Wait { after_revision: 8 };
        assert_eq!(
            verify_signed_device_observation_request(&changed_wait),
            Err(DeviceClientError::AuthorizationDenied)
        );

        let route_request = sign_route_request(
            &profile,
            &RoomId::new("external-room").unwrap(),
            1,
            CorrelationId::new("external-route").unwrap(),
            DeviceRouteOperationWire::Disconnect,
            &TestDeviceSigner(SigningKey::from_bytes(&[22; 32])),
        )
        .unwrap();
        assert_eq!(verify_signed_device_route_request(&route_request), Ok(()));
        let mut changed_route = route_request;
        changed_route.operation = DeviceRouteOperationWire::Rebind;
        assert_eq!(
            verify_signed_device_route_request(&changed_route),
            Err(DeviceClientError::AuthorizationDenied)
        );
    }

    #[test]
    fn sibling_routes_require_last_loss_and_ordinary_reconnect() {
        let root = SigningKey::from_bytes(&[71; 32]);
        let first_key = SigningKey::from_bytes(&[72; 32]);
        let second_key = SigningKey::from_bytes(&[73; 32]);
        let capabilities = vec![
            DeviceCapabilityWire::Propose,
            DeviceCapabilityWire::ReceivePrivateProjection,
        ];
        let first_profile = signed_profile("route-first", &root, &first_key, capabilities.clone());
        let second_profile = signed_profile("route-second", &root, &second_key, capabilities);
        let room_id = RoomId::new("route-lifecycle-room").unwrap();
        let state: SessionState<OracleSessionGame<2>> = SessionState::pending(
            room_id.clone(),
            PrincipalId::new("route-clock").unwrap(),
            PrincipalId::new("route-environment").unwrap(),
        );
        let source =
            OracleRoomActionSource::new(0x7171, 2, "unused-route-invite", 1, "route-countdown")
                .unwrap();
        let adapter =
            RuntimeLoopbackDeviceAdapter::new(state, source, LoopbackCodec::CanonicalNdjson);
        adapter.enroll(&first_profile).unwrap();
        adapter.enroll(&second_profile).unwrap();
        let mut first =
            PlayerDeviceClient::new(first_profile, LoopbackDeviceTransport::new(adapter.clone()))
                .unwrap();
        let mut second = PlayerDeviceClient::new(
            second_profile,
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .unwrap();
        let create = first.observe(&room_id).unwrap();
        assert!(matches!(
            first
                .invoke(
                    &create,
                    "room-create",
                    CommandId::new("route-create").unwrap(),
                )
                .unwrap(),
            DeviceActionResult::Committed { revision: 1, .. }
        ));

        let first_down = first.disconnect_route(&room_id).unwrap();
        assert_eq!(first_down.authoritative_revision, 1);
        assert!(first_down.member_connected);
        assert_eq!(
            first.observe(&room_id),
            Err(DeviceClientError::TransportUnavailable)
        );
        let second_down = second.disconnect_route(&room_id).unwrap();
        assert_eq!(second_down.authoritative_revision, 2);
        assert!(!second_down.member_connected);

        let first_up = first.rebind_route(&room_id).unwrap();
        assert_eq!(first_up.authoritative_revision, 2);
        assert!(!first_up.member_connected);
        let reconnect = first.observe(&room_id).unwrap();
        assert!(reconnect.projection.payload.own_hand.is_none());
        assert!(reconnect.projection.payload.granted_hands.is_empty());
        assert_eq!(reconnect.actions.len(), 1);
        assert_eq!(reconnect.actions[0].id, "room-reconnect");
        assert!(reconnect.action_templates.is_empty());

        let second_up = second.rebind_route(&room_id).unwrap();
        assert!(!second_up.member_connected);
        assert_eq!(second.observe(&room_id).unwrap().actions.len(), 1);
        assert!(matches!(
            first
                .invoke(
                    &reconnect,
                    "room-reconnect",
                    CommandId::new("route-reconnect").unwrap(),
                )
                .unwrap(),
            DeviceActionResult::Committed { revision: 3, .. }
        ));
        assert!(first.observe(&room_id).unwrap().actions.len() > 1);
        assert!(second.observe(&room_id).unwrap().actions.len() > 1);
    }

    #[test]
    fn certified_room_replays_external_reads_and_writes_exactly() {
        let root_key = SigningKey::from_bytes(&[31; 32]);
        let device_key = SigningKey::from_bytes(&[32; 32]);
        let profile = signed_profile(
            "remote-room-device",
            &root_key,
            &device_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
        );
        let room_id = RoomId::new("remote-room").unwrap();
        let adapter = RuntimeLoopbackDeviceAdapter::new(
            SessionState::pending(
                room_id.clone(),
                PrincipalId::new("clock").unwrap(),
                PrincipalId::new("game").unwrap(),
            ),
            CreateRoomActions,
            LoopbackCodec::CanonicalNdjson,
        );
        let mut room = CertifiedDeviceRoom::new(adapter.clone());
        let signer = TestDeviceSigner(SigningKey::from_bytes(&[32; 32]));
        let signed_observe = sign_observation_request(
            &profile,
            &room_id,
            0,
            CorrelationId::new("remote-observe-0").unwrap(),
            DeviceObservationModeWire::Snapshot,
            &signer,
        )
        .unwrap();
        let observation = room.observe(signed_observe.clone()).unwrap();
        assert_eq!(observation.projection.current_revision, 0);
        assert_eq!(observation.actions[0].id, "create-room");

        let client = PlayerDeviceClient::new(
            profile.clone(),
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .unwrap();
        let prepared = client
            .prepare(
                &observation,
                "create-room",
                CommandId::new("remote-create").unwrap(),
            )
            .unwrap();
        let signed_action = prepared.sign(&profile, &signer).unwrap();
        let result = room.invoke(&signed_action).unwrap();
        assert_eq!(
            result,
            DeviceActionResult::Committed {
                command_id: CommandId::new("remote-create").unwrap(),
                revision: 1,
            }
        );
        assert_eq!(adapter.revision(), Some(1));
        let mut room = CertifiedDeviceRoom::from_checkpoint(
            room.checkpoint().unwrap(), LoopbackCodec::CanonicalNdjson,
        ).unwrap();
        assert_eq!(room.invoke(&signed_action).unwrap(), result);
        assert_eq!(room.adapter.revision(), Some(1));
        assert_eq!(
            room.observe(signed_observe).unwrap(),
            observation,
            "a retry must not reinterpret the signed snapshot against revision 1"
        );

        let mut conflicting = signed_action;
        conflicting.action_id = "create-room-conflict".to_owned();
        assert_eq!(
            room.invoke(&conflicting),
            Err(DeviceClientError::AuthorizationDenied),
            "mutating a signed retry fails before cache lookup"
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the acceptance scenario keeps certificate identity, lifecycle, service scheduling, and successor assertions together"
    )]
    fn certified_authority_services_advance_countdown_and_deal_without_a_window() {
        let host_root = SigningKey::from_bytes(&[40; 32]);
        let host_key = SigningKey::from_bytes(&[41; 32]);
        let guest_root = SigningKey::from_bytes(&[42; 32]);
        let guest_key = SigningKey::from_bytes(&[43; 32]);
        let clock_root = SigningKey::from_bytes(&[44; 32]);
        let clock_key = SigningKey::from_bytes(&[45; 32]);
        let environment_root = SigningKey::from_bytes(&[46; 32]);
        let environment_key = SigningKey::from_bytes(&[47; 32]);
        let host_profile = signed_profile(
            "service-host",
            &host_root,
            &host_key,
            vec![DeviceCapabilityWire::Propose],
        );
        let guest_profile = signed_profile(
            "service-guest",
            &guest_root,
            &guest_key,
            vec![DeviceCapabilityWire::Propose],
        );
        let clock_profile = signed_profile(
            "authority-clock",
            &clock_root,
            &clock_key,
            vec![DeviceCapabilityWire::Propose],
        );
        let environment_profile = signed_profile(
            "game-environment",
            &environment_root,
            &environment_key,
            vec![DeviceCapabilityWire::Propose],
        );
        let room_id = RoomId::new("certified-service-room").unwrap();
        let mut state: SessionState<OracleSessionGame<2>> = SessionState::pending(
            room_id.clone(),
            clock_profile.player_id.clone(),
            environment_profile.player_id.clone(),
        );
        state
            .invites
            .push(InviteRecord::new_reusable("service-guest-invite", u64::MAX).unwrap());
        let source =
            OracleRoomActionSource::new(0x5eed, 2, "service-guest-invite", 3, "service-countdown")
                .unwrap()
                .without_hand_sharing();
        let adapter =
            RuntimeLoopbackDeviceAdapter::new(state, source, LoopbackCodec::CanonicalNdjson);
        adapter.enable_physical_identities([92; 32]).unwrap();
        adapter.enroll(&host_profile).unwrap();
        adapter.enroll(&guest_profile).unwrap();
        let mut host =
            PlayerDeviceClient::new(host_profile, LoopbackDeviceTransport::new(adapter.clone()))
                .unwrap();
        let mut guest = PlayerDeviceClient::new(
            guest_profile.clone(),
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .unwrap();
        let spectator_profile = signed_profile(
            "strict-spectator",
            &SigningKey::from_bytes(&[48; 32]),
            &SigningKey::from_bytes(&[49; 32]),
            vec![DeviceCapabilityWire::Propose],
        );
        let mut certified = CertifiedDeviceRoom::new(adapter.clone());
        certified
            .ensure_enrolled(&spectator_profile.certificate)
            .unwrap();
        let mut spectator = PlayerDeviceClient::new(
            spectator_profile.clone(),
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .unwrap();
        assert_eq!(
            certified.enroll_authority_service(&guest_profile),
            Err(DeviceClientError::AuthorizationDenied)
        );
        certified.enroll_authority_service(&clock_profile).unwrap();
        certified
            .enroll_authority_service(&environment_profile)
            .unwrap();
        assert_eq!(
            certified.enroll_authority_service(&clock_profile),
            Err(DeviceClientError::InvalidProfile)
        );

        macro_rules! invoke {
            ($client:expr, $action:literal, $command:literal) => {{
                let observation = $client.observe(&room_id).unwrap();
                assert!(
                    observation.actions.iter().any(|item| item.id == $action),
                    "expected advertised action {} at revision {}",
                    $action,
                    observation.projection.current_revision
                );
                assert!(matches!(
                    $client
                        .invoke(&observation, $action, CommandId::new($command).unwrap(),)
                        .unwrap(),
                    DeviceActionResult::Committed { .. }
                ));
            }};
        }

        invoke!(host, "room-create", "service-create");
        invoke!(guest, "room-join", "service-join");
        let chat_observation = host.observe(&room_id).unwrap();
        assert_eq!(chat_observation.action_templates.len(), 1);
        assert_eq!(chat_observation.action_templates[0].id, "chat-send");
        assert!(matches!(
            host.invoke_payload(
                &chat_observation,
                &CommandPayload::Chat {
                    text: "hello from the certified desktop".to_owned(),
                },
                CommandId::new("service-chat").unwrap(),
            )
            .unwrap(),
            DeviceActionResult::Committed { .. }
        ));
        let guest_chat = guest.observe(&room_id).unwrap();
        assert!(matches!(
            guest_chat.chat_tail.as_slice(),
            [DeviceChatEntry { text, .. }] if text == "hello from the certified desktop"
        ));
        invoke!(host, "room-take-seat-0", "service-seat-host");
        invoke!(guest, "room-take-seat-1", "service-seat-guest");
        invoke!(host, "room-ready", "service-ready-host");
        invoke!(guest, "room-ready", "service-ready-guest");
        invoke!(host, "countdown-arm", "service-countdown-arm");

        let duration = std::time::Duration::from_secs(3);
        assert_eq!(
            certified
                .drive_authority_services_elapsed(2, std::time::Duration::ZERO, duration)
                .unwrap(),
            0
        );
        assert_eq!(
            certified
                .drive_authority_services_elapsed(2, std::time::Duration::from_secs(2), duration)
                .unwrap(),
            0
        );
        invoke!(guest, "countdown-abort", "service-abort");
        invoke!(host, "countdown-arm", "service-rearm");
        // Even if the scheduler did not see the intermediate Lobby, the new
        // arm event has a distinct generation and gets its own full delay.
        assert_eq!(
            certified
                .drive_authority_services_elapsed(2, std::time::Duration::from_secs(2), duration)
                .unwrap(),
            0
        );
        assert_eq!(
            certified
                .drive_authority_services_elapsed(2, std::time::Duration::from_secs(3), duration)
                .unwrap(),
            0
        );
        assert_eq!(
            certified
                .drive_authority_services_elapsed(2, std::time::Duration::from_secs(5), duration)
                .unwrap(),
            2
        );
        let observation = host.observe(&room_id).unwrap();
        assert_eq!(observation.projection.current_revision, 12);
        assert_eq!(observation.projection.payload.phase, RoomPhase::Running);
        assert!(matches!(
            observation
                .projection
                .payload
                .public_game_state
                .as_ref()
                .map(|game| game.phase),
            Some(PublicGamePhase::Bidding)
        ));
        assert_eq!(certified.drive_authority_services(2).unwrap(), 0);
        invoke!(spectator, "room-join", "strict-spectator-join");
        let view = spectator.observe(&room_id).unwrap();
        assert_eq!(view.physical_hands.len(), 2);
        assert!(view.physical_hands.iter().all(|card| card.face.is_none()));
        assert_eq!(
            view.physical_hands
                .iter()
                .map(|card| &card.id)
                .collect::<Vec<_>>(),
            observation
                .physical_hands
                .iter()
                .map(|card| &card.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            observation
                .physical_hands
                .iter()
                .filter(|card| card.face.is_some())
                .count(),
            1
        );
        let mut leaked = view.clone();
        leaked.physical_hands[0].face = Some(0);
        assert_eq!(
            leaked.validate_for(&spectator_profile, &room_id),
            Err(DeviceClientError::InvalidObservation)
        );
        assert!(view.projection.payload.own_hand.is_none());
        assert!(view.projection.payload.granted_hands.is_empty());
        assert!(!view.actions.iter().any(|action| matches!(
            action.payload,
            CommandPayload::RequestHand { .. } | CommandPayload::GrantHand { .. }
        )));
        // Bypass the UI's action list with a correctly signed handcrafted RPC.
        // The authority must enforce the same policy, not merely hide a button.
        let wire = DeviceActionRequest {
            room_id: room_id.clone(),
            session_epoch: view.projection.session_epoch,
            command_id: CommandId::new("forged-hand-request").unwrap(),
            player_id: spectator_profile.player_id.clone(),
            device_id: spectator_profile.device_id.clone(),
            expected_revision: view.projection.current_revision,
            expected_projection_hash: view.projection_hash,
            action_id: "hand-request-0".to_owned(),
            payload: CommandPayload::RequestHand {
                player: observation.projection.principal_id,
            },
        }
        .sign(
            &spectator_profile,
            &TestDeviceSigner(SigningKey::from_bytes(&[49; 32])),
        )
        .unwrap();
        assert_eq!(
            certified.invoke(&wire),
            Err(DeviceClientError::UnknownAction)
        );
        let after = spectator.observe(&room_id).unwrap();
        assert_eq!(
            after.projection.current_revision,
            view.projection.current_revision
        );
        assert!(after.projection.payload.granted_hands.is_empty());
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the acceptance test keeps certificate, advertisement, request, response, replay, and revision evidence together"
    )]
    fn exact_target_capture_is_signed_replay_safe_and_non_authoritative() {
        let root_key = SigningKey::from_bytes(&[9; 32]);
        let requester_key = SigningKey::from_bytes(&[10; 32]);
        let provider_key = SigningKey::from_bytes(&[11; 32]);
        let requester = signed_profile(
            "capture-requester",
            &root_key,
            &requester_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::RequestCapture,
            ],
        );
        let provider = signed_profile(
            "capture-provider",
            &root_key,
            &provider_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ProvideCapture,
            ],
        );
        let room = RoomId::new("signed-capture-room").unwrap();
        let state = SessionState::pending(
            room.clone(),
            PrincipalId::new("clock").unwrap(),
            PrincipalId::new("game").unwrap(),
        );
        let adapter = RuntimeLoopbackDeviceAdapter::new(
            state,
            CreateRoomActions,
            LoopbackCodec::CanonicalNdjson,
        );
        adapter.enroll(&provider).unwrap();
        let mut certified = CertifiedDeviceRoom::new(adapter.clone());
        certified.ensure_enrolled(&requester.certificate).unwrap();
        adapter.set_cooperation_now_unix_ms(100).unwrap();

        let advertisement_unsigned = UnsignedCaptureProviderAdvertisementWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: provider.player_id.clone(),
            provider_device_id: provider.device_id.clone(),
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
            consent_policy: CaptureConsentPolicyWire::HarnessOnly,
            max_total_bytes: 8 * 1024 * 1024,
            advertisement_sequence: 1,
            expires_at_unix_ms: 1_000,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: provider.device_id.clone(),
            },
        };
        let advertisement_signature = wire_signature(
            &canonical_capture_provider_advertisement_bytes(&advertisement_unsigned).unwrap(),
            &provider_key,
        );
        let advertisement = advertisement_unsigned
            .attach_signature(advertisement_signature)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        adapter
            .register_capture_provider(
                &provider,
                advertisement,
                AcceptingCaptureHandler {
                    key: provider_key,
                    calls: Arc::clone(&calls),
                },
            )
            .unwrap();

        let mut requester_client = PlayerDeviceClient::new(
            requester.clone(),
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .unwrap();
        let observed = requester_client.observe(&room).unwrap();
        requester_client
            .invoke(
                &observed,
                "create-room",
                CommandId::new("signed-room-create").unwrap(),
            )
            .unwrap();
        let observed = requester_client.observe(&room).unwrap();
        let request_unsigned = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new("signed-capture-request").unwrap(),
            room_id: room,
            membership_epoch: 1,
            player_id: requester.player_id.clone(),
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            observed_revision: observed.projection.current_revision,
            expires_at_unix_ms: 1_000,
            replay_nonce: "signed-capture-nonce".to_owned(),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            viewport: None,
            label: "signed capture".to_owned(),
            max_total_bytes: 8 * 1024 * 1024,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: requester.device_id.clone(),
            },
        };
        let request_signature = wire_signature(
            &canonical_capture_request_bytes(&request_unsigned).unwrap(),
            &requester_key,
        );
        let request = request_unsigned
            .attach_signature(request_signature)
            .unwrap();
        let revision_before = adapter.revision();
        let result = certified
            .cooperate(
                &requester.certificate,
                &provider.device_id,
                DeviceCooperationRequest::Capture(request.clone()),
            )
            .unwrap();
        let DeviceCooperationResult::Capture(response) = result;
        assert!(matches!(
            response.outcome,
            CaptureResponseOutcomeWire::Accepted { ref artifacts }
                if artifacts.len() == 1 && artifacts[0].captured_revision == 1
        ));
        assert_eq!(adapter.revision(), revision_before);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut after_disconnect_unsigned = request.unsigned();
        after_disconnect_unsigned.request_id =
            CaptureRequestId::new("signed-capture-after-disconnect").unwrap();
        after_disconnect_unsigned.replay_nonce = "signed-capture-after-disconnect-nonce".to_owned();
        let after_disconnect_signature = wire_signature(
            &canonical_capture_request_bytes(&after_disconnect_unsigned).unwrap(),
            &requester_key,
        );
        let after_disconnect = after_disconnect_unsigned
            .attach_signature(after_disconnect_signature)
            .unwrap();
        assert_eq!(
            certified.cooperate(
                &requester.certificate,
                &provider.device_id,
                DeviceCooperationRequest::Capture(request)
            ),
            Err(DeviceClientError::AuthorizationDenied)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        adapter
            .change_route(
                &provider,
                &after_disconnect.room_id,
                DeviceRouteOperationWire::Disconnect,
            )
            .unwrap();
        assert_eq!(
            certified.cooperate(
                &requester.certificate,
                &provider.device_id,
                DeviceCooperationRequest::Capture(after_disconnect.clone())
            ),
            Err(DeviceClientError::TransportUnavailable)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        adapter
            .change_route(
                &provider,
                &after_disconnect.room_id,
                DeviceRouteOperationWire::Rebind,
            )
            .unwrap();
        assert!(
            certified
                .cooperate(
                    &requester.certificate,
                    &provider.device_id,
                    DeviceCooperationRequest::Capture(after_disconnect)
                )
                .is_ok()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
