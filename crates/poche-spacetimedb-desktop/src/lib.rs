// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A deliberately game-shaped desktop surface over the authoritative Poche
//! `SpacetimeDB` model. The UI is a top-down projection of integer millimetre
//! poses; it never becomes logical-state authority.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]

pub mod file_control;

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    clipboard::{Clipboard, ClipboardRead},
    image::Image,
    log::LogPlugin,
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::Screenshot,
    },
    text::{EditableText, TextCursorStyle, TextEdit},
    window::{ExitCondition, PresentMode, PrimaryWindow, WindowResolution},
    winit::WinitPlugin,
};
use poche_bevy_spacetimedb::{
    BridgeHandle, BridgeIntent, BridgeModel, BridgeNotice, PocheSpacetimePlugin,
};
use poche_spacetimedb_client::{
    CardPoseView, ClientConfig, DEFAULT_DATABASE, DEFAULT_URI, RoomCapability, valid_join_code,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const CARD_WIDTH: f32 = 76.0;
const CARD_HEIGHT: f32 = 108.0;
const POSE_PERIOD: Duration = Duration::from_millis(50);
const AUTOMATION_WIDTH: u32 = 1180;
const AUTOMATION_HEIGHT: u32 = 760;
const FONT_BYTES: &[u8] = include_bytes!("../../poche-native-ui/assets/CaskaydiaCove-Regular.ttf");
const MAINCLOUD_URI: &str = "https://maincloud.spacetimedb.com";
const MAINCLOUD_DATABASE: &str = "poche-6quz6";

#[derive(Clone, Debug, Resource, Eq, PartialEq)]
pub struct AuthorityEndpoint {
    pub profile: String,
    pub uri: String,
    pub database: String,
}

impl AuthorityEndpoint {
    fn from_environment() -> Self {
        let uri = std::env::var("POCHE_SPACETIMEDB_URI").unwrap_or_else(|_| DEFAULT_URI.into());
        let database = std::env::var("POCHE_SPACETIMEDB_DATABASE").unwrap_or_else(|_| {
            if is_maincloud_uri(&uri) {
                MAINCLOUD_DATABASE.into()
            } else {
                DEFAULT_DATABASE.into()
            }
        });
        Self {
            profile: classify_authority(&uri).into(),
            uri,
            database,
        }
    }

    /// Resolve a named or explicit authority, with environment fallbacks when omitted.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown server shorthand or an invalid database name.
    pub fn select(server: Option<&str>, database: Option<&str>) -> Result<Self, String> {
        let mut selected = match server {
            None => Self::from_environment(),
            Some("local") => Self {
                profile: "local".into(),
                uri: DEFAULT_URI.into(),
                database: DEFAULT_DATABASE.into(),
            },
            Some("maincloud") => Self {
                profile: "maincloud".into(),
                uri: MAINCLOUD_URI.into(),
                database: MAINCLOUD_DATABASE.into(),
            },
            Some(uri) if uri.starts_with("http://") || uri.starts_with("https://") => Self {
                profile: classify_authority(uri).into(),
                uri: uri.trim_end_matches('/').into(),
                database: DEFAULT_DATABASE.into(),
            },
            Some(value) => {
                return Err(format!(
                    "--server expects local, maincloud, or an http(s) URL; got {value:?}"
                ));
            }
        };
        if let Some(database) = database {
            selected.database = database.into();
        }
        if !valid_database_name(&selected.database) {
            return Err(format!(
                "database names must contain lowercase letters or digits separated by single hyphens; got {:?}",
                selected.database
            ));
        }
        Ok(selected)
    }

    fn client_config(&self, profile_name: impl Into<String>) -> ClientConfig {
        ClientConfig {
            uri: self.uri.clone(),
            database: self.database.clone(),
            profile_name: profile_name.into(),
        }
    }

    fn summary(&self) -> String {
        format!("{} · {}", self.profile, self.database)
    }
}

impl Default for AuthorityEndpoint {
    fn default() -> Self {
        Self::from_environment()
    }
}

fn is_maincloud_uri(uri: &str) -> bool {
    uri.trim_end_matches('/') == MAINCLOUD_URI
}

fn classify_authority(uri: &str) -> &'static str {
    if is_maincloud_uri(uri) {
        "maincloud"
    } else if matches!(
        uri.trim_end_matches('/'),
        "http://127.0.0.1:3000" | "http://localhost:3000"
    ) {
        "local"
    } else {
        "custom"
    }
}

fn valid_database_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderMode {
    #[default]
    Windowed,
    WindowlessImage,
}

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub render_mode: RenderMode,
    pub file_control: Option<file_control::FileControlOptions>,
    pub authority: AuthorityEndpoint,
}

#[derive(Clone, Debug, Resource)]
enum RenderSurface {
    Windowed,
    Windowless {
        width: u32,
        height: u32,
        target: Option<Handle<Image>>,
    },
}

