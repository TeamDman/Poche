// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{fmt::Write as _, time::Instant};

use ed25519_dalek::{Signer, SigningKey};
use poche_player_client::{
    AdvertisedAction, DeviceActionResult, DeviceObservation, DeviceProfile,
    LoopbackDeviceTransport, PlayerDeviceClient,
};
use poche_protocol::{
    CertificateId, CommandId, CommandPayload, CountdownToken, DeviceCapabilityWire,
    DeviceCustodyWire, DeviceId, InviteProof, PrincipalId, ProjectionEnvelope,
    REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, RoomId, RoomPhase,
    SignatureAlgorithm, SignatureBytes, SignatureIntent, UnsignedDeviceCertificateWire,
    canonical_device_certificate_bytes,
};
use poche_runtime::{
    AdvertisedActionSource, LoopbackCodec, OracleGameActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::{InviteRecord, SessionPhase, SessionState};
use serde::{Deserialize, Serialize};

use crate::{
    PuppetError, PuppetErrorCode, PuppetRunOptions, PuppetSurface, PuppetTransport,
    native::{PendingPuppetCapture, capture_terminal_projection},
};

pub(crate) type Adapter = RuntimeLoopbackDeviceAdapter<OracleSessionGame<2>, PuppetActionSource>;
pub(crate) type Client = PlayerDeviceClient<LoopbackDeviceTransport<Adapter>>;

#[derive(Clone)]
pub(crate) struct PuppetActionSource {
    game: OracleGameActionSource,
    creator: PrincipalId,
    authority_clock: PrincipalId,
    game_environment: PrincipalId,
    invites: Vec<(PrincipalId, String)>,
    seats: Vec<(PrincipalId, u8)>,
}

impl AdvertisedActionSource<OracleSessionGame<2>> for PuppetActionSource {
    fn actions(
        &self,
        state: &SessionState<OracleSessionGame<2>>,
        projection: &ProjectionEnvelope,
    ) -> Result<Vec<AdvertisedAction>, poche_player_client::DeviceClientError> {
        let principal = &projection.principal_id;
        if principal == &self.authority_clock {
            return Ok(match &state.phase {
                SessionPhase::Countdown { token, .. } => vec![advertised(
                    "countdown-expire",
                    "Expire the deterministic countdown",
                    CommandPayload::CountdownExpired {
                        countdown_token: token.clone(),
                    },
                )],
                _ => Vec::new(),
            });
        }
        if principal == &self.game_environment {
            return self.game.actions(state, projection);
        }
        if state.members.is_empty() {
            return Ok(if principal == &self.creator {
                vec![advertised(
                    "room-create",
                    "Create the puppet room",
                    CommandPayload::CreateRoom,
                )]
            } else {
                Vec::new()
            });
        }
        let Some(member) = state.member(principal) else {
            return Ok(self
                .invites
                .iter()
                .find(|(candidate, _)| candidate == principal)
                .and_then(|(_, invite)| InviteProof::new(invite.clone()).ok())
                .map_or_else(Vec::new, |invite| {
                    vec![advertised(
                        "room-join",
                        "Join the puppet room",
                        CommandPayload::RedeemInvite { invite },
                    )]
                }));
        };
        if matches!(state.phase, SessionPhase::Lobby) {
            if let Some((_, seat)) = self
                .seats
                .iter()
                .find(|(candidate, _)| candidate == principal)
            {
                if member.seat != Some(*seat) {
                    return Ok(vec![advertised(
                        "room-take-seat",
                        "Take the configured puppet seat",
                        CommandPayload::TakeSeat { seat: *seat },
                    )]);
                }
                if !member.ready {
                    return Ok(vec![advertised(
                        "room-ready",
                        "Ready the configured puppet player",
                        CommandPayload::Ready,
                    )]);
                }
            }
            if principal == &self.creator && self.all_players_ready(state) {
                return Ok(vec![advertised(
                    "countdown-arm",
                    "Arm the deterministic countdown",
                    CommandPayload::ArmCountdown {
                        deadline_tick: 1,
                        countdown_token: CountdownToken::new("puppet-countdown").map_err(|_| {
                            poche_player_client::DeviceClientError::ProtocolViolation
                        })?,
                    },
                )]);
            }
        }
        self.game.actions(state, projection)
    }
}

impl PuppetActionSource {
    fn all_players_ready(&self, state: &SessionState<OracleSessionGame<2>>) -> bool {
        self.seats.iter().all(|(principal, seat)| {
            state
                .member(principal)
                .is_some_and(|member| member.seat == Some(*seat) && member.ready)
        })
    }
}

fn advertised(id: &str, label: &str, payload: CommandPayload) -> AdvertisedAction {
    AdvertisedAction {
        id: id.to_owned(),
        label: label.to_owned(),
        payload,
    }
}

struct HarnessDevice {
    label: &'static str,
    role: &'static str,
    client: Client,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRevisionEvidence {
    pub device: String,
    pub role: String,
    pub principal_id: String,
    pub device_id: String,
    pub revision: u64,
    pub projection_hash: String,
    pub room_phase: String,
    pub advertised_actions: usize,
    pub own_hand_cards: usize,
    pub public_history_events: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuppetStepEvidence {
    pub status: String,
    pub sequence: u32,
    pub command_id: String,
    pub acting_device: String,
    pub acting_principal: String,
    pub observed_revision: u64,
    pub observed_projection_hash: String,
    pub action_id: String,
    pub action_label: String,
    pub committed_revision: u64,
    pub observed_by: Vec<DeviceRevisionEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuppetDeviceEvidence {
    pub label: String,
    pub role: String,
    pub principal_id: String,
    pub device_id: String,
    pub final_revision: u64,
    pub final_projection_hash: String,
    pub final_room_phase: String,
}

/// Inspectable binding between one signed cross-device request, its bounded
/// private transfer, and the artifact later persisted by the requester.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuppetCaptureEvidence {
    pub status: String,
    pub label: String,
    pub request_id: String,
    pub requester_device_id: String,
    pub provider_device_id: String,
    pub requested_revision: u64,
    pub captured_revision: u64,
    pub projection_hash: String,
    pub scene_hash: Option<String>,
    pub provider_kind: String,
    pub representation: String,
    pub windowless: bool,
    pub transferred_bytes: u64,
    pub transfer_chunks: u32,
    pub artifact_directory: String,
    pub manifest_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuppetRunReport {
    pub schema: String,
    pub run_id: String,
    pub scenario: String,
    pub surface: String,
    pub transport: String,
    pub seed: u64,
    pub status: String,
    pub room_id: String,
    pub final_revision: u64,
    pub final_room_phase: String,
    pub final_scores: Vec<u16>,
    pub public_history_events: usize,
    pub public_history_hash: String,
    pub step_count: u32,
    pub devices: Vec<PuppetDeviceEvidence>,
    pub steps: Vec<PuppetStepEvidence>,
    pub captures: Vec<PuppetCaptureEvidence>,
    pub artifact_directory: String,
    pub evidence_boundary: String,
}

pub(crate) struct PuppetExecution {
    pub report: PuppetRunReport,
    pub captures: Vec<PendingPuppetCapture>,
}

pub(crate) fn run_two_player_full_round(
    options: &PuppetRunOptions,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PuppetExecution, PuppetError> {
    if cancelled() {
        return Err(PuppetError::new(
            PuppetErrorCode::Cancelled,
            "puppet run was cancelled",
        ));
    }
    let fixture = Fixture::new(options.seed, options.transport)?;
    let room_id = fixture.room_id.clone();
    let adapter = fixture.adapter;
    let capture_identity = fixture.capture_identity;
    let mut devices = fixture.devices;
    let started = Instant::now();
    let mut steps = Vec::new();

    loop {
        ensure_running(options, started, cancelled, steps.len())?;
        let Some((actor_index, observation, action)) = next_action(&mut devices, &room_id)? else {
            return Err(PuppetError::new(
                PuppetErrorCode::NoAction,
                "no certified device advertised the next semantic action",
            ));
        };
        let sequence = u32::try_from(steps.len()).map_err(|_| {
            PuppetError::new(PuppetErrorCode::StepLimit, "puppet step count overflowed")
        })?;
        let command_id_text = format!("puppet-{sequence:04}");
        let command_id = CommandId::new(command_id_text.clone()).map_err(|_| {
            PuppetError::new(
                PuppetErrorCode::InvalidFixture,
                "puppet command identity is invalid",
            )
        })?;
        let action_started = Instant::now();
        let result = devices[actor_index]
            .client
            .invoke(&observation, &action.id, command_id)
            .map_err(device_error)?;
        let committed_revision = match result {
            DeviceActionResult::Committed { revision, .. } => revision,
            DeviceActionResult::Denied { .. } => {
                return Err(PuppetError::new(
                    PuppetErrorCode::ActionDenied,
                    "an advertised puppet action was denied",
                ));
            }
        };
        let witnesses = observe_all(&mut devices, &room_id)?;
        if action_started.elapsed() > options.per_action_timeout {
            return Err(PuppetError::new(
                PuppetErrorCode::ActionTimeout,
                "puppet action exceeded its semantic deadline",
            ));
        }
        if witnesses
            .iter()
            .any(|witness| witness.revision != committed_revision)
        {
            return Err(PuppetError::new(
                PuppetErrorCode::DeviceProtocol,
                "enrolled devices did not converge on the committed revision",
            ));
        }
        let terminal = witnesses
            .iter()
            .all(|witness| witness.room_phase == "post_game");
        steps.push(PuppetStepEvidence {
            status: "complete".to_owned(),
            sequence,
            command_id: command_id_text,
            acting_device: devices[actor_index].label.to_owned(),
            acting_principal: observation.projection.principal_id.as_str().to_owned(),
            observed_revision: observation.projection.current_revision,
            observed_projection_hash: semantic_hash(&observation),
            action_id: action.id,
            action_label: action.label,
            committed_revision,
            observed_by: witnesses,
        });
        if terminal {
            break;
        }
    }

    let captures =
        capture_for_surface(options, &room_id, &adapter, &mut devices, &capture_identity)?;
    let mut report = build_report(options, &room_id, &mut devices, steps)?;
    report.captures = captures
        .iter()
        .map(|capture| capture.evidence.clone())
        .collect();
    if !captures.is_empty() {
        "The full game is proved by exact certified-device observations and reducer commits. One terminal native view is additionally bound to an authorized same-player capture request, real windowless Bevy render target, encrypted bounded transfer, and requester-side shared artifact pipeline; intermediate native checkpoints and external-network transport are not claimed."
            .clone_into(&mut report.evidence_boundary);
    }
    Ok(PuppetExecution { report, captures })
}

fn capture_for_surface(
    options: &PuppetRunOptions,
    room_id: &RoomId,
    adapter: &Adapter,
    devices: &mut [HarnessDevice],
    identity: &CaptureIdentity,
) -> Result<Vec<PendingPuppetCapture>, PuppetError> {
    match options.surface {
        PuppetSurface::Headless => Ok(Vec::new()),
        PuppetSurface::Native => {
            let requester = devices
                .iter_mut()
                .find(|device| device.label == "alice-agent")
                .ok_or_else(invalid_fixture)?;
            Ok(vec![capture_terminal_projection(
                options,
                room_id,
                adapter,
                &mut requester.client,
                &identity.requester_key,
                &identity.provider_profile,
                &identity.provider_key,
            )?])
        }
    }
}

fn ensure_running(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    steps: usize,
) -> Result<(), PuppetError> {
    if cancelled() {
        return Err(PuppetError::new(
            PuppetErrorCode::Cancelled,
            "puppet run was cancelled",
        ));
    }
    if started.elapsed() > options.whole_run_timeout {
        return Err(PuppetError::new(
            PuppetErrorCode::RunTimeout,
            "puppet run exceeded its whole-run watchdog",
        ));
    }
    if steps
        >= usize::try_from(options.max_steps).map_err(|_| {
            PuppetError::new(PuppetErrorCode::StepLimit, "puppet step limit is invalid")
        })?
    {
        return Err(PuppetError::new(
            PuppetErrorCode::StepLimit,
            "puppet run exceeded its semantic step limit",
        ));
    }
    Ok(())
}

fn next_action(
    devices: &mut [HarnessDevice],
    room_id: &RoomId,
) -> Result<Option<(usize, DeviceObservation, AdvertisedAction)>, PuppetError> {
    for (index, device) in devices.iter_mut().enumerate() {
        let observation = device.client.observe(room_id).map_err(device_error)?;
        if let Some(action) = observation.actions.first().cloned() {
            return Ok(Some((index, observation, action)));
        }
    }
    Ok(None)
}

fn observe_all(
    devices: &mut [HarnessDevice],
    room_id: &RoomId,
) -> Result<Vec<DeviceRevisionEvidence>, PuppetError> {
    devices
        .iter_mut()
        .map(|device| {
            let observation = device.client.observe(room_id).map_err(device_error)?;
            Ok(DeviceRevisionEvidence {
                device: device.label.to_owned(),
                role: device.role.to_owned(),
                principal_id: observation.projection.principal_id.as_str().to_owned(),
                device_id: device.client.profile().device_id.as_str().to_owned(),
                revision: observation.projection.current_revision,
                projection_hash: semantic_hash(&observation),
                room_phase: room_phase(observation.projection.payload.phase).to_owned(),
                advertised_actions: observation.actions.len(),
                own_hand_cards: observation
                    .projection
                    .payload
                    .own_hand
                    .as_ref()
                    .map_or(0, |hand| hand.cards.len()),
                public_history_events: observation.projection.payload.public_history.len(),
            })
        })
        .collect()
}

fn build_report(
    options: &PuppetRunOptions,
    room_id: &RoomId,
    devices: &mut [HarnessDevice],
    steps: Vec<PuppetStepEvidence>,
) -> Result<PuppetRunReport, PuppetError> {
    let final_observations = observe_all(devices, room_id)?;
    let terminal = devices
        .first_mut()
        .ok_or_else(|| {
            PuppetError::new(
                PuppetErrorCode::InvalidFixture,
                "puppet fixture contains no devices",
            )
        })?
        .client
        .observe(room_id)
        .map_err(device_error)?;
    let game = terminal
        .projection
        .payload
        .public_game_state
        .as_ref()
        .ok_or_else(|| {
            PuppetError::new(
                PuppetErrorCode::DeviceProtocol,
                "terminal puppet projection omitted public game state",
            )
        })?;
    let history_bytes =
        serde_json::to_vec(&terminal.projection.payload.public_history).map_err(|_| {
            PuppetError::new(
                PuppetErrorCode::DeviceProtocol,
                "public history could not be encoded",
            )
        })?;
    let final_revision = terminal.projection.current_revision;
    let step_count = u32::try_from(steps.len()).map_err(|_| {
        PuppetError::new(PuppetErrorCode::StepLimit, "puppet step count overflowed")
    })?;
    let devices = final_observations
        .into_iter()
        .map(|observation| PuppetDeviceEvidence {
            label: observation.device,
            role: observation.role,
            principal_id: observation.principal_id,
            device_id: observation.device_id,
            final_revision: observation.revision,
            final_projection_hash: observation.projection_hash,
            final_room_phase: observation.room_phase,
        })
        .collect();
    Ok(PuppetRunReport {
        schema: "poche.puppet.run.v1".to_owned(),
        run_id: format!("{}-seed-{}", options.scenario, options.seed),
        scenario: options.scenario.clone(),
        surface: options.surface.as_str().to_owned(),
        transport: format!("certified-device-{}", options.transport.as_str()),
        seed: options.seed,
        status: "complete".to_owned(),
        room_id: room_id.as_str().to_owned(),
        final_revision,
        final_room_phase: room_phase(terminal.projection.payload.phase).to_owned(),
        final_scores: game.scores.clone(),
        public_history_events: terminal.projection.payload.public_history.len(),
        public_history_hash: blake3::hash(&history_bytes).to_hex().to_string(),
        step_count,
        devices,
        steps,
        captures: Vec::new(),
        artifact_directory: String::new(),
        evidence_boundary: "Headless evidence proves exact observations, advertised actions, reducer commits, and cross-device revision convergence; it contains no graphical-capture claim.".to_owned(),
    })
}

fn semantic_hash(observation: &DeviceObservation) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in observation.projection_hash.0 {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String is infallible");
    }
    encoded
}

const fn room_phase(phase: RoomPhase) -> &'static str {
    match phase {
        RoomPhase::Lobby => "lobby",
        RoomPhase::Countdown => "countdown",
        RoomPhase::Running => "running",
        RoomPhase::Paused => "paused",
        RoomPhase::PostGame => "post_game",
        RoomPhase::Closed => "closed",
    }
}

fn device_error(_: poche_player_client::DeviceClientError) -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "certified device client rejected the puppet operation",
    )
}

struct Fixture {
    room_id: RoomId,
    adapter: Adapter,
    devices: Vec<HarnessDevice>,
    capture_identity: CaptureIdentity,
}

struct CaptureIdentity {
    requester_key: SigningKey,
    provider_profile: DeviceProfile,
    provider_key: SigningKey,
}

impl Fixture {
    #[allow(
        clippy::too_many_lines,
        reason = "the deterministic seven-device fixture keeps every player, sibling, spectator, clock, and environment identity visibly enumerated"
    )]
    fn new(seed: u64, transport: PuppetTransport) -> Result<Self, PuppetError> {
        let alice_root = deterministic_key("alice-root", seed);
        let alice_requester_key = deterministic_key("alice-agent", seed);
        let alice_provider_key = deterministic_key("alice-native", seed);
        let bob_root = deterministic_key("bob-root", seed);
        let bob_agent_key = deterministic_key("bob-agent", seed);
        let bob_native_key = deterministic_key("bob-native", seed);
        let spectator_root = deterministic_key("spectator-root", seed);
        let spectator_key = deterministic_key("spectator-browser", seed);
        let clock_root = deterministic_key("authority-clock-root", seed);
        let clock_key = deterministic_key("authority-clock", seed);
        let environment_root = deterministic_key("game-environment-root", seed);
        let environment_key = deterministic_key("game-environment", seed);
        let alice = principal_for_key(&alice_root)?;
        let bob = principal_for_key(&bob_root)?;
        let spectator = principal_for_key(&spectator_root)?;
        let authority_clock = principal_for_key(&clock_root)?;
        let game_environment = principal_for_key(&environment_root)?;
        let room_id = RoomId::new(format!("puppet-room-{seed}")).map_err(|_| invalid_fixture())?;
        let mut state = SessionState::pending(
            room_id.clone(),
            authority_clock.clone(),
            game_environment.clone(),
        );
        for invite in ["puppet-bob-invite", "puppet-spectator-invite"] {
            state
                .invites
                .push(InviteRecord::new(invite, u64::MAX).map_err(|_| invalid_fixture())?);
        }
        let source = PuppetActionSource {
            game: OracleGameActionSource::new(seed),
            creator: alice.clone(),
            authority_clock: authority_clock.clone(),
            game_environment: game_environment.clone(),
            invites: vec![
                (bob.clone(), "puppet-bob-invite".to_owned()),
                (spectator.clone(), "puppet-spectator-invite".to_owned()),
            ],
            seats: vec![(alice.clone(), 0), (bob.clone(), 1)],
        };
        let codec = match transport {
            PuppetTransport::LoopbackTyped => LoopbackCodec::Typed,
            PuppetTransport::LoopbackNdjson => LoopbackCodec::CanonicalNdjson,
        };
        let adapter = RuntimeLoopbackDeviceAdapter::new(state, source, codec);
        let requester_profile = signed_profile(
            "alice-agent",
            &alice_root,
            &alice_requester_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
                DeviceCapabilityWire::RequestCapture,
            ],
            DeviceCustodyWire::NativeLocal,
        )?;
        let provider_profile = signed_profile(
            "alice-native",
            &alice_root,
            &alice_provider_key,
            vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
                DeviceCapabilityWire::ProvideCapture,
            ],
            DeviceCustodyWire::NativeLocal,
        )?;
        let profiles = vec![
            ("alice-agent", "player-policy", requester_profile),
            ("alice-native", "player-sibling", provider_profile.clone()),
            (
                "bob-agent",
                "player-policy",
                signed_profile(
                    "bob-agent",
                    &bob_root,
                    &bob_agent_key,
                    vec![
                        DeviceCapabilityWire::Propose,
                        DeviceCapabilityWire::ReceivePrivateProjection,
                    ],
                    DeviceCustodyWire::NativeLocal,
                )?,
            ),
            (
                "bob-native",
                "player-sibling",
                signed_profile(
                    "bob-native",
                    &bob_root,
                    &bob_native_key,
                    vec![
                        DeviceCapabilityWire::Propose,
                        DeviceCapabilityWire::ReceivePrivateProjection,
                    ],
                    DeviceCustodyWire::NativeLocal,
                )?,
            ),
            (
                "spectator-browser",
                "spectator",
                signed_profile(
                    "spectator-browser",
                    &spectator_root,
                    &spectator_key,
                    vec![
                        DeviceCapabilityWire::Propose,
                        DeviceCapabilityWire::ReceivePrivateProjection,
                    ],
                    DeviceCustodyWire::BrowserLocal,
                )?,
            ),
            (
                "authority-clock",
                "authority-clock",
                signed_profile(
                    "authority-clock",
                    &clock_root,
                    &clock_key,
                    vec![DeviceCapabilityWire::Propose],
                    DeviceCustodyWire::NativeLocal,
                )?,
            ),
            (
                "game-environment",
                "game-environment",
                signed_profile(
                    "game-environment",
                    &environment_root,
                    &environment_key,
                    vec![DeviceCapabilityWire::Propose],
                    DeviceCustodyWire::NativeLocal,
                )?,
            ),
        ];
        let mut devices = Vec::with_capacity(profiles.len());
        for (label, role, profile) in profiles {
            adapter.enroll(&profile).map_err(device_error)?;
            let client =
                PlayerDeviceClient::new(profile, LoopbackDeviceTransport::new(adapter.clone()))
                    .map_err(device_error)?;
            devices.push(HarnessDevice {
                label,
                role,
                client,
            });
        }
        Ok(Self {
            room_id,
            adapter,
            devices,
            capture_identity: CaptureIdentity {
                requester_key: alice_requester_key,
                provider_profile,
                provider_key: alice_provider_key,
            },
        })
    }
}

