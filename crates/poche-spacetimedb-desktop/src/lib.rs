// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A deliberately game-shaped desktop surface over the authoritative Poche
//! `SpacetimeDB` model. Networked integer-millimetre poses are projected into
//! a Bevy 3D tabletop; rendering never becomes logical-state authority.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]

pub mod file_control;
pub mod identity_vault;
pub mod observability;

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::{RenderTarget, ScalingMode, Viewport, visibility::RenderLayers},
    clipboard::{Clipboard, ClipboardRead},
    core_pipeline::core_3d::Opaque3d,
    dev_tools::fps_overlay::{
        FpsOverlayConfig, FpsOverlayPlugin, FpsOverlaySystems, FrameTimeGraphConfig,
    },
    image::Image,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    log::LogPlugin,
    prelude::*,
    render::{
        Render, RenderApp, RenderPlugin, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_phase::ViewBinnedRenderPhases,
        render_resource::{
            Extent3d, PipelineCache, TextureDimension, TextureFormat, TextureUsages,
        },
        settings::{Backends, WgpuSettings},
        view::{ExtractedView, screenshot::Screenshot},
    },
    text::{EditableText, TextCursorStyle, TextEdit},
    window::{ExitCondition, PresentMode, PrimaryWindow, WindowResized, WindowResolution},
    winit::WinitPlugin,
};
use identity_vault::{IdentityAccount, IdentityVault};
use poche_bevy_spacetimedb::{
    BridgeHandle, BridgeIntent, BridgeModel, BridgeNotice, PocheSpacetimePlugin,
};
use poche_slug::{SlugFont, rasterize_text_rgba};
use poche_spacetimedb_client::{
    CardPoseView, ClientConfig, ClientSnapshot, DEFAULT_DATABASE, DEFAULT_URI, RoomCapability,
    valid_join_code,
};
use poche_spatial::{
    AabbMm, HalfExtentsMm, LayoutId, ObjectId, Point3Mm, SceneObjectKind, SpatialLayout, TableId,
    ZoneClassification, ZoneId, registered_layout,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const CARD_PICK_RADIUS_PX: f32 = 68.0;
const CARD_WORLD_WIDTH: f32 = 0.064;
const CARD_WORLD_HEIGHT: f32 = 0.088;
const CARD_WORLD_THICKNESS: f32 = 0.002;
const POSE_PERIOD: Duration = Duration::from_millis(50);
const FULL_TURN_MDEG: i32 = 360_000;
const ROTATION_REPEAT_DELAY: Duration = Duration::from_millis(300);
const ROTATION_REPEAT_PERIOD: Duration = Duration::from_millis(100);
const ROTATION_SNAP_MDEG: [i32; 6] = [0, 15_000, 30_000, 45_000, 60_000, 90_000];
const AUTOMATION_WIDTH: u32 = 1180;
const AUTOMATION_HEIGHT: u32 = 760;
const CAMERA_ORBIT_SENSITIVITY: f32 = 0.0045;
const CAMERA_PAN_SENSITIVITY: f32 = 0.0014;
const CAMERA_KEYBOARD_SPEED: f32 = 0.72;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.14;
const CAMERA_MIN_DISTANCE: f32 = 0.58;
const CAMERA_MAX_DISTANCE: f32 = 4.5;
const CAMERA_SMOOTHING: f32 = 10.0;
const CAMERA_MIN_PITCH: f32 = 18.0_f32.to_radians();
const CAMERA_MAX_PITCH: f32 = 78.0_f32.to_radians();
const TACTICAL_CAMERA_PITCH: f32 = 68.0_f32.to_radians();
const TACTICAL_CAMERA_DISTANCE: f32 = 2.6;
const TACTICAL_VIEW_HEIGHT: f32 = 2.1;
const TACTICAL_MIN_SCALE: f32 = 0.45;
const TACTICAL_MAX_SCALE: f32 = 2.5;
const FONT_BYTES: &[u8] = include_bytes!("../../poche-native-ui/assets/CaskaydiaCove-Regular.ttf");
const MAINCLOUD_URI: &str = "https://maincloud.spacetimedb.com";
const MAINCLOUD_DATABASE: &str = "poche-6quz6";

fn hidden_fps_overlay_config(font: Handle<Font>) -> FpsOverlayConfig {
    FpsOverlayConfig {
        text_config: TextFont {
            font: FontSource::Handle(font),
            font_size: FontSize::Px(18.0),
            ..default()
        },
        text_color: Color::srgb(0.96, 0.88, 0.58),
        enabled: false,
        refresh_interval: Duration::from_millis(100),
        frame_time_graph_config: FrameTimeGraphConfig {
            enabled: false,
            min_fps: 30.0,
            target_fps: 60.0,
        },
    }
}

fn set_fps_overlay_visible(config: &mut FpsOverlayConfig, visible: bool) {
    config.enabled = visible;
    config.frame_time_graph_config.enabled = visible;
}

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

    fn client_config(&self, account_id: impl Into<String>) -> ClientConfig {
        ClientConfig {
            uri: self.uri.clone(),
            database: self.database.clone(),
            account_id: account_id.into(),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsBackend {
    Auto,
    Dx12,
    Vulkan,
}

impl Default for GraphicsBackend {
    fn default() -> Self {
        if cfg!(target_os = "windows") {
            Self::Dx12
        } else {
            Self::Auto
        }
    }
}

impl GraphicsBackend {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "dx12" => Ok(Self::Dx12),
            "vulkan" => Ok(Self::Vulkan),
            _ => Err(format!(
                "--graphics-backend expects auto, dx12, or vulkan; got {value:?}"
            )),
        }
    }

    const fn backends(self) -> Option<Backends> {
        match self {
            Self::Auto => None,
            Self::Dx12 => Some(Backends::DX12),
            Self::Vulkan => Some(Backends::VULKAN),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub render_mode: RenderMode,
    pub graphics_backend: GraphicsBackend,
    pub file_control: Option<file_control::FileControlOptions>,
    pub identity_vault_path: Option<PathBuf>,
    pub log_file_path: Option<PathBuf>,
    pub authority: AuthorityEndpoint,
}

#[derive(Debug, Resource)]
struct RotationSnap {
    index: usize,
}

impl Default for RotationSnap {
    fn default() -> Self {
        Self { index: 3 }
    }
}

impl RotationSnap {
    fn mdeg(&self) -> i32 {
        ROTATION_SNAP_MDEG[self.index]
    }

    fn step_mdeg(&self) -> i32 {
        let snap = self.mdeg();
        if snap == 0 { 5_000 } else { snap }
    }

    fn cycle(&mut self) {
        self.index = (self.index + 1) % ROTATION_SNAP_MDEG.len();
    }

    fn label(&self) -> String {
        let snap = self.mdeg();
        if snap == 0 {
            "Rotation snap: off".into()
        } else {
            format!("Rotation snap: {}°", snap / 1_000)
        }
    }
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
            "--identity-vault" => {
                options.identity_vault_path = Some(PathBuf::from(
                    args.next().ok_or("--identity-vault requires a path")?,
                ));
            }
            "--log-file" => {
                options.log_file_path = Some(PathBuf::from(
                    args.next().ok_or("--log-file requires a path")?,
                ));
            }
            "--graphics-backend" => {
                options.graphics_backend = GraphicsBackend::parse(
                    &args.next().ok_or("--graphics-backend requires a value")?,
                )?;
            }
            "--windowless" => options.render_mode = RenderMode::WindowlessImage,
            "--help" | "-h" => {
                println!(
                    "poche [--server local|maincloud|URL] [--database NAME]\n\
                     \x20     [--identity-vault PATH] [--log-file FILE_OR_EXISTING_DIRECTORY]\n\
                     \x20     [--graphics-backend auto|dx12|vulkan]\n\
                     \x20     [--control-root PATH --instance-id ID] [--windowless]\n\
                     The safe default is local. The maincloud shorthand selects\n\
                     https://maincloud.spacetimedb.com and poche-6quz6. Explicit options override\n\
                     POCHE_SPACETIMEDB_URI and POCHE_SPACETIMEDB_DATABASE. Developer control\n\
                     publishes a fresh file endpoint; --windowless avoids an OS window. On\n\
                     Windows, DX12 is the default graphics backend; Vulkan remains selectable."
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
    let log_path = if windowless {
        None
    } else {
        Some(observability::initialize(options.log_file_path.as_deref())?)
    };
    let mut wgpu_settings = WgpuSettings::default();
    if let Some(backends) = options.graphics_backend.backends() {
        wgpu_settings.backends = Some(backends);
    }
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
        .set(LogPlugin {
            custom_layer: observability::file_log_layer,
            ..default()
        })
        .set(ImagePlugin::default_nearest())
        .set(RenderPlugin {
            render_creation: wgpu_settings.into(),
            synchronous_pipeline_compilation: windowless,
            ..default()
        });
    if windowless {
        plugins = plugins.disable::<WinitPlugin>().disable::<LogPlugin>();
    }

    let control_root = options
        .file_control
        .as_ref()
        .map(|control| control.root.clone());
    let identity_vault_path = options
        .identity_vault_path
        .clone()
        .or_else(|| {
            control_root
                .as_ref()
                .map(|root| root.join("identity-vault-v1.json"))
        })
        .map_or_else(IdentityVault::default_path, Ok)?;
    let identity_vault = IdentityVault::load(identity_vault_path)?;
    let control = options
        .file_control
        .map(|control| file_control::FileControlPlugin::prepare(control, options.render_mode))
        .transpose()?;

    let mut app = App::new();
    app.add_plugins(plugins);
    app.add_plugins(TableRenderReadinessPlugin);
    if let Some(path) = &log_path {
        tracing::info!(log_file = %path.display(), "Poche durable logging initialized");
    }
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(FONT_BYTES.to_vec()));
    if !windowless {
        app.add_plugins(FpsOverlayPlugin {
            config: hidden_fps_overlay_config(font.clone()),
        });
    }
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
        .insert_resource(identity_vault)
        .add_plugins(PocheSpacetimePlugin)
        .init_resource::<Clipboard>()
        .insert_resource(UiState::for_authority(&authority))
        .init_resource::<PendingClipboard>()
        .init_resource::<PoseDisplay>()
        .init_resource::<DragState>()
        .init_resource::<RotationSnap>()
        .init_resource::<CameraOptions>()
        .init_resource::<TableCameraController>()
        .add_message::<ButtonActivation>()
        .add_observer(activate_button)
        .add_systems(
            Update,
            toggle_fps_overlay.before(FpsOverlaySystems::Customize),
        )
        .add_systems(
            Startup,
            (setup_render_target, setup_spatial_renderer, setup_menu).chain(),
        )
        .add_systems(
            Update,
            (
                handle_buttons,
                handle_escape_key,
                poll_clipboard,
                handle_bridge_notices,
                advance_room_loading,
                sync_frontend_screen,
                enter_room,
                sync_escape_menu,
                sync_room_labels,
                sync_card_entities,
                sync_player_entities,
                update_rendered_scene_metrics,
                update_table_camera,
                update_hand_camera,
                drag_cards,
                animate_and_place_cards,
                update_status_labels,
                update_rotation_snap_labels,
                log_window_resize,
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
    screen: UiScreen,
    pending_flow: Option<PendingFlow>,
    active_account_id: Option<String>,
    account_label: String,
    display_name: String,
    capability: Option<RoomCapability>,
    confirm_leave: bool,
    escape_menu_open: bool,
    escape_menu_page: EscapeMenuPage,
    rendered_card_count: usize,
    rendered_player_count: usize,
    room_scene_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiScreen {
    IdentityGate,
    Connecting,
    LoadingRoom,
    ResumeOffer,
    MainMenu,
    Table,
    LobbyEnded,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum EscapeMenuPage {
    #[default]
    Main,
    Options,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingFlow {
    SelectIdentity,
    CreateRoom,
    JoinRoom,
    LeaveRoom,
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
            status: format!("Using {}. Choose an identity.", authority.summary()),
            screen: UiScreen::IdentityGate,
            pending_flow: None,
            active_account_id: None,
            account_label: String::new(),
            display_name: String::new(),
            capability: None,
            confirm_leave: false,
            escape_menu_open: false,
            escape_menu_page: EscapeMenuPage::Main,
            rendered_card_count: 0,
            rendered_player_count: 0,
            room_scene_generation: 0,
        }
    }
}

#[derive(Resource, Default)]
struct PendingClipboard(Option<ClipboardRead>);

#[derive(Component)]
struct MainMenuRoot;

#[derive(Component)]
struct FrontendRoot(UiScreen);

#[derive(Component)]
struct RoomRoot;

#[derive(Component)]
struct PocheUiCamera;

#[derive(Component, Clone, ExtractComponent)]
struct TabletopCamera {
    scene_generation: u64,
}

#[derive(Clone, Default, Resource)]
struct TableRenderReadiness(Arc<AtomicU64>);

impl TableRenderReadiness {
    fn mark_rendered(&self, scene_generation: u64) {
        self.0.fetch_max(scene_generation, Ordering::Release);
    }

    fn has_rendered(&self, scene_generation: u64) -> bool {
        scene_generation != 0 && self.0.load(Ordering::Acquire) >= scene_generation
    }
}

struct TableRenderReadinessPlugin;

impl Plugin for TableRenderReadinessPlugin {
    fn build(&self, app: &mut App) {
        let readiness = TableRenderReadiness::default();
        app.insert_resource(readiness.clone())
            .add_plugins(ExtractComponentPlugin::<TabletopCamera>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(readiness)
            .add_systems(Render, report_rendered_table.after(RenderSystems::Render));
    }
}

fn report_rendered_table(
    readiness: Res<TableRenderReadiness>,
    pipeline_cache: Res<PipelineCache>,
    opaque_phases: Res<ViewBinnedRenderPhases<Opaque3d>>,
    cameras: Query<(&TabletopCamera, &ExtractedView)>,
) {
    for (camera, view) in &cameras {
        let Some(phase) = opaque_phases.get(&view.retained_view_entity) else {
            continue;
        };
        let has_meshes = !phase.multidrawable_meshes.is_empty()
            || !phase.batchable_meshes.is_empty()
            || !phase.unbatchable_meshes.is_empty();
        let pipelines_ready = phase
            .multidrawable_meshes
            .keys()
            .all(|key| pipeline_cache.get_render_pipeline(key.pipeline).is_some())
            && phase
                .batchable_meshes
                .keys()
                .all(|(key, _)| pipeline_cache.get_render_pipeline(key.pipeline).is_some())
            && phase
                .unbatchable_meshes
                .keys()
                .all(|(key, _)| pipeline_cache.get_render_pipeline(key.pipeline).is_some());
        if has_meshes && pipelines_ready {
            // This runs after the render graph. The next main-world update can
            // remove the opaque loading curtain without exposing a frame in
            // which Bevy has not yet drawn the static table geometry.
            readiness.mark_rendered(camera.scene_generation);
        }
    }
}

#[derive(Component)]
struct HandCamera;

#[derive(Component)]
struct SpatialPlayer;

#[derive(Resource)]
struct SpatialAssets {
    card_mesh: Handle<Mesh>,
    card_face_material: Handle<StandardMaterial>,
    card_back_material: Handle<StandardMaterial>,
    card_label_mesh: Handle<Mesh>,
    avatar_mesh: Handle<Mesh>,
    self_avatar_material: Handle<StandardMaterial>,
    peer_avatar_material: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct CanonicalLayout(SpatialLayout);

#[derive(Component)]
struct StatusLabel;

#[derive(Component)]
struct RoomCodeLabel;

#[derive(Component)]
struct SeatLabel(u8);

#[derive(Component)]
struct HandSummary;

#[derive(Component)]
struct GameStatusLabel;

#[derive(Component)]
struct LatencyLabel;

#[derive(Component)]
struct RotationSnapLabel;

#[derive(Component)]
struct PlayerListLabel;

#[derive(Component)]
struct ActivityLogLabel;

#[derive(Component)]
struct EscapeMenuRoot;

#[derive(Component)]
struct EscapeMainPanel;

#[derive(Component)]
struct EscapeOptionsPanel;

#[derive(Component)]
struct InvertCameraYLabel;

#[derive(Component)]
struct LeaveButtonLabel;

#[derive(Component, Clone, Copy, Eq, PartialEq)]
enum Field {
    IdentityLabel,
    Invitation,
}

#[derive(Component, Clone)]
enum UiAction {
    CreateIdentity,
    SelectIdentity(String),
    RefreshIdentities,
    PreviousIdentity,
    NextIdentity,
    OpenIdentities,
    CancelConnecting,
    ResumeLobby,
    SkipResume,
    ReturnToTitle,
    Create,
    Paste,
    Join,
    JoinHistory(String),
    CopyCode,
    TakeSeat(u8),
    ReleaseSeat,
    Bid(u8),
    PlayFirstCard,
    Leave,
    CycleRotationSnap,
    ResumeMenu,
    OpenOptions,
    CloseOptions,
    ToggleCameraYInversion,
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

fn setup_spatial_renderer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let layout = registered_layout(
        TableId::new(1),
        LayoutId::new(2, 1).expect("two-player layout id is valid"),
    )
    .expect("registered two-player layout is valid");
    commands.insert_resource(ClearColor(Color::srgb(0.012, 0.022, 0.026)));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.84, 0.9),
        brightness: 220.0,
        affects_lightmapped_meshes: true,
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-2.5, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(0).with(1),
    ));

    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.035, 0.055, 0.06),
        perceptual_roughness: 0.95,
        ..default()
    });
    let table_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.035, 0.30, 0.19),
        perceptual_roughness: 0.84,
        ..default()
    });
    let seat_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.10, 0.045),
        perceptual_roughness: 0.78,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(3.0, 0.12, 3.0))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(0.0, -0.08, 0.0),
        RenderLayers::layer(0),
    ));
    for object in layout.scene_objects() {
        let position = point_to_world(object.pose.translation);
        match object.kind {
            SceneObjectKind::Table => {
                commands.spawn((
                    Mesh3d(meshes.add(cuboid_from_half_extents(object.half_extents))),
                    MeshMaterial3d(table_material.clone()),
                    Transform::from_translation(position),
                    RenderLayers::layer(0),
                ));
            }
            SceneObjectKind::Seat => {
                commands.spawn((
                    Mesh3d(meshes.add(Cylinder::new(0.22, 0.12))),
                    MeshMaterial3d(seat_material.clone()),
                    Transform::from_translation(position),
                    RenderLayers::layer(0),
                ));
            }
            SceneObjectKind::ScoreSheet => {
                commands.spawn((
                    Mesh3d(meshes.add(cuboid_from_half_extents(object.half_extents))),
                    MeshMaterial3d(materials.add(Color::srgb(0.88, 0.84, 0.67))),
                    Transform::from_translation(position),
                    RenderLayers::layer(0),
                ));
            }
            SceneObjectKind::Zone => {
                let color = match object.id {
                    ObjectId::Zone(ZoneId::Play) => Color::srgba(0.92, 0.72, 0.18, 0.22),
                    ObjectId::Zone(ZoneId::Hand(_)) => Color::srgba(0.16, 0.62, 0.78, 0.18),
                    ObjectId::Zone(ZoneId::Won(_)) => Color::srgba(0.62, 0.28, 0.74, 0.18),
                    _ => Color::srgba(0.82, 0.86, 0.9, 0.12),
                };
                commands.spawn((
                    Mesh3d(meshes.add(cuboid_from_half_extents(object.half_extents))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: color,
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_translation(position),
                    RenderLayers::layer(0),
                ));
            }
            SceneObjectKind::Player => {}
        }
    }

    commands.insert_resource(SpatialAssets {
        card_mesh: meshes.add(Cuboid::new(
            CARD_WORLD_WIDTH,
            CARD_WORLD_THICKNESS,
            CARD_WORLD_HEIGHT,
        )),
        card_face_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.96, 0.94, 0.84),
            perceptual_roughness: 0.8,
            ..default()
        }),
        card_back_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.20, 0.48),
            perceptual_roughness: 0.72,
            ..default()
        }),
        card_label_mesh: meshes.add(Plane3d::default()),
        avatar_mesh: meshes.add(Capsule3d::new(0.12, 0.24)),
        self_avatar_material: materials.add(Color::srgb(0.92, 0.72, 0.20)),
        peer_avatar_material: materials.add(Color::srgb(0.22, 0.48, 0.82)),
    });
    commands.insert_resource(CanonicalLayout(layout));
}

