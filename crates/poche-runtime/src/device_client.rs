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
    CaptureProviderAdvertisementWire, CommandPayload, CorrelationId, DeviceCertificateWire,
    DeviceId, EventId, GameActionWire, MemberProjection, PROTOCOL_VERSION_V1, ProjectionEnvelope,
    ProjectionId, ProjectionPayload, RoomId, RoomPhase, SIGNATURE_DOMAIN_V1, SemanticHash,
    SignatureAlgorithm, SignatureBytes, SignatureMetadata,
    canonical_capture_provider_advertisement_bytes, canonical_capture_request_bytes,
    canonical_capture_response_bytes, canonical_device_certificate_bytes,
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
                // The host-authoritative session generation is zero-based;
                // replicated device certificates/cooperation wires are
                // deliberately one-based so zero remains invalid on wire.
                .and_then(|member| member.membership_epoch.checked_add(1))
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
    use poche_player_client::{DeviceProfile, LoopbackDeviceTransport, PlayerDeviceClient};
    use poche_protocol::{
        CaptureArtifactDescriptorWire, CaptureArtifactId, CaptureConsentPolicyWire,
        CapturePrivacyWire, CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
        CaptureResponseOutcomeWire, CaptureTransferDescriptorWire, CaptureTransferId,
        CaptureViewportWire, CertificateId, CommandId, CommandPayload,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        DeviceCapabilityWire, DeviceCustodyWire, DeviceSignatureIntentWire, PrincipalId,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureIntent,
        UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
        UnsignedCaptureResponseWire, UnsignedDeviceCertificateWire, capture_request_hash,
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
