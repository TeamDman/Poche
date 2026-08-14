// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Externally hosted certified-device puppet.
//!
//! Unlike the fast loopback scenario, every observation and action in this
//! module crosses a real Axum socket and both Ed25519 verification layers.
//! Rendering devices remain independent siblings of the policy devices; no
//! local-process mutation or borrowed reducer handle exists here.

use std::{thread, time::Instant};

use ed25519_dalek::{Signer as _, SigningKey};
use poche_player_client::{
    AdvertisedAction, AdvertisedActionPolicy, DeviceActionResult, DeviceObservation, DeviceProfile,
    DeviceSigner, DeviceTransport, HttpDeviceTransport, PlayerDeviceClient, PolicyScope,
};
use poche_protocol::{
    CommandId, DeviceCapabilityWire, DeviceCustodyWire, InviteProof, RoomId, RoomPhase,
    SignatureBytes,
};

use crate::{
    PuppetError, PuppetErrorCode, PuppetRunOptions, PuppetSurface, PuppetTransport,
    scenario::{
        Client, DeviceRevisionEvidence, PuppetDeviceEvidence, PuppetExecution, PuppetRunReport,
        PuppetStepEvidence, deterministic_key, device_error, hex, invalid_fixture, room_phase,
        semantic_hash, signed_profile,
    },
};

const ALICE_AGENT: usize = 0;
const ALICE_BROWSER: usize = 1;
const BOB_AGENT: usize = 3;
const BOB_NATIVE: usize = 4;

struct ExternalDevice {
    label: &'static str,
    role: &'static str,
    client: Client,
}

struct KeySigner(SigningKey);

impl DeviceSigner for KeySigner {
    fn sign_device_bytes(
        &self,
        _profile: &DeviceProfile,
        canonical_bytes: &[u8],
    ) -> Result<SignatureBytes, poche_player_client::DeviceClientError> {
        SignatureBytes::new(hex(&self.0.sign(canonical_bytes).to_bytes()))
            .map_err(|_| poche_player_client::DeviceClientError::SigningFailed)
    }
}

struct ExternalServer {
    runtime: tokio::runtime::Runtime,
    task: tokio::task::JoinHandle<()>,
    endpoint: String,
}

impl ExternalServer {
    fn start(seed: u64, room_id: &RoomId, invite: &str) -> Result<Self, PuppetError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| external_transport_error())?;
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .map_err(|_| external_transport_error())?;
        let address = listener
            .local_addr()
            .map_err(|_| external_transport_error())?;
        let config = poche_web_spike::CertifiedRoomConfig {
            room_id: room_id.as_str().to_owned(),
            invite: invite.to_owned(),
            game_seed: seed,
            seat_count: 2,
            countdown_deadline_tick: 1,
            countdown_token: format!("external-puppet-countdown-{seed}"),
        };
        let task = runtime.spawn(async move {
            if let Err(error) = poche_web_spike::serve_with_certified_room(listener, config).await {
                eprintln!("external Poche puppet server stopped: {error}");
            }
        });
        Ok(Self {
            runtime,
            task,
            endpoint: format!("http://{address}"),
        })
    }
}

impl Drop for ExternalServer {
    fn drop(&mut self) {
        self.task.abort();
        // Keeping the runtime as an owned field ensures the server and all
        // service tasks are bounded by this scenario's lifetime.
        let _ = &self.runtime;
    }
}