impl RenderSurface {
    const fn from_mode(mode: RenderMode) -> Self {
        match mode {
            RenderMode::Windowed => Self::Windowed,
            RenderMode::WindowlessImage => Self::Windowless {
                width: AUTOMATION_WIDTH,
                height: AUTOMATION_HEIGHT,
                target: None,
            },
        }
    }

    fn render_target(&self) -> Option<RenderTarget> {
        match self {
            Self::Windowless {
                target: Some(target),
                ..
            } => Some(RenderTarget::Image(target.clone().into())),
            Self::Windowed | Self::Windowless { target: None, .. } => None,
        }
    }

    fn screenshot(&self) -> Option<Screenshot> {
        match self {
            Self::Windowed => Some(Screenshot::primary_window()),
            Self::Windowless {
                target: Some(target),
                ..
            } => Some(Screenshot::image(target.clone())),
            Self::Windowless { target: None, .. } => None,
        }
    }
}

/// Parse the ordinary executable's developer-control options.
///
/// # Errors
///
/// Returns an error for malformed or incomplete command-line options.
pub fn run_from_env() -> Result<(), String> {
    let mut options = LaunchOptions::default();
    let mut control_root: Option<PathBuf> = None;
    let mut instance_id: Option<String> = None;
    let mut server: Option<String> = None;
    let mut database: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--control-root" => {
                control_root = Some(PathBuf::from(
                    args.next().ok_or("--control-root requires a path")?,
                ));
            }
            "--instance-id" => {
                instance_id = Some(args.next().ok_or("--instance-id requires a value")?);
            }
            "--server" => {
                server = Some(args.next().ok_or("--server requires a value")?);
            }
            "--database" => {
                database = Some(args.next().ok_or("--database requires a value")?);
            }
            "--windowless" => options.render_mode = RenderMode::WindowlessImage,
            "--help" | "-h" => {
                println!(
                    "poche [--server local|maincloud|URL] [--database NAME]\n\
                     \x20     [--control-root PATH --instance-id ID] [--windowless]\n\
                     The safe default is local. The maincloud shorthand selects\n\
                     https://maincloud.spacetimedb.com and poche-6quz6. Explicit options override\n\
                     POCHE_SPACETIMEDB_URI and POCHE_SPACETIMEDB_DATABASE. Developer control\n\
                     publishes a fresh file endpoint; --windowless avoids an OS window."
                );
                return Ok(());
            }
            unknown => return Err(format!("unknown option {unknown:?}")),
        }
    }
    options.file_control = match (control_root, instance_id) {
        (Some(root), Some(instance_id)) => {
            Some(file_control::FileControlOptions { root, instance_id })
        }
        (None, None) => None,
        _ => return Err("--control-root and --instance-id must be supplied together".into()),
    };
    options.authority = AuthorityEndpoint::select(server.as_deref(), database.as_deref())?;
    run(options)
}

/// Run the desktop client with typed options.
///
/// # Errors
///
/// Returns before Bevy starts when the file-control endpoint cannot be prepared.
pub fn run(options: LaunchOptions) -> Result<(), String> {
    let authority = options.authority.clone();
    let windowless = options.render_mode == RenderMode::WindowlessImage;
    let surface = RenderSurface::from_mode(options.render_mode);
    let window_plugin = if windowless {
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                title: "Poche · SpacetimeDB table".into(),
                resolution: WindowResolution::new(AUTOMATION_WIDTH, AUTOMATION_HEIGHT),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }
    };
    let mut plugins = DefaultPlugins
        .set(window_plugin)
        .set(ImagePlugin::default_nearest());
    if windowless {
        plugins = plugins
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>()
            .disable::<LogPlugin>();
    }

    let control_root = options
        .file_control
        .as_ref()
        .map(|control| control.root.clone());
    let control = options
        .file_control
        .map(|control| file_control::FileControlPlugin::prepare(control, options.render_mode))
        .transpose()?;

    let mut app = App::new();
    app.add_plugins(plugins);
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(FONT_BYTES.to_vec()));
    app.insert_resource(PocheFont(font));
    if windowless {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1. / 60.,
        )));
        // UI layout and projection code still need a deterministic viewport;
        // this entity is not connected to Winit or an OS window.
        app.world_mut().spawn((
            Window {
                resolution: WindowResolution::new(AUTOMATION_WIDTH, AUTOMATION_HEIGHT),
                visible: false,
                ..default()
            },
            PrimaryWindow,
        ));
    }
    app.insert_resource(surface)
        .insert_resource(authority.clone())
        .add_plugins(PocheSpacetimePlugin)
        .init_resource::<Clipboard>()
        .insert_resource(UiState::for_authority(&authority))
        .init_resource::<PendingClipboard>()
        .init_resource::<PoseDisplay>()
        .init_resource::<DragState>()
        .add_message::<ButtonActivation>()
        .add_observer(activate_button)
        .add_systems(Startup, (setup_render_target, setup_menu).chain())
        .add_systems(
            Update,
            (
                handle_buttons,
                poll_clipboard,
                handle_bridge_notices,
                enter_room,
                sync_room_labels,
                sync_card_entities,
                drag_cards,
                animate_and_place_cards,
                update_status_labels,
                apply_poche_font,
            )
                .chain(),
        );
    if let Some(control) = control {
        app.add_plugins(control);
    }
    app.run();
    if let Some(root) = control_root {
        file_control::mark_stopped(&root);
    }
    Ok(())
}

