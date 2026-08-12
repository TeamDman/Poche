// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Real reducer-backed loopback adapter for the shared player-device client.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use poche_player_client::{
    AdvertisedAction, DeviceActionRequest, DeviceActionResult, DeviceClientError,
    DeviceCooperationRequest, DeviceCooperationResult, DeviceObservation, DeviceProfile,
    LoopbackDeviceAuthority,
};
use poche_protocol::{
    CommandPayload, CorrelationId, DeviceId, EventId, GameActionWire, MemberProjection,
    PROTOCOL_VERSION_V1, ProjectionEnvelope, ProjectionId, ProjectionPayload, RoomId, RoomPhase,
    SIGNATURE_DOMAIN_V1, SemanticHash, SignatureAlgorithm, SignatureBytes, SignatureMetadata,
};
use poche_session::{GameTurn, SessionGame, SessionPhase, SessionState, project_viewer};

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
    action_source: A,
    next_projection: u64,
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
        _profile: &DeviceProfile,
        _target_device: &DeviceId,
        _request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        Err(DeviceClientError::TransportUnavailable)
    }
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
    let payload = if profile.player_id == shared.authority.state.game_environment {
        environment_projection(&shared.authority.state)?
    } else if shared.authority.state.members.is_empty() {
        ProjectionPayload {
            phase: RoomPhase::Lobby,
            members: Vec::new(),
            public_game_state: None,
            own_hand: None,
            granted_hands: Vec::new(),
            public_history: Vec::new(),
        }
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

fn environment_projection<G: SessionGame>(
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
    use poche_player_client::{DeviceProfile, LoopbackDeviceTransport, PlayerDeviceClient};
    use poche_protocol::{
        CertificateId, CommandId, CommandPayload, DeviceCapabilityWire, DeviceCustodyWire,
        PrincipalId, REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1,
        SignatureIntent, UnsignedDeviceCertificateWire,
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
}
