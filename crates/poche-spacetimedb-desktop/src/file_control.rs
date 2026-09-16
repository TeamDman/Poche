//! Opt-in file control for disposable, ad-hoc exploration of a live Poche
//! device. Requests use the same typed `SpacetimeDB` bridge as human input; this
//! endpoint cannot mutate server state or reveal another player's private hand.

#![allow(
    clippy::collapsible_if,
    clippy::single_match,
    clippy::single_match_else
)]

use super::{
    AuthorityEndpoint, CameraOptions, EscapeMenuPage, IdentityVault, LeaveActivation, PendingFlow,
    PoseDisplay, RenderMode, RenderSurface, UiScreen, UiState, activate_leave,
    begin_identity_selection, toggle_escape_menu,
};
use bevy::{
    input::InputSystems,
    picking::{
        PickingSystems,
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    },
    prelude::*,
    render::view::screenshot::save_to_disk,
    window::PrimaryWindow,
};
use poche_bevy_spacetimedb::{BridgeHandle, BridgeIntent, BridgeModel};
use poche_spacetimedb_client::{RoomCapability, valid_join_code};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: u16 = 6;
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
    SelectIdentity {
        label: String,
    },
    ResumeLobby,
    ReturnToTitle,
    CreateLobby,
    JoinLobby {
        join_code: String,
    },
    TakeSeat {
        seat: u8,
    },
    ReleaseSeat,
    ToggleTableMenu,
    OpenOptions,
    ToggleCameraYInversion,
    CloseOptions,
    ActivateLeave,
    SetWindowMaximized {
        maximized: bool,
    },
    SetWindowMinimized {
        minimized: bool,
    },
    Bid {
        tricks: u8,
    },
    PlayOwnCard {
        card_index: usize,
    },
    MoveOwnCard {
        card_index: usize,
        position_mm: [i32; 3],
        rotation_mdeg: [i32; 3],
    },
    /// Windowless-only logical-pixel pointer input, through the ordinary drag
    /// and Bevy UI picking paths. Does not move the operating system cursor.
    Pointer {
        x: f32,
        y: f32,
        primary_down: bool,
    },
    /// Press/release a supported game-control key through ordinary Update input.
    Key {
        key: String,
        down: bool,
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
    pub rendered_card_count: usize,
    pub rendered_player_count: usize,
    pub held_card_key: Option<String>,
    pub visible_hand_copies: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<FileControlGame>,
    pub revealed_cards: Vec<FileControlRevealedCard>,
    pub activity: Vec<FileControlActivity>,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlGame {
    pub phase: String,
    pub actor_seat: Option<u8>,
    pub dealer_seat: Option<u8>,
    pub round_index: u16,
    pub hand_size: u8,
    pub hand_counts: [u8; 2],
    pub bids: [Option<u8>; 2],
    pub trick_count: u8,
    pub trick_seats: [Option<u8>; 2],
    pub tricks_won: [u8; 2],
    pub scores: [u16; 2],
    pub pot_cents: u32,
    pub trump: Option<u8>,
    pub action_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlRevealedCard {
    pub card_key: String,
    pub face: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlActivity {
    pub sequence: u64,
    pub kind: String,
    pub summary: String,
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
                "create_select_or_resume_identity".into(),
                "create_or_join_as_selected_identity".into(),
                "take_or_release_seat".into(),
                "toggle_table_menu".into(),
                "return_to_title_and_change_window_state".into(),
                "activate_leave_button".into(),
                "bid_or_play_owned_card".into(),
                "move_owned_card".into(),
                "windowless_pointer_and_game_keys".into(),
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
            .init_resource::<QueuedDeviceInput>()
            .add_systems(
                PreUpdate,
                apply_device_input
                    .after(InputSystems)
                    .before(PickingSystems::ProcessInput),
            )
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

#[derive(Resource, Default)]
struct QueuedDeviceInput {
    pending: Option<(u64, DeviceInput)>,
    completed: Option<(u64, Result<(), String>)>,
}

enum DeviceInput {
    Pointer { point: Vec2, primary_down: bool },
    Key { key: KeyCode, down: bool },
}

fn supported_key(name: &str) -> Result<KeyCode, String> {
    match name.to_ascii_lowercase().as_str() {
        "q" => Ok(KeyCode::KeyQ),
        "e" => Ok(KeyCode::KeyE),
        "o" => Ok(KeyCode::KeyO),
        "z" => Ok(KeyCode::KeyZ),
        "w" => Ok(KeyCode::KeyW),
        "a" => Ok(KeyCode::KeyA),
        "s" => Ok(KeyCode::KeyS),
        "d" => Ok(KeyCode::KeyD),
        "space" => Ok(KeyCode::Space),
        "arrowup" | "up" => Ok(KeyCode::ArrowUp),
        "arrowdown" | "down" => Ok(KeyCode::ArrowDown),
        "arrowleft" | "left" => Ok(KeyCode::ArrowLeft),
        "arrowright" | "right" => Ok(KeyCode::ArrowRight),
        "escape" | "esc" => Ok(KeyCode::Escape),
        "f3" => Ok(KeyCode::F3),
        _ => Err("supported keys: Q E O Z W A S D Space ArrowUp ArrowDown ArrowLeft ArrowRight Escape F3".into()),
    }
}

fn validate_pointer(point: Vec2, width: f32, height: f32) -> Result<(), String> {
    if !point.is_finite() || point.x < 0.0 || point.y < 0.0 || point.x >= width || point.y >= height
    {
        Err("pointer coordinates must be finite logical pixels within the current viewport".into())
    } else {
        Ok(())
    }
}

fn apply_device_input(
    mut queue: ResMut<QueuedDeviceInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    surface: Res<RenderSurface>,
    mut pointer_events: MessageWriter<PointerInput>,
) {
    let Some((sequence, input)) = queue.pending.take() else {
        return;
    };
    let result = match input {
        DeviceInput::Key { key, down } => {
            if down {
                keys.press(key);
            } else {
                keys.release(key);
            }
            Ok(())
        }
        DeviceInput::Pointer {
            point,
            primary_down,
        } => {
            if !matches!(surface.as_ref(), RenderSurface::Windowless { .. }) {
                Err("synthetic pointer input requires a windowless viewport".into())
            } else if let Ok((entity, mut window)) = windows.single_mut() {
                validate_pointer(point, window.width(), window.height()).and_then(|()| {
                    let target = surface
                        .render_target()
                        .and_then(|target| target.normalize(Some(entity)))
                        .ok_or("the windowless render target is not ready")?;
                    let previous = window.cursor_position().unwrap_or(point);
                    window.set_cursor_position(Some(point));
                    let location = Location {
                        target,
                        position: point,
                    };
                    pointer_events.write(PointerInput::new(
                        PointerId::Mouse,
                        location.clone(),
                        PointerAction::Move {
                            delta: point - previous,
                        },
                    ));
                    // Both custom card dragging and native Bevy button observers
                    // see the same edge. Holding a button does not repeat presses.
                    if primary_down != mouse.pressed(MouseButton::Left) {
                        let action = if primary_down {
                            mouse.press(MouseButton::Left);
                            PointerAction::Press(PointerButton::Primary)
                        } else {
                            mouse.release(MouseButton::Left);
                            PointerAction::Release(PointerButton::Primary)
                        };
                        pointer_events.write(PointerInput::new(PointerId::Mouse, location, action));
                    }
                    Ok(())
                })
            } else {
                Err("the primary viewport is unavailable".into())
            }
        }
    };
    // Last observes this only after the ordinary Update systems have consumed
    // the injected input. This acknowledges input processing, not server acceptance.
    queue.completed = Some((sequence, result));
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
    Input(u64),
    IdentitySelected,
    RoomCreated,
    RoomJoined,
    RoomLeft,
    Seat(Option<u8>),
    Pose {
        card_key: String,
        sequence: u64,
        position_mm: [i32; 3],
        rotation_mdeg: [i32; 3],
    },
    GameAction(u64),
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
    authority: Res<AuthorityEndpoint>,
    mut vault: ResMut<IdentityVault>,
    mut camera_options: ResMut<CameraOptions>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut device_input: ResMut<QueuedDeviceInput>,
) {
    endpoint.frame = endpoint.frame.saturating_add(1);
    if let Some(mut pending) = endpoint.pending.take() {
        match pending_result(&mut pending, &state, &model, endpoint.frame, &device_input) {
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
            let account = vault.reload().and_then(|()| {
                let existing = vault
                    .accounts_for(&authority.uri, &authority.database)
                    .into_iter()
                    .find(|account| account.label.eq_ignore_ascii_case(&name))
                    .cloned();
                existing.map_or_else(
                    || vault.create(&name, &authority.uri, &authority.database),
                    Ok,
                )
            });
            account.and_then(|account| {
                pending.completion = Completion::IdentitySelected;
                begin_identity_selection(&mut state, &account, &bridge, &authority)
            })
        }
        FileControlAction::SelectIdentity { label } => vault
            .reload()
            .and_then(|()| {
                vault
                    .accounts_for(&authority.uri, &authority.database)
                    .into_iter()
                    .find(|account| account.label.eq_ignore_ascii_case(&label))
                    .cloned()
                    .ok_or_else(|| "the requested identity is absent from this vault".into())
            })
            .and_then(|account| {
                pending.completion = Completion::IdentitySelected;
                begin_identity_selection(&mut state, &account, &bridge, &authority)
            }),
        FileControlAction::ResumeLobby => {
            if model.snapshot.room_id().is_some() {
                pending.completion = Completion::RoomJoined;
                super::begin_room_loading(
                    &mut state,
                    "Developer control is synchronizing the resumed lobby…",
                );
                Ok(())
            } else {
                Err("the selected identity has no active lobby to resume".into())
            }
        }
        FileControlAction::ReturnToTitle => {
            if state.screen == UiScreen::LobbyEnded {
                state.screen = UiScreen::MainMenu;
                state.status = "Ready to rejoin a recent lobby or create another one.".into();
                Ok(())
            } else {
                Err("return to title is available only after leaving a lobby".into())
            }
        }
        FileControlAction::CreateLobby => {
            if !model.connected || state.active_account_id.is_none() {
                Err("select an identity before creating a lobby".into())
            } else {
                state.pending_flow = Some(PendingFlow::CreateRoom);
                super::begin_room_loading(
                    &mut state,
                    "Developer control is creating and synchronizing a shared table…",
                );
                pending.completion = Completion::RoomCreated;
                bridge.send(BridgeIntent::Create {
                    display_name: state.display_name.clone(),
                })
            }
        }
        FileControlAction::JoinLobby { join_code } => {
            if !model.connected || state.active_account_id.is_none() {
                Err("select an identity before joining a lobby".into())
            } else {
                let code = join_code.trim().to_ascii_uppercase();
                state.capability = Some(RoomCapability {
                    room_id: String::new(),
                    join_code: code.clone(),
                });
                state.pending_flow = Some(PendingFlow::JoinRoom);
                super::begin_room_loading(
                    &mut state,
                    "Developer control is joining and synchronizing the shared table…",
                );
                pending.completion = Completion::RoomJoined;
                bridge.send(BridgeIntent::Join {
                    display_name: state.display_name.clone(),
                    join_code: code,
                })
            }
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
        FileControlAction::ToggleTableMenu => {
            if model.snapshot.room_id().is_none() {
                Err("join a room before opening its table menu".into())
            } else {
                toggle_escape_menu(&mut state);
                Ok(())
            }
        }
        FileControlAction::OpenOptions => {
            if state.escape_menu_open {
                state.escape_menu_page = EscapeMenuPage::Options;
                state.confirm_leave = false;
                state.status = "Developer control opened camera options.".into();
                Ok(())
            } else {
                Err("open the table menu before opening options".into())
            }
        }
        FileControlAction::ToggleCameraYInversion => {
            if !state.escape_menu_open || state.escape_menu_page != EscapeMenuPage::Options {
                Err("open camera options before toggling Y inversion".into())
            } else {
                camera_options.invert_y = !camera_options.invert_y;
                state.status = format!(
                    "{}. This affects RMB vertical orbit.",
                    camera_options.invert_y_label()
                );
                Ok(())
            }
        }
        FileControlAction::CloseOptions => {
            if !state.escape_menu_open || state.escape_menu_page != EscapeMenuPage::Options {
                Err("camera options are not open".into())
            } else {
                state.escape_menu_page = EscapeMenuPage::Main;
                state.status = "Developer control returned to the table menu.".into();
                Ok(())
            }
        }
        FileControlAction::ActivateLeave => {
            match activate_leave(&mut state, model.snapshot.room_id(), &bridge) {
                Ok(LeaveActivation::ConfirmationArmed) => Ok(()),
                Ok(LeaveActivation::Submitted) => {
                    pending.completion = Completion::RoomLeft;
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        FileControlAction::SetWindowMaximized { maximized } => {
            if !matches!(surface.as_ref(), RenderSurface::Windowed) {
                Err("window maximize is available only on an interactive window".into())
            } else if let Ok(mut window) = windows.single_mut() {
                window.set_maximized(maximized);
                state.status = if maximized {
                    "Maximizing the game window…".into()
                } else {
                    "Restoring the game window…".into()
                };
                Ok(())
            } else {
                Err("the primary game window is unavailable".into())
            }
        }
        FileControlAction::SetWindowMinimized { minimized } => {
            if !matches!(surface.as_ref(), RenderSurface::Windowed) {
                Err("window minimize is available only on an interactive window".into())
            } else if let Ok(mut window) = windows.single_mut() {
                window.set_minimized(minimized);
                state.status = if minimized {
                    "Minimizing the game window…".into()
                } else {
                    "Restoring the game window…".into()
                };
                Ok(())
            } else {
                Err("the primary game window is unavailable".into())
            }
        }
        FileControlAction::Bid { tricks } => submit_game_action(
            &model,
            &mut pending,
            |room_id| BridgeIntent::Bid { room_id, tricks },
            &bridge,
        ),
        FileControlAction::PlayOwnCard { card_index } => {
            submit_play_action(card_index, &model, &mut pending, &bridge)
        }
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
        FileControlAction::Pointer { x, y, primary_down } => {
            let point = Vec2::new(x, y);
            let valid = if matches!(surface.as_ref(), RenderSurface::Windowless { .. }) {
                windows
                    .single()
                    .map_err(|_| "the primary viewport is unavailable".into())
                    .and_then(|window| validate_pointer(point, window.width(), window.height()))
            } else {
                Err(
                    "synthetic pointer input requires --windowless so it cannot move the OS cursor"
                        .into(),
                )
            };
            valid.map(|()| {
                device_input.pending = Some((
                    pending.request.sequence,
                    DeviceInput::Pointer {
                        point,
                        primary_down,
                    },
                ));
                pending.completion = Completion::Input(pending.request.sequence);
            })
        }
        FileControlAction::Key { key, down } => supported_key(&key).map(|key| {
            device_input.pending = Some((pending.request.sequence, DeviceInput::Key { key, down }));
            pending.completion = Completion::Input(pending.request.sequence);
        }),
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
    state: &UiState,
    model: &BridgeModel,
    frame: u64,
    device_input: &QueuedDeviceInput,
) -> Option<Result<(), String>> {
    if pending.started.elapsed() >= APP_REQUEST_TIMEOUT {
        return Some(Err(
            "live game did not reach the requested state before timeout".into(),
        ));
    }
    match &mut pending.completion {
        Completion::Immediate => Some(Ok(())),
        Completion::Input(sequence) => device_input
            .completed
            .as_ref()
            .filter(|(completed, _)| completed == sequence)
            .map(|(_, result)| result.clone()),
        Completion::IdentitySelected => {
            if model.connected && matches!(state.screen, UiScreen::MainMenu | UiScreen::ResumeOffer)
            {
                Some(Ok(()))
            } else if !state.busy && state.pending_flow != Some(PendingFlow::SelectIdentity) {
                Some(Err(state.status.clone()))
            } else {
                None
            }
        }
        Completion::RoomCreated => {
            if state.screen == UiScreen::Table
                && model.snapshot.room_id().is_some()
                && state.capability.is_some()
            {
                Some(Ok(()))
            } else if state.screen == UiScreen::Table && model.snapshot.room_id().is_some() {
                // The room row and creator capability are delivered by independent bridge
                // notices. A creator request is not complete until both have arrived.
                None
            } else if !state.busy && !matches!(state.pending_flow, Some(PendingFlow::CreateRoom)) {
                Some(Err(state.status.clone()))
            } else {
                None
            }
        }
        Completion::RoomJoined => {
            if state.screen == UiScreen::Table && model.snapshot.room_id().is_some() {
                Some(Ok(()))
            } else if !state.busy
                && !matches!(
                    state.pending_flow,
                    Some(PendingFlow::CreateRoom | PendingFlow::JoinRoom)
                )
            {
                Some(Err(state.status.clone()))
            } else {
                None
            }
        }
        Completion::RoomLeft => {
            if state.screen == UiScreen::LobbyEnded && model.snapshot.room_id().is_none() {
                Some(Ok(()))
            } else if !state.busy && state.pending_flow != Some(PendingFlow::LeaveRoom) {
                Some(Err(state.status.clone()))
            } else {
                None
            }
        }
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
        Completion::GameAction(expected) => model
            .snapshot
            .game
            .as_ref()
            .filter(|game| game.action_count >= *expected)
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
    let hand = sorted_hand(model);
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

fn sorted_hand(model: &BridgeModel) -> Vec<&poche_spacetimedb_client::HandCardView> {
    let mut hand = model.snapshot.hand.iter().collect::<Vec<_>>();
    hand.sort_by(|left, right| left.card_key.cmp(&right.card_key));
    hand
}

fn submit_game_action(
    model: &BridgeModel,
    pending: &mut PendingRequest,
    intent: impl FnOnce(String) -> BridgeIntent,
    bridge: &BridgeHandle,
) -> Result<(), String> {
    let room_id = model
        .snapshot
        .room_id()
        .ok_or("join a room before taking a game action")?;
    let expected = model
        .snapshot
        .game
        .as_ref()
        .ok_or("take a seat and wait for the deal before taking a game action")?
        .action_count
        .saturating_add(1);
    pending.completion = Completion::GameAction(expected);
    bridge.send(intent(room_id.into()))
}

fn submit_play_action(
    card_index: usize,
    model: &BridgeModel,
    pending: &mut PendingRequest,
    bridge: &BridgeHandle,
) -> Result<(), String> {
    let card_id = sorted_hand(model)
        .get(card_index)
        .ok_or("owned card index is outside this device's private hand")?
        .card_id
        .clone();
    submit_game_action(
        model,
        pending,
        |room_id| BridgeIntent::PlayCard { room_id, card_id },
        bridge,
    )
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
        FileControlAction::SetName { name } | FileControlAction::SelectIdentity { label: name } => {
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
        FileControlAction::Bid { tricks } if *tricks > 2 => {
            return Err("the first two-player round permits bids from 0 through 2".into());
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
        FileControlAction::Pointer { x, y, .. }
            if !x.is_finite() || !y.is_finite() || *x < 0.0 || *y < 0.0 =>
        {
            return Err("pointer coordinates must be finite non-negative logical pixels".into());
        }
        FileControlAction::Key { key, .. } => {
            supported_key(key)?;
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
        surface: match state.screen {
            UiScreen::IdentityGate => "identity_gate",
            UiScreen::Connecting => "connecting",
            UiScreen::LoadingRoom => "loading_room",
            UiScreen::ResumeOffer => "resume_offer",
            UiScreen::MainMenu => "main_menu",
            UiScreen::Table => "table",
            UiScreen::LobbyEnded => "lobby_ended",
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
        rendered_card_count: state.rendered_card_count,
        held_card_key: state.held_card_key.clone(),
        visible_hand_copies: state.visible_hand_copies,
        rendered_player_count: state.rendered_player_count,
        game: model.snapshot.game.as_ref().map(|game| FileControlGame {
            phase: game.phase.clone(),
            actor_seat: game.actor_seat,
            dealer_seat: game.dealer_seat,
            round_index: game.round_index,
            hand_size: game.hand_size,
            hand_counts: game.hand_counts,
            bids: game.bids,
            trick_count: game.trick_count,
            trick_seats: game.trick_seats,
            tricks_won: game.tricks_won,
            scores: game.scores,
            pot_cents: game.pot_cents,
            trump: game.trump,
            action_count: game.action_count,
        }),
        revealed_cards: model
            .snapshot
            .revealed_cards
            .iter()
            .map(|card| FileControlRevealedCard {
                card_key: card.card_key.clone(),
                face: card.face.clone(),
            })
            .collect(),
        activity: model
            .snapshot
            .activity
            .iter()
            .map(|event| FileControlActivity {
                sequence: event.sequence,
                kind: event.kind.clone(),
                summary: event.summary.clone(),
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
        let minimize: FileControlAction =
            serde_json::from_str(r#"{"kind":"set_window_minimized","minimized":true}"#).unwrap();
        assert_eq!(
            minimize,
            FileControlAction::SetWindowMinimized { minimized: true }
        );
    }

    #[test]
    fn synthetic_pointer_rejects_nonfinite_and_outside_viewport_coordinates() {
        for point in [
            Vec2::new(f32::NAN, 10.0),
            Vec2::new(0.0, f32::INFINITY),
            Vec2::new(-1.0, 2.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(0.0, 80.0),
        ] {
            assert!(validate_pointer(point, 100.0, 80.0).is_err());
        }
        assert!(validate_pointer(Vec2::ZERO, 100.0, 80.0).is_ok());
        assert!(validate_pointer(Vec2::new(99.0, 79.0), 100.0, 80.0).is_ok());
        assert_eq!(supported_key("Q").unwrap(), KeyCode::KeyQ);
        assert_eq!(supported_key("Space").unwrap(), KeyCode::Space);
        assert_eq!(supported_key("arrowleft").unwrap(), KeyCode::ArrowLeft);
        assert!(supported_key("Alt+F4").is_err());
        assert!(supported_key("").is_err());
    }

    #[derive(Resource, Default)]
    struct InputEdges(Vec<(bool, bool, bool, bool)>);

    fn input_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin))
            .add_message::<PointerInput>()
            .init_resource::<QueuedDeviceInput>()
            .init_resource::<InputEdges>()
            .insert_resource(RenderSurface::Windowless {
                width: 100,
                height: 80,
                target: Some(Handle::default()),
            })
            .add_systems(PreUpdate, apply_device_input.after(InputSystems))
            .add_systems(
                Update,
                |keys: Res<ButtonInput<KeyCode>>,
                 mouse: Res<ButtonInput<MouseButton>>,
                 mut edges: ResMut<InputEdges>| {
                    edges.0.push((
                        keys.just_pressed(KeyCode::KeyQ),
                        keys.just_released(KeyCode::KeyQ),
                        mouse.just_pressed(MouseButton::Left),
                        mouse.just_released(MouseButton::Left),
                    ));
                },
            );
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(100, 80),
                ..default()
            },
            PrimaryWindow,
        ));
        app
    }

    #[test]
    fn queued_input_survives_bevy_preupdate_clear_and_is_visible_to_normal_update() {
        let mut app = input_test_app();
        app.world_mut().resource_mut::<QueuedDeviceInput>().pending = Some((
            1,
            DeviceInput::Key {
                key: KeyCode::KeyQ,
                down: true,
            },
        ));
        assert!(
            app.world()
                .resource::<QueuedDeviceInput>()
                .completed
                .is_none()
        );
        app.update();
        assert_eq!(
            app.world().resource::<QueuedDeviceInput>().completed,
            Some((1, Ok(())))
        );
        app.update(); // Held input must not recreate a just-pressed edge.
        app.world_mut().resource_mut::<QueuedDeviceInput>().pending = Some((
            2,
            DeviceInput::Key {
                key: KeyCode::KeyQ,
                down: false,
            },
        ));
        app.update();
        app.world_mut().resource_mut::<QueuedDeviceInput>().pending = Some((
            3,
            DeviceInput::Pointer {
                point: Vec2::new(35.0, 40.0),
                primary_down: true,
            },
        ));
        app.update();
        app.update();
        app.world_mut().resource_mut::<QueuedDeviceInput>().pending = Some((
            4,
            DeviceInput::Pointer {
                point: Vec2::new(55.0, 45.0),
                primary_down: false,
            },
        ));
        app.update();
        assert_eq!(
            app.world().resource::<InputEdges>().0,
            [
                (true, false, false, false),
                (false, false, false, false),
                (false, true, false, false),
                (false, false, true, false),
                (false, false, false, false),
                (false, false, false, true),
            ]
        );
        let mut windows = app
            .world_mut()
            .query_filtered::<&Window, With<PrimaryWindow>>();
        assert_eq!(
            windows.single(app.world()).unwrap().cursor_position(),
            Some(Vec2::new(55.0, 45.0))
        );
        let events: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PointerInput>>()
            .drain()
            .collect();
        assert!(
            events.iter().any(|event| matches!(
                event.action,
                PointerAction::Release(PointerButton::Primary)
            ))
        );
        assert!(events.iter().all(|event| matches!(
            event.location.target,
            bevy::camera::NormalizedRenderTarget::Image(_)
        )));
    }

    #[test]
    fn synthetic_pointer_cannot_move_an_interactive_os_window_cursor() {
        let mut app = input_test_app();
        app.insert_resource(RenderSurface::Windowed);
        app.world_mut().resource_mut::<QueuedDeviceInput>().pending = Some((
            1,
            DeviceInput::Pointer {
                point: Vec2::new(35.0, 40.0),
                primary_down: true,
            },
        ));
        app.update();
        assert!(matches!(
            &app.world().resource::<QueuedDeviceInput>().completed,
            Some((1, Err(_)))
        ));
        assert!(
            !app.world()
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left)
        );
        let mut windows = app
            .world_mut()
            .query_filtered::<&Window, With<PrimaryWindow>>();
        assert_eq!(windows.single(app.world()).unwrap().cursor_position(), None);
    }
}
