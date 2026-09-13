//! Opt-in file control for disposable, ad-hoc exploration of a live Poche
//! device. Requests use the same typed `SpacetimeDB` bridge as human input; this
//! endpoint cannot mutate server state or reveal another player's private hand.

#![allow(
    clippy::collapsible_if,
    clippy::single_match,
    clippy::single_match_else
)]

use super::{PoseDisplay, RenderMode, RenderSurface, UiState};
use bevy::{prelude::*, render::view::screenshot::save_to_disk};
use poche_bevy_spacetimedb::{BridgeHandle, BridgeIntent, BridgeModel};
use poche_spacetimedb_client::{ClientConfig, RoomCapability, valid_join_code};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: u16 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);
const APP_REQUEST_TIMEOUT: Duration = Duration::from_secs(18);
const CAPTURE_PREROLL_FRAMES: u64 = 8;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct FileControlOptions {
    pub root: PathBuf,
    pub instance_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlDescriptor {
    pub schema_version: u16,
    pub instance_id: String,
    pub process_id: u32,
    pub started_unix_ms: u64,
    pub surface: String,
    pub transport_boundary: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileControlRequest {
    pub schema_version: u16,
    pub request_id: String,
    pub sequence: u64,
    pub action: FileControlAction,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileControlAction {
    Observe {
        #[serde(default)]
        include_join_code: bool,
    },
    SetName {
        name: String,
    },
    CreateLobby,
    JoinLobby {
        join_code: String,
    },
    TakeSeat {
        seat: u8,
    },
    ReleaseSeat,
    MoveOwnCard {
        card_index: usize,
        position_mm: [i32; 3],
        rotation_mdeg: [i32; 3],
    },
    Capture,
    Stop,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileControlStatus {
    Completed,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileControlResponse {
    pub schema_version: u16,
    pub request_id: String,
    pub sequence: u64,
    pub status: FileControlStatus,
    pub request_elapsed_ms: f64,
    pub observation: FileControlObservation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileControlObservation {
    pub privacy: String,
    pub instance_id: String,
    pub surface: String,
    pub connected: bool,
    pub status: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_seat: Option<u8>,
    pub own_hand: Vec<FileControlHandCard>,
    pub members: Vec<FileControlMember>,
    pub card_poses: Vec<FileControlCardPose>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_authority_latency_ms: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlHandCard {
    pub card_key: String,
    pub card_id: String,
    pub face: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlMember {
    pub identity: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<u8>,
    pub connected: bool,
    pub is_self: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlCardPose {
    pub card_key: String,
    pub card_id: String,
    pub owner: String,
    pub owner_seat: u8,
    pub logical_location: String,
    pub position_mm: [i32; 3],
    pub rotation_mdeg: [i32; 3],
    pub sequence: u64,
    pub is_own: bool,
}

pub struct FileControlPlugin {
    endpoint: FileControlEndpoint,
}

impl FileControlPlugin {
    /// Claim a fresh root and publish an immutable endpoint descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if the root is not fresh or cannot be initialized.
    pub fn prepare(options: FileControlOptions, mode: RenderMode) -> Result<Self, String> {
        validate_identifier(&options.instance_id, "instance identifier")?;
        if let Some(parent) = options.root.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create control parent: {error}"))?;
        }
        fs::create_dir(&options.root)
            .map_err(|error| format!("developer control root must be fresh: {error}"))?;
        for child in [
            "requests",
            "processing",
            "processed",
            "responses",
            "captures",
        ] {
            fs::create_dir(options.root.join(child))
                .map_err(|error| format!("could not create control directory: {error}"))?;
        }
        let descriptor = FileControlDescriptor {
            schema_version: SCHEMA_VERSION,
            instance_id: options.instance_id.clone(),
            process_id: std::process::id(),
            started_unix_ms: unix_millis()?,
            surface: match mode {
                RenderMode::Windowed => "interactive_window",
                RenderMode::WindowlessImage => "windowless_image",
            }
            .into(),
            transport_boundary: "local developer input only; SpacetimeDB carries game traffic"
                .into(),
            capabilities: vec![
                "observe_private_local".into(),
                "create_or_join_as_device".into(),
                "take_or_release_seat".into(),
                "move_owned_card".into(),
                "capture_gpu".into(),
                "stop".into(),
            ],
        };
        atomic_json(&options.root.join("instance.json"), &descriptor)?;
        Ok(Self {
            endpoint: FileControlEndpoint {
                root: options.root,
                instance_id: options.instance_id,
                last_sequence: None,
                frame: 0,
                pending: None,
            },
        })
    }
}

impl Plugin for FileControlPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.endpoint.clone())
            .add_systems(Last, drive_file_control);
    }
}

#[derive(Clone, Resource)]
struct FileControlEndpoint {
    root: PathBuf,
    instance_id: String,
    last_sequence: Option<u64>,
    frame: u64,
    pending: Option<PendingRequest>,
}

#[derive(Clone)]
struct PendingRequest {
    request: FileControlRequest,
    processing_path: PathBuf,
    started: Instant,
    completion: Completion,
}

#[derive(Clone)]
enum Completion {
    Immediate,
    RoomJoined,
    Seat(Option<u8>),
    Pose {
        card_key: String,
        sequence: u64,
        position_mm: [i32; 3],
        rotation_mdeg: [i32; 3],
    },
    Capture {
        path: PathBuf,
        requested: bool,
        ready_frame: u64,
    },
}

#[allow(clippy::too_many_arguments)]
fn drive_file_control(
    mut endpoint: ResMut<FileControlEndpoint>,
    mut state: ResMut<UiState>,
    model: Res<BridgeModel>,
    bridge: Res<BridgeHandle>,
    surface: Res<RenderSurface>,
    mut poses: ResMut<PoseDisplay>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    endpoint.frame = endpoint.frame.saturating_add(1);
    if let Some(mut pending) = endpoint.pending.take() {
        match pending_result(&mut pending, &model, endpoint.frame) {
            Some(result) => {
                let capture = match &pending.completion {
                    Completion::Capture { path, .. } => Some(path.to_string_lossy().into_owned()),
                    _ => None,
                };
                let status = if result.is_ok() {
                    FileControlStatus::Completed
                } else {
                    FileControlStatus::Rejected
                };
                let _ = complete_request(
                    &endpoint,
                    &pending,
                    status,
                    result.err(),
                    capture,
                    &state,
                    &model,
                );
            }
            None => {
                if let Completion::Capture {
                    path,
                    requested,
                    ready_frame,
                } = &mut pending.completion
                    && !*requested
                    && endpoint.frame >= *ready_frame
                {
                    if let Some(screenshot) = surface.screenshot() {
                        commands
                            .spawn(screenshot)
                            .observe(save_to_disk(path.clone()));
                        *requested = true;
                    }
                }
                endpoint.pending = Some(pending);
            }
        }
        return;
    }

    let claimed = match claim_next(&endpoint) {
        Ok(value) => value,
        Err(error) => {
            state.status = error;
            return;
        }
    };
    let Some((request, processing_path)) = claimed else {
        return;
    };
    let started = Instant::now();
    let action = request.action.clone();
    let invalid = validate_request(&endpoint, &request).err();
    endpoint.last_sequence = Some(request.sequence);
    let mut pending = PendingRequest {
        request,
        processing_path,
        started,
        completion: Completion::Immediate,
    };
    if let Some(error) = invalid {
        let _ = complete_request(
            &endpoint,
            &pending,
            FileControlStatus::Rejected,
            Some(error),
            None,
            &state,
            &model,
        );
        return;
    }

    let submission = match action {
        FileControlAction::Observe { .. } => Ok(()),
        FileControlAction::SetName { name } => {
            state.display_name = name;
            state.status = "Developer control set this device's profile name.".into();
            Ok(())
        }
        FileControlAction::CreateLobby => {
            let name = state.display_name.trim().to_owned();
            state.busy = true;
            state.status = "Connecting to the table authority…".into();
            pending.completion = Completion::RoomJoined;
            bridge.send(BridgeIntent::Create {
                config: ClientConfig::local(&name),
                display_name: name,
            })
        }
        FileControlAction::JoinLobby { join_code } => {
            let name = state.display_name.trim().to_owned();
            let code = join_code.trim().to_ascii_uppercase();
            state.capability = Some(RoomCapability {
                room_id: String::new(),
                join_code: code.clone(),
            });
            state.busy = true;
            state.status = "Joining the shared table…".into();
            pending.completion = Completion::RoomJoined;
            bridge.send(BridgeIntent::Join {
                config: ClientConfig::local(&name),
                display_name: name,
                join_code: code,
            })
        }
        FileControlAction::TakeSeat { seat } => model.snapshot.room_id().map_or_else(
            || Err("join a room before taking a seat".into()),
            |room_id| {
                pending.completion = Completion::Seat(Some(seat));
                bridge.send(BridgeIntent::TakeSeat {
                    room_id: room_id.into(),
                    seat,
                })
            },
        ),
        FileControlAction::ReleaseSeat => model.snapshot.room_id().map_or_else(
            || Err("join a room before releasing a seat".into()),
            |room_id| {
                pending.completion = Completion::Seat(None);
                bridge.send(BridgeIntent::ReleaseSeat {
                    room_id: room_id.into(),
                })
            },
        ),
        FileControlAction::MoveOwnCard {
            card_index,
            position_mm,
            rotation_mdeg,
        } => submit_pose(
            card_index,
            position_mm,
            rotation_mdeg,
            &model,
            &bridge,
            &mut poses,
            &mut pending,
        ),
        FileControlAction::Capture => {
            let path = endpoint
                .root
                .join("captures")
                .join(format!("{}.png", pending.request.request_id));
            pending.completion = Completion::Capture {
                path,
                requested: false,
                ready_frame: endpoint.frame.saturating_add(CAPTURE_PREROLL_FRAMES),
            };
            Ok(())
        }
        FileControlAction::Stop => {
            let _ = complete_request(
                &endpoint,
                &pending,
                FileControlStatus::Completed,
                None,
                None,
                &state,
                &model,
            );
            exit.write(AppExit::Success);
            return;
        }
    };

    if let Err(error) = submission {
        state.busy = false;
        state.status.clone_from(&error);
        let _ = complete_request(
            &endpoint,
            &pending,
            FileControlStatus::Rejected,
            Some(error),
            None,
            &state,
            &model,
        );
    } else if matches!(pending.completion, Completion::Immediate) {
        let _ = complete_request(
            &endpoint,
            &pending,
            FileControlStatus::Completed,
            None,
            None,
            &state,
            &model,
        );
    } else if let Completion::Capture {
        path,
        requested,
        ready_frame,
    } = &mut pending.completion
    {
        if endpoint.frame >= *ready_frame
            && let Some(screenshot) = surface.screenshot()
        {
            commands
                .spawn(screenshot)
                .observe(save_to_disk(path.clone()));
            *requested = true;
        }
        endpoint.pending = Some(pending);
    } else {
        endpoint.pending = Some(pending);
    }
}

fn pending_result(
    pending: &mut PendingRequest,
    model: &BridgeModel,
    frame: u64,
) -> Option<Result<(), String>> {
    if pending.started.elapsed() >= APP_REQUEST_TIMEOUT {
        return Some(Err(
            "live game did not reach the requested state before timeout".into(),
        ));
    }
    match &mut pending.completion {
        Completion::Immediate => Some(Ok(())),
        Completion::RoomJoined => model.snapshot.room_id().map(|_| Ok(())),
        Completion::Seat(expected) => (model.snapshot.own_seat() == *expected).then_some(Ok(())),
        Completion::Pose {
            card_key,
            sequence,
            position_mm,
            rotation_mdeg,
        } => model
            .snapshot
            .card_poses
            .iter()
            .find(|pose| &pose.card_key == card_key)
            .filter(|pose| {
                pose.sequence >= *sequence
                    && pose.position_mm == *position_mm
                    && pose.rotation_mdeg == *rotation_mdeg
            })
            .map(|_| Ok(())),
        Completion::Capture {
            path,
            requested,
            ready_frame,
        } => {
            if !*requested && frame >= *ready_frame {
                // The screenshot is spawned by the system after this check so
                // it can borrow Commands and the render surface safely.
                None
            } else if *requested && path.exists() {
                Some(Ok(()))
            } else {
                None
            }
        }
    }
}

fn submit_pose(
    card_index: usize,
    position_mm: [i32; 3],
    rotation_mdeg: [i32; 3],
    model: &BridgeModel,
    bridge: &BridgeHandle,
    poses: &mut PoseDisplay,
    pending: &mut PendingRequest,
) -> Result<(), String> {
    let room_id = model
        .snapshot
        .room_id()
        .ok_or("join a room before moving a card")?;
    let mut hand = model.snapshot.hand.iter().collect::<Vec<_>>();
    hand.sort_by(|left, right| left.card_key.cmp(&right.card_key));
    let card = hand
        .get(card_index)
        .ok_or("owned card index is outside this device's private hand")?;
    let network = model
        .snapshot
        .card_poses
        .iter()
        .find(|pose| pose.card_key == card.card_key)
        .ok_or("owned card has no public physical pose")?;
    let sequence = network.sequence.saturating_add(1);
    if let Some(display) = poses.0.get_mut(&card.card_key) {
        display.current = position_mm.map(|value| value as f32);
        display.target = display.current;
        display.rotation_mdeg = rotation_mdeg;
        display.sequence = sequence;
    }
    pending.completion = Completion::Pose {
        card_key: card.card_key.clone(),
        sequence,
        position_mm,
        rotation_mdeg,
    };
    bridge.send(BridgeIntent::SetCardPose {
        room_id: room_id.into(),
        card_id: card.card_id.clone(),
        sequence,
        position_mm,
        rotation_mdeg,
    })
}

fn validate_request(
    endpoint: &FileControlEndpoint,
    request: &FileControlRequest,
) -> Result<(), String> {
    if request.schema_version != SCHEMA_VERSION {
        return Err("request schema version is unsupported".into());
    }
    validate_identifier(&request.request_id, "request identifier")?;
    if endpoint
        .last_sequence
        .is_some_and(|last| request.sequence <= last)
    {
        return Err("request sequence is stale".into());
    }
    match &request.action {
        FileControlAction::SetName { name } => {
            if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
                return Err("profile name must contain 1–32 visible characters".into());
            }
        }
        FileControlAction::CreateLobby if endpoint.pending.is_some() => {
            return Err("another request is already pending".into());
        }
        FileControlAction::JoinLobby { join_code } if !valid_join_code(join_code.trim()) => {
            return Err("join code has the wrong shape".into());
        }
        FileControlAction::TakeSeat { seat } if *seat > 1 => {
            return Err("seat must be 0 or 1".into());
        }
        FileControlAction::MoveOwnCard {
            position_mm,
            rotation_mdeg,
            ..
        } if position_mm
            .iter()
            .any(|value| value.unsigned_abs() > 10_000)
            || rotation_mdeg
                .iter()
                .any(|value| value.unsigned_abs() > 360_000) =>
        {
            return Err("pose is outside the server's bounded physical space".into());
        }
        _ => {}
    }
    Ok(())
}

fn claim_next(
    endpoint: &FileControlEndpoint,
) -> Result<Option<(FileControlRequest, PathBuf)>, String> {
    let mut paths = fs::read_dir(endpoint.root.join("requests"))
        .map_err(|error| format!("request directory is unavailable: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let Some(path) = paths.into_iter().next() else {
        return Ok(None);
    };
    if fs::metadata(&path)
        .map_err(|error| format!("request metadata is unavailable: {error}"))?
        .len()
        > MAX_REQUEST_BYTES
    {
        return Err("request exceeds the developer-control bound".into());
    }
    let file_name = path.file_name().ok_or("request has no file name")?;
    let processing = endpoint.root.join("processing").join(file_name);
    fs::rename(&path, &processing)
        .map_err(|error| format!("request could not be claimed: {error}"))?;
    let request = serde_json::from_slice(
        &fs::read(&processing).map_err(|error| format!("request could not be read: {error}"))?,
    )
    .map_err(|error| format!("request is not valid JSON: {error}"))?;
    Ok(Some((request, processing)))
}

#[allow(clippy::too_many_arguments)]
fn complete_request(
    endpoint: &FileControlEndpoint,
    pending: &PendingRequest,
    status: FileControlStatus,
    error: Option<String>,
    capture_path: Option<String>,
    state: &UiState,
    model: &BridgeModel,
) -> Result<(), String> {
    let include_join_code = matches!(
        pending.request.action,
        FileControlAction::Observe {
            include_join_code: true
        }
    );
    let response = FileControlResponse {
        schema_version: SCHEMA_VERSION,
        request_id: pending.request.request_id.clone(),
        sequence: pending.request.sequence,
        status,
        request_elapsed_ms: pending.started.elapsed().as_secs_f64() * 1_000.,
        observation: observation(endpoint, state, model, include_join_code),
        error,
        capture_path,
    };
    atomic_json(
        &endpoint
            .root
            .join("responses")
            .join(format!("{}.json", pending.request.request_id)),
        &response,
    )?;
    let file_name = pending
        .processing_path
        .file_name()
        .ok_or("processing request has no file name")?;
    fs::rename(
        &pending.processing_path,
        endpoint.root.join("processed").join(file_name),
    )
    .map_err(|error| format!("processed request could not be archived: {error}"))?;
    Ok(())
}

fn observation(
    endpoint: &FileControlEndpoint,
    state: &UiState,
    model: &BridgeModel,
    include_join_code: bool,
) -> FileControlObservation {
    let identity = model.snapshot.identity.as_deref();
    FileControlObservation {
        privacy: if include_join_code {
            "private local developer artifact; bearer lobby code explicitly included"
        } else {
            "private local developer artifact; bearer lobby code omitted"
        }
        .into(),
        instance_id: endpoint.instance_id.clone(),
        surface: if model.snapshot.room_id().is_some() {
            "table"
        } else {
            "main_menu"
        }
        .into(),
        connected: model.connected,
        status: state.status.clone(),
        display_name: state.display_name.clone(),
        join_code: include_join_code
            .then(|| {
                state
                    .capability
                    .as_ref()
                    .map(|value| value.join_code.clone())
            })
            .flatten(),
        room_id: model.snapshot.room_id().map(str::to_owned),
        viewer_identity: model.snapshot.identity.clone(),
        own_seat: model.snapshot.own_seat(),
        own_hand: model
            .snapshot
            .hand
            .iter()
            .map(|card| FileControlHandCard {
                card_key: card.card_key.clone(),
                card_id: card.card_id.clone(),
                face: card.face.clone(),
            })
            .collect(),
        members: model
            .snapshot
            .members
            .iter()
            .map(|member| FileControlMember {
                identity: member.identity.clone(),
                display_name: member.display_name.clone(),
                seat: member.seat,
                connected: member.connected,
                is_self: member.is_self,
            })
            .collect(),
        card_poses: model
            .snapshot
            .card_poses
            .iter()
            .map(|pose| FileControlCardPose {
                card_key: pose.card_key.clone(),
                card_id: pose.card_id.clone(),
                owner: pose.owner.clone(),
                owner_seat: pose.owner_seat,
                logical_location: pose.logical_location.clone(),
                position_mm: pose.position_mm,
                rotation_mdeg: pose.rotation_mdeg,
                sequence: pose.sequence,
                is_own: Some(pose.owner.as_str()) == identity,
            })
            .collect(),
        last_authority_latency_ms: model
            .last_command_latency
            .map(|value| value.as_secs_f64() * 1_000.),
    }
}

/// Atomically send one request to a live instance and wait for its correlated
/// response. This is also the building block used by the acceptance puppet.
///
/// # Errors
///
/// Returns if the instance, request, or response is unavailable or invalid.
pub fn send_file_control_request(
    root: &Path,
    action: FileControlAction,
    timeout: Option<Duration>,
) -> Result<FileControlResponse, String> {
    let descriptor: FileControlDescriptor = serde_json::from_slice(
        &fs::read(root.join("instance.json"))
            .map_err(|error| format!("live descriptor is unavailable: {error}"))?,
    )
    .map_err(|error| format!("live descriptor is invalid: {error}"))?;
    if descriptor.schema_version != SCHEMA_VERSION {
        return Err("live instance schema version is unsupported".into());
    }
    let request_id = random_identifier()?;
    let clock = unix_millis()?;
    let sequence = clock
        .saturating_mul(1_000_000)
        .saturating_add(REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed) % 1_000_000);
    let request = FileControlRequest {
        schema_version: SCHEMA_VERSION,
        request_id: request_id.clone(),
        sequence,
        action,
    };
    atomic_json(
        &root
            .join("requests")
            .join(format!("{sequence:020}-{request_id}.json")),
        &request,
    )?;
    let response_path = root.join("responses").join(format!("{request_id}.json"));
    let deadline = Instant::now() + timeout.unwrap_or(RESPONSE_TIMEOUT);
    loop {
        if let Ok(bytes) = fs::read(&response_path) {
            let response: FileControlResponse = serde_json::from_slice(&bytes)
                .map_err(|error| format!("control response is invalid: {error}"))?;
            if response.request_id != request_id || response.sequence != sequence {
                return Err("control response correlation failed".into());
            }
            return Ok(response);
        }
        if root.join("stopped.json").exists() {
            return Err("live instance stopped before responding".into());
        }
        if Instant::now() >= deadline {
            return Err("live instance did not respond before timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn mark_stopped(root: &Path) {
    let _ = atomic_json(
        &root.join("stopped.json"),
        &serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "process_id": std::process::id(),
            "stopped_unix_ms": unix_millis().unwrap_or_default(),
        }),
    );
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("developer-control path has no parent")?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("developer-control file name is invalid")?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", random_identifier()?));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("control JSON could not be encoded: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("control temporary file could not be created: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("control temporary file could not be written: {error}"))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("control file could not be published: {error}"))
}

fn random_identifier() -> Result<String, String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("random request identifier is unavailable: {error}"))?;
    let mut identifier = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        identifier.push(char::from(HEX[usize::from(byte >> 4)]));
        identifier.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(identifier)
}

fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(format!("{label} is invalid"))
    } else {
        Ok(())
    }
}

fn unix_millis() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system time is unavailable")?
            .as_millis(),
    )
    .map_err(|_| "system time does not fit milliseconds".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_root_is_fresh_and_declares_transport_boundary() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("alice");
        let plugin = FileControlPlugin::prepare(
            FileControlOptions {
                root: root.clone(),
                instance_id: "alice-window".into(),
            },
            RenderMode::WindowlessImage,
        )
        .unwrap();
        let descriptor: FileControlDescriptor =
            serde_json::from_slice(&fs::read(root.join("instance.json")).unwrap()).unwrap();
        assert_eq!(descriptor.surface, "windowless_image");
        assert!(descriptor.transport_boundary.contains("SpacetimeDB"));
        assert!(
            FileControlPlugin::prepare(
                FileControlOptions {
                    root,
                    instance_id: "alice-window".into(),
                },
                RenderMode::WindowlessImage,
            )
            .is_err()
        );
        drop(plugin);
    }

    #[test]
    fn identifiers_and_private_observation_flags_are_strict() {
        assert!(validate_identifier("alice-window", "instance").is_ok());
        assert!(validate_identifier("rm /s", "instance").is_err());
        let action: FileControlAction =
            serde_json::from_str(r#"{"kind":"observe","include_join_code":true}"#).unwrap();
        assert_eq!(
            action,
            FileControlAction::Observe {
                include_join_code: true
            }
        );
    }
}
