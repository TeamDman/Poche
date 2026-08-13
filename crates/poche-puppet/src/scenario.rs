// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{fmt::Write as _, time::Instant};

use poche_player_client::{
    AdvertisedAction, DeviceActionResult, DeviceObservation, DeviceProfile,
    LoopbackDeviceTransport, PlayerDeviceClient,
};
use poche_protocol::{
    CertificateId, CommandId, CommandPayload, CountdownToken, DeviceCapabilityWire,
    DeviceCustodyWire, DeviceId, InviteProof, PrincipalId, ProjectionEnvelope,
    REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, RoomId, RoomPhase,
    SignatureAlgorithm, SignatureBytes, SignatureIntent, UnsignedDeviceCertificateWire,
};
use poche_runtime::{
    AdvertisedActionSource, LoopbackCodec, OracleGameActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::{InviteRecord, SessionPhase, SessionState};
use serde::{Deserialize, Serialize};

use crate::{PuppetError, PuppetErrorCode, PuppetRunOptions, PuppetTransport};

type Adapter = RuntimeLoopbackDeviceAdapter<OracleSessionGame<2>, PuppetActionSource>;
type Client = PlayerDeviceClient<LoopbackDeviceTransport<Adapter>>;

#[derive(Clone)]
struct PuppetActionSource {
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
    pub artifact_directory: String,
    pub evidence_boundary: String,
}

pub(crate) fn run_two_player_full_round(
    options: &PuppetRunOptions,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PuppetRunReport, PuppetError> {
    if cancelled() {
        return Err(PuppetError::new(
            PuppetErrorCode::Cancelled,
            "puppet run was cancelled",
        ));
    }
    let fixture = Fixture::new(options.seed, options.transport)?;
    let room_id = fixture.room_id.clone();
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

    build_report(options, &room_id, &mut devices, steps)
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
    devices: Vec<HarnessDevice>,
}

impl Fixture {
    fn new(seed: u64, transport: PuppetTransport) -> Result<Self, PuppetError> {
        let alice = principal("11")?;
        let bob = principal("22")?;
        let spectator = principal("33")?;
        let authority_clock = principal("44")?;
        let game_environment = principal("55")?;
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
        let profiles = [
            (
                "alice-agent",
                "player-policy",
                alice,
                "a1",
                DeviceCustodyWire::NativeLocal,
            ),
            (
                "alice-browser",
                "player-sibling",
                principal("11")?,
                "a2",
                DeviceCustodyWire::BrowserLocal,
            ),
            (
                "bob-agent",
                "player-policy",
                bob,
                "b1",
                DeviceCustodyWire::NativeLocal,
            ),
            (
                "bob-native",
                "player-sibling",
                principal("22")?,
                "b2",
                DeviceCustodyWire::NativeLocal,
            ),
            (
                "spectator-browser",
                "spectator",
                spectator,
                "c1",
                DeviceCustodyWire::BrowserLocal,
            ),
            (
                "authority-clock",
                "authority-clock",
                authority_clock,
                "d1",
                DeviceCustodyWire::NativeLocal,
            ),
            (
                "game-environment",
                "game-environment",
                game_environment,
                "e1",
                DeviceCustodyWire::NativeLocal,
            ),
        ];
        let mut devices = Vec::with_capacity(profiles.len());
        for (label, role, player, device_octet, custody) in profiles {
            let profile = profile(label, player, device_octet, custody)?;
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
        Ok(Self { room_id, devices })
    }
}

fn principal(octet: &str) -> Result<PrincipalId, PuppetError> {
    PrincipalId::new(octet.repeat(32)).map_err(|_| invalid_fixture())
}

fn profile(
    label: &str,
    player_id: PrincipalId,
    device_octet: &str,
    custody: DeviceCustodyWire,
) -> Result<DeviceProfile, PuppetError> {
    let device_key = device_octet.repeat(32);
    let device_id = DeviceId::new(device_key.clone()).map_err(|_| invalid_fixture())?;
    let certificate = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new(format!("puppet-{label}"))
            .map_err(|_| invalid_fixture())?,
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_key,
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities: vec![
            DeviceCapabilityWire::Propose,
            DeviceCapabilityWire::ReceivePrivateProjection,
        ],
        custody,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: player_id.clone(),
        },
    }
    .attach_signature(SignatureBytes::new("00".repeat(64)).map_err(|_| invalid_fixture())?)
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

const fn invalid_fixture() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::InvalidFixture,
        "deterministic puppet fixture is structurally invalid",
    )
}