fn deterministic_key(label: &str, seed: u64) -> SigningKey {
    let mut hasher = blake3::Hasher::new_derive_key("poche/puppet-device-key/v1");
    hasher.update(label.as_bytes());
    hasher.update(&seed.to_be_bytes());
    SigningKey::from_bytes(hasher.finalize().as_bytes())
}

fn principal_for_key(key: &SigningKey) -> Result<PrincipalId, PuppetError> {
    PrincipalId::new(hex(&key.verifying_key().to_bytes())).map_err(|_| invalid_fixture())
}

fn signed_profile(
    label: &str,
    root_key: &SigningKey,
    device_key: &SigningKey,
    capabilities: Vec<DeviceCapabilityWire>,
    custody: DeviceCustodyWire,
) -> Result<DeviceProfile, PuppetError> {
    let player_key = hex(&root_key.verifying_key().to_bytes());
    let device_public_key = hex(&device_key.verifying_key().to_bytes());
    let player_id = PrincipalId::new(player_key).map_err(|_| invalid_fixture())?;
    let device_id = DeviceId::new(device_public_key.clone()).map_err(|_| invalid_fixture())?;
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new(format!("puppet-{label}"))
            .map_err(|_| invalid_fixture())?,
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_public_key,
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities,
        custody,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: player_id.clone(),
        },
    };
    let bytes = canonical_device_certificate_bytes(&unsigned).map_err(|_| invalid_fixture())?;
    let signature = SignatureBytes::new(hex(&root_key.sign(&bytes).to_bytes()))
        .map_err(|_| invalid_fixture())?;
    let certificate = unsigned
        .attach_signature(signature)
        .map_err(|_| invalid_fixture())?;
    Ok(DeviceProfile {
        schema_version: DeviceProfile::SCHEMA_VERSION_V1,
        label: label.to_owned(),
        player_id,
        device_id,
        certificate,
        signing_key_handle: format!("puppet-protected:{label}"),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        },
    )
}

const fn invalid_fixture() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::InvalidFixture,
        "deterministic puppet fixture is structurally invalid",
    )
}