/// Complete one game across a process-shaped loopback HTTP carrier.
#[allow(
    clippy::too_many_lines,
    reason = "the vertical slice keeps lifecycle, graphical sibling takeover, terminal convergence, and evidence construction in one auditable orchestration"
)]
pub(crate) fn run_external_full_game(
    options: &PuppetRunOptions,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PuppetExecution, PuppetError> {
    if options.surface != PuppetSurface::Headless {
        return Err(PuppetError::new(
            PuppetErrorCode::UnsupportedSurface,
            "external graphical surfaces are not connected to the shared authority yet",
        ));
    }
    if options.transport != PuppetTransport::HttpLoopback {
        return Err(PuppetError::new(
            PuppetErrorCode::InvalidFixture,
            "the external-device scenario requires the HTTP loopback carrier",
        ));
    }
    if cancelled() {
        return Err(cancelled_error());
    }

    let room_id = RoomId::new(format!("external-puppet-room-{}", options.seed))
        .map_err(|_| invalid_fixture())?;
    let invite = format!("external-puppet-invite-{}", options.seed);
    let server = ExternalServer::start(options.seed, &room_id, &invite)?;
    let mut devices = external_devices(options.seed, &server.endpoint, &invite)?;
    let started = Instant::now();
    let mut steps = Vec::new();

    invoke_named(
        options,
        started,
        cancelled,
        &room_id,
        &mut devices,
        &mut steps,
        ALICE_AGENT,
        "room-create",
    )?;
    invoke_named(
        options,
        started,
        cancelled,
        &room_id,
        &mut devices,
        &mut steps,
        BOB_AGENT,
        "room-join",
    )?;
    for (device, action) in [
        (ALICE_AGENT, "room-take-seat-0"),
        (BOB_AGENT, "room-take-seat-1"),
        (ALICE_AGENT, "room-ready"),
        (BOB_AGENT, "room-ready"),
        (ALICE_AGENT, "countdown-arm"),
    ] {
        invoke_named(
            options,
            started,
            cancelled,
            &room_id,
            &mut devices,
            &mut steps,
            device,
            action,
        )?;
    }
    wait_for_phase(
        options,
        started,
        cancelled,
        &room_id,
        &mut devices,
        RoomPhase::Running,
    )?;

    let mut alice_graphical_takeover = false;
    let mut bob_graphical_takeover = false;
    loop {
        ensure_running(options, started, cancelled, steps.len())?;
        let (device_index, observation, action) = next_game_action(
            &room_id,
            &mut devices,
            options.seed,
            alice_graphical_takeover,
            bob_graphical_takeover,
        )?
        .ok_or_else(no_action_error)?;
        if device_index == ALICE_BROWSER {
            alice_graphical_takeover = true;
        }
        if device_index == BOB_NATIVE {
            bob_graphical_takeover = true;
        }
        invoke_observed(
            options,
            started,
            cancelled,
            &room_id,
            &mut devices,
            &mut steps,
            device_index,
            &observation,
            action,
        )?;
        let terminal = devices[ALICE_AGENT]
            .client
            .observe(&room_id)
            .map_err(device_error)?;
        if terminal.projection.payload.phase == RoomPhase::PostGame {
            break;
        }
    }
    if !alice_graphical_takeover || !bob_graphical_takeover {
        return Err(PuppetError::new(
            PuppetErrorCode::DeviceProtocol,
            "graphical sibling devices did not take over ordinary player actions",
        ));
    }

    let report = build_report(options, &room_id, &mut devices, steps)?;
    drop(server);
    Ok(PuppetExecution {
        report,
        captures: Vec::new(),
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the fixture intentionally enumerates every distinct player root, device key, custody class, certificate, and transport epoch"
)]
fn external_devices(
    seed: u64,
    endpoint: &str,
    invite: &str,
) -> Result<Vec<ExternalDevice>, PuppetError> {
    let alice_root = deterministic_key("external-alice-root", seed);
    let bob_root = deterministic_key("external-bob-root", seed);
    let alice_agent_key = deterministic_key("external-alice-agent", seed);
    let alice_browser_key = deterministic_key("external-alice-browser", seed);
    let alice_native_key = deterministic_key("external-alice-native", seed);
    let bob_agent_key = deterministic_key("external-bob-agent", seed);
    let bob_native_key = deterministic_key("external-bob-native", seed);
    let private_player = vec![
        DeviceCapabilityWire::Propose,
        DeviceCapabilityWire::ReceivePrivateProjection,
    ];
    let profiles = [
        (
            "alice-agent",
            "player-policy",
            signed_profile(
                "external-alice-agent",
                &alice_root,
                &alice_agent_key,
                private_player.clone(),
                DeviceCustodyWire::NativeLocal,
            )?,
            alice_agent_key,
            0,
            None,
        ),
        (
            "alice-browser",
            "player-browser",
            signed_profile(
                "external-alice-browser",
                &alice_root,
                &alice_browser_key,
                private_player.clone(),
                DeviceCustodyWire::BrowserLocal,
            )?,
            alice_browser_key,
            1,
            None,
        ),
        (
            "alice-native",
            "player-native",
            signed_profile(
                "external-alice-native",
                &alice_root,
                &alice_native_key,
                private_player.clone(),
                DeviceCustodyWire::NativeLocal,
            )?,
            alice_native_key,
            1,
            None,
        ),
        (
            "bob-agent",
            "player-policy",
            signed_profile(
                "external-bob-agent",
                &bob_root,
                &bob_agent_key,
                private_player.clone(),
                DeviceCustodyWire::NativeLocal,
            )?,
            bob_agent_key,
            1,
            Some(invite),
        ),
        (
            "bob-native",
            "player-native",
            signed_profile(
                "external-bob-native",
                &bob_root,
                &bob_native_key,
                private_player,
                DeviceCustodyWire::NativeLocal,
            )?,
            bob_native_key,
            1,
            None,
        ),
    ];
    profiles
        .into_iter()
        .map(|(label, role, profile, key, epoch, invite)| {
            let mut transport =
                HttpDeviceTransport::new(endpoint, epoch, KeySigner(key)).map_err(device_error)?;
            if let Some(invite) = invite {
                transport = transport.with_join_invite(
                    InviteProof::new(invite.to_owned()).map_err(|_| invalid_fixture())?,
                );
            }
            let client =
                PlayerDeviceClient::new(profile, Box::new(transport) as Box<dyn DeviceTransport>)
                    .map_err(device_error)?;
            Ok(ExternalDevice {
                label,
                role,
                client,
            })
        })
        .collect()
}

#[allow(
    clippy::too_many_arguments,
    reason = "the explicit action helper keeps the deadline, exact observation, actor, and evidence sinks visible"
)]
fn invoke_named(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    room_id: &RoomId,
    devices: &mut [ExternalDevice],
    steps: &mut Vec<PuppetStepEvidence>,
    device_index: usize,
    action_id: &str,
) -> Result<(), PuppetError> {
    ensure_running(options, started, cancelled, steps.len())?;
    let observation = devices[device_index]
        .client
        .observe(room_id)
        .map_err(device_error)?;
    let action = observation
        .action(action_id)
        .cloned()
        .ok_or_else(no_action_error)?;
    invoke_observed(
        options,
        started,
        cancelled,
        room_id,
        devices,
        steps,
        device_index,
        &observation,
        action,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "one external commit binds its actor, signed observation, command result, and converged device evidence"
)]
fn invoke_observed(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    room_id: &RoomId,
    devices: &mut [ExternalDevice],
    steps: &mut Vec<PuppetStepEvidence>,
    device_index: usize,
    observation: &DeviceObservation,
    action: AdvertisedAction,
) -> Result<(), PuppetError> {
    let sequence = u32::try_from(steps.len()).map_err(|_| step_limit_error())?;
    let command_id_text = format!("external-puppet-{sequence:04}");
    let command_id = CommandId::new(command_id_text.clone()).map_err(|_| invalid_fixture())?;
    let action_started = Instant::now();
    let result = devices[device_index]
        .client
        .invoke(observation, &action.id, command_id)
        .map_err(device_error)?;
    let committed_revision = match result {
        DeviceActionResult::Committed { revision, .. } => revision,
        DeviceActionResult::Denied { .. } => {
            return Err(PuppetError::new(
                PuppetErrorCode::ActionDenied,
                "an advertised external puppet action was denied",
            ));
        }
    };
    let witnesses = observe_converged(options, started, cancelled, room_id, devices)?;
    if action_started.elapsed() > options.per_action_timeout
        || witnesses
            .iter()
            .any(|witness| witness.revision < committed_revision)
    {
        return Err(PuppetError::new(
            PuppetErrorCode::ActionTimeout,
            "external puppet devices did not converge after an action deadline",
        ));
    }
    steps.push(PuppetStepEvidence {
        status: "complete".to_owned(),
        sequence,
        command_id: command_id_text,
        acting_device: devices[device_index].label.to_owned(),
        acting_principal: observation.projection.principal_id.as_str().to_owned(),
        observed_revision: observation.projection.current_revision,
        observed_projection_hash: semantic_hash(observation),
        action_id: action.id,
        action_label: action.label,
        committed_revision,
        observed_by: witnesses,
    });
    Ok(())
}