#[derive(Resource)]
struct UiState {
    busy: bool,
    status: String,
    display_name: String,
    capability: Option<RoomCapability>,
    joined_once: bool,
    confirm_leave: bool,
}

#[derive(Resource)]
struct PocheFont(Handle<Font>);

fn apply_poche_font(font: Res<PocheFont>, mut text: Query<&mut TextFont, Added<TextFont>>) {
    for mut text_font in &mut text {
        *text_font = text_font.clone().with_font(font.0.clone());
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::for_authority(&AuthorityEndpoint::default())
    }
}

impl UiState {
    fn for_authority(authority: &AuthorityEndpoint) -> Self {
        Self {
            busy: false,
            status: format!("Using {}. Create or join a lobby.", authority.summary()),
            display_name: String::new(),
            capability: None,
            joined_once: false,
            confirm_leave: false,
        }
    }
}

#[derive(Resource, Default)]
struct PendingClipboard(Option<ClipboardRead>);

#[derive(Component)]
struct MainMenuRoot;

#[derive(Component)]
struct RoomRoot;

#[derive(Component)]
struct PocheUiCamera;

#[derive(Component)]
struct StatusLabel;

#[derive(Component)]
struct RoomCodeLabel;

#[derive(Component)]
struct SeatLabel(u8);

#[derive(Component)]
struct HandSummary;

#[derive(Component)]
struct LatencyLabel;

#[derive(Component, Clone, Copy, Eq, PartialEq)]
enum Field {
    Name,
    Invitation,
}

#[derive(Component, Clone)]
enum UiAction {
    Create,
    Paste,
    Join,
    CopyCode,
    TakeSeat(u8),
    ReleaseSeat,
    Leave,
}

#[derive(Message)]
struct ButtonActivation(Entity);

fn activate_button(
    mut click: On<Pointer<Click>>,
    buttons: Query<(), With<UiAction>>,
    mut activations: MessageWriter<ButtonActivation>,
) {
    if click.button == bevy::picking::pointer::PointerButton::Primary
        && buttons.contains(click.entity)
    {
        click.propagate(false);
        activations.write(ButtonActivation(click.entity));
    }
}

fn setup_render_target(mut images: ResMut<Assets<Image>>, mut surface: ResMut<RenderSurface>) {
    let RenderSurface::Windowless {
        width,
        height,
        target,
    } = surface.as_mut()
    else {
        return;
    };
    let mut image = Image::new_uninit(
        Extent3d {
            width: *width,
            height: *height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    *target = Some(images.add(image));
}

fn setup_menu(
    mut commands: Commands,
    surface: Res<RenderSurface>,
    authority: Res<AuthorityEndpoint>,
    state: Res<UiState>,
) {
    let mut camera = commands.spawn((Camera2d, PocheUiCamera));
    if let Some(target) = surface.render_target() {
        camera.insert(target);
    }
    let camera = camera.id();
    commands
        .spawn((
            MainMenuRoot,
            UiTargetCamera(camera),
            Node {
                width: percent(100.),
                height: percent(100.),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14.),
                ..default()
            },
            BackgroundColor(Color::srgb(0.025, 0.055, 0.06)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("POCHE"),
                TextFont::from_font_size(58.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            parent.spawn((
                Text::new("A shared card table"),
                TextFont::from_font_size(22.),
                TextColor(Color::srgb(0.7, 0.82, 0.8)),
            ));
            parent.spawn((
                Text::new(format!("Authority: {}", authority.summary())),
                TextFont::from_font_size(16.),
                TextColor(Color::srgb(0.58, 0.72, 0.7)),
            ));
            parent.spawn(Text::new("Player profile name"));
            spawn_field(parent, Field::Name, 380., 32, false);
            spawn_button(parent, "Create lobby", UiAction::Create, true);
            parent.spawn((
                Node {
                    width: px(520.),
                    height: px(1.),
                    margin: px(7.).vertical(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.36, 0.35)),
            ));
            parent.spawn(Text::new("Lobby code"));
            spawn_field(parent, Field::Invitation, 420., 23, false);
            parent
                .spawn(Node {
                    column_gap: px(10.),
                    ..default()
                })
                .with_children(|row| {
                    spawn_button(row, "Paste valid code", UiAction::Paste, false);
                    spawn_button(row, "Join lobby", UiAction::Join, true);
                });
            parent.spawn((
                StatusLabel,
                Text::new(&state.status),
                TextFont::from_font_size(17.),
                TextColor(Color::srgb(0.88, 0.9, 0.86)),
                Node {
                    max_width: px(760.),
                    margin: px(12.).top(),
                    ..default()
                },
            ));
        });
}

fn spawn_field(
    parent: &mut ChildSpawnerCommands,
    field: Field,
    width: f32,
    max: usize,
    multiline: bool,
) {
    parent.spawn((
        field,
        Node {
            width: px(width),
            min_height: px(48.),
            padding: px(10.).all(),
            ..default()
        },
        EditableText {
            max_characters: Some(max),
            visible_lines: multiline.then_some(2.),
            allow_newlines: multiline,
            ..default()
        },
        TextCursorStyle::default(),
        TextFont::from_font_size(22.),
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgb(0.10, 0.16, 0.17)),
    ));
}

fn spawn_button(parent: &mut ChildSpawnerCommands, label: &str, action: UiAction, primary: bool) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_width: px(150.),
                padding: px(13.).all(),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(if primary {
                Color::srgb(0.08, 0.48, 0.38)
            } else {
                Color::srgb(0.13, 0.26, 0.27)
            }),
        ))
        .with_child((Text::new(label), TextFont::from_font_size(20.)));
}

