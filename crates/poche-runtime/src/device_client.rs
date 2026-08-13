// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Real reducer-backed loopback adapter for the shared player-device client.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use poche_player_client::{
    AdvertisedAction, DeviceActionRequest, DeviceActionResult, DeviceClientError,
    DeviceCooperationRequest, DeviceCooperationResult, DeviceObservation, DeviceProfile,
    LoopbackDeviceAuthority,
};
use poche_protocol::{
    CaptureProviderAdvertisementWire, CommandPayload, CorrelationId, CountdownToken,
    DeviceActionWire, DeviceCertificateWire, DeviceId, DeviceObservationRequestWire, EventId,
    GameActionWire, InviteProof, MemberProjection, PROTOCOL_VERSION_V1, ProjectionEnvelope,
    ProjectionId, ProjectionPayload, RoomId, RoomPhase, SIGNATURE_DOMAIN_V1, SemanticHash,
    SignatureAlgorithm, SignatureBytes, SignatureMetadata,
    canonical_capture_provider_advertisement_bytes, canonical_capture_request_bytes,
    canonical_capture_response_bytes, canonical_device_action_bytes,
    canonical_device_certificate_bytes, canonical_device_observation_request_bytes,
};
use poche_session::{
    CaptureAuthorizationContext, CaptureReplayWindow, GameTurn, SessionGame, SessionPhase,
    SessionState, authorize_capture_request_with_provider, authorize_capture_response,
    project_viewer,
};

use crate::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    ScriptedClient,
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
        })
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
        Ok(actions)
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
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError>;
}

/// Cloneable device adapter whose clones share one actual in-process authority.
/// Every invoke enters `InProcessAuthority::drive_all`; no test-only state
/// mutation path exists.
pub struct RuntimeLoopbackDeviceAdapter<G: SessionGame, A> {
    shared: Arc<Mutex<SharedLoopbackState<G, A>>>,
}

impl<G: SessionGame, A> Clone for RuntimeLoopbackDeviceAdapter<G, A> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<G: SessionGame, A: AdvertisedActionSource<G>> RuntimeLoopbackDeviceAdapter<G, A> {
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
            })),
        }
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

    /// Register an enrolled target device's signed capture advertisement and
    /// non-authoritative provider handler.
    ///
    /// # Errors
    ///
    /// Rejects an unenrolled/mismatched device, invalid certificate or
    /// advertisement signature, or duplicate provider registration.
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
            || shared.capture_providers.contains_key(&profile.device_id)
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
}

struct CachedRemoteObservation {
    request: DeviceObservationRequestWire,
    observation: DeviceObservation,
}

struct CachedRemoteAction {
    action: DeviceActionWire,
    result: DeviceActionResult,
}

/// Process-external certified-device boundary around one real reducer-backed
/// room adapter. HTTP and Veilid servers may share this service rather than
/// reimplementing certificate enrollment, signature verification, or replay.
pub struct CertifiedDeviceRoom<G: SessionGame, A> {
    adapter: RuntimeLoopbackDeviceAdapter<G, A>,
    profiles: BTreeMap<DeviceId, DeviceProfile>,
    observation_cache: BTreeMap<(DeviceId, String), CachedRemoteObservation>,
    action_cache: BTreeMap<(DeviceId, String), CachedRemoteAction>,
}

impl<G, A> CertifiedDeviceRoom<G, A>
where
    G: SessionGame + Send + 'static,
    G::Error: Send,
    A: AdvertisedActionSource<G>,
{
    #[must_use]
    pub fn new(adapter: RuntimeLoopbackDeviceAdapter<G, A>) -> Self {
        Self {
            adapter,
            profiles: BTreeMap::new(),
            observation_cache: BTreeMap::new(),
            action_cache: BTreeMap::new(),
        }
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
        let profile = self.ensure_enrolled(&request.certificate)?;
        let mut adapter = self.adapter.clone();
        let observation = match request.mode {
            poche_protocol::DeviceObservationModeWire::Snapshot => {
                adapter.observe(&profile, &request.room_id)?
            }
            poche_protocol::DeviceObservationModeWire::Wait { after_revision } => {
                adapter.wait(&profile, &request.room_id, after_revision)?
            }
        };
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
            .ok_or(DeviceClientError::UnknownAction)?;
        if action.payload != request.payload {
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
            .cooperate(request)?;
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
    verify_ed25519(
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
    if !verify_ed25519(
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
    verify_ed25519(
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
    verify_ed25519(
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
    verify_ed25519(
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
    verify_ed25519(
        &certificate.device_signing_public_key,
        response.signature.signature.as_str(),
        &bytes,
    )
    .then_some(())
    .ok_or(DeviceClientError::ProtocolViolation)
}

fn verify_ed25519(public_key: &str, signature: &str, bytes: &[u8]) -> bool {
    let Some(public_key) = decode_hex::<32>(public_key) else {
        return false;
    };
    let Some(signature) = decode_hex::<64>(signature) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    verifying_key
        .verify(bytes, &Signature::from_bytes(&signature))
        .is_ok()
}

fn decode_hex<const BYTES: usize>(encoded: &str) -> Option<[u8; BYTES]> {
    if encoded.len() != BYTES * 2 {
        return None;
    }
    let mut decoded = [0_u8; BYTES];
    for (index, byte) in decoded.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(encoded.get(offset..offset + 2)?, 16).ok()?;
    }
    Some(decoded)
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
    let payload = if profile.player_id == shared.authority.state.game_environment
        || profile.player_id == shared.authority.state.authority_clock
        || shared.authority.state.member(&profile.player_id).is_none()
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
    Ok(DeviceObservation {
        projection,
        projection_hash,
        actions,
    })
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ed25519_dalek::{Signer, SigningKey};
    use poche_player_client::{
        DeviceProfile, DeviceSigner, LoopbackDeviceTransport, PlayerDeviceClient,
        sign_observation_request,
    };
    use poche_protocol::{
        CaptureArtifactDescriptorWire, CaptureArtifactId, CaptureConsentPolicyWire,
        CapturePrivacyWire, CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
        CaptureResponseOutcomeWire, CaptureTransferDescriptorWire, CaptureTransferId,
        CaptureViewportWire, CertificateId, CommandId, CommandPayload,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        DeviceCapabilityWire, DeviceCustodyWire, DeviceObservationModeWire,
        DeviceSignatureIntentWire, PrincipalId, REPLICATION_SCHEMA_VERSION_V1,
        REPLICATION_SIGNATURE_DOMAIN_V1, SignatureIntent, UnsignedCaptureProviderAdvertisementWire,
        UnsignedCaptureRequestWire, UnsignedCaptureResponseWire, UnsignedDeviceCertificateWire,
        capture_request_hash,
    };

    use super::*;
    use crate::OracleSessionGame;

    struct CreateRoomActions;

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
        let state = SessionState::pending(
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
        assert_eq!(room.invoke(&signed_action).unwrap(), result);
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
        adapter.enroll(&requester).unwrap();
        adapter.enroll(&provider).unwrap();
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
        let result = requester_client
            .cooperate(
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
        assert_eq!(
            requester_client.cooperate(
                &provider.device_id,
                DeviceCooperationRequest::Capture(request)
            ),
            Err(DeviceClientError::AuthorizationDenied)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
