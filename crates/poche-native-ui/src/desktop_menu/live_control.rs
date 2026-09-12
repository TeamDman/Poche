//! Opt-in, local file-driven control for disposable live UI exploration.
//!
//! Requests become Bevy pointer/keyboard input. This module never calls a
//! reducer, submits a game command directly, reads the OS clipboard, or joins
//! the Veilid authority graph as another device.

use super::{Action, DesktopMenuRoot, DesktopMenuStatus, Field, LiveAction};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    prelude::*,
    text::EditableText,
    window::PrimaryWindow,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const FILE_CONTROL_SCHEMA_VERSION: u16 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_TEXT_CHARACTERS: usize = 4096;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlRequest {
    pub schema_version: u16,
    pub request_id: String,
    pub sequence: u64,
    pub action: FileControlAction,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileControlAction {
    Observe {
        /// Include the bearer invitation in this one private local response.
        #[serde(default)]
        include_invitation: bool,
    },
    TypeText {
        field: FileControlField,
        text: String,
    },
    Click {
        target: FileControlTarget,
    },
    Capture,
    Stop,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileControlField {
    Name,
    Invitation,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileControlTarget {
    CreateLobby,
    JoinLobby,
    Seat { ordinal: u8 },
    LiveAction { id: String },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileControlStatus {
    Completed,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlResponse {
    pub schema_version: u16,
    pub request_id: String,
    pub sequence: u64,
    pub status: FileControlStatus,
    pub observation: FileControlObservation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlObservation {
    pub privacy: String,
    pub instance_id: String,
    pub surface: String,
    pub menu_busy: bool,
    pub menu_status: String,
    pub name_field: String,
    pub invitation_field_characters: usize,
    pub invitation_field_valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_invitation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_principal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_phase: Option<poche_protocol::RoomPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_phase: Option<poche_protocol::PublicGamePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_seat: Option<u8>,
    pub own_hand_cards: usize,
    pub members: Vec<FileControlMember>,
    pub available_actions: Vec<FileControlAdvertisedAction>,
    pub last_finding: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlMember {
    pub principal: String,
    pub connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<u8>,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileControlAdvertisedAction {
    pub id: String,
    pub label: String,
}

pub struct FileControlPlugin {
    endpoint: FileControlEndpoint,
}

impl FileControlPlugin {
    /// Claim a fresh explicit root and publish its immutable descriptor before
    /// Bevy starts. Existing roots are rejected rather than reused.
    ///
    /// # Errors
    ///
    /// Returns an error when identifiers are invalid, the root already exists,
    /// or the descriptor and endpoint directories cannot be published.
    pub fn prepare(options: FileControlOptions, surface: &str) -> Result<Self, String> {
        validate_identifier(&options.instance_id, "instance identifier")?;
        if let Some(parent) = options.root.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "developer control parent could not be created".to_owned())?;
        }
        fs::create_dir(&options.root)
            .map_err(|_| "developer control root must be fresh".to_owned())?;
        for child in [
            "requests",
            "processing",
            "processed",
            "responses",
            "captures",
        ] {
            fs::create_dir(options.root.join(child))
                .map_err(|_| "developer control directories could not be created".to_owned())?;
        }
        let descriptor = FileControlDescriptor {
            schema_version: FILE_CONTROL_SCHEMA_VERSION,
            instance_id: options.instance_id.clone(),
            process_id: std::process::id(),
            started_unix_ms: unix_millis()?,
            surface: surface.to_owned(),
            transport_boundary: "local developer input only; Veilid carries game traffic"
                .to_owned(),
            capabilities: vec![
                "observe_private_local".to_owned(),
                "type_keyboard".to_owned(),
                "click_measured_target".to_owned(),
                "capture_gpu".to_owned(),
                "stop".to_owned(),
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
    stage: PendingStage,
}

#[derive(Clone)]
enum PendingStage {
    Click {
        target: FileControlTarget,
        pointer_stage: u8,
        ready_frame: u64,
    },
    Type {
        field: FileControlField,
        text: Vec<char>,
        pointer_stage: u8,
        character_index: usize,
        ready_frame: u64,
    },
    Settle {
        ready_frame: u64,
    },
    Capture {
        path: PathBuf,
        requested: bool,
    },
}

fn drive_file_control(world: &mut World) {
    let Some(mut endpoint) = world.remove_resource::<FileControlEndpoint>() else {
        return;
    };
    endpoint.frame = endpoint.frame.saturating_add(1);
    let stop = endpoint.advance(world).unwrap_or(false);
    world.insert_resource(endpoint);
    if stop {
        world.write_message(AppExit::Success);
    }
}

impl FileControlEndpoint {
    #[allow(
        clippy::too_many_lines,
        reason = "request claiming and state-machine entry stay together so each accepted request has one auditable path"
    )]
    fn advance(&mut self, world: &mut World) -> Result<bool, String> {
        if let Some(mut pending) = self.pending.take() {
            match self.advance_pending(world, &mut pending) {
                Ok(true) => {
                    self.complete(world, pending, FileControlStatus::Completed, None, None)?;
                }
                Ok(false) => self.pending = Some(pending),
                Err(error) => {
                    self.complete(
                        world,
                        pending,
                        FileControlStatus::Rejected,
                        Some(error),
                        None,
                    )?;
                }
            }
            return Ok(false);
        }
        let Some((request, processing_path)) = self.claim_next()? else {
            return Ok(false);
        };
        let error = self.validate_request(&request).err();
        if let Some(error) = error {
            let pending = PendingRequest {
                request,
                processing_path,
                stage: PendingStage::Settle {
                    ready_frame: self.frame,
                },
            };
            self.complete(
                world,
                pending,
                FileControlStatus::Rejected,
                Some(error),
                None,
            )?;
            return Ok(false);
        }
        self.last_sequence = Some(request.sequence);
        match request.action.clone() {
            FileControlAction::Observe { .. } => {
                let pending = PendingRequest {
                    request,
                    processing_path,
                    stage: PendingStage::Settle {
                        ready_frame: self.frame,
                    },
                };
                self.complete(world, pending, FileControlStatus::Completed, None, None)?;
            }
            FileControlAction::Stop => {
                let pending = PendingRequest {
                    request,
                    processing_path,
                    stage: PendingStage::Settle {
                        ready_frame: self.frame,
                    },
                };
                self.complete(world, pending, FileControlStatus::Completed, None, None)?;
                return Ok(true);
            }
            FileControlAction::Click { target } => {
                self.pending = Some(PendingRequest {
                    request,
                    processing_path,
                    stage: PendingStage::Click {
                        target,
                        pointer_stage: 0,
                        ready_frame: self.frame,
                    },
                });
            }
            FileControlAction::TypeText { field, text } => {
                self.pending = Some(PendingRequest {
                    request,
                    processing_path,
                    stage: PendingStage::Type {
                        field,
                        text: text.chars().collect(),
                        pointer_stage: 0,
                        character_index: 0,
                        ready_frame: self.frame,
                    },
                });
            }
            FileControlAction::Capture => {
                let path = self
                    .root
                    .join("captures")
                    .join(format!("{}.png", request.request_id));
                self.pending = Some(PendingRequest {
                    request,
                    processing_path,
                    stage: PendingStage::Capture {
                        path,
                        requested: false,
                    },
                });
            }
        }
        Ok(false)
    }

    fn advance_pending(
        &self,
        world: &mut World,
        pending: &mut PendingRequest,
    ) -> Result<bool, String> {
        match &mut pending.stage {
            PendingStage::Click {
                target,
                pointer_stage,
                ready_frame,
            } => {
                if self.frame < *ready_frame {
                    return Ok(false);
                }
                emit_pointer(world, target, *pointer_stage)?;
                if *pointer_stage < 2 {
                    *pointer_stage += 1;
                    *ready_frame = self.frame.saturating_add(3);
                } else {
                    pending.stage = PendingStage::Settle {
                        ready_frame: self.frame.saturating_add(4),
                    };
                }
                Ok(false)
            }
            PendingStage::Type {
                field,
                text,
                pointer_stage,
                character_index,
                ready_frame,
            } => {
                if self.frame < *ready_frame {
                    return Ok(false);
                }
                if *pointer_stage < 3 {
                    emit_field_pointer(world, *field, *pointer_stage)?;
                    *pointer_stage += 1;
                    *ready_frame = self.frame.saturating_add(3);
                    return Ok(false);
                }
                if let Some(character) = text.get(*character_index).copied() {
                    emit_character(world, character)?;
                    *character_index += 1;
                    *ready_frame = self.frame.saturating_add(1);
                    Ok(false)
                } else {
                    pending.stage = PendingStage::Settle {
                        ready_frame: self.frame.saturating_add(4),
                    };
                    Ok(false)
                }
            }
            PendingStage::Settle { ready_frame } => Ok(self.frame >= *ready_frame),
            PendingStage::Capture { path, requested } => {
                if *requested {
                    Ok(world.resource::<crate::LaunchClock>().screenshot_completed)
                } else {
                    if path.exists() {
                        return Err("capture target unexpectedly exists".to_owned());
                    }
                    world.resource_mut::<crate::AcceptanceOptions>().screenshot =
                        Some(path.clone());
                    let mut clock = world.resource_mut::<crate::LaunchClock>();
                    clock.screenshot_requested = false;
                    clock.screenshot_completed = false;
                    *requested = true;
                    Ok(false)
                }
            }
        }
    }

    fn complete(
        &self,
        world: &mut World,
        pending: PendingRequest,
        status: FileControlStatus,
        error: Option<String>,
        capture_path: Option<String>,
    ) -> Result<(), String> {
        let capture_path = capture_path.or_else(|| match &pending.stage {
            PendingStage::Capture { path, .. } => Some(path.to_string_lossy().into_owned()),
            _ => None,
        });
        let response = FileControlResponse {
            schema_version: FILE_CONTROL_SCHEMA_VERSION,
            request_id: pending.request.request_id.clone(),
            sequence: pending.request.sequence,
            status,
            observation: observation(
                world,
                &self.instance_id,
                matches!(
                    pending.request.action,
                    FileControlAction::Observe {
                        include_invitation: true
                    }
                ),
            ),
            error,
            capture_path,
        };
        atomic_json(
            &self
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
            self.root.join("processed").join(file_name),
        )
        .map_err(|_| "processed request could not be archived".to_owned())?;
        Ok(())
    }

    fn claim_next(&self) -> Result<Option<(FileControlRequest, PathBuf)>, String> {
        let mut paths = fs::read_dir(self.root.join("requests"))
            .map_err(|_| "request directory is unavailable".to_owned())?
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
        let metadata = fs::metadata(&path).map_err(|_| "request metadata is unavailable")?;
        if metadata.len() > MAX_REQUEST_BYTES {
            return Err("request exceeds the developer-control bound".to_owned());
        }
        let file_name = path.file_name().ok_or("request has no file name")?;
        let processing = self.root.join("processing").join(file_name);
        fs::rename(&path, &processing).map_err(|_| "request could not be claimed".to_owned())?;
        let bytes = fs::read(&processing).map_err(|_| "request could not be read".to_owned())?;
        let request = serde_json::from_slice(&bytes)
            .map_err(|_| "request is not valid developer-control JSON".to_owned())?;
        Ok(Some((request, processing)))
    }

    fn validate_request(&self, request: &FileControlRequest) -> Result<(), String> {
        if request.schema_version != FILE_CONTROL_SCHEMA_VERSION {
            return Err("request schema version is unsupported".to_owned());
        }
        validate_identifier(&request.request_id, "request identifier")?;
        if self
            .last_sequence
            .is_some_and(|last| request.sequence <= last)
        {
            return Err("request sequence is stale".to_owned());
        }
        match &request.action {
            FileControlAction::TypeText { field, text } => {
                let limit = if *field == FileControlField::Name {
                    48
                } else {
                    MAX_TEXT_CHARACTERS
                };
                if text.is_empty()
                    || text.chars().count() > limit
                    || text.chars().any(char::is_control)
                {
                    return Err("typed text is outside the field bound".to_owned());
                }
            }
            FileControlAction::Click {
                target: FileControlTarget::LiveAction { id },
            } => validate_identifier(id, "live action identifier")?,
            FileControlAction::Click {
                target: FileControlTarget::Seat { ordinal },
            } if *ordinal >= 8 => {
                return Err("seat target is outside the supported bound".to_owned());
            }
            FileControlAction::Observe { .. }
            | FileControlAction::Capture
            | FileControlAction::Stop
            | FileControlAction::Click { .. } => {}
        }
        Ok(())
    }
}

fn emit_pointer(world: &mut World, target: &FileControlTarget, stage: u8) -> Result<(), String> {
    let entity = find_target(world, target).ok_or("requested UI target is not present")?;
    emit_pointer_for_entity(
        world,
        entity,
        matches!(target, FileControlTarget::Seat { .. }),
        stage,
    )
}

fn emit_field_pointer(
    world: &mut World,
    requested: FileControlField,
    stage: u8,
) -> Result<(), String> {
    let field = match requested {
        FileControlField::Name => Field::Name,
        FileControlField::Invitation => Field::Invitation,
    };
    let entity = world
        .query::<(Entity, &Field)>()
        .iter(world)
        .find(|(_, candidate)| **candidate == field)
        .map(|(entity, _)| entity)
        .ok_or("requested text field is not present")?;
    emit_pointer_for_entity(world, entity, false, stage)
}

fn emit_pointer_for_entity(
    world: &mut World,
    entity: Entity,
    world_space: bool,
    stage: u8,
) -> Result<(), String> {
    let (position, camera) = if world_space {
        let origin = world
            .get::<GlobalTransform>(entity)
            .ok_or("seat geometry is unavailable")?
            .translation();
        let (camera_entity, camera, transform) = world
            .query_filtered::<(Entity, &Camera, &GlobalTransform), With<crate::TabletopCamera>>()
            .single(world)
            .map_err(|_| "table camera is unavailable")?;
        let position = camera
            .world_to_viewport(transform, origin)
            .map_err(|_| "seat target is outside the viewport")?;
        (position, camera_entity)
    } else {
        let position = world
            .get::<UiGlobalTransform>(entity)
            .ok_or("UI geometry is unavailable")?
            .translation;
        let camera = world
            .get::<ComputedUiTargetCamera>(entity)
            .and_then(ComputedUiTargetCamera::get)
            .ok_or("UI camera is unavailable")?;
        (position, camera)
    };
    let render_target = world
        .get::<bevy::camera::RenderTarget>(camera)
        .and_then(|target| target.normalize(None))
        .ok_or("UI render target is unavailable")?;
    let action = match stage {
        0 => PointerAction::Move { delta: Vec2::ZERO },
        1 => PointerAction::Press(PointerButton::Primary),
        _ => PointerAction::Release(PointerButton::Primary),
    };
    world.write_message(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: render_target,
            position,
        },
        action,
    ));
    Ok(())
}

fn emit_character(world: &mut World, character: char) -> Result<(), String> {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .map_err(|_| "keyboard input endpoint is unavailable")?;
    let text = character.to_string();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        world.write_message(KeyboardInput {
            key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
            logical_key: Key::Character(text.clone().into()),
            state,
            text: Some(text.clone().into()),
            repeat: false,
            window,
        });
    }
    Ok(())
}

fn find_target(world: &mut World, target: &FileControlTarget) -> Option<Entity> {
    match target {
        FileControlTarget::Seat { ordinal } => world
            .query::<(Entity, &crate::lobby_scene::SeatControl, &Mesh3d)>()
            .iter(world)
            .find(|(_, seat, _)| seat.0.get() == *ordinal)
            .map(|(entity, _, _)| entity),
        FileControlTarget::LiveAction { id } => world
            .query::<(Entity, &LiveAction)>()
            .iter(world)
            .find(|(_, action)| action.0 == *id)
            .map(|(entity, _)| entity),
        FileControlTarget::CreateLobby | FileControlTarget::JoinLobby => world
            .query::<(Entity, &Action)>()
            .iter(world)
            .find(|(_, action)| {
                matches!(
                    (target, **action),
                    (FileControlTarget::CreateLobby, Action::Create)
                        | (FileControlTarget::JoinLobby, Action::Join)
                )
            })
            .map(|(entity, _)| entity),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the privacy-filtered observation is assembled in one place so secret-bearing and public fields cannot drift"
)]
fn observation(
    world: &mut World,
    instance_id: &str,
    include_invitation: bool,
) -> FileControlObservation {
    let menu_present = world
        .query_filtered::<Entity, With<DesktopMenuRoot>>()
        .iter(world)
        .next()
        .is_some();
    let mut name_field = String::new();
    let mut invitation = String::new();
    for (field, text) in world.query::<(&Field, &EditableText)>().iter(world) {
        let value = text.value().into_iter().collect::<String>();
        match field {
            Field::Name => name_field = value,
            Field::Invitation => invitation = value,
        }
    }
    let invitation_field_valid = !invitation.is_empty()
        && world
            .get_resource::<super::InvitationValidator>()
            .is_some_and(|validator| (validator.0)(invitation.trim()));
    let status = world.get_resource::<DesktopMenuStatus>();
    let controller = world.get_resource::<crate::NativeController>();
    let live = world.get_resource::<crate::NativeLiveDevice>();
    let (
        room_invitation,
        room_id,
        viewer_principal,
        revision,
        room_phase,
        game_phase,
        own_seat,
        own_hand_cards,
        members,
        available_actions,
    ) = live.map_or_else(
        || {
            (
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                0,
                Vec::new(),
                Vec::new(),
            )
        },
        |live| {
            let view = live.observation();
            let principal = &view.projection.principal_id;
            (
                include_invitation
                    .then(|| live.room_invitation().map(str::to_owned))
                    .flatten(),
                Some(view.projection.room_id.as_str().to_owned()),
                Some(principal.as_str().to_owned()),
                Some(view.projection.current_revision),
                Some(view.projection.payload.phase),
                view.projection
                    .payload
                    .public_game_state
                    .as_ref()
                    .map(|game| game.phase),
                view.projection
                    .payload
                    .members
                    .iter()
                    .find(|member| &member.principal_id == principal)
                    .and_then(|member| member.seat),
                view.projection
                    .payload
                    .own_hand
                    .as_ref()
                    .map_or(0, |hand| hand.cards.len()),
                view.projection
                    .payload
                    .members
                    .iter()
                    .map(|member| FileControlMember {
                        principal: member.principal_id.as_str().to_owned(),
                        connected: member.connected,
                        seat: member.seat,
                        ready: member.ready,
                    })
                    .collect(),
                view.actions
                    .iter()
                    .map(|action| FileControlAdvertisedAction {
                        id: action.id.clone(),
                        label: action.label.clone(),
                    })
                    .collect(),
            )
        },
    );
    FileControlObservation {
        privacy: if include_invitation {
            "private local developer artifact; invitation explicitly included"
        } else {
            "local developer artifact; invitation omitted"
        }
        .to_owned(),
        instance_id: instance_id.to_owned(),
        surface: if menu_present {
            "main_menu".to_owned()
        } else if live.is_some() {
            "table".to_owned()
        } else {
            "starting".to_owned()
        },
        menu_busy: status.is_some_and(|status| status.busy),
        menu_status: status.map_or_else(String::new, |status| status.message.clone()),
        name_field,
        invitation_field_characters: invitation.chars().count(),
        invitation_field_valid,
        room_invitation,
        room_id,
        viewer_principal,
        revision,
        room_phase,
        game_phase,
        own_seat,
        own_hand_cards,
        members,
        available_actions,
        last_finding: controller.map_or_else(String::new, |controller| {
            controller.last_finding().to_owned()
        }),
    }
}

/// Atomically submit one request and wait for its correlated response. This is
/// used by the unified executable's developer CLI; it does not contact Veilid.
///
/// # Errors
///
/// Returns an error when the descriptor/request/response is invalid or cannot
/// be published, correlation fails, the instance stops, or the timeout expires.
pub fn send_file_control_request(
    root: &Path,
    action: FileControlAction,
    timeout: Option<Duration>,
) -> Result<FileControlResponse, String> {
    let descriptor: FileControlDescriptor = serde_json::from_slice(
        &fs::read(root.join("instance.json"))
            .map_err(|_| "live instance descriptor is unavailable".to_owned())?,
    )
    .map_err(|_| "live instance descriptor is invalid".to_owned())?;
    if descriptor.schema_version != FILE_CONTROL_SCHEMA_VERSION {
        return Err("live instance schema version is unsupported".to_owned());
    }
    let request_id = random_identifier()?;
    let sequence = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system time is unavailable")?
            .as_nanos(),
    )
    .map_err(|_| "system time does not fit the request sequence")?;
    let request = FileControlRequest {
        schema_version: FILE_CONTROL_SCHEMA_VERSION,
        request_id: request_id.clone(),
        sequence,
        action,
    };
    let path = root
        .join("requests")
        .join(format!("{sequence:020}-{request_id}.json"));
    atomic_json(&path, &request)?;
    let response_path = root.join("responses").join(format!("{request_id}.json"));
    let deadline = Instant::now() + timeout.unwrap_or(RESPONSE_TIMEOUT);
    loop {
        if let Ok(bytes) = fs::read(&response_path) {
            let response: FileControlResponse = serde_json::from_slice(&bytes)
                .map_err(|_| "developer-control response is invalid".to_owned())?;
            if response.request_id != request_id || response.sequence != sequence {
                return Err("developer-control response correlation failed".to_owned());
            }
            return Ok(response);
        }
        if root.join("stopped.json").exists() {
            return Err("live instance stopped before responding".to_owned());
        }
        if Instant::now() >= deadline {
            return Err("live instance did not respond before the timeout".to_owned());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn mark_file_control_stopped(root: &Path) {
    let _ = atomic_json(
        &root.join("stopped.json"),
        &serde_json::json!({
            "schema_version": FILE_CONTROL_SCHEMA_VERSION,
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
        .map_err(|_| "developer-control JSON could not be encoded".to_owned())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "developer-control temporary file could not be created".to_owned())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "developer-control temporary file could not be written".to_owned())?;
    fs::rename(&temporary, path)
        .map_err(|_| "developer-control file could not be published".to_owned())
}

fn random_identifier() -> Result<String, String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| "random request identifier is unavailable")?;
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
    .map_err(|_| "system time does not fit milliseconds".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_fresh_and_descriptor_states_the_transport_boundary() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("alice");
        let plugin = FileControlPlugin::prepare(
            FileControlOptions {
                root: root.clone(),
                instance_id: "alice-window".to_owned(),
            },
            "windowless_image",
        )
        .unwrap();
        let descriptor: FileControlDescriptor =
            serde_json::from_slice(&fs::read(root.join("instance.json")).unwrap()).unwrap();
        assert_eq!(descriptor.instance_id, "alice-window");
        assert!(descriptor.transport_boundary.contains("Veilid"));
        assert!(
            FileControlPlugin::prepare(
                FileControlOptions {
                    root,
                    instance_id: "alice-window".to_owned(),
                },
                "windowless_image",
            )
            .is_err()
        );
        drop(plugin);
    }

    #[test]
    fn request_validation_rejects_shell_like_targets_and_oversized_text() {
        assert!(validate_identifier("room-ready", "action").is_ok());
        assert!(validate_identifier("rm /s", "action").is_err());
        let parent = tempfile::tempdir().unwrap();
        let plugin = FileControlPlugin::prepare(
            FileControlOptions {
                root: parent.path().join("bob"),
                instance_id: "bob-window".to_owned(),
            },
            "interactive_window",
        )
        .unwrap();
        let endpoint = &plugin.endpoint;
        let request = FileControlRequest {
            schema_version: FILE_CONTROL_SCHEMA_VERSION,
            request_id: "request-1".to_owned(),
            sequence: 1,
            action: FileControlAction::TypeText {
                field: FileControlField::Name,
                text: "x".repeat(49),
            },
        };
        assert!(endpoint.validate_request(&request).is_err());
    }

    #[test]
    #[ignore = "GPU-backed incremental file control with no OS window"]
    fn windowless_instance_accepts_separate_observe_type_capture_and_stop_requests() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("alice-live");
        let controller_root = root.clone();
        let controller = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !controller_root.join("instance.json").exists() {
                assert!(Instant::now() < deadline, "instance descriptor timed out");
                std::thread::sleep(Duration::from_millis(20));
            }
            let observed = send_file_control_request(
                &controller_root,
                FileControlAction::Observe {
                    include_invitation: false,
                },
                Some(Duration::from_secs(30)),
            )
            .unwrap();
            assert_eq!(observed.observation.surface, "main_menu");
            assert!(observed.observation.room_invitation.is_none());
            let typed = send_file_control_request(
                &controller_root,
                FileControlAction::TypeText {
                    field: FileControlField::Name,
                    text: "Alice".to_owned(),
                },
                Some(Duration::from_secs(30)),
            )
            .unwrap();
            assert_eq!(typed.observation.name_field, "Alice");
            let captured = send_file_control_request(
                &controller_root,
                FileControlAction::Capture,
                Some(Duration::from_secs(30)),
            )
            .unwrap();
            let capture = PathBuf::from(captured.capture_path.unwrap());
            let pixels = image::open(&capture).unwrap().to_rgba8();
            assert_eq!(pixels.dimensions(), (1280, 800));
            assert!(pixels.pixels().any(|pixel| pixel != pixels.get_pixel(0, 0)));
            send_file_control_request(
                &controller_root,
                FileControlAction::Stop,
                Some(Duration::from_secs(30)),
            )
            .unwrap();
        });
        crate::run_menu_with_file_control(
            crate::NativeUiLaunchOptions {
                render_mode: crate::NativeRenderMode::WindowlessImage,
                external_tracing: bevy::log::tracing::dispatcher::has_been_set(),
                ..default()
            },
            crate::desktop_menu::DesktopConnectionWorker::start(|_| {
                Err("unused connection worker")
            })
            .unwrap(),
            crate::desktop_menu::InvitationValidator(|value| value == "valid-test-invitation"),
            FileControlOptions {
                root: root.clone(),
                instance_id: "alice-window".to_owned(),
            },
        )
        .unwrap();
        controller.join().unwrap();
        assert!(root.join("stopped.json").exists());
    }
}