fn handle_buttons(
    mut activations: MessageReader<ButtonActivation>,
    actions: Query<&UiAction>,
    fields: Query<(&Field, &EditableText)>,
    bridge: Res<BridgeHandle>,
    model: Res<BridgeModel>,
    mut clipboard: ResMut<Clipboard>,
    mut pending: ResMut<PendingClipboard>,
    mut state: ResMut<UiState>,
    authority: Res<AuthorityEndpoint>,
) {
    for ButtonActivation(entity) in activations.read() {
        let Ok(action) = actions.get(*entity) else {
            continue;
        };
        let field_value = |wanted| {
            fields
                .iter()
                .find(|(field, _)| **field == wanted)
                .map(|(_, value)| value.value().into_iter().collect::<String>())
                .unwrap_or_default()
        };
        match action {
            UiAction::Paste => {
                pending.0 = Some(clipboard.fetch_text());
            }
            UiAction::Create | UiAction::Join if state.busy => {}
            UiAction::Create => {
                let name = field_value(Field::Name).trim().to_string();
                if !valid_name(&name) {
                    state.status = "Enter a visible player name (1–32 characters).".into();
                    continue;
                }
                state.display_name.clone_from(&name);
                state.busy = true;
                state.status = "Connecting to the table authority…".into();
                if let Err(error) = bridge.send(BridgeIntent::Create {
                    config: authority.client_config(&name),
                    display_name: name,
                }) {
                    state.busy = false;
                    state.status = error;
                }
            }
            UiAction::Join => {
                let name = field_value(Field::Name).trim().to_string();
                let code = field_value(Field::Invitation).trim().to_ascii_uppercase();
                if !valid_name(&name) {
                    state.status = "Enter a visible player name (1–32 characters).".into();
                    continue;
                }
                if !valid_join_code(&code) {
                    state.status = "Enter a code shaped like PCH-0000-0000-0000-0000.".into();
                    continue;
                }
                state.display_name.clone_from(&name);
                state.capability = Some(RoomCapability {
                    room_id: String::new(),
                    join_code: code.clone(),
                });
                state.busy = true;
                state.status = "Joining the shared table…".into();
                if let Err(error) = bridge.send(BridgeIntent::Join {
                    config: authority.client_config(&name),
                    display_name: name,
                    join_code: code,
                }) {
                    state.busy = false;
                    state.status = error;
                }
            }
            UiAction::CopyCode => {
                state.status = match state
                    .capability
                    .as_ref()
                    .map(|capability| clipboard.set_text(capability.join_code.clone()))
                {
                    Some(Ok(())) => "Lobby code copied.".into(),
                    _ => "The lobby code could not be copied.".into(),
                };
            }
            UiAction::TakeSeat(seat) => {
                if let Some(room_id) = model.snapshot.room_id() {
                    state.status = format!("Requesting seat {}…", seat + 1);
                    if let Err(error) = bridge.send(BridgeIntent::TakeSeat {
                        room_id: room_id.into(),
                        seat: *seat,
                    }) {
                        state.status = error;
                    }
                }
            }
            UiAction::ReleaseSeat => {
                if let Some(room_id) = model.snapshot.room_id() {
                    let _ = bridge.send(BridgeIntent::ReleaseSeat {
                        room_id: room_id.into(),
                    });
                }
            }
            UiAction::Leave => {
                if !state.confirm_leave {
                    state.confirm_leave = true;
                    state.status = "Choose Leave lobby again to confirm.".into();
                } else if let Some(room_id) = model.snapshot.room_id() {
                    state.confirm_leave = false;
                    let _ = bridge.send(BridgeIntent::Leave {
                        room_id: room_id.into(),
                    });
                }
            }
        }
    }
}