fn spawn_spatial_cameras(
    commands: &mut Commands,
    surface: &RenderSurface,
    table_transform: Transform,
    scene_generation: u64,
) {
    let mut camera = commands.spawn((
        Camera3d::default(),
        Camera::default(),
        table_transform,
        TabletopCamera { scene_generation },
        RenderLayers::layer(0),
    ));
    if let Some(target) = surface.render_target() {
        camera.insert(target);
    }
    let mut hand_camera = commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: false,
            order: 1,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        Transform::from_xyz(0.0, 0.48, 0.52).looking_at(Vec3::new(0.0, 0.04, 0.52), Vec3::NEG_Z),
        HandCamera,
        RenderLayers::layer(1),
    ));
    if let Some(target) = surface.render_target() {
        hand_camera.insert(target);
    }
}

fn point_to_world(point: Point3Mm) -> Vec3 {
    Vec3::new(
        point.x.get() as f32,
        point.y.get() as f32,
        point.z.get() as f32,
    ) / 1_000.0
}

fn cuboid_from_half_extents(half: HalfExtentsMm) -> Cuboid {
    Cuboid::new(
        half.x as f32 * 0.002,
        half.y as f32 * 0.002,
        half.z as f32 * 0.002,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraPose {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl CameraPose {
    fn transform(self) -> Transform {
        let horizontal = self.distance * self.pitch.cos();
        let offset = Vec3::new(
            self.yaw.sin() * horizontal,
            self.distance * self.pitch.sin(),
            self.yaw.cos() * horizontal,
        );
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}

#[derive(Resource, Debug)]
struct TableCameraController {
    current: CameraPose,
    target: CameraPose,
    perspective_target: CameraPose,
    tactical_target: CameraPose,
    current_orthographic_scale: f32,
    target_orthographic_scale: f32,
    mode: TableCameraMode,
    last_seat: CameraSeat,
}

#[derive(Resource, Debug)]
struct CameraOptions {
    invert_y: bool,
}

impl Default for CameraOptions {
    fn default() -> Self {
        Self { invert_y: true }
    }
}

impl CameraOptions {
    fn invert_y_label(&self) -> String {
        format!(
            "Invert camera Y: {}",
            if self.invert_y { "On" } else { "Off" }
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TableCameraMode {
    #[default]
    Perspective,
    Tactical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CameraSeat {
    Uninitialized,
    Spectator,
    Seated(u8),
}

impl From<Option<u8>> for CameraSeat {
    fn from(seat: Option<u8>) -> Self {
        seat.map_or(Self::Spectator, Self::Seated)
    }
}

impl Default for TableCameraController {
    fn default() -> Self {
        let pose = camera_home(None);
        Self {
            current: pose,
            target: pose,
            perspective_target: pose,
            tactical_target: tactical_camera_home(None),
            current_orthographic_scale: 1.0,
            target_orthographic_scale: 1.0,
            mode: TableCameraMode::Perspective,
            last_seat: CameraSeat::Uninitialized,
        }
    }
}

impl TableCameraController {
    fn reset_for_seat(&mut self, seat: Option<u8>, immediate: bool) {
        self.perspective_target = camera_home(seat);
        self.tactical_target = tactical_camera_home(seat);
        self.target = match self.mode {
            TableCameraMode::Perspective => self.perspective_target,
            TableCameraMode::Tactical => self.tactical_target,
        };
        self.target_orthographic_scale = 1.0;
        if immediate {
            self.current = self.target;
            self.current_orthographic_scale = self.target_orthographic_scale;
        }
        self.last_seat = seat.into();
    }

    fn toggle_mode(&mut self) {
        match self.mode {
            TableCameraMode::Perspective => {
                self.perspective_target = self.target;
                self.target = self.tactical_target;
                self.mode = TableCameraMode::Tactical;
            }
            TableCameraMode::Tactical => {
                self.tactical_target = self.target;
                self.target = self.perspective_target;
                self.mode = TableCameraMode::Perspective;
            }
        }
    }
}

fn camera_home(seat: Option<u8>) -> CameraPose {
    match seat {
        Some(0) => CameraPose {
            focus: Vec3::new(0.0, 0.02, -0.08),
            yaw: 0.0,
            pitch: 43.0_f32.to_radians(),
            distance: 1.88,
        },
        Some(1) => CameraPose {
            focus: Vec3::new(0.0, 0.02, 0.08),
            yaw: std::f32::consts::PI,
            pitch: 43.0_f32.to_radians(),
            distance: 1.88,
        },
        _ => CameraPose {
            focus: Vec3::new(0.0, 0.02, 0.0),
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 46.0_f32.to_radians(),
            distance: 1.94,
        },
    }
}

fn tactical_camera_home(seat: Option<u8>) -> CameraPose {
    CameraPose {
        focus: Vec3::new(0.0, 0.02, 0.0),
        yaw: camera_home(seat).yaw,
        pitch: TACTICAL_CAMERA_PITCH,
        distance: TACTICAL_CAMERA_DISTANCE,
    }
}

#[cfg(test)]
fn player_camera_transform(seat: Option<u8>) -> Transform {
    camera_home(seat).transform()
}

fn setup_menu(
    mut commands: Commands,
    surface: Res<RenderSurface>,
    authority: Res<AuthorityEndpoint>,
    state: Res<UiState>,
    vault: Res<IdentityVault>,
    model: Res<BridgeModel>,
) {
    let mut camera = commands.spawn((Camera2d, IsDefaultUiCamera, PocheUiCamera));
    if let Some(target) = surface.render_target() {
        camera.insert(target);
    }
    let camera = camera.id();
    spawn_frontend(
        &mut commands,
        camera,
        &authority,
        &state,
        &vault,
        model.snapshot.room_id().is_some(),
    );
}

fn spawn_frontend(
    commands: &mut Commands,
    camera: Entity,
    authority: &AuthorityEndpoint,
    state: &UiState,
    vault: &IdentityVault,
    has_resumable_room: bool,
) {
    let accounts = vault
        .accounts_for(&authority.uri, &authority.database)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let mut root = commands.spawn((
        FrontendRoot(state.screen),
        UiTargetCamera(camera),
        Node {
            width: percent(100.),
            height: percent(100.),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(14.),
            padding: px(28.).all(),
            ..default()
        },
        BackgroundColor(Color::srgb(0.025, 0.055, 0.06)),
    ));
    if state.screen == UiScreen::MainMenu {
        root.insert(MainMenuRoot);
    }
    root.with_children(|parent| match state.screen {
        UiScreen::IdentityGate => {
            spawn_brand(parent, authority);
            parent.spawn((
                Text::new("WHO IS PLAYING?"),
                TextFont::from_font_size(28.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            if accounts.is_empty() {
                parent.spawn((
                    Text::new("Create the first identity on this installation."),
                    TextFont::from_font_size(17.),
                    TextColor(Color::srgb(0.72, 0.82, 0.8)),
                ));
            } else {
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8.),
                        min_width: px(360.),
                        ..default()
                    })
                    .with_children(|list| {
                        for account in &accounts {
                            spawn_button(
                                list,
                                &account.label,
                                UiAction::SelectIdentity(account.account_id.clone()),
                                true,
                            );
                        }
                    });
            }
            parent.spawn((
                Text::new("Create another identity"),
                TextFont::from_font_size(17.),
                TextColor(Color::srgb(0.78, 0.86, 0.82)),
            ));
            spawn_field(parent, Field::IdentityLabel, 380., 32, false);
            spawn_button(parent, "Create identity", UiAction::CreateIdentity, false);
            spawn_button(parent, "Refresh identities", UiAction::RefreshIdentities, false);
            spawn_frontend_status(parent, state);
        }
        UiScreen::Connecting => {
            spawn_brand(parent, authority);
            parent.spawn((
                Text::new(format!("SIGNING IN AS {}", state.account_label)),
                TextFont::from_font_size(28.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            parent.spawn((
                Text::new("Recovering the protected credential and asking the authority for this account's active room…"),
                TextFont::from_font_size(17.),
                TextColor(Color::srgb(0.72, 0.82, 0.8)),
                Node { max_width: px(680.), ..default() },
            ));
            spawn_button(parent, "Cancel", UiAction::CancelConnecting, false);
            spawn_frontend_status(parent, state);
        }
        UiScreen::LoadingRoom => {
            spawn_brand(parent, authority);
            parent.spawn((
                Text::new("PREPARING THE TABLE"),
                TextFont::from_font_size(32.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            parent.spawn((
                Text::new("Synchronizing the lobby, seats, private hand, and shared 3D scene…"),
                TextFont::from_font_size(18.),
                TextColor(Color::srgb(0.78, 0.86, 0.82)),
                Node {
                    max_width: px(680.),
                    ..default()
                },
            ));
            parent.spawn((
                Text::new("●  ●  ●"),
                TextFont::from_font_size(24.),
                TextColor(Color::srgb(0.34, 0.78, 0.65)),
            ));
            spawn_frontend_status(parent, state);
        }
        UiScreen::ResumeOffer => {
            spawn_identity_selector(parent, state);
            parent.spawn((
                Text::new("UNFINISHED LOBBY FOUND"),
                TextFont::from_font_size(32.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            parent.spawn((
                Text::new(format!(
                    "{} still belongs to a shared table. Rejoin with the same authenticated identity and resume its seat and private hand.",
                    state.account_label
                )),
                TextFont::from_font_size(18.),
                TextColor(Color::srgb(0.82, 0.88, 0.84)),
                Node { max_width: px(700.), ..default() },
            ));
            spawn_button(parent, "Rejoin lobby", UiAction::ResumeLobby, true);
            spawn_button(parent, "Not now", UiAction::SkipResume, false);
            spawn_frontend_status(parent, state);
        }
        UiScreen::MainMenu => {
            spawn_identity_selector(parent, state);
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
            if has_resumable_room {
                spawn_button(parent, "Resume existing lobby", UiAction::ResumeLobby, true);
            }
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
            if let Some(account) = state
                .active_account_id
                .as_deref()
                .and_then(|account_id| vault.account(account_id))
                && !account.lobby_history.is_empty()
            {
                parent.spawn((
                    Text::new("RECENT LOBBIES"),
                    TextFont::from_font_size(18.),
                    TextColor(Color::srgb(0.96, 0.88, 0.58)),
                    Node {
                        margin: px(8.).top(),
                        ..default()
                    },
                ));
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(7.),
                        min_width: px(420.),
                        ..default()
                    })
                    .with_children(|history| {
                        for entry in account.lobby_history.iter().take(4) {
                            spawn_button(
                                history,
                                &format!("Join {}", entry.join_code),
                                UiAction::JoinHistory(entry.join_code.clone()),
                                false,
                            );
                        }
                    });
            }
            spawn_frontend_status(parent, state);
            parent.spawn((
                Text::new("F3 · performance overlay"),
                TextFont::from_font_size(14.),
                TextColor(Color::srgb(0.52, 0.65, 0.64)),
            ));
        }
        UiScreen::LobbyEnded => {
            spawn_identity_selector(parent, state);
            parent.spawn((
                Text::new("YOU HAVE LEFT THE LOBBY"),
                TextFont::from_font_size(34.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
            ));
            parent.spawn((
                Text::new("Your authenticated identity remains available. A valid lobby code can be used to join that room again."),
                TextFont::from_font_size(18.),
                TextColor(Color::srgb(0.78, 0.86, 0.82)),
                Node { max_width: px(680.), ..default() },
            ));
            spawn_button(parent, "Return to title", UiAction::ReturnToTitle, true);
            spawn_frontend_status(parent, state);
        }
        UiScreen::Table => {}
    });
}

fn spawn_brand(parent: &mut ChildSpawnerCommands, authority: &AuthorityEndpoint) {
    parent.spawn((
        Text::new("POCHE"),
        TextFont::from_font_size(50.),
        TextColor(Color::srgb(0.96, 0.88, 0.58)),
    ));
    parent.spawn((
        Text::new(format!("Authority: {}", authority.summary())),
        TextFont::from_font_size(15.),
        TextColor(Color::srgb(0.58, 0.72, 0.7)),
    ));
}

fn spawn_identity_selector(parent: &mut ChildSpawnerCommands, state: &UiState) {
    parent
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(22.),
            align_items: AlignItems::Center,
            column_gap: px(8.),
            ..default()
        })
        .with_children(|row| {
            spawn_button(row, "‹", UiAction::PreviousIdentity, false);
            spawn_button(
                row,
                if state.account_label.is_empty() {
                    "Choose identity"
                } else {
                    &state.account_label
                },
                UiAction::OpenIdentities,
                false,
            );
            spawn_button(row, "›", UiAction::NextIdentity, false);
        });
}

fn spawn_frontend_status(parent: &mut ChildSpawnerCommands, state: &UiState) {
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
    mut rotation_snap: ResMut<RotationSnap>,
    mut camera_options: ResMut<CameraOptions>,
    authority: Res<AuthorityEndpoint>,
    mut vault: ResMut<IdentityVault>,
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
            UiAction::CreateIdentity => {
                let label = field_value(Field::IdentityLabel).trim().to_owned();
                match vault
                    .create(&label, &authority.uri, &authority.database)
                    .and_then(|account| {
                        begin_identity_selection(&mut state, &account, &bridge, &authority)
                    }) {
                    Ok(()) => {}
                    Err(error) => state.status = error,
                }
            }
            UiAction::SelectIdentity(account_id) => {
                let result = vault
                    .reload()
                    .and_then(|()| {
                        vault.account(account_id).cloned().ok_or_else(|| {
                            "that identity is no longer in the local catalogue".into()
                        })
                    })
                    .and_then(|account| {
                        if !account.belongs_to(&authority.uri, &authority.database) {
                            return Err("that identity belongs to a different authority".into());
                        }
                        begin_identity_selection(&mut state, &account, &bridge, &authority)
                    });
                if let Err(error) = result {
                    state.status = error;
                }
            }
            UiAction::RefreshIdentities => {
                state.status = match vault.reload() {
                    Ok(()) => "Identity catalogue refreshed.".into(),
                    Err(error) => error,
                };
            }
            UiAction::PreviousIdentity | UiAction::NextIdentity => {
                let step = if matches!(action, UiAction::PreviousIdentity) {
                    -1
                } else {
                    1
                };
                match adjacent_account(&mut vault, &authority, &state, step).and_then(|account| {
                    begin_identity_selection(&mut state, &account, &bridge, &authority)
                }) {
                    Ok(()) => {}
                    Err(error) => state.status = error,
                }
            }
            UiAction::OpenIdentities => {
                let _ = bridge.send(BridgeIntent::Disconnect);
                state.screen = UiScreen::IdentityGate;
                state.pending_flow = None;
                state.busy = false;
                state.status = match vault.reload() {
                    Ok(()) => "Choose an identity for this game window.".into(),
                    Err(error) => error,
                };
            }
            UiAction::CancelConnecting => {
                let _ = bridge.send(BridgeIntent::Disconnect);
                state.screen = UiScreen::IdentityGate;
                state.pending_flow = None;
                state.busy = false;
                state.status = "Sign-in cancelled. Choose an identity.".into();
            }
            UiAction::ResumeLobby => {
                if model.snapshot.room_id().is_some() {
                    begin_room_loading(&mut state, "Rejoining this identity's active lobby…");
                } else {
                    state.status = "This identity no longer has an active lobby.".into();
                }
            }
            UiAction::SkipResume => {
                state.screen = UiScreen::MainMenu;
                state.status =
                    "Lobby left waiting; you can resume it from this title screen.".into();
            }
            UiAction::ReturnToTitle => {
                state.screen = UiScreen::MainMenu;
                state.status = "Ready to create or join a lobby.".into();
            }
            UiAction::Paste => {
                pending.0 = Some(clipboard.fetch_text());
            }
            UiAction::Create | UiAction::Join | UiAction::JoinHistory(_) if state.busy => {}
            UiAction::Create => {
                if state.active_account_id.is_none() || !model.connected {
                    state.status = "Choose and connect an identity before creating a lobby.".into();
                    continue;
                }
                state.busy = true;
                state.pending_flow = Some(PendingFlow::CreateRoom);
                begin_room_loading(&mut state, "Creating and synchronizing a shared table…");
                if let Err(error) = bridge.send(BridgeIntent::Create {
                    display_name: state.display_name.clone(),
                }) {
                    state.busy = false;
                    state.pending_flow = None;
                    state.screen = UiScreen::MainMenu;
                    state.status = error;
                }
            }
            UiAction::Join => {
                let code = field_value(Field::Invitation).trim().to_ascii_uppercase();
                begin_joining_lobby(&mut state, &model, &bridge, code);
            }
            UiAction::JoinHistory(code) => {
                begin_joining_lobby(&mut state, &model, &bridge, code.clone());
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
            UiAction::Bid(tricks) => {
                if let Some(room_id) = model.snapshot.room_id() {
                    state.status = format!("Bidding {tricks} trick(s)…");
                    if let Err(error) = bridge.send(BridgeIntent::Bid {
                        room_id: room_id.into(),
                        tricks: *tricks,
                    }) {
                        state.status = error;
                    }
                }
            }
            UiAction::PlayFirstCard => {
                if let (Some(room_id), Some(card)) =
                    (model.snapshot.room_id(), model.snapshot.hand.first())
                {
                    state.status = format!("Playing {}…", card.face);
                    if let Err(error) = bridge.send(BridgeIntent::PlayCard {
                        room_id: room_id.into(),
                        card_id: card.card_id.clone(),
                    }) {
                        state.status = error;
                    }
                }
            }
            UiAction::Leave => {
                if let Err(error) = activate_leave(&mut state, model.snapshot.room_id(), &bridge) {
                    state.status = error;
                }
            }
            UiAction::CycleRotationSnap => {
                rotation_snap.cycle();
                state.status = format!(
                    "{}; hold Q or E while dragging a card.",
                    rotation_snap.label()
                );
            }
            UiAction::ResumeMenu => {
                state.escape_menu_open = false;
                state.escape_menu_page = EscapeMenuPage::Main;
                state.confirm_leave = false;
                state.status = "Returned to the table.".into();
            }
            UiAction::OpenOptions => {
                state.escape_menu_page = EscapeMenuPage::Options;
                state.confirm_leave = false;
                state.status = "Camera options opened.".into();
            }
            UiAction::CloseOptions => {
                state.escape_menu_page = EscapeMenuPage::Main;
                state.status = "Returned to the table menu.".into();
            }
            UiAction::ToggleCameraYInversion => {
                camera_options.invert_y = !camera_options.invert_y;
                state.status = format!(
                    "{}. This affects RMB vertical orbit.",
                    camera_options.invert_y_label()
                );
            }
        }
    }
}

fn begin_identity_selection(
    state: &mut UiState,
    account: &IdentityAccount,
    bridge: &BridgeHandle,
    authority: &AuthorityEndpoint,
) -> Result<(), String> {
    bridge.send(BridgeIntent::Connect {
        config: authority.client_config(&account.account_id),
    })?;
    state.active_account_id = Some(account.account_id.clone());
    state.account_label.clone_from(&account.label);
    state.display_name.clone_from(&account.display_name);
    state.screen = UiScreen::Connecting;
    state.pending_flow = Some(PendingFlow::SelectIdentity);
    state.busy = true;
    state.capability = None;
    state.status = format!("Signing in as {}…", account.label);
    Ok(())
}

fn begin_joining_lobby(
    state: &mut UiState,
    model: &BridgeModel,
    bridge: &BridgeHandle,
    code: String,
) {
    if state.active_account_id.is_none() || !model.connected {
        state.status = "Choose and connect an identity before joining a lobby.".into();
        return;
    }
    if !valid_join_code(&code) {
        state.status = "Enter a code shaped like PCH-0000-0000-0000-0000.".into();
        return;
    }
    state.capability = Some(RoomCapability {
        room_id: String::new(),
        join_code: code.clone(),
    });
    state.pending_flow = Some(PendingFlow::JoinRoom);
    begin_room_loading(state, "Joining and synchronizing the shared table…");
    if let Err(error) = bridge.send(BridgeIntent::Join {
        display_name: state.display_name.clone(),
        join_code: code,
    }) {
        state.busy = false;
        state.pending_flow = None;
        state.screen = UiScreen::MainMenu;
        state.status = error;
    }
}

fn begin_room_loading(state: &mut UiState, status: &str) {
    state.screen = UiScreen::LoadingRoom;
    state.room_scene_generation = state.room_scene_generation.saturating_add(1).max(1);
    state.busy = true;
    state.status = status.into();
}

fn remember_capability(state: &UiState, vault: &mut IdentityVault) {
    let (Some(account_id), Some(capability)) = (
        state.active_account_id.as_deref(),
        state.capability.as_ref(),
    ) else {
        return;
    };
    if capability.room_id.is_empty() || capability.join_code.is_empty() {
        return;
    }
    if let Err(error) = vault.remember_lobby(account_id, &capability.room_id, &capability.join_code)
    {
        tracing::warn!(%error, "could not persist recent lobby capability");
    }
}

fn advance_room_loading(
    mut state: ResMut<UiState>,
    model: Res<BridgeModel>,
    render_readiness: Res<TableRenderReadiness>,
    table_cameras: Query<(), With<TabletopCamera>>,
) {
    if state.screen != UiScreen::LoadingRoom {
        return;
    }
    if !room_projection_ready(&model.snapshot, state.capability.as_ref())
        || !room_scene_projection_ready(&model.snapshot, &state, !table_cameras.is_empty())
    {
        return;
    }
    if !render_readiness.has_rendered(state.room_scene_generation) {
        return;
    }
    state.screen = UiScreen::Table;
    state.busy = false;
    state.pending_flow = None;
    state.status = "The synchronized table is ready.".into();
}

fn room_scene_projection_ready(
    snapshot: &ClientSnapshot,
    state: &UiState,
    table_camera_exists: bool,
) -> bool {
    let expected_players = snapshot
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .count();
    table_camera_exists
        && state.rendered_card_count == snapshot.card_poses.len()
        && state.rendered_player_count == expected_players
}

fn room_projection_ready(snapshot: &ClientSnapshot, capability: Option<&RoomCapability>) -> bool {
    let Some(room_id) = snapshot.room_id() else {
        return false;
    };
    if !capability
        .is_some_and(|capability| capability.room_id == room_id && !capability.join_code.is_empty())
        || !snapshot.members.iter().any(|member| member.is_self)
    {
        return false;
    }
    let Some(game) = snapshot.game.as_ref() else {
        return true;
    };
    if snapshot
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .count()
        < 2
    {
        return false;
    }
    if let Some(own_seat) = snapshot.own_seat()
        && game.hand_counts[usize::from(own_seat)] as usize != snapshot.hand.len()
    {
        return false;
    }
    snapshot.card_poses.len() >= usize::from(game.hand_size) * 2
}

fn ui_camera_clear_for_screen(screen: UiScreen) -> bevy::camera::ClearColorConfig {
    if screen == UiScreen::Table {
        bevy::camera::ClearColorConfig::None
    } else {
        bevy::camera::ClearColorConfig::Default
    }
}

fn log_window_resize(mut resized: MessageReader<WindowResized>, state: Res<UiState>) {
    for event in resized.read() {
        tracing::info!(
            width = event.width,
            height = event.height,
            screen = ?state.screen,
            "Poche window resized"
        );
    }
}

fn adjacent_account(
    vault: &mut IdentityVault,
    authority: &AuthorityEndpoint,
    state: &UiState,
    step: isize,
) -> Result<IdentityAccount, String> {
    vault.reload()?;
    let accounts = vault
        .accounts_for(&authority.uri, &authority.database)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if accounts.is_empty() {
        return Err("Create an identity before cycling accounts.".into());
    }
    let current = state
        .active_account_id
        .as_deref()
        .and_then(|id| accounts.iter().position(|account| account.account_id == id))
        .unwrap_or(0);
    let len = isize::try_from(accounts.len()).expect("account count fits isize");
    let next = (isize::try_from(current).expect("account index fits isize") + step).rem_euclid(len)
        as usize;
    Ok(accounts[next].clone())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LeaveActivation {
    ConfirmationArmed,
    Submitted,
}

fn activate_leave(
    state: &mut UiState,
    room_id: Option<&str>,
    bridge: &BridgeHandle,
) -> Result<LeaveActivation, String> {
    let room_id = room_id.ok_or_else(|| "Join a lobby before leaving it.".to_string())?;
    if !state.confirm_leave {
        state.confirm_leave = true;
        state.status = "Leaving releases your seat. Choose Confirm leave lobby to continue.".into();
        return Ok(LeaveActivation::ConfirmationArmed);
    }
    bridge.send(BridgeIntent::Leave {
        room_id: room_id.into(),
    })?;
    state.confirm_leave = false;
    state.pending_flow = Some(PendingFlow::LeaveRoom);
    state.busy = true;
    state.status = "Leaving lobby…".into();
    Ok(LeaveActivation::Submitted)
}

fn toggle_escape_menu(state: &mut UiState) {
    state.confirm_leave = false;
    if !state.escape_menu_open {
        state.escape_menu_open = true;
        state.escape_menu_page = EscapeMenuPage::Main;
        state.status = "Table menu opened. The shared table remains live.".into();
    } else if state.escape_menu_page == EscapeMenuPage::Options {
        state.escape_menu_page = EscapeMenuPage::Main;
        state.status = "Returned to the table menu.".into();
    } else {
        state.escape_menu_open = false;
        state.status = "Returned to the table.".into();
    }
}

fn handle_escape_key(
    keys: Res<ButtonInput<KeyCode>>,
    room: Query<(), With<RoomRoot>>,
    mut state: ResMut<UiState>,
) {
    if !room.is_empty() && keys.just_pressed(KeyCode::Escape) {
        toggle_escape_menu(&mut state);
    }
}

fn toggle_fps_overlay(keys: Res<ButtonInput<KeyCode>>, overlay: Option<ResMut<FpsOverlayConfig>>) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    if let Some(mut overlay) = overlay {
        let visible = !overlay.enabled;
        set_fps_overlay_visible(&mut overlay, visible);
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

fn handle_bridge_notices(
    mut notices: MessageReader<BridgeNotice>,
    mut state: ResMut<UiState>,
    mut vault: ResMut<IdentityVault>,
) {
    for notice in notices.read() {
        match notice {
            BridgeNotice::Connected { identity } => {
                if let Some(account_id) = state.active_account_id.as_deref()
                    && let Err(error) = vault.record_principal(account_id, identity)
                {
                    state.busy = false;
                    state.pending_flow = None;
                    state.screen = UiScreen::IdentityGate;
                    state.status = error;
                    continue;
                }
                state.status = "Authenticated; recovering this identity's room state…".into();
            }
            BridgeNotice::RoomCreated(capability) => {
                state.capability = Some(capability.clone());
                state.status = "Lobby created.".into();
                remember_capability(&state, &mut vault);
            }
            BridgeNotice::Snapshot(snapshot) => {
                if let Some(capability) = &snapshot.room_capability {
                    state.capability = Some(capability.clone());
                    remember_capability(&state, &mut vault);
                }
                match state.pending_flow {
                    Some(PendingFlow::SelectIdentity) if snapshot.identity.is_some() => {
                        state.busy = false;
                        state.pending_flow = None;
                        state.screen = if snapshot.room_id().is_some() {
                            state.status = "This identity has an unfinished lobby.".into();
                            UiScreen::ResumeOffer
                        } else {
                            state.status = "Signed in. Ready to create or join a lobby.".into();
                            UiScreen::MainMenu
                        };
                    }
                    Some(PendingFlow::CreateRoom | PendingFlow::JoinRoom)
                        if snapshot.room_id().is_some() =>
                    {
                        state.status = "Room accepted; synchronizing its complete scene…".into();
                    }
                    Some(PendingFlow::LeaveRoom) if snapshot.room_id().is_none() => {
                        state.busy = false;
                        state.pending_flow = None;
                        state.capability = None;
                        state.escape_menu_open = false;
                        state.escape_menu_page = EscapeMenuPage::Main;
                        state.screen = UiScreen::LobbyEnded;
                        state.status = "You have left this lobby.".into();
                    }
                    _ => {}
                }
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
                    state.screen = UiScreen::LobbyEnded;
                    state.status = "You have left this lobby.".into();
                }
                if result.is_err() {
                    if matches!(*operation, "create_room" | "join_room") {
                        state.capability = None;
                    }
                    state.busy = false;
                    state.pending_flow = None;
                }
            }
            BridgeNotice::Disconnected(reason) => {
                if state.pending_flow != Some(PendingFlow::SelectIdentity) {
                    state.busy = false;
                    state.screen = UiScreen::IdentityGate;
                    state.status = reason
                        .clone()
                        .unwrap_or_else(|| "Disconnected. Choose an identity to reconnect.".into());
                }
            }
            BridgeNotice::Error(error) => {
                if matches!(
                    state.pending_flow,
                    Some(PendingFlow::CreateRoom | PendingFlow::JoinRoom)
                ) {
                    state.capability = None;
                }
                state.busy = false;
                state.screen = match state.pending_flow {
                    Some(PendingFlow::SelectIdentity) => UiScreen::IdentityGate,
                    Some(PendingFlow::CreateRoom | PendingFlow::JoinRoom) => UiScreen::MainMenu,
                    Some(PendingFlow::LeaveRoom) | None => state.screen,
                };
                state.pending_flow = None;
                state.status.clone_from(error);
            }
        }
    }
}

fn sync_frontend_screen(
    state: Res<UiState>,
    authority: Res<AuthorityEndpoint>,
    vault: Res<IdentityVault>,
    model: Res<BridgeModel>,
    roots: Query<(Entity, &FrontendRoot)>,
    mut ui_cameras: Query<(Entity, &mut Camera), With<PocheUiCamera>>,
    mut commands: Commands,
) {
    let wants_frontend = state.screen != UiScreen::Table;
    let already_correct = roots
        .iter()
        .any(|(_, root)| wants_frontend && root.0 == state.screen);
    if already_correct && roots.iter().count() == 1 && !vault.is_changed() {
        return;
    }
    for (entity, _) in &roots {
        commands.entity(entity).despawn();
    }
    if !wants_frontend {
        return;
    }
    let Ok((camera, mut ui_camera)) = ui_cameras.single_mut() else {
        return;
    };
    ui_camera.order = 10;
    ui_camera.clear_color = ui_camera_clear_for_screen(state.screen);
    spawn_frontend(
        &mut commands,
        camera,
        &authority,
        &state,
        &vault,
        model.snapshot.room_id().is_some(),
    );
}

fn enter_room(
    model: Res<BridgeModel>,
    mut state: ResMut<UiState>,
    surface: Res<RenderSurface>,
    rotation_snap: Res<RotationSnap>,
    camera_options: Res<CameraOptions>,
    frontend: Query<Entity, With<FrontendRoot>>,
    room: Query<Entity, With<RoomRoot>>,
    cards: Query<Entity, With<CardVisual>>,
    players: Query<Entity, With<SpatialPlayer>>,
    mut ui_cameras: Query<(Entity, &mut Camera), With<PocheUiCamera>>,
    table_cameras: Query<
        Entity,
        (
            With<TabletopCamera>,
            Without<PocheUiCamera>,
            Without<HandCamera>,
        ),
    >,
    hand_cameras: Query<
        Entity,
        (
            With<HandCamera>,
            Without<TabletopCamera>,
            Without<PocheUiCamera>,
        ),
    >,
    mut poses: ResMut<PoseDisplay>,
    mut drag: ResMut<DragState>,
    mut camera_controller: ResMut<TableCameraController>,
    mut commands: Commands,
) {
    let should_prepare_table = model.snapshot.room_id().is_some()
        && matches!(state.screen, UiScreen::LoadingRoom | UiScreen::Table);
    if !should_prepare_table {
        for entity in &room {
            commands.entity(entity).despawn();
        }
        for entity in &cards {
            commands.entity(entity).despawn();
        }
        for entity in &players {
            commands.entity(entity).despawn();
        }
        poses.0.clear();
        drag.card_key = None;
        drag.reset_rotation_repeat();
        camera_controller.last_seat = CameraSeat::Uninitialized;
        for entity in &table_cameras {
            commands.entity(entity).despawn();
        }
        for entity in &hand_cameras {
            commands.entity(entity).despawn();
        }
        if !matches!(state.screen, UiScreen::Connecting | UiScreen::LoadingRoom) {
            state.busy = false;
        }
        state.confirm_leave = false;
        state.escape_menu_open = false;
        state.escape_menu_page = EscapeMenuPage::Main;
        return;
    }
    if table_cameras.is_empty() && hand_cameras.is_empty() {
        camera_controller.reset_for_seat(model.snapshot.own_seat(), true);
        spawn_spatial_cameras(
            &mut commands,
            &surface,
            camera_controller.current.transform(),
            state.room_scene_generation,
        );
    }
    if state.screen == UiScreen::LoadingRoom || !room.is_empty() {
        return;
    }
    for entity in &frontend {
        commands.entity(entity).despawn();
    }
    let Ok((camera, mut ui_camera)) = ui_cameras.single_mut() else {
        return;
    };
    ui_camera.order = 10;
    ui_camera.clear_color = ui_camera_clear_for_screen(UiScreen::Table);
    state.confirm_leave = false;
    state.escape_menu_open = false;
    state.escape_menu_page = EscapeMenuPage::Main;
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .with_children(|root| {
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
                Button,
                UiAction::CycleRotationSnap,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(22.),
                    top: px(82.),
                    padding: px(9.).all(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.13, 0.26, 0.27)),
                children![(
                    RotationSnapLabel,
                    Text::new(rotation_snap.label()),
                    TextFont::from_font_size(16.),
                )],
            ));
            root.spawn((
                PlayerListLabel,
                Text::new("PLAYERS\nLoading roster…"),
                TextFont::from_font_size(16.),
                TextColor(Color::srgb(0.88, 0.92, 0.88)),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(22.),
                    top: px(136.),
                    min_width: px(225.),
                    max_width: px(320.),
                    padding: px(12.).all(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.035, 0.08, 0.085, 0.88)),
            ));
            root.spawn((
                ActivityLogLabel,
                Text::new("ACTIVITY\nWaiting for public activity…"),
                TextFont::from_font_size(14.),
                TextColor(Color::srgb(0.78, 0.84, 0.8)),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(22.),
                    top: px(238.),
                    width: px(300.),
                    padding: px(12.).all(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.035, 0.08, 0.085, 0.88)),
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
                        left: px(26.),
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
                GameStatusLabel,
                Text::new("Waiting for both seats to begin the first deal."),
                TextFont::from_font_size(20.),
                TextColor(Color::srgb(0.96, 0.88, 0.58)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(26.),
                    bottom: px(150.),
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
                spawn_button(bar, "Bid 0 tricks", UiAction::Bid(0), true);
                spawn_button(bar, "Bid 1 trick", UiAction::Bid(1), true);
                spawn_button(bar, "Play your card", UiAction::PlayFirstCard, true);
                bar.spawn((
                    LatencyLabel,
                    Text::new(
                        "Drag card · Q turn left · E turn right · RMB orbit · MMB/WASD pan · Wheel zoom · O tactical · Space reset · Esc menu · F3 stats",
                    ),
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
            root.spawn((
                EscapeMenuRoot,
                GlobalZIndex(100),
                Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    width: percent(100.),
                    height: percent(100.),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.76)),
            ))
            .with_children(|overlay| {
                overlay
                    .spawn((
                        EscapeMainPanel,
                        Node {
                            width: px(440.),
                            padding: px(26.).all(),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(14.),
                            align_items: AlignItems::Stretch,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.035, 0.075, 0.08)),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Text::new("TABLE MENU"),
                            TextFont::from_font_size(30.),
                            TextColor(Color::srgb(0.96, 0.88, 0.58)),
                        ));
                        panel.spawn((
                            Text::new("The shared table remains live while this menu is open."),
                            TextFont::from_font_size(15.),
                            TextColor(Color::srgb(0.72, 0.82, 0.8)),
                        ));
                        spawn_button(panel, "Resume table", UiAction::ResumeMenu, true);
                        spawn_button(panel, "Stand up", UiAction::ReleaseSeat, false);
                        spawn_button(panel, "Options", UiAction::OpenOptions, false);
                        panel
                            .spawn((
                                Button,
                                UiAction::Leave,
                                Node {
                                    min_width: px(150.),
                                    padding: px(13.).all(),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.40, 0.13, 0.12)),
                            ))
                            .with_child((
                                LeaveButtonLabel,
                                Text::new("Leave lobby"),
                                TextFont::from_font_size(20.),
                            ));
                        panel.spawn((
                            Text::new("Press Esc to resume."),
                            TextFont::from_font_size(14.),
                            TextColor(Color::srgb(0.62, 0.72, 0.7)),
                        ));
                    });
                overlay
                    .spawn((
                        EscapeOptionsPanel,
                        Node {
                            display: Display::None,
                            width: px(440.),
                            padding: px(26.).all(),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(14.),
                            align_items: AlignItems::Stretch,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.035, 0.075, 0.08)),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Text::new("OPTIONS"),
                            TextFont::from_font_size(30.),
                            TextColor(Color::srgb(0.96, 0.88, 0.58)),
                        ));
                        panel.spawn((
                            Text::new(
                                "Camera controls are local to this game window and do not affect the shared table.",
                            ),
                            TextFont::from_font_size(15.),
                            TextColor(Color::srgb(0.72, 0.82, 0.8)),
                        ));
                        panel
                            .spawn((
                                Button,
                                UiAction::ToggleCameraYInversion,
                                Node {
                                    min_width: px(150.),
                                    padding: px(13.).all(),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.13, 0.26, 0.27)),
                            ))
                            .with_child((
                                InvertCameraYLabel,
                                Text::new(camera_options.invert_y_label()),
                                TextFont::from_font_size(20.),
                            ));
                        panel.spawn((
                            Text::new("This reverses RMB vertical orbit. The default is On."),
                            TextFont::from_font_size(14.),
                            TextColor(Color::srgb(0.62, 0.72, 0.7)),
                        ));
                        spawn_button(panel, "Back", UiAction::CloseOptions, true);
                        panel.spawn((
                            Text::new("Press Esc to return to the table menu."),
                            TextFont::from_font_size(14.),
                            TextColor(Color::srgb(0.62, 0.72, 0.7)),
                        ));
                    });
            });
        });
}

fn sync_escape_menu(
    state: Res<UiState>,
    camera_options: Res<CameraOptions>,
    mut menus: Query<
        &mut Node,
        (
            With<EscapeMenuRoot>,
            Without<EscapeMainPanel>,
            Without<EscapeOptionsPanel>,
        ),
    >,
    mut panels: Query<
        (
            &mut Node,
            Option<&EscapeMainPanel>,
            Option<&EscapeOptionsPanel>,
        ),
        (
            Or<(With<EscapeMainPanel>, With<EscapeOptionsPanel>)>,
            Without<EscapeMenuRoot>,
        ),
    >,
    mut leave_labels: Query<&mut Text, With<LeaveButtonLabel>>,
    mut invert_y_labels: Query<&mut Text, (With<InvertCameraYLabel>, Without<LeaveButtonLabel>)>,
) {
    if !(state.is_changed() || camera_options.is_changed()) {
        return;
    }
    for mut node in &mut menus {
        node.display = if state.escape_menu_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (mut node, main, options) in &mut panels {
        let visible = (main.is_some() && state.escape_menu_page == EscapeMenuPage::Main)
            || (options.is_some() && state.escape_menu_page == EscapeMenuPage::Options);
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut label in &mut leave_labels {
        label.0 = if state.confirm_leave {
            "Confirm leave lobby".into()
        } else {
            "Leave lobby".into()
        };
    }
    for mut label in &mut invert_y_labels {
        label.0 = camera_options.invert_y_label();
    }
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
            Without<PlayerListLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut seats: Query<
        (&SeatLabel, &mut Text, &mut Node),
        (
            Without<RoomCodeLabel>,
            Without<HandSummary>,
            Without<LatencyLabel>,
            Without<UiAction>,
            Without<PlayerListLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut hands: Query<
        &mut Text,
        (
            With<HandSummary>,
            Without<RoomCodeLabel>,
            Without<SeatLabel>,
            Without<LatencyLabel>,
            Without<PlayerListLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut latency: Query<
        &mut Text,
        (
            With<LatencyLabel>,
            Without<RoomCodeLabel>,
            Without<HandSummary>,
            Without<SeatLabel>,
            Without<PlayerListLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut game_status: Query<
        &mut Text,
        (
            With<GameStatusLabel>,
            Without<RoomCodeLabel>,
            Without<HandSummary>,
            Without<SeatLabel>,
            Without<LatencyLabel>,
            Without<PlayerListLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut player_list: Query<
        &mut Text,
        (
            With<PlayerListLabel>,
            Without<RoomCodeLabel>,
            Without<SeatLabel>,
            Without<HandSummary>,
            Without<LatencyLabel>,
            Without<GameStatusLabel>,
            Without<ActivityLogLabel>,
        ),
    >,
    mut activity_log: Query<
        &mut Text,
        (
            With<ActivityLogLabel>,
            Without<RoomCodeLabel>,
            Without<SeatLabel>,
            Without<HandSummary>,
            Without<LatencyLabel>,
            Without<GameStatusLabel>,
            Without<PlayerListLabel>,
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
        text.0 = if own_seat.is_none() {
            "Take a seat; cards are dealt when both seats are occupied.".into()
        } else if model.snapshot.game.is_none() {
            "Waiting for both seats; the authority will deal when the table is ready.".into()
        } else if private_hand_is_synchronizing(&model.snapshot) {
            "Synchronizing your private hand… actions are temporarily disabled.".into()
        } else if model.snapshot.hand.is_empty() {
            "Your hand is empty; the first round is complete.".into()
        } else {
            format!(
                "Your private hand · {} cards · drag any card to wiggle it for the other player",
                model.snapshot.hand.len()
            )
        };
    }
    for mut text in &mut latency {
        text.0 = model.last_command_latency.map_or_else(
            || {
                "Drag: move · Q/E: turn · wheel: zoom · O: tactical camera · F3: stats".into()
            },
            |value| {
                format!(
                    "Last authority response {:.1} ms · drag · Q/E turn · wheel zoom · O tactical · F3 stats",
                    value.as_secs_f64() * 1000.
                )
            },
        );
    }
    for mut text in &mut game_status {
        text.0 = model.snapshot.game.as_ref().map_or_else(
            || "Waiting for both seats to begin the first deal.".into(),
            |game| {
                let actor = game
                    .actor_seat
                    .map_or_else(|| "none".into(), |seat| format!("seat {}", seat + 1));
                format!(
                    "{} · round {} · actor {actor} · bids {:?} · trick {}/{}",
                    game.phase,
                    game.round_index + 1,
                    game.bids,
                    game.trick_count,
                    game.hand_size
                )
            },
        );
    }
    for mut text in &mut player_list {
        let mut members = model.snapshot.members.iter().collect::<Vec<_>>();
        members.sort_by_key(|member| (member.seat.is_none(), member.seat, &member.display_name));
        let rows = members
            .into_iter()
            .map(|member| {
                let presence = if member.connected { "●" } else { "○" };
                let place = member.seat.map_or_else(
                    || "standing".to_string(),
                    |seat| format!("seat {}", seat + 1),
                );
                format!(
                    "{presence} {} · {place}{}",
                    member.display_name,
                    if member.is_self { " · you" } else { "" }
                )
            })
            .collect::<Vec<_>>();
        text.0 = if rows.is_empty() {
            "PLAYERS\nNo one is in this lobby.".into()
        } else {
            format!("PLAYERS\n{}", rows.join("\n"))
        };
    }
    for mut text in &mut activity_log {
        let rows = model
            .snapshot
            .activity
            .iter()
            .rev()
            .take(8)
            .map(|event| format!("#{}  {}", event.sequence + 1, event.summary))
            .collect::<Vec<_>>();
        text.0 = if rows.is_empty() {
            "ACTIVITY\nNo public actions yet.".into()
        } else {
            format!("ACTIVITY · newest first\n{}", rows.join("\n"))
        };
    }
    for (action, mut node) in &mut actions {
        node.display =
            match action {
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
                UiAction::Bid(tricks) => {
                    let may_bid = may_bid(&model.snapshot, *tricks);
                    if may_bid {
                        Display::Flex
                    } else {
                        Display::None
                    }
                }
                UiAction::PlayFirstCard => {
                    let may_play =
                        model.snapshot.game.as_ref().is_some_and(|game| {
                            game.phase == "playing" && game.actor_seat == own_seat
                        }) && !model.snapshot.hand.is_empty();
                    if may_play {
                        Display::Flex
                    } else {
                        Display::None
                    }
                }
                UiAction::Leave
                | UiAction::CopyCode
                | UiAction::CycleRotationSnap
                | UiAction::ResumeMenu
                | UiAction::OpenOptions
                | UiAction::CloseOptions
                | UiAction::ToggleCameraYInversion => Display::Flex,
                UiAction::CreateIdentity
                | UiAction::SelectIdentity(_)
                | UiAction::RefreshIdentities
                | UiAction::PreviousIdentity
                | UiAction::NextIdentity
                | UiAction::OpenIdentities
                | UiAction::CancelConnecting
                | UiAction::ResumeLobby
                | UiAction::SkipResume
                | UiAction::ReturnToTitle
                | UiAction::Create
                | UiAction::Paste
                | UiAction::Join
                | UiAction::JoinHistory(_) => continue,
            };
    }
}

fn own_public_hand_count(snapshot: &ClientSnapshot) -> Option<u8> {
    let seat = usize::from(snapshot.own_seat()?);
    snapshot
        .game
        .as_ref()
        .and_then(|game| game.hand_counts.get(seat).copied())
}

fn private_hand_is_synchronizing(snapshot: &ClientSnapshot) -> bool {
    snapshot.hand.is_empty() && own_public_hand_count(snapshot).is_some_and(|count| count > 0)
}

fn may_bid(snapshot: &ClientSnapshot, tricks: u8) -> bool {
    let own_seat = snapshot.own_seat();
    !snapshot.hand.is_empty()
        && snapshot.game.as_ref().is_some_and(|game| {
            game.phase == "bidding"
                && game.actor_seat == own_seat
                && own_public_hand_count(snapshot) == Some(game.hand_size)
                && tricks <= game.hand_size
        })
}

fn update_rotation_snap_labels(
    rotation_snap: Res<RotationSnap>,
    mut labels: Query<&mut Text, With<RotationSnapLabel>>,
) {
    if !rotation_snap.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.0 = rotation_snap.label();
    }
}

#[derive(Clone, Debug)]
struct DisplayPose {
    current: [f32; 3],
    target: [f32; 3],
    current_rotation: Quat,
    rotation_mdeg: [i32; 3],
    sequence: u64,
}

#[derive(Resource, Default)]
struct PoseDisplay(HashMap<String, DisplayPose>);

#[derive(Component)]
struct CardVisual {
    key: String,
    label: String,
}

#[derive(Resource, Default)]
struct DragState {
    card_key: Option<String>,
    last_sent: Option<Instant>,
    rotation_direction: i8,
    next_rotation_step: Option<Instant>,
}

impl DragState {
    fn rotation_step_due(&mut self, direction: i8, now: Instant) -> bool {
        if direction == 0 {
            self.rotation_direction = 0;
            self.next_rotation_step = None;
            return false;
        }
        if self.rotation_direction != direction {
            self.rotation_direction = direction;
            self.next_rotation_step = Some(now + ROTATION_REPEAT_DELAY);
            return true;
        }
        if self.next_rotation_step.is_some_and(|next| now >= next) {
            self.next_rotation_step = Some(now + ROTATION_REPEAT_PERIOD);
            return true;
        }
        false
    }

    fn reset_rotation_repeat(&mut self) {
        self.rotation_direction = 0;
        self.next_rotation_step = None;
    }
}

fn sync_card_entities(
    model: Res<BridgeModel>,
    mut poses: ResMut<PoseDisplay>,
    existing: Query<(Entity, &CardVisual)>,
    table_camera: Query<(), With<TabletopCamera>>,
    entered_scene: Query<(), (With<TabletopCamera>, Added<TabletopCamera>)>,
    assets: Res<SpatialAssets>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    if (!model.is_changed() && entered_scene.is_empty()) || table_camera.is_empty() {
        return;
    }
    let wanted: HashSet<_> = model
        .snapshot
        .card_poses
        .iter()
        .map(|pose| pose.card_key.clone())
        .collect();
    for (entity, card) in &existing {
        let desired = model.snapshot.visible_card_face(&card.key).unwrap_or("P");
        if !wanted.contains(&card.key) || desired != card.label {
            commands.entity(entity).despawn();
            if !wanted.contains(&card.key) {
                poses.0.remove(&card.key);
            }
        }
    }
    let existing: HashSet<_> = existing
        .iter()
        .filter(|(_, card)| {
            model.snapshot.visible_card_face(&card.key).unwrap_or("P") == card.label
        })
        .map(|(_, card)| card.key.clone())
        .collect();
    for network in &model.snapshot.card_poses {
        let target = network.position_mm.map(|value| value as f32);
        let display = poses
            .0
            .entry(network.card_key.clone())
            .or_insert(DisplayPose {
                current: target,
                target,
                current_rotation: rotation_mdeg_quat(network.rotation_mdeg),
                rotation_mdeg: network.rotation_mdeg,
                sequence: network.sequence,
            });
        display.target = target;
        display.rotation_mdeg = network.rotation_mdeg;
        display.sequence = display.sequence.max(network.sequence);
        if existing.contains(&network.card_key) {
            continue;
        }
        let visible_face = model.snapshot.visible_card_face(&network.card_key);
        let label = visible_face.unwrap_or("P");
        let is_own = model
            .snapshot
            .hand
            .iter()
            .any(|card| card.card_key == network.card_key);
        let layers = if is_own {
            RenderLayers::layer(0).with(1)
        } else {
            RenderLayers::layer(0)
        };
        let (texture, aspect) = card_label_texture(
            &mut images,
            label,
            if visible_face.is_some() {
                [18, 18, 16]
            } else {
                [224, 232, 248]
            },
        );
        let label_material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let (label_width, label_height) = card_label_size(aspect);
        let card = commands
            .spawn((
                CardVisual {
                    key: network.card_key.clone(),
                    label: label.into(),
                },
                Mesh3d(assets.card_mesh.clone()),
                MeshMaterial3d(if visible_face.is_some() {
                    assets.card_face_material.clone()
                } else {
                    assets.card_back_material.clone()
                }),
                pose_transform(target, network.rotation_mdeg),
                layers.clone(),
            ))
            .id();
        let label =
            commands
                .spawn((
                    Mesh3d(assets.card_label_mesh.clone()),
                    MeshMaterial3d(label_material),
                    Transform::from_xyz(0.0, CARD_WORLD_THICKNESS * 0.55, 0.0)
                        .with_scale(Vec3::new(label_width, 1.0, label_height)),
                    layers,
                ))
                .id();
        commands.entity(card).add_child(label);
    }
}

fn card_label_size(aspect: f32) -> (f32, f32) {
    let maximum_width = CARD_WORLD_WIDTH * 0.82;
    let maximum_height = CARD_WORLD_HEIGHT * 0.72;
    let height = maximum_height.min(maximum_width / aspect.max(0.01));
    (height * aspect, height)
}

fn card_label_texture(
    images: &mut Assets<Image>,
    label: &str,
    color: [u8; 3],
) -> (Handle<Image>, f32) {
    let font = SlugFont::parse(FONT_BYTES, 0, '?').expect("embedded Poche font is valid");
    let raster = rasterize_text_rgba(&font, label, color).expect("card label raster is valid");
    let aspect = raster.design_width / raster.design_height.max(f32::EPSILON);
    let image = Image::new(
        Extent3d {
            width: raster.width,
            height: raster.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        raster.bytes,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    (images.add(image), aspect)
}

fn pose_transform(position_mm: [f32; 3], rotation_mdeg: [i32; 3]) -> Transform {
    Transform::from_translation(mm_position(position_mm))
        .with_rotation(rotation_mdeg_quat(rotation_mdeg))
}

fn rotation_mdeg_quat(rotation_mdeg: [i32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        mdeg_radians(rotation_mdeg[0]),
        mdeg_radians(rotation_mdeg[1]),
        mdeg_radians(rotation_mdeg[2]),
    )
}

fn smooth_card_rotation(current: Quat, target_mdeg: [i32; 3], alpha: f32) -> Quat {
    current.slerp(rotation_mdeg_quat(target_mdeg), alpha)
}

fn mm_position(position_mm: [f32; 3]) -> Vec3 {
    Vec3::from_array(position_mm) / 1_000.0
}

fn mdeg_radians(value: i32) -> f32 {
    (value as f32 / 1_000.0).to_radians()
}

fn sync_player_entities(
    model: Res<BridgeModel>,
    layout: Res<CanonicalLayout>,
    assets: Res<SpatialAssets>,
    existing: Query<(Entity, &SpatialPlayer)>,
    table_camera: Query<(), With<TabletopCamera>>,
    entered_scene: Query<(), (With<TabletopCamera>, Added<TabletopCamera>)>,
    mut commands: Commands,
) {
    if (!model.is_changed() && entered_scene.is_empty()) || table_camera.is_empty() {
        return;
    }
    for (entity, _) in &existing {
        commands.entity(entity).despawn();
    }
    for member in model
        .snapshot
        .members
        .iter()
        .filter(|member| member.seat.is_some())
    {
        let seat = member.seat.expect("filtered to seated players");
        let position = layout
            .0
            .seats()
            .iter()
            .find(|placement| placement.seat.get() == seat)
            .map_or(Vec3::ZERO, |placement| {
                point_to_world(placement.player_pose.translation)
            });
        commands.spawn((
            SpatialPlayer,
            Mesh3d(assets.avatar_mesh.clone()),
            MeshMaterial3d(if member.is_self {
                assets.self_avatar_material.clone()
            } else {
                assets.peer_avatar_material.clone()
            }),
            Transform::from_translation(position),
        ));
    }
}

fn update_rendered_scene_metrics(
    cards: Query<(), With<CardVisual>>,
    players: Query<(), With<SpatialPlayer>>,
    mut state: ResMut<UiState>,
) {
    let card_count = cards.iter().count();
    let player_count = players.iter().count();
    if state.rendered_card_count != card_count || state.rendered_player_count != player_count {
        state.rendered_card_count = card_count;
        state.rendered_player_count = player_count;
    }
}

fn update_table_camera(
    time: Res<Time>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    model: Res<BridgeModel>,
    layout: Res<CanonicalLayout>,
    state: Res<UiState>,
    camera_options: Res<CameraOptions>,
    room: Query<(), With<RoomRoot>>,
    mut controller: ResMut<TableCameraController>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<TabletopCamera>>,
) {
    let seat = model.snapshot.own_seat();
    if controller.last_seat != CameraSeat::from(seat) {
        let immediate = controller.last_seat == CameraSeat::Uninitialized;
        controller.reset_for_seat(seat, immediate);
    }
    if !state.escape_menu_open && !room.is_empty() {
        if keys.just_pressed(KeyCode::KeyO) {
            controller.toggle_mode();
        }
        if keys.just_pressed(KeyCode::Space) {
            controller.reset_for_seat(seat, false);
        }
        let scroll_lines = match mouse_scroll.unit {
            MouseScrollUnit::Line => mouse_scroll.delta.y,
            MouseScrollUnit::Pixel => {
                mouse_scroll.delta.y / MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR
            }
        };
        match controller.mode {
            TableCameraMode::Perspective => {
                controller.target.distance =
                    zoom_camera_distance(controller.target.distance, scroll_lines);
            }
            TableCameraMode::Tactical => {
                controller.target_orthographic_scale =
                    zoom_orthographic_scale(controller.target_orthographic_scale, scroll_lines);
            }
        }
        let delta = mouse_motion.delta;
        if mouse_buttons.pressed(MouseButton::Right) {
            controller.target.yaw = (controller.target.yaw - delta.x * CAMERA_ORBIT_SENSITIVITY)
                .rem_euclid(std::f32::consts::TAU);
            controller.target.pitch = (controller.target.pitch
                + camera_pitch_delta(delta.y, camera_options.invert_y))
            .clamp(CAMERA_MIN_PITCH, CAMERA_MAX_PITCH);
        }

        let forward = Vec3::new(
            -controller.target.yaw.sin(),
            0.0,
            -controller.target.yaw.cos(),
        );
        let right = Vec3::new(
            controller.target.yaw.cos(),
            0.0,
            -controller.target.yaw.sin(),
        );
        if mouse_buttons.pressed(MouseButton::Middle) {
            let scale = controller.target.distance * CAMERA_PAN_SENSITIVITY;
            controller.target.focus += (-right * delta.x + forward * delta.y) * scale;
        }
        let mut keyboard = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            keyboard += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            keyboard -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            keyboard += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            keyboard -= right;
        }
        if keyboard.length_squared() > 0.0 {
            controller.target.focus +=
                keyboard.normalize() * CAMERA_KEYBOARD_SPEED * time.delta_secs();
        }
        controller.target.focus = clamp_table_focus(&layout.0, controller.target.focus);
    }

    let alpha = 1.0 - (-CAMERA_SMOOTHING * time.delta_secs()).exp();
    controller.current.focus = controller
        .current
        .focus
        .lerp(controller.target.focus, alpha);
    controller.current.yaw = lerp_angle(controller.current.yaw, controller.target.yaw, alpha);
    controller.current.pitch += (controller.target.pitch - controller.current.pitch) * alpha;
    controller.current.distance +=
        (controller.target.distance - controller.current.distance) * alpha;
    controller.current_orthographic_scale +=
        (controller.target_orthographic_scale - controller.current_orthographic_scale) * alpha;
    let transform = controller.current.transform();
    for (mut camera, mut projection) in &mut cameras {
        *camera = transform;
        apply_table_projection(
            &mut projection,
            controller.mode,
            controller.current_orthographic_scale,
        );
    }
}

fn camera_pitch_delta(mouse_delta_y: f32, invert_y: bool) -> f32 {
    mouse_delta_y * CAMERA_ORBIT_SENSITIVITY * if invert_y { 1.0 } else { -1.0 }
}

fn clamp_table_focus(layout: &SpatialLayout, focus: Vec3) -> Vec3 {
    let Some(table) = layout
        .scene_objects()
        .into_iter()
        .find(|object| object.kind == SceneObjectKind::Table)
    else {
        return focus;
    };
    let center = point_to_world(table.pose.translation);
    let extent_x = table.half_extents.x as f32 / 500.0;
    let extent_z = table.half_extents.z as f32 / 500.0;
    let top = center.y + table.half_extents.y as f32 / 1_000.0;
    Vec3::new(
        focus.x.clamp(center.x - extent_x, center.x + extent_x),
        top,
        focus.z.clamp(center.z - extent_z, center.z + extent_z),
    )
}

fn lerp_angle(from: f32, to: f32, alpha: f32) -> f32 {
    let delta =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + delta * alpha
}

fn zoom_camera_distance(distance: f32, scroll_lines: f32) -> f32 {
    (distance * (-CAMERA_ZOOM_SENSITIVITY * scroll_lines).exp())
        .clamp(CAMERA_MIN_DISTANCE, CAMERA_MAX_DISTANCE)
}

fn zoom_orthographic_scale(scale: f32, scroll_lines: f32) -> f32 {
    (scale * (-CAMERA_ZOOM_SENSITIVITY * scroll_lines).exp())
        .clamp(TACTICAL_MIN_SCALE, TACTICAL_MAX_SCALE)
}

fn apply_table_projection(projection: &mut Projection, mode: TableCameraMode, scale: f32) {
    match (mode, &mut *projection) {
        (TableCameraMode::Perspective, Projection::Perspective(_)) => {}
        (TableCameraMode::Perspective, _) => {
            *projection = Projection::Perspective(PerspectiveProjection::default());
        }
        (TableCameraMode::Tactical, Projection::Orthographic(orthographic)) => {
            orthographic.scale = scale;
        }
        (TableCameraMode::Tactical, _) => {
            *projection = Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: TACTICAL_VIEW_HEIGHT,
                },
                scale,
                ..OrthographicProjection::default_3d()
            });
        }
    }
}

fn fifths(value: u32, numerator: u32) -> u32 {
    value / 5 * numerator + value % 5 * numerator / 5
}

fn update_hand_camera(
    surface: Res<RenderSurface>,
    windows: Query<&Window, With<PrimaryWindow>>,
    model: Res<BridgeModel>,
    poses: Res<PoseDisplay>,
    mut hand_cameras: Query<
        (&mut Camera, &mut Transform, &Projection),
        (With<HandCamera>, Without<TabletopCamera>),
    >,
    mut table_cameras: Query<&mut Camera, (With<TabletopCamera>, Without<HandCamera>)>,
) {
    let size = match &*surface {
        RenderSurface::Windowless { width, height, .. } => UVec2::new(*width, *height),
        RenderSurface::Windowed => {
            let Ok(window) = windows.single() else { return };
            UVec2::new(window.physical_width(), window.physical_height())
        }
    };
    let own_positions = model
        .snapshot
        .hand
        .iter()
        .filter_map(|card| {
            poses
                .0
                .get(&card.card_key)
                .map(|pose| mm_position(pose.current))
        })
        .collect::<Vec<_>>();
    let has_hand = !own_positions.is_empty();
    let hand_top = fifths(size.y, 3);
    let actions_top = fifths(size.y, 4);
    for mut camera in &mut table_cameras {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::new(size.x, if has_hand { hand_top } else { actions_top }),
            ..default()
        });
    }
    for (mut camera, mut transform, projection) in &mut hand_cameras {
        camera.is_active = has_hand && size.x >= 10 && size.y >= 10;
        if !camera.is_active {
            continue;
        }
        let center = own_positions.iter().copied().sum::<Vec3>() / own_positions.len() as f32;
        let Projection::Perspective(projection) = projection else {
            continue;
        };
        let viewport_size = UVec2::new(fifths(size.x, 3), actions_top - hand_top);
        let aspect = viewport_size.x as f32 / viewport_size.y.max(1) as f32;
        let width = (own_positions.len().saturating_sub(1) as f32 * 0.072 + 0.064) * 1.2;
        let distance =
            (0.106_f32.max(width / aspect) / (2.0 * (projection.fov * 0.5).tan())).max(0.15);
        let hand_up = hand_camera_up(model.snapshot.own_seat());
        *transform =
            Transform::from_translation(center + Vec3::Y * distance).looking_at(center, hand_up);
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(size.x / 5, hand_top),
            physical_size: viewport_size,
            ..default()
        });
    }
}

fn hand_camera_up(seat: Option<u8>) -> Vec3 {
    if seat == Some(1) {
        Vec3::Z
    } else {
        Vec3::NEG_Z
    }
}

fn drag_cards(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    rotation_snap: Res<RotationSnap>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), Or<(With<TabletopCamera>, With<HandCamera>)>>,
    model: Res<BridgeModel>,
    bridge: Res<BridgeHandle>,
    layout: Res<CanonicalLayout>,
    mut state: ResMut<UiState>,
    mut poses: ResMut<PoseDisplay>,
    mut drag: ResMut<DragState>,
) {
    if state.screen != UiScreen::Table {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some((camera, camera_transform)) = cameras
        .iter()
        .filter(|(camera, _)| {
            camera.is_active
                && camera
                    .logical_viewport_rect()
                    .is_some_and(|rect| rect.contains(cursor))
        })
        .max_by_key(|(camera, _)| camera.order)
    else {
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
                let center = camera
                    .world_to_viewport(camera_transform, mm_position(pose.current))
                    .ok()?;
                let distance = center.distance(cursor);
                (distance <= CARD_PICK_RADIUS_PX).then_some((key.clone(), distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(key, _)| key);
        drag.last_sent = None;
        drag.reset_rotation_repeat();
    }

    let Some(key) = drag.card_key.clone() else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        let height_mm = poses.0.get(&key).map(|pose| pose.current[1]);
        if let Some(physical) = height_mm
            .and_then(|height| cursor_to_world_mm(camera, camera_transform, cursor, height))
            && let Some(pose) = poses.0.get_mut(&key)
        {
            pose.current[0] = physical[0];
            pose.current[2] = physical[2];
        }
        let rotation_direction =
            card_rotation_direction(keys.pressed(KeyCode::KeyQ), keys.pressed(KeyCode::KeyE));
        if drag.rotation_step_due(rotation_direction, Instant::now())
            && let Some(pose) = poses.0.get_mut(&key)
        {
            pose.rotation_mdeg[1] = rotate_mdeg(
                pose.rotation_mdeg[1],
                rotation_direction,
                rotation_snap.step_mdeg(),
            );
        }
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
        let dropped_in_play = poses
            .0
            .get(&key)
            .is_some_and(|pose| is_play_drop(&layout.0, pose.current));
        let may_play = model.snapshot.game.as_ref().is_some_and(|game| {
            game.phase == "playing" && game.actor_seat == model.snapshot.own_seat()
        });
        if dropped_in_play && may_play {
            send_pose(
                &key,
                &model.snapshot.card_poses,
                &mut poses,
                &bridge,
                model.snapshot.room_id(),
            );
            if let (Some(room_id), Some(card)) = (
                model.snapshot.room_id(),
                model.snapshot.hand.iter().find(|card| card.card_key == key),
            ) {
                state.status = format!("Playing {}…", card.face);
                let _ = bridge.send(BridgeIntent::PlayCard {
                    room_id: room_id.into(),
                    card_id: card.card_id.clone(),
                });
            }
        } else if dropped_in_play {
            restore_card_to_hand(&key, &model, &layout.0, &mut poses);
            send_pose(
                &key,
                &model.snapshot.card_poses,
                &mut poses,
                &bridge,
                model.snapshot.room_id(),
            );
            state.status =
                "That card cannot enter PLAY now; its logical location remains your hand.".into();
        } else {
            send_pose(
                &key,
                &model.snapshot.card_poses,
                &mut poses,
                &bridge,
                model.snapshot.room_id(),
            );
        }
        drag.card_key = None;
        drag.last_sent = None;
        drag.reset_rotation_repeat();
    }
}

fn is_play_drop(layout: &SpatialLayout, position_mm: [f32; 3]) -> bool {
    let center = position_mm.map(|value| value.round() as i32);
    AabbMm::from_center(
        Point3Mm::new(center[0], center[1], center[2]),
        HalfExtentsMm::new(32, 1, 44),
    )
    .is_some_and(|bounds| {
        layout.classify_bounds(bounds) == ZoneClassification::Snapped(ZoneId::Play)
    })
}

fn restore_card_to_hand(
    key: &str,
    model: &BridgeModel,
    layout: &SpatialLayout,
    poses: &mut PoseDisplay,
) {
    let Some(seat) = model.snapshot.own_seat() else {
        return;
    };
    let Some(zone) = layout
        .zones()
        .iter()
        .find(|zone| matches!(zone.id, ZoneId::Hand(id) if id.get() == seat))
    else {
        return;
    };
    let center = [
        i32::midpoint(zone.inner.min.x.get(), zone.inner.max.x.get()),
        i32::midpoint(zone.inner.min.y.get(), zone.inner.max.y.get()),
        i32::midpoint(zone.inner.min.z.get(), zone.inner.max.z.get()),
    ];
    if let Some(pose) = poses.0.get_mut(key) {
        pose.current = center.map(|value| value as f32);
        pose.target = pose.current;
    }
}

fn rotate_mdeg(current: i32, direction: i8, step: i32) -> i32 {
    (current + i32::from(direction) * step).rem_euclid(FULL_TURN_MDEG)
}

fn card_rotation_direction(q_pressed: bool, e_pressed: bool) -> i8 {
    // Positive rotation around table-up appears counter-clockwise through the
    // upright tabletop camera: Q turns left and E turns right.
    i8::from(q_pressed) - i8::from(e_pressed)
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
    drag: Res<DragState>,
    mut poses: ResMut<PoseDisplay>,
    mut cards: Query<(&CardVisual, &mut Transform)>,
) {
    let alpha = 1. - (-18. * time.delta_secs()).exp();
    for (key, pose) in &mut poses.0 {
        if drag.card_key.as_deref() != Some(key) {
            for axis in 0..3 {
                pose.current[axis] += (pose.target[axis] - pose.current[axis]) * alpha;
            }
        }
        pose.current_rotation =
            smooth_card_rotation(pose.current_rotation, pose.rotation_mdeg, alpha);
    }
    for (card, mut transform) in &mut cards {
        let Some(pose) = poses.0.get(&card.key) else {
            continue;
        };
        *transform = Transform::from_translation(mm_position(pose.current))
            .with_rotation(pose.current_rotation);
    }
}

fn cursor_to_world_mm(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    cursor: Vec2,
    height_mm: f32,
) -> Option<[f32; 3]> {
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let plane_height = height_mm / 1_000.0;
    let distance = ray.intersect_plane(
        Vec3::new(0.0, plane_height, 0.0),
        InfinitePlane3d::new(Vec3::Y),
    )?;
    let point = ray.get_point(distance) * 1_000.0;
    Some([
        point.x.clamp(-9_000.0, 9_000.0),
        height_mm,
        point.z.clamp(-9_000.0, 9_000.0),
    ])
}

fn update_status_labels(state: Res<UiState>, mut labels: Query<&mut Text, With<StatusLabel>>) {
    if state.is_changed() {
        for mut label in &mut labels {
            label.0.clone_from(&state.status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_spacetimedb_client::{GameView, HandCardView, MemberView, RoomView};

    fn bidding_snapshot(private_hand_ready: bool) -> ClientSnapshot {
        ClientSnapshot {
            identity: Some("alice-id".into()),
            members: vec![MemberView {
                identity: "alice-id".into(),
                display_name: "Alice".into(),
                seat: Some(0),
                connected: true,
                is_self: true,
            }],
            hand: private_hand_ready
                .then(|| HandCardView {
                    card_key: "room:alice:card-0-0".into(),
                    card_id: "card-0-0".into(),
                    face: "A♠".into(),
                })
                .into_iter()
                .collect(),
            game: Some(GameView {
                phase: "bidding".into(),
                actor_seat: Some(0),
                dealer_seat: Some(0),
                round_index: 0,
                hand_size: 1,
                hand_counts: [1, 1],
                bids: [None, None],
                trick_count: 0,
                trick_seats: [None, None],
                trick_cards: [None, None],
                tricks_won: [0, 0],
                scores: [0, 0],
                pot_cents: 50,
                trump: Some(0),
                action_count: 0,
            }),
            ..ClientSnapshot::default()
        }
    }

    #[test]
    fn bidding_waits_for_the_private_hand_view_to_match_the_public_projection() {
        let synchronizing = bidding_snapshot(false);
        assert!(private_hand_is_synchronizing(&synchronizing));
        assert!(!may_bid(&synchronizing, 0));

        let ready = bidding_snapshot(true);
        assert!(!private_hand_is_synchronizing(&ready));
        assert!(may_bid(&ready, 0));
        assert!(may_bid(&ready, 1));
        assert!(!may_bid(&ready, 2));
    }

    #[test]
    fn room_loading_waits_for_complete_private_and_spatial_projection() {
        let capability = RoomCapability {
            room_id: "room-one".into(),
            join_code: "PCH-1111-1111-1111-1111".into(),
        };
        let mut snapshot = bidding_snapshot(false);
        snapshot.rooms.push(RoomView {
            room_id: capability.room_id.clone(),
        });
        snapshot.room_capability = Some(capability.clone());
        snapshot.members.push(MemberView {
            identity: "bob-id".into(),
            display_name: "Bob".into(),
            seat: Some(1),
            connected: true,
            is_self: false,
        });

        assert!(!room_projection_ready(&snapshot, Some(&capability)));
        snapshot.hand.push(HandCardView {
            card_key: "room:alice:card-0-0".into(),
            card_id: "card-0-0".into(),
            face: "A♠".into(),
        });
        assert!(!room_projection_ready(&snapshot, Some(&capability)));
        for (seat, owner) in [(0, "alice-id"), (1, "bob-id")] {
            snapshot.card_poses.push(CardPoseView {
                card_key: format!("room:{owner}:card-{seat}-0"),
                card_id: format!("card-{seat}-0"),
                owner: owner.into(),
                owner_seat: seat,
                logical_location: format!("hand:{seat}"),
                position_mm: [0, 40, 0],
                rotation_mdeg: [0, 0, 0],
                sequence: 0,
            });
        }
        assert!(room_projection_ready(&snapshot, Some(&capability)));
    }

    #[test]
    fn loading_does_not_reveal_an_unseated_table_before_the_render_world_confirms_it() {
        let capability = RoomCapability {
            room_id: "room-one".into(),
            join_code: "PCH-1111-1111-1111-1111".into(),
        };
        let mut snapshot = ClientSnapshot::default();
        snapshot.rooms.push(RoomView {
            room_id: capability.room_id.clone(),
        });
        snapshot.members.extend([
            MemberView {
                identity: "alice-id".into(),
                display_name: "Alice".into(),
                seat: None,
                connected: true,
                is_self: false,
            },
            MemberView {
                identity: "bob-id".into(),
                display_name: "Bob".into(),
                seat: None,
                connected: true,
                is_self: true,
            },
        ]);
        let state = UiState {
            room_scene_generation: 2,
            ..UiState::default()
        };
        let render_readiness = TableRenderReadiness::default();

        assert!(room_projection_ready(&snapshot, Some(&capability)));
        assert!(!room_scene_projection_ready(&snapshot, &state, false));
        assert!(room_scene_projection_ready(&snapshot, &state, true));
        assert!(!render_readiness.has_rendered(state.room_scene_generation));
        render_readiness.mark_rendered(1);
        assert!(
            !render_readiness.has_rendered(state.room_scene_generation),
            "a render confirmation from an older room scene must not reveal this one"
        );
        render_readiness.mark_rendered(state.room_scene_generation);
        assert!(render_readiness.has_rendered(state.room_scene_generation));
    }

    #[test]
    fn every_frontend_screen_clears_a_fresh_swapchain_image() {
        for screen in [
            UiScreen::IdentityGate,
            UiScreen::Connecting,
            UiScreen::LoadingRoom,
            UiScreen::ResumeOffer,
            UiScreen::MainMenu,
            UiScreen::LobbyEnded,
        ] {
            assert!(matches!(
                ui_camera_clear_for_screen(screen),
                bevy::camera::ClearColorConfig::Default
            ));
        }
        assert!(matches!(
            ui_camera_clear_for_screen(UiScreen::Table),
            bevy::camera::ClearColorConfig::None
        ));
    }

    #[test]
    fn pose_transform_maps_network_units_at_the_rendering_boundary() {
        let pose = pose_transform([1_200.0, 80.0, -800.0], [0, 180_000, 0]);
        assert!(
            pose.translation
                .abs_diff_eq(Vec3::new(1.2, 0.08, -0.8), 0.000_01)
        );
        assert!(
            (pose.rotation * Vec3::X).abs_diff_eq(Vec3::NEG_X, 0.000_01),
            "180 degrees around table-up should reverse the card's local x axis"
        );
    }

    #[test]
    fn displayed_card_rotation_moves_partway_toward_the_exact_target() {
        let target = [0, 90_000, 0];
        let halfway = smooth_card_rotation(Quat::IDENTITY, target, 0.5);
        let expected_halfway = rotation_mdeg_quat([0, 45_000, 0]);
        assert!((halfway * Vec3::X).abs_diff_eq(expected_halfway * Vec3::X, 0.000_01));
        assert!(!(halfway * Vec3::X).abs_diff_eq(rotation_mdeg_quat(target) * Vec3::X, 0.000_01));
    }

    #[test]
    fn card_labels_preserve_aspect_inside_the_canonical_face() {
        for aspect in [0.4, 1.0, 2.5] {
            let (width, height) = card_label_size(aspect);
            assert!(width <= CARD_WORLD_WIDTH * 0.82 + f32::EPSILON);
            assert!(height <= CARD_WORLD_HEIGHT * 0.72 + f32::EPSILON);
            assert!((width / height - aspect).abs() < 0.000_01);
        }
    }

    #[test]
    fn player_cameras_look_from_their_own_side_of_the_table() {
        let seat_zero = player_camera_transform(Some(0));
        let seat_one = player_camera_transform(Some(1));
        assert!(seat_zero.translation.z > 0.0);
        assert!(seat_one.translation.z < 0.0);
        assert!((seat_zero.forward().dot(Vec3::NEG_Z)) > 0.5);
        assert!((seat_one.forward().dot(Vec3::Z)) > 0.5);
        assert_eq!(hand_camera_up(Some(0)), Vec3::NEG_Z);
        assert_eq!(hand_camera_up(Some(1)), Vec3::Z);
    }

    #[test]
    fn camera_focus_is_clamped_to_twice_the_table_top() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 1).expect("layout id"))
            .expect("registered layout");
        let clamped = clamp_table_focus(&layout, Vec3::new(99.0, -20.0, -99.0));
        let table = layout
            .scene_objects()
            .into_iter()
            .find(|object| object.kind == SceneObjectKind::Table)
            .expect("table object");
        let center = point_to_world(table.pose.translation);
        let expected = Vec3::new(
            center.x + table.half_extents.x as f32 / 500.0,
            center.y + table.half_extents.y as f32 / 1_000.0,
            center.z - table.half_extents.z as f32 / 500.0,
        );
        assert!(clamped.abs_diff_eq(expected, f32::EPSILON));
    }

    #[test]
    fn camera_reset_changes_the_target_without_teleporting_the_view() {
        let mut controller = TableCameraController::default();
        controller.current.focus = Vec3::new(0.4, 0.02, 0.3);
        let before = controller.current;
        controller.reset_for_seat(Some(1), false);
        assert_eq!(controller.current.focus, before.focus);
        assert_eq!(controller.target.focus, camera_home(Some(1)).focus);
        assert_ne!(controller.current.focus, controller.target.focus);
    }

    #[test]
    fn tactical_camera_toggle_preserves_each_mode_and_smooths_from_current_pose() {
        let mut controller = TableCameraController::default();
        controller.target.focus = Vec3::new(0.2, 0.02, -0.1);
        let perspective_target = controller.target;
        let current = controller.current;

        controller.toggle_mode();
        assert_eq!(controller.mode, TableCameraMode::Tactical);
        assert_eq!(controller.current, current);
        assert_eq!(controller.target, tactical_camera_home(None));

        controller.target.focus = Vec3::new(-0.3, 0.02, 0.25);
        let tactical_target = controller.target;
        controller.toggle_mode();
        assert_eq!(controller.mode, TableCameraMode::Perspective);
        assert_eq!(controller.target, perspective_target);

        controller.toggle_mode();
        assert_eq!(controller.target, tactical_target);
    }

    #[test]
    fn tactical_camera_uses_a_bounded_zoomable_orthographic_projection() {
        assert!(tactical_camera_home(None).pitch > camera_home(None).pitch);
        let mut projection = Projection::Perspective(PerspectiveProjection::default());
        apply_table_projection(&mut projection, TableCameraMode::Tactical, 1.25);
        let Projection::Orthographic(orthographic) = projection else {
            panic!("tactical camera must use parallel projection");
        };
        assert!((orthographic.scale - 1.25).abs() < f32::EPSILON);
        assert!(matches!(
            orthographic.scaling_mode,
            ScalingMode::FixedVertical { viewport_height }
                if (viewport_height - TACTICAL_VIEW_HEIGHT).abs() < f32::EPSILON
        ));
        assert!((zoom_orthographic_scale(1.0, 10_000.0) - TACTICAL_MIN_SCALE).abs() < f32::EPSILON);
        assert!(
            (zoom_orthographic_scale(1.0, -10_000.0) - TACTICAL_MAX_SCALE).abs() < f32::EPSILON
        );
    }

    #[test]
    fn camera_wheel_zoom_is_proportional_and_bounded() {
        let start = camera_home(None).distance;
        let zoomed_in = zoom_camera_distance(start, 1.0);
        assert!(zoomed_in < start);
        assert!((zoom_camera_distance(zoomed_in, -1.0) - start).abs() < 0.000_01);
        assert!((zoom_camera_distance(start, 10_000.0) - CAMERA_MIN_DISTANCE).abs() < f32::EPSILON);
        assert!(
            (zoom_camera_distance(start, -10_000.0) - CAMERA_MAX_DISTANCE).abs() < f32::EPSILON
        );
    }

    #[test]
    fn fps_overlay_starts_hidden_and_toggles_text_and_graph_together() {
        let mut config = hidden_fps_overlay_config(Handle::<Font>::default());
        assert!(!config.enabled);
        assert!(!config.frame_time_graph_config.enabled);

        set_fps_overlay_visible(&mut config, true);
        assert!(config.enabled);
        assert!(config.frame_time_graph_config.enabled);

        set_fps_overlay_visible(&mut config, false);
        assert!(!config.enabled);
        assert!(!config.frame_time_graph_config.enabled);
    }

    #[test]
    fn escape_menu_cancels_a_pending_leave_confirmation() {
        let mut state = UiState {
            escape_menu_open: true,
            confirm_leave: true,
            ..UiState::default()
        };
        toggle_escape_menu(&mut state);
        assert!(!state.escape_menu_open);
        assert!(!state.confirm_leave);
    }

    #[test]
    fn escape_from_options_returns_to_the_table_menu_before_resuming() {
        let mut state = UiState {
            escape_menu_open: true,
            escape_menu_page: EscapeMenuPage::Options,
            ..UiState::default()
        };
        toggle_escape_menu(&mut state);
        assert!(state.escape_menu_open);
        assert_eq!(state.escape_menu_page, EscapeMenuPage::Main);

        toggle_escape_menu(&mut state);
        assert!(!state.escape_menu_open);
    }

    #[test]
    fn camera_y_inversion_is_on_by_default_and_reverses_the_old_response() {
        let options = CameraOptions::default();
        assert!(options.invert_y);
        assert!(camera_pitch_delta(2.0, options.invert_y) > 0.0);
        assert!(camera_pitch_delta(2.0, false) < 0.0);
        assert!(
            (camera_pitch_delta(2.0, options.invert_y) + camera_pitch_delta(2.0, false)).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn only_a_complete_card_inside_the_canonical_play_zone_proposes_play() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 1).expect("layout id"))
            .expect("registered layout");
        assert!(is_play_drop(&layout, [0.0, 40.0, 0.0]));
        assert!(!is_play_drop(&layout, [0.0, 40.0, 520.0]));
        assert!(!is_play_drop(&layout, [90.0, 40.0, 0.0]));
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

    #[test]
    fn rotation_snap_cycles_and_wraps_in_millidegrees() {
        let mut snap = RotationSnap::default();
        assert_eq!(snap.label(), "Rotation snap: 45°");
        assert_eq!(rotate_mdeg(350_000, 1, snap.step_mdeg()), 35_000);
        assert_eq!(rotate_mdeg(10_000, -1, snap.step_mdeg()), 325_000);

        snap.cycle();
        assert_eq!(snap.label(), "Rotation snap: 60°");
        snap.cycle();
        assert_eq!(snap.label(), "Rotation snap: 90°");
        snap.cycle();
        assert_eq!(snap.label(), "Rotation snap: off");
        assert_eq!(snap.step_mdeg(), 5_000);
    }

    #[test]
    fn q_turns_cards_counter_clockwise_and_e_turns_them_clockwise() {
        let step = 90_000;
        let q_angle = rotate_mdeg(0, card_rotation_direction(true, false), step);
        let e_angle = rotate_mdeg(0, card_rotation_direction(false, true), step);
        assert_eq!(q_angle, 90_000);
        assert_eq!(e_angle, 270_000);
        assert!((rotation_mdeg_quat([0, q_angle, 0]) * Vec3::X).abs_diff_eq(Vec3::NEG_Z, 0.000_01));
        assert!((rotation_mdeg_quat([0, e_angle, 0]) * Vec3::X).abs_diff_eq(Vec3::Z, 0.000_01));
    }

    #[test]
    fn graphics_backend_names_are_explicit() {
        assert_eq!(GraphicsBackend::parse("auto"), Ok(GraphicsBackend::Auto));
        assert_eq!(GraphicsBackend::parse("dx12"), Ok(GraphicsBackend::Dx12));
        assert_eq!(
            GraphicsBackend::parse("vulkan"),
            Ok(GraphicsBackend::Vulkan)
        );
        assert!(GraphicsBackend::parse("metal").is_err());
    }
}