fn next_game_action(
    room_id: &RoomId,
    devices: &mut [ExternalDevice],
    seed: u64,
    alice_takeover_complete: bool,
    bob_takeover_complete: bool,
) -> Result<Option<(usize, DeviceObservation, AdvertisedAction)>, PuppetError> {
    let candidates = [
        if alice_takeover_complete {
            ALICE_AGENT
        } else {
            ALICE_BROWSER
        },
        if bob_takeover_complete {
            BOB_AGENT
        } else {
            BOB_NATIVE
        },
    ];
    for index in candidates {
        let observation = devices[index]
            .client
            .observe(room_id)
            .map_err(device_error)?;
        if let Some(action) = (AdvertisedActionPolicy::SeededRandom { seed })
            .select(&observation, PolicyScope::PlayerGameActions)
            .cloned()
        {
            return Ok(Some((index, observation, action)));
        }
    }
    Ok(None)
}

fn wait_for_phase(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    room_id: &RoomId,
    devices: &mut [ExternalDevice],
    target: RoomPhase,
) -> Result<(), PuppetError> {
    loop {
        ensure_running(options, started, cancelled, 0)?;
        let witnesses = observe_converged(options, started, cancelled, room_id, devices)?;
        if witnesses
            .iter()
            .all(|witness| witness.room_phase == room_phase(target))
        {
            return Ok(());
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn observe_converged(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    room_id: &RoomId,
    devices: &mut [ExternalDevice],
) -> Result<Vec<DeviceRevisionEvidence>, PuppetError> {
    let convergence_started = Instant::now();
    loop {
        ensure_running(options, started, cancelled, 0)?;
        let observations = observe_all(devices, room_id)?;
        if observations.first().is_some_and(|first| {
            observations
                .iter()
                .all(|item| item.revision == first.revision)
        }) {
            return Ok(observations);
        }
        if convergence_started.elapsed() > options.per_action_timeout {
            return Err(PuppetError::new(
                PuppetErrorCode::ActionTimeout,
                "external devices did not converge on one authority revision",
            ));
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn observe_all(
    devices: &mut [ExternalDevice],
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
    devices: &mut [ExternalDevice],
    steps: Vec<PuppetStepEvidence>,
) -> Result<PuppetRunReport, PuppetError> {
    let final_observations = observe_all(devices, room_id)?;
    let terminal = devices[ALICE_AGENT]
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
                "external terminal projection omitted public game state",
            )
        })?;
    let history = serde_json::to_vec(&terminal.projection.payload.public_history)
        .map_err(|_| external_protocol_error())?;
    let step_count = u32::try_from(steps.len()).map_err(|_| step_limit_error())?;
    Ok(PuppetRunReport {
        schema: "poche.puppet.run.v1".to_owned(),
        run_id: format!("{}-seed-{}", options.scenario, options.seed),
        scenario: options.scenario.clone(),
        surface: options.surface.as_str().to_owned(),
        transport: "certified-device-http-loopback".to_owned(),
        seed: options.seed,
        status: "complete".to_owned(),
        room_id: room_id.as_str().to_owned(),
        final_revision: terminal.projection.current_revision,
        final_room_phase: room_phase(terminal.projection.payload.phase).to_owned(),
        final_scores: game.scores.clone(),
        public_history_events: terminal.projection.payload.public_history.len(),
        public_history_hash: blake3::hash(&history).to_hex().to_string(),
        step_count,
        devices: final_observations
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
            .collect(),
        steps,
        captures: Vec::new(),
        artifact_directory: String::new(),
        evidence_boundary: "This run proves a complete game through one real Axum socket authority: root-certified policy, browser-custody, and native-custody sibling devices use signed exact-recipient HTTP observations and ordinary advertised actions; graphical pixels and external capture transfer are not yet claimed by this headless slice."
            .to_owned(),
    })
}

fn ensure_running(
    options: &PuppetRunOptions,
    started: Instant,
    cancelled: &mut impl FnMut() -> bool,
    steps: usize,
) -> Result<(), PuppetError> {
    if cancelled() {
        return Err(cancelled_error());
    }
    if started.elapsed() > options.whole_run_timeout {
        return Err(PuppetError::new(
            PuppetErrorCode::RunTimeout,
            "external puppet run exceeded its whole-run watchdog",
        ));
    }
    if steps >= usize::try_from(options.max_steps).map_err(|_| step_limit_error())? {
        return Err(step_limit_error());
    }
    Ok(())
}

const fn cancelled_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::Cancelled,
        "external puppet run was cancelled",
    )
}

const fn no_action_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::NoAction,
        "the expected external certified action was not advertised",
    )
}

const fn step_limit_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::StepLimit,
        "external puppet step count exceeded its configured bound",
    )
}

const fn external_transport_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "the external HTTP authority could not be started",
    )
}

const fn external_protocol_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "the external puppet evidence violated its protocol contract",
    )
}