fn poll_clipboard(
    mut pending: ResMut<PendingClipboard>,
    mut fields: Query<(&Field, &mut EditableText)>,
    mut state: ResMut<UiState>,
) {
    let Some(result) = pending.0.as_mut().and_then(ClipboardRead::poll_result) else {
        return;
    };
    pending.0 = None;
    match result {
        Ok(text) if valid_join_code(text.trim()) => {
            for (field, mut input) in &mut fields {
                if *field == Field::Invitation {
                    input.queue_edit(TextEdit::SelectAll);
                    input.queue_edit(TextEdit::Insert(text.trim().to_ascii_uppercase().into()));
                }
            }
            state.status = "Valid lobby code pasted. Choose Join lobby.".into();
        }
        _ => state.status = "The clipboard does not contain a valid Poche lobby code.".into(),
    }
}

fn handle_bridge_notices(mut notices: MessageReader<BridgeNotice>, mut state: ResMut<UiState>) {
    for notice in notices.read() {
        match notice {
            BridgeNotice::Connected => state.status = "Connected; waiting for room state…".into(),
            BridgeNotice::RoomCreated(capability) => {
                state.capability = Some(capability.clone());
                state.status = "Lobby created.".into();
            }
            BridgeNotice::Snapshot(snapshot) if snapshot.room_id().is_some() => {
                state.busy = false;
                state.joined_once = true;
            }
            BridgeNotice::Command {
                operation,
                elapsed,
                result,
            } => {
                state.status = match result {
                    Ok(()) => format!(
                        "{operation} accepted in {:.1} ms",
                        elapsed.as_secs_f64() * 1000.
                    ),
                    Err(error) => format!("{operation} denied: {error}"),
                };
                if operation == &"leave_room" && result.is_ok() {
                    state.status = "You have left this lobby.".into();
                }
            }
            BridgeNotice::Disconnected(reason) => {
                state.busy = false;
                state.status = reason
                    .clone()
                    .unwrap_or_else(|| "Disconnected from SpacetimeDB.".into());
            }
            BridgeNotice::Error(error) => {
                state.busy = false;
                state.status.clone_from(error);
            }
            BridgeNotice::Snapshot(_) => {}
        }
    }
}

fn enter_room(
    model: Res<BridgeModel>,
    state: Res<UiState>,
    menu: Query<Entity, With<MainMenuRoot>>,
    room: Query<Entity, With<RoomRoot>>,
    cameras: Query<Entity, With<PocheUiCamera>>,
    mut commands: Commands,
) {
    if model.snapshot.room_id().is_none() || !room.is_empty() {
        return;
    }
    for entity in &menu {
        commands.entity(entity).despawn();
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands
        .spawn((
            RoomRoot,
            UiTargetCamera(camera),
            Node {
                width: percent(100.),
                height: percent(100.),
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgb(0.02, 0.045, 0.045)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(18.),
                    right: px(18.),
                    top: px(72.),
                    bottom: px(142.),
                    border_radius: BorderRadius::all(px(240.)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.035, 0.28, 0.20)),
                Outline::new(px(3.), px(0.), Color::srgb(0.33, 0.55, 0.4)),
            ));
            root.spawn((
                Text::new("POCHE · shared table"),
                TextFont::from_font_size(28.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(24.),
                    top: px(18.),
                    ..default()
                },
            ));
            root.spawn((
                RoomCodeLabel,
                Text::new("Lobby code"),
                TextFont::from_font_size(17.),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(130.),
                    top: px(22.),
                    ..default()
                },
            ));
            spawn_absolute_button(root, "Copy", UiAction::CopyCode, 22., 22., true);
            for seat in 0..2 {
                let top = if seat == 0 { 535. } else { 96. };
                root.spawn((
                    SeatLabel(seat),
                    Text::new(format!("Seat {} · empty", seat + 1)),
                    TextFont::from_font_size(20.),
                    TextColor(Color::WHITE),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(42.),
                        top: px(top),
                        ..default()
                    },
                ));
            }
            root.spawn((
                HandSummary,
                Text::new("Take a seat to receive a private hand."),
                TextFont::from_font_size(18.),
                TextColor(Color::srgb(0.92, 0.92, 0.84)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(26.),
                    bottom: px(110.),
                    ..default()
                },
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(18.),
                    right: px(18.),
                    bottom: px(18.),
                    min_height: px(78.),
                    padding: px(12.).all(),
                    column_gap: px(10.),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.055, 0.09, 0.095)),
            ))
            .with_children(|bar| {
                spawn_button(bar, "Take seat 1", UiAction::TakeSeat(0), true);
                spawn_button(bar, "Take seat 2", UiAction::TakeSeat(1), true);
                spawn_button(bar, "Stand up", UiAction::ReleaseSeat, false);
                spawn_button(bar, "Leave lobby", UiAction::Leave, false);
                bar.spawn((
                    LatencyLabel,
                    Text::new("Card drag: move · Q/E: rotate"),
                    TextFont::from_font_size(15.),
                    TextColor(Color::srgb(0.72, 0.82, 0.8)),
                ));
            });
            root.spawn((
                StatusLabel,
                Text::new(&state.status),
                TextFont::from_font_size(16.),
                TextColor(Color::srgb(0.98, 0.82, 0.52)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(24.),
                    top: px(52.),
                    ..default()
                },
            ));
        });
}

fn spawn_absolute_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: UiAction,
    right: f32,
    top: f32,
    primary: bool,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                position_type: PositionType::Absolute,
                right: px(right),
                top: px(top),
                padding: px(8.).all(),
                ..default()
            },
            BackgroundColor(if primary {
                Color::srgb(0.08, 0.48, 0.38)
            } else {
                Color::srgb(0.13, 0.26, 0.27)
            }),
        ))
        .with_child(Text::new(label));
}

fn sync_room_labels(
    model: Res<BridgeModel>,
    state: Res<UiState>,
    mut room_codes: Query<
        &mut Text,
        (
            With<RoomCodeLabel>,
            Without<SeatLabel>,
            Without<HandSummary>,
            Without<LatencyLabel>,
        ),
    >,
    mut seats: Query<
        (&SeatLabel, &mut Text, &mut Node),
        (
            Without<RoomCodeLabel>,
            Without<HandSummary>,
            Without<LatencyLabel>,
            Without<UiAction>,
        ),
    >,
    mut hands: Query<
        &mut Text,
        (
            With<HandSummary>,
            Without<RoomCodeLabel>,
            Without<SeatLabel>,
            Without<LatencyLabel>,
        ),
    >,
    mut latency: Query<
        &mut Text,
        (
            With<LatencyLabel>,
            Without<RoomCodeLabel>,
            Without<HandSummary>,
            Without<SeatLabel>,
        ),
    >,
    mut actions: Query<(&UiAction, &mut Node), Without<SeatLabel>>,
) {
    if !(model.is_changed() || state.is_changed()) {
        return;
    }
    for mut text in &mut room_codes {
        text.0 = state.capability.as_ref().map_or_else(
            || "Lobby code unavailable".into(),
            |value| value.join_code.clone(),
        );
    }
    let own_seat = model.snapshot.own_seat();
    for (seat, mut text, mut node) in &mut seats {
        let occupant = model
            .snapshot
            .members
            .iter()
            .find(|member| member.seat == Some(seat.0));
        text.0 = occupant.map_or_else(
            || format!("Seat {} · empty", seat.0 + 1),
            |member| {
                format!(
                    "Seat {} · {}{}",
                    seat.0 + 1,
                    member.display_name,
                    if member.is_self { " · you" } else { "" }
                )
            },
        );
        let presented_at_bottom = own_seat.map_or(seat.0 == 0, |own| own == seat.0);
        node.top = px(if presented_at_bottom { 535. } else { 96. });
    }
    for mut text in &mut hands {
        text.0 = if model.snapshot.hand.is_empty() {
            "Take a seat; cards are dealt when both seats are occupied.".into()
        } else {
            format!(
                "Your private hand · {} cards · drag any card to wiggle it for the other player",
                model.snapshot.hand.len()
            )
        };
    }
    for mut text in &mut latency {
        text.0 = model.last_command_latency.map_or_else(
            || "Card drag: move · Q/E: rotate".into(),
            |value| {
                format!(
                    "Last authority response {:.1} ms · drag · Q/E rotate",
                    value.as_secs_f64() * 1000.
                )
            },
        );
    }
    for (action, mut node) in &mut actions {
        node.display = match action {
            UiAction::TakeSeat(seat) => {
                let occupied = model
                    .snapshot
                    .members
                    .iter()
                    .any(|member| member.seat == Some(*seat));
                if own_seat.is_none() && !occupied {
                    Display::Flex
                } else {
                    Display::None
                }
            }
            UiAction::ReleaseSeat => {
                if own_seat.is_some() {
                    Display::Flex
                } else {
                    Display::None
                }
            }
            UiAction::Leave | UiAction::CopyCode => Display::Flex,
            UiAction::Create | UiAction::Paste | UiAction::Join => continue,
        };
    }
}

#[derive(Clone, Debug)]
struct DisplayPose {
    current: [f32; 3],
    target: [f32; 3],
    rotation_mdeg: [i32; 3],
    sequence: u64,
}

#[derive(Resource, Default)]
struct PoseDisplay(HashMap<String, DisplayPose>);

#[derive(Component)]
struct CardVisual(String);

#[derive(Resource, Default)]
struct DragState {
    card_key: Option<String>,
    last_cursor: Option<Vec2>,
    last_sent: Option<Instant>,
}

fn sync_card_entities(
    model: Res<BridgeModel>,
    mut poses: ResMut<PoseDisplay>,
    existing: Query<(Entity, &CardVisual)>,
    root: Query<Entity, With<RoomRoot>>,
    mut commands: Commands,
) {
    if !model.is_changed() || root.is_empty() {
        return;
    }
    let wanted: HashSet<_> = model
        .snapshot
        .card_poses
        .iter()
        .map(|pose| pose.card_key.clone())
        .collect();
    for (entity, card) in &existing {
        if !wanted.contains(&card.0) {
            commands.entity(entity).despawn();
            poses.0.remove(&card.0);
        }
    }
    let existing: HashSet<_> = existing.iter().map(|(_, card)| card.0.clone()).collect();
    let root = root.single().expect("one room root");
    for network in &model.snapshot.card_poses {
        let target = network.position_mm.map(|value| value as f32);
        let display = poses
            .0
            .entry(network.card_key.clone())
            .or_insert(DisplayPose {
                current: target,
                target,
                rotation_mdeg: network.rotation_mdeg,
                sequence: network.sequence,
            });
        display.target = target;
        display.rotation_mdeg = network.rotation_mdeg;
        display.sequence = display.sequence.max(network.sequence);
        if existing.contains(&network.card_key) {
            continue;
        }
        let own_face = model
            .snapshot
            .hand
            .iter()
            .find(|card| card.card_key == network.card_key)
            .map(|card| card.face.as_str());
        let label = own_face.unwrap_or("P");
        let color = if own_face.is_some() {
            Color::srgb(0.96, 0.94, 0.84)
        } else {
            Color::srgb(0.18, 0.24, 0.43)
        };
        commands.entity(root).with_child((
            CardVisual(network.card_key.clone()),
            Node {
                position_type: PositionType::Absolute,
                width: px(CARD_WIDTH),
                height: px(CARD_HEIGHT),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(8.)),
                ..default()
            },
            UiTransform::default(),
            BackgroundColor(color),
            Outline::new(px(2.), px(0.), Color::srgb(0.04, 0.04, 0.04)),
            children![(
                Text::new(label),
                TextFont::from_font_size(27.),
                TextColor(if own_face.is_some() {
                    Color::srgb(0.08, 0.08, 0.07)
                } else {
                    Color::srgb(0.82, 0.85, 0.94)
                }),
            )],
        ));
    }
}

fn drag_cards(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    model: Res<BridgeModel>,
    bridge: Res<BridgeHandle>,
    mut poses: ResMut<PoseDisplay>,
    mut drag: ResMut<DragState>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let own_identity = model.snapshot.identity.as_deref();
    let own_keys: HashSet<_> = model
        .snapshot
        .card_poses
        .iter()
        .filter(|pose| Some(pose.owner.as_str()) == own_identity)
        .map(|pose| pose.card_key.as_str())
        .collect();

    if mouse.just_pressed(MouseButton::Left) {
        drag.card_key = poses
            .0
            .iter()
            .filter(|(key, _)| own_keys.contains(key.as_str()))
            .filter_map(|(key, pose)| {
                let center = project_pose(pose.current, model.snapshot.own_seat(), window);
                let distance = center.distance(cursor);
                (distance <= CARD_HEIGHT * 0.65).then_some((key.clone(), distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(key, _)| key);
        drag.last_cursor = Some(cursor);
        drag.last_sent = None;
    }

    let Some(key) = drag.card_key.clone() else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        let seat = model.snapshot.own_seat();
        let physical = unproject_cursor(cursor, seat, window);
        if let Some(pose) = poses.0.get_mut(&key) {
            pose.current[0] = physical[0];
            pose.current[2] = physical[2];
            pose.current[1] = 160.;
            let delta = drag.last_cursor.map_or(0., |last| cursor.x - last.x);
            pose.rotation_mdeg[2] = pose.rotation_mdeg[2]
                .saturating_add((delta * 700.) as i32)
                .clamp(-360_000, 360_000);
            if keys.pressed(KeyCode::KeyQ) {
                pose.rotation_mdeg[1] = pose.rotation_mdeg[1].saturating_sub(1_500);
            }
            if keys.pressed(KeyCode::KeyE) {
                pose.rotation_mdeg[1] = pose.rotation_mdeg[1].saturating_add(1_500);
            }
        }
        drag.last_cursor = Some(cursor);
        if drag
            .last_sent
            .is_none_or(|last| last.elapsed() >= POSE_PERIOD)
        {
            send_pose(
                &key,
                &model.snapshot.card_poses,
                &mut poses,
                &bridge,
                model.snapshot.room_id(),
            );
            drag.last_sent = Some(Instant::now());
        }
    }
    if mouse.just_released(MouseButton::Left) {
        send_pose(
            &key,
            &model.snapshot.card_poses,
            &mut poses,
            &bridge,
            model.snapshot.room_id(),
        );
        drag.card_key = None;
        drag.last_cursor = None;
        drag.last_sent = None;
    }
}

fn send_pose(
    key: &str,
    network: &[CardPoseView],
    poses: &mut PoseDisplay,
    bridge: &BridgeHandle,
    room_id: Option<&str>,
) {
    let (Some(room_id), Some(source), Some(pose)) = (
        room_id,
        network.iter().find(|pose| pose.card_key == key),
        poses.0.get_mut(key),
    ) else {
        return;
    };
    pose.sequence = pose.sequence.saturating_add(1);
    let position_mm = pose.current.map(|value| value.round() as i32);
    let _ = bridge.send(BridgeIntent::SetCardPose {
        room_id: room_id.into(),
        card_id: source.card_id.clone(),
        sequence: pose.sequence,
        position_mm,
        rotation_mdeg: pose.rotation_mdeg,
    });
}

fn animate_and_place_cards(
    time: Res<Time>,
    model: Res<BridgeModel>,
    drag: Res<DragState>,
    windows: Query<&Window>,
    mut poses: ResMut<PoseDisplay>,
    mut cards: Query<(&CardVisual, &mut Node, &mut UiTransform)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let alpha = 1. - (-18. * time.delta_secs()).exp();
    for (key, pose) in &mut poses.0 {
        if drag.card_key.as_deref() != Some(key) {
            for axis in 0..3 {
                pose.current[axis] += (pose.target[axis] - pose.current[axis]) * alpha;
            }
        }
    }
    for (card, mut node, mut transform) in &mut cards {
        let Some(pose) = poses.0.get(&card.0) else {
            continue;
        };
        let center = project_pose(pose.current, model.snapshot.own_seat(), window);
        node.left = px(center.x - CARD_WIDTH * 0.5);
        node.top = px(center.y - CARD_HEIGHT * 0.5 - pose.current[1] * 0.025);
        transform.rotation = Rot2::degrees(pose.rotation_mdeg[2] as f32 / 1_000.);
    }
}

fn project_pose(position: [f32; 3], own_seat: Option<u8>, window: &Window) -> Vec2 {
    let (mut x, mut z) = (position[0], position[2]);
    if own_seat == Some(1) {
        x = -x;
        z = -z;
    }
    let scale = (window.width().min(window.height()) / 6_500.).clamp(0.075, 0.14);
    Vec2::new(
        window.width() * 0.5 + x * scale,
        window.height() * 0.48 + z * scale,
    )
}

fn unproject_cursor(cursor: Vec2, own_seat: Option<u8>, window: &Window) -> [f32; 3] {
    let scale = (window.width().min(window.height()) / 6_500.).clamp(0.075, 0.14);
    let mut x = (cursor.x - window.width() * 0.5) / scale;
    let mut z = (cursor.y - window.height() * 0.48) / scale;
    if own_seat == Some(1) {
        x = -x;
        z = -z;
    }
    [x.clamp(-9_000., 9_000.), 160., z.clamp(-9_000., 9_000.)]
}

fn update_status_labels(state: Res<UiState>, mut labels: Query<&mut Text, With<StatusLabel>>) {
    if state.is_changed() {
        for mut label in &mut labels {
            label.0.clone_from(&state.status);
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= 32 && !name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabletop_projection_round_trips_for_both_viewer_seats() {
        let window = Window {
            resolution: WindowResolution::new(1_180, 760),
            ..default()
        };
        for seat in [Some(0), Some(1)] {
            let source = [1_200., 160., -800.];
            let screen = project_pose(source, seat, &window);
            let round_trip = unproject_cursor(screen, seat, &window);
            assert!((source[0] - round_trip[0]).abs() < 0.01);
            assert!((source[2] - round_trip[2]).abs() < 0.01);
        }
    }

    #[test]
    fn named_authorities_select_distinct_endpoints() {
        let local = AuthorityEndpoint::select(Some("local"), None).expect("local profile");
        assert_eq!(local.uri, DEFAULT_URI);
        assert_eq!(local.database, DEFAULT_DATABASE);

        let maincloud =
            AuthorityEndpoint::select(Some("maincloud"), None).expect("maincloud profile");
        assert_eq!(maincloud.uri, MAINCLOUD_URI);
        assert_eq!(maincloud.database, MAINCLOUD_DATABASE);
    }

    #[test]
    fn explicit_database_overrides_a_named_authority() {
        let selected = AuthorityEndpoint::select(Some("maincloud"), Some("poche-staging-1"))
            .expect("maincloud override");
        assert_eq!(selected.database, "poche-staging-1");
        assert!(AuthorityEndpoint::select(Some("maincloud"), Some("Poche_bad")).is_err());
    }
}
