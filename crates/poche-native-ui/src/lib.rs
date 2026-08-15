// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bevy leaf adapter for Poche's canonical, viewer-scoped spatial scene.
//!
//! Bevy ECS components mirror validated spatial records. Input observers may
//! propose typed intent through [`NativeController`], but renderer transforms
//! never become game or card-location authority.

// Bevy ECS system parameters are intentionally passed as wrapper values. The
// numeric casts cross the already-proved ±10,000 mm integer-to-f32 boundary.
#![allow(clippy::cast_precision_loss, clippy::needless_pass_by_value)]

use std::{
    collections::{BTreeSet, HashMap},
    f32::consts::PI,
    io::Cursor,
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    image::Image,
    input::mouse::AccumulatedMouseMotion,
    log::LogPlugin,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    },
    window::{CursorIcon, ExitCondition, PrimaryWindow, SystemCursorIcon, WindowPlugin},
    winit::WinitPlugin,
};
use poche_capture::{
    CaptureCameraMetadata, CapturePipeline, CaptureProvider, CaptureProviderPoll,
    CaptureQualification, CaptureSurfaceMetadata, PersistedCapture, RawCaptureArtifact,
    RawCaptureBundle,
};
use poche_player_client::{AdvertisedAction, DeviceObservation};
use poche_protocol::{
    CaptureArtifactId, CaptureConsentPolicyWire, CaptureDenialReasonWire, CapturePrivacyWire,
    CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId, CommandPayload,
    DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1, DeviceId,
    DeviceSignatureIntentWire, GameActionWire, PrincipalId, RoomId, SemanticHash,
    SignatureAlgorithm, SignatureBytes, UnsignedCaptureProviderAdvertisementWire,
    UnsignedCaptureRequestWire,
};
use poche_slug::{
    DEFAULT_BAND_SIZE_FONT_UNITS, DirectionalBands, GlyphGeometry, Point, SlugError, SlugFont,
    build_directional_bands, build_gpu_glyph_metadata, coverage_banded,
};
use poche_spatial::{
    AabbMm, AnimationEndpoint, CardFace, CardLocation, CardObjectId, HalfExtentsMm, LayoutId,
    ObjectId, Point3Mm, PoseMm, ResolvedCardPlay, SeatId, SpatialLayout, SpatialScene, TableId,
    TextBinding, YawMilliDegrees, ZoneId, reconstruct_animation_endpoint, registered_layout,
    resolve_card_play, resolve_drag_play, spatial_scene_hash, spatial_scene_hash_hex,
};
use poche_ui::{
    ConnectionPresentation, PresentationInput, PresentationModel, embedded_spatial_fixture,
    realize_presentation_spatial,
};
use serde::Serialize;

mod native_capture;
mod native_live;

pub use native_capture::*;
pub use native_live::*;

/// Explicit OFL-licensed font consumed by Slug and native UI text.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/CaskaydiaCove-Regular.ttf");

const METRES_PER_MILLIMETRE: f32 = 0.001;
const SLUG_RASTER_PIXELS_PER_EM: f32 = 96.0;
const SLUG_RASTER_PADDING_PIXELS: u32 = 2;
const CAMERA_MOVE_METRES_PER_SECOND: f32 = 0.8;
const CAMERA_ROTATE_RADIANS_PER_SECOND: f32 = 1.35;
const CAMERA_MOUSE_PAN_METRES_PER_PIXEL: f32 = 0.002_2;
const CAMERA_RESET_SECONDS: f32 = 0.55;
const CAMERA_MIN_PITCH: f32 = 0.22;
const CAMERA_MAX_PITCH: f32 = 1.32;

/// Renderer buffer packet made from the stable Slug ABI.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugGpuPacket {
    /// Two vec4 records for every quadratic curve.
    pub curve_vec4s: Vec<[f32; 4]>,
    /// Concatenated directional-band tables and curve-index lists.
    pub band_words: Vec<u32>,
    /// Sixteen canonical words per glyph.
    pub metadata_words: Vec<[u32; 16]>,
    /// Character and horizontal advance for every glyph instance.
    pub instances: Vec<(char, f32)>,
}

/// Convert semantic text to the renderer-neutral Slug GPU contract.
///
/// # Errors
///
/// Returns a stable Slug error for invalid font or generated geometry.
pub fn slug_packet_for_text(text: &str) -> Result<SlugGpuPacket, SlugError> {
    let font = SlugFont::parse(FONT_BYTES, 0, '?')?;
    let metrics = font.metrics();
    let mut packet = SlugGpuPacket {
        curve_vec4s: Vec::new(),
        band_words: Vec::new(),
        metadata_words: Vec::new(),
        instances: Vec::new(),
    };
    let mut curve_count = 0_u32;
    for character in text.chars() {
        let geometry = font.glyph_geometry(character)?;
        let bands = build_directional_bands(
            &geometry.curves,
            geometry.bounds,
            DEFAULT_BAND_SIZE_FONT_UNITS,
        )?;
        let band_start = u32::try_from(packet.band_words.len())
            .map_err(|_| SlugError::CountOverflow("native band buffer"))?;
        let mut packed_bands = bands.packed_words(geometry.curves.len())?;
        let header_words = (bands.horizontal.len() + bands.vertical.len()) * 4;
        for header in packed_bands[..header_words].chunks_exact_mut(4) {
            header[1] = header[1]
                .checked_add(band_start)
                .ok_or(SlugError::CountOverflow("native band offset"))?;
            header[2] = header[2]
                .checked_add(band_start)
                .ok_or(SlugError::CountOverflow("native band offset"))?;
        }
        let metadata =
            build_gpu_glyph_metadata(&geometry, &bands, metrics, curve_count, band_start)?;
        for curve in &geometry.curves {
            packet.curve_vec4s.extend(curve.gpu_vec4s());
        }
        curve_count = curve_count
            .checked_add(
                u32::try_from(geometry.curves.len())
                    .map_err(|_| SlugError::CountOverflow("native curve buffer"))?,
            )
            .ok_or(SlugError::CountOverflow("native curve buffer"))?;
        packet.band_words.extend(packed_bands);
        packet.metadata_words.push(metadata.into_words());
        packet.instances.push((character, geometry.advance));
    }
    Ok(packet)
}

/// Accepted typed presentation transition shared by named and drag input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommittedPresentation {
    /// Resolved typed play and its spatial sidecar.
    pub play: ResolvedCardPlay,
    /// Reconstructable renderer endpoint.
    pub endpoint: AnimationEndpoint,
    /// Monotonic local presentation generation.
    pub generation: u64,
}

/// Canonical-to-render adapter state.
#[derive(Resource)]
pub struct NativeController {
    layout: SpatialLayout,
    scene: SpatialScene,
    issuing_seat: SeatId,
    legal_plays: Option<BTreeSet<u8>>,
    committed: Option<CommittedPresentation>,
    last_finding: String,
    generation: u64,
    last_commit_cost: Option<Duration>,
}

impl NativeController {
    /// Construct an adapter around a validated exact-recipient scene.
    ///
    /// # Errors
    ///
    /// Rejects an invalid or mismatched scene/layout/seat.
    pub fn try_new(
        layout: SpatialLayout,
        scene: SpatialScene,
        issuing_seat: SeatId,
    ) -> Result<Self, String> {
        scene
            .validate()
            .map_err(|error| format!("invalid viewer scene: {error:?}"))?;
        if layout.id() != scene.layout
            || layout.table_id() != scene.table_id
            || issuing_seat.get() >= layout.id().players()
        {
            return Err("scene, layout, and issuing seat do not share one frame".to_owned());
        }
        Ok(Self {
            layout,
            scene,
            issuing_seat,
            legal_plays: None,
            committed: None,
            last_finding: "ready: drag an owned card to PLAY, press P, or use --play-card"
                .to_owned(),
            generation: 0,
            last_commit_cost: None,
        })
    }

    /// Borrow the registered layout.
    #[must_use]
    pub const fn layout(&self) -> &SpatialLayout {
        &self.layout
    }

    /// Borrow the immutable canonical viewer scene.
    #[must_use]
    pub const fn scene(&self) -> &SpatialScene {
        &self.scene
    }

    /// Return the issuing seat.
    #[must_use]
    pub const fn issuing_seat(&self) -> SeatId {
        self.issuing_seat
    }

    /// Return the latest accepted presentation transition.
    #[must_use]
    pub const fn committed(&self) -> Option<CommittedPresentation> {
        self.committed
    }

    /// Return the latest spatial classification/finding.
    #[must_use]
    pub fn last_finding(&self) -> &str {
        &self.last_finding
    }

    /// Resolve and commit a named card.
    ///
    /// # Errors
    ///
    /// Returns stable spatial input evidence as display text.
    pub fn commit_named(&mut self, face: CardFace) -> Result<CommittedPresentation, String> {
        let started = Instant::now();
        let resolved = resolve_card_play(&self.layout, &self.scene, self.issuing_seat, face)
            .map_err(|finding| format!("{finding:?}"))?;
        self.accept_resolved(resolved, started)
    }

    /// Resolve and commit a dropped opaque card.
    ///
    /// # Errors
    ///
    /// Returns stable spatial input evidence as display text.
    pub fn commit_drag(
        &mut self,
        object: CardObjectId,
        released_bounds: AabbMm,
    ) -> Result<CommittedPresentation, String> {
        let started = Instant::now();
        let resolved = resolve_drag_play(
            &self.layout,
            &self.scene,
            self.issuing_seat,
            object,
            released_bounds,
        )
        .map_err(|finding| format!("{finding:?}"))?;
        self.accept_resolved(resolved, started)
    }

    /// Return the first viewer-authorized card in the issuing hand.
    #[must_use]
    pub fn first_owned_face(&self) -> Option<CardFace> {
        self.scene.cards.iter().find_map(|card| {
            matches!(
                card.location,
                CardLocation::Hand { seat, .. } if seat == self.issuing_seat
            )
            .then_some(card.face)
            .flatten()
        })
    }

    fn accept_resolved(
        &mut self,
        play: ResolvedCardPlay,
        started: Instant,
    ) -> Result<CommittedPresentation, String> {
        if self
            .legal_plays
            .as_ref()
            .is_some_and(|legal| !legal.contains(&play.face.code()))
        {
            return Err(format!(
                "{} was not advertised for this exact projection",
                play.face.label()
            ));
        }
        let endpoint = reconstruct_animation_endpoint(&self.layout, play.record)
            .map_err(|finding| format!("{finding:?}"))?;
        self.generation = self.generation.saturating_add(1);
        let committed = CommittedPresentation {
            play,
            endpoint,
            generation: self.generation,
        };
        let elapsed = started.elapsed();
        self.last_finding = format!(
            "accepted {} -> PLAY (generation {}, semantic commit {} µs)",
            play.face.label(),
            self.generation,
            elapsed.as_micros()
        );
        self.last_commit_cost = Some(elapsed);
        self.committed = Some(committed);
        Ok(committed)
    }
}

/// Build the native fixture from the exact-recipient shared replay.
///
/// # Errors
///
/// Returns fixture, projection, layout, or seat-map failures.
pub fn replay_fixture_controller() -> Result<NativeController, String> {
    let fixture = embedded_spatial_fixture()?;
    NativeController::try_new(fixture.layout, fixture.scene, fixture.issuing_seat)
}

/// Build the native leaf adapter from one exact-recipient device observation.
/// Only game actions advertised beside that exact projection are admitted to
/// the presentation model; no renderer-specific action graph is synthesized.
///
/// # Errors
///
/// Returns a stable projection/layout/seat realization failure.
pub fn native_controller_from_observation(
    observation: &DeviceObservation,
) -> Result<NativeController, String> {
    let payload = &observation.projection.payload;
    let players = payload
        .public_game_state
        .as_ref()
        .and_then(|game| u8::try_from(game.hand_counts.len()).ok())
        .or_else(|| {
            payload
                .members
                .iter()
                .filter_map(|member| member.seat)
                .max()
                .map(|seat| seat.saturating_add(1).max(2))
        })
        .unwrap_or(2);
    let layout_id = LayoutId::new(players, 1)
        .ok_or_else(|| format!("unsupported live player count {players}"))?;
    let table_hash = blake3::hash(observation.projection.room_id.as_str().as_bytes());
    let mut table_bytes = [0_u8; 8];
    table_bytes.copy_from_slice(&table_hash.as_bytes()[..8]);
    let layout = registered_layout(TableId::new(u64::from_be_bytes(table_bytes)), layout_id)
        .map_err(|error| format!("live layout failed: {error:?}"))?;
    let legal_actions = observation
        .actions
        .iter()
        .filter_map(|action| match &action.payload {
            CommandPayload::GameAction { action } => Some(action.clone()),
            _ => None,
        })
        .collect();
    let presentation = PresentationModel::from_input(PresentationInput {
        viewer: observation.projection.principal_id.as_str().to_owned(),
        projection: payload.clone(),
        legal_actions,
        connection: ConnectionPresentation::Connected,
        countdown: None,
        chat: Vec::new(),
        notices: Vec::new(),
    });
    let scene = realize_presentation_spatial(
        &layout,
        observation.projection.projection_epoch,
        &presentation,
    )
    .map_err(|error| format!("live spatial projection failed: {error:?}"))?;
    let issuing_ordinal = payload
        .members
        .iter()
        .find(|member| member.principal_id == observation.projection.principal_id)
        .and_then(|member| member.seat)
        .ok_or_else(|| "live viewer does not occupy a seat".to_owned())?;
    let issuing_seat = SeatId::new(issuing_ordinal, layout_id)
        .ok_or_else(|| "live viewer seat is outside the selected layout".to_owned())?;
    let legal_plays = observation
        .actions
        .iter()
        .filter_map(|action| match &action.payload {
            CommandPayload::GameAction {
                action: GameActionWire::Play { card },
            } => Some(*card),
            _ => None,
        })
        .collect();
    let mut controller = NativeController::try_new(layout, scene, issuing_seat)?;
    controller.legal_plays = Some(legal_plays);
    Ok(controller)
}

/// Resolve a spatially accepted play back to the opaque action advertised for
/// the exact observation that produced the scene.
#[must_use]
pub fn advertised_action_for_play<'a>(
    observation: &'a DeviceObservation,
    committed: &CommittedPresentation,
) -> Option<&'a AdvertisedAction> {
    observation.actions.iter().find(|action| {
        matches!(
            action.payload,
            CommandPayload::GameAction {
                action: GameActionWire::Play { card }
            } if card == committed.play.face.code()
        )
    })
}

/// Parse a dense, compact Unicode, or CLI-word card spelling.
#[must_use]
pub fn parse_card_face(value: &str) -> Option<CardFace> {
    if let Ok(code) = value.parse::<u8>() {
        return CardFace::new(code);
    }
    let normalized = value.trim().to_lowercase().replace(['-', '_', ' '], "");
    let suits = [
        ("clubs", '♣', 0_u8),
        ("diamonds", '♦', 1),
        ("hearts", '♥', 2),
        ("spades", '♠', 3),
    ];
    let (rank, suit) = suits.iter().find_map(|(word, glyph, index)| {
        normalized
            .strip_suffix(word)
            .or_else(|| normalized.strip_suffix(*glyph))
            .map(|rank| (rank, *index))
    })?;
    let rank = rank.strip_suffix("of").unwrap_or(rank);
    let rank = match rank {
        "2" => 0,
        "3" => 1,
        "4" => 2,
        "5" => 3,
        "6" => 4,
        "7" => 5,
        "8" => 6,
        "9" => 7,
        "10" => 8,
        "j" | "jack" => 9,
        "q" | "queen" => 10,
        "k" | "king" => 11,
        "a" | "ace" => 12,
        _ => return None,
    };
    CardFace::new(suit * 13 + rank)
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct CanonicalMirror {
    id: ObjectId,
    pose: PoseMm,
    location: Option<CardLocation>,
}

#[derive(Component, Clone, Copy, Debug)]
struct DraggableCard(CardObjectId);

#[derive(Component, Clone, Copy, Debug, Default)]
struct DragPreview {
    pixels: Vec2,
}

#[derive(Resource, Default)]
struct DebugOverlay {
    enabled: bool,
}

#[derive(Resource, Default)]
struct TweenClock {
    generation: u64,
    elapsed_seconds: f32,
}

#[derive(Component)]
struct TabletopCamera;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraView {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl CameraView {
    fn home() -> Self {
        let target = Vec3::new(0.0, 0.08, 0.0);
        let offset = Vec3::new(0.0, 1.55, 1.65) - target;
        Self {
            target,
            yaw: offset.x.atan2(offset.z),
            pitch: (offset.y / offset.length()).asin(),
            distance: offset.length(),
        }
    }

    fn eye(self) -> Vec3 {
        let horizontal = self.distance * self.pitch.cos();
        self.target
            + Vec3::new(
                horizontal * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                horizontal * self.yaw.cos(),
            )
    }

    fn transform(self) -> Transform {
        Transform::from_translation(self.eye()).looking_at(self.target, Vec3::Y)
    }

    fn interpolate(self, to: Self, factor: f32) -> Self {
        let yaw_delta = (to.yaw - self.yaw + PI).rem_euclid(2.0 * PI) - PI;
        Self {
            target: self.target.lerp(to.target, factor),
            yaw: self.yaw + yaw_delta * factor,
            pitch: self.pitch + (to.pitch - self.pitch) * factor,
            distance: self.distance + (to.distance - self.distance) * factor,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CameraResetTween {
    from: CameraView,
    elapsed_seconds: f32,
}

#[derive(Resource, Debug)]
struct CameraRig {
    view: CameraView,
    reset: Option<CameraResetTween>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            view: CameraView::home(),
            reset: None,
        }
    }
}

impl CameraRig {
    fn begin_reset(&mut self) {
        self.reset = Some(CameraResetTween {
            from: self.view,
            elapsed_seconds: 0.0,
        });
    }

    fn cancel_reset(&mut self) {
        self.reset = None;
    }

    fn advance_reset(&mut self, delta_seconds: f32) {
        let Some(mut tween) = self.reset else {
            return;
        };
        tween.elapsed_seconds += delta_seconds;
        let linear = (tween.elapsed_seconds / CAMERA_RESET_SECONDS).clamp(0.0, 1.0);
        let eased = linear * linear * (3.0 - 2.0 * linear);
        if linear < 1.0 {
            self.view = tween.from.interpolate(CameraView::home(), eased);
            self.reset = Some(tween);
        } else {
            self.view = CameraView::home();
            self.reset = None;
        }
    }
}

#[derive(Resource)]
struct LaunchClock {
    started: Instant,
    first_update: Option<Duration>,
    frame_samples: Vec<f64>,
    screenshot_requested: bool,
    report_written: bool,
}

impl Default for LaunchClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            first_update: None,
            frame_samples: Vec::new(),
            screenshot_requested: false,
            report_written: false,
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
struct AcceptanceOptions {
    screenshot: Option<PathBuf>,
    report: Option<PathBuf>,
    exit_after: Option<Duration>,
    debug_overlay: bool,
}

#[derive(Serialize)]
struct AcceptanceReport {
    scene_semantic_hash: String,
    startup_to_first_update_microseconds: u128,
    semantic_commit_microseconds: Option<u128>,
    semantic_commit_nanoseconds: Option<u128>,
    sampled_frames: usize,
    mean_frame_milliseconds: f64,
    p95_frame_milliseconds: f64,
    committed_face: Option<String>,
    scene_objects: usize,
    scene_cards: usize,
    semantic_text_runs: usize,
    visible_face_runs: usize,
    hidden_cards_without_face_runs: usize,
    debug_overlay: bool,
    screenshot: Option<String>,
    qualification: &'static str,
}

/// Typed launch options shared by the standalone development binary and the
/// unified `poche desktop` command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeRenderMode {
    /// Ordinary human-operated desktop window backed by the OS swapchain.
    #[default]
    InteractiveWindow,
    /// GPU-backed image target with no primary window or Winit event loop.
    WindowlessImage,
}

#[derive(Clone, Debug, Default)]
pub struct NativeUiLaunchOptions {
    pub play_card: Option<String>,
    pub screenshot: Option<PathBuf>,
    pub acceptance_report: Option<PathBuf>,
    pub exit_after_seconds: Option<f64>,
    pub debug_overlay: bool,
    /// Select the render destination explicitly. Puppet and capture workers
    /// use [`NativeRenderMode::WindowlessImage`]; ordinary desktop launch uses
    /// [`NativeRenderMode::InteractiveWindow`].
    pub render_mode: NativeRenderMode,
    /// Optional exact-target provider handle owned by this graphical device.
    pub capture_provider: Option<NativeCaptureProvider>,
    /// Exact live projection identity rendered by this window.
    pub capture_context: Option<NativeCaptureContext>,
    /// The caller already installed the process tracing subscriber.
    pub external_tracing: bool,
}

const AUTOMATION_RENDER_WIDTH: u32 = 1280;
const AUTOMATION_RENDER_HEIGHT: u32 = 800;
const AUTOMATION_CAPTURE_PREROLL_FRAMES: u32 = 60;

/// Render destination selected before Bevy starts. Automation owns an image
/// target; interactive play owns the primary window swapchain.
#[derive(Clone, Debug, Resource)]
enum NativeRenderSurface {
    Windowed,
    Windowless {
        width: u32,
        height: u32,
        target: Option<Handle<Image>>,
    },
}

impl NativeRenderSurface {
    const fn from_mode(mode: NativeRenderMode) -> Self {
        match mode {
            NativeRenderMode::WindowlessImage => Self::Windowless {
                width: AUTOMATION_RENDER_WIDTH,
                height: AUTOMATION_RENDER_HEIGHT,
                target: None,
            },
            NativeRenderMode::InteractiveWindow => Self::Windowed,
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

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "validated positive window scale is intentionally quantized into integer capture metadata"
    )]
    fn extent(&self, window: Option<&Window>) -> Option<(u32, u32, u32)> {
        match self {
            Self::Windowed => window.map(|window| {
                (
                    window.resolution.physical_width(),
                    window.resolution.physical_height(),
                    (window.resolution.scale_factor() * 1_000.0).round() as u32,
                )
            }),
            Self::Windowless { width, height, .. } => Some((*width, *height, 1_000)),
        }
    }
}

/// Run the real Bevy window with arguments from the current process.
///
/// # Errors
///
/// Returns argument or fixture failures before the event loop starts.
pub fn run_from_env() -> Result<(), String> {
    let mut options = NativeUiLaunchOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--play-card" => {
                options.play_card = Some(
                    args.next()
                        .ok_or_else(|| "--play-card requires a face".to_owned())?,
                );
            }
            "--debug-overlay" => options.debug_overlay = true,
            "--screenshot" => {
                options.screenshot =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        "--screenshot requires an output path".to_owned()
                    })?));
            }
            "--acceptance-report" => {
                options.acceptance_report =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        "--acceptance-report requires an output path".to_owned()
                    })?));
            }
            "--exit-after-seconds" => {
                options.exit_after_seconds = Some(
                    args.next()
                        .ok_or_else(|| "--exit-after-seconds requires a number".to_owned())?
                        .parse::<f64>()
                        .map_err(|error| format!("invalid exit duration: {error}"))?,
                );
            }
            "--help" | "-h" => {
                println!(
                    "poche-native-ui [--play-card FACE|first] [--debug-overlay] [--screenshot PATH] \
                     [--acceptance-report PATH] [--exit-after-seconds N]\n\
                     Camera: middle-drag pan; WASD move; arrows rotate; Space reset view\n\
                     Game/debug: P play first owned card; F3 toggle spatial audit overlay"
                );
                return Ok(());
            }
            unknown => return Err(format!("unknown native UI option {unknown:?}")),
        }
    }

    run(options)
}

/// Run the Bevy leaf adapter from typed unified-executable options.
///
/// # Errors
///
/// Returns invalid launch options or fixture failures before the event loop.
pub fn run(options: NativeUiLaunchOptions) -> Result<(), String> {
    run_with_live_device(options, None)
}

/// Run the Bevy leaf adapter from one certified device's exact observation.
/// Human input is queued back through that device's configured transport.
///
/// # Errors
///
/// Returns invalid launch options or an invalid first live projection.
pub fn run_live(
    options: NativeUiLaunchOptions,
    live_device: NativeLiveDevice,
) -> Result<(), String> {
    run_with_live_device(options, Some(live_device))
}

#[allow(
    clippy::too_many_lines,
    reason = "the Bevy application contract is kept together so windowed and windowless plugin selection cannot drift"
)]
fn run_with_live_device(
    options: NativeUiLaunchOptions,
    mut live_device: Option<NativeLiveDevice>,
) -> Result<(), String> {
    let NativeUiLaunchOptions {
        play_card,
        screenshot,
        acceptance_report,
        exit_after_seconds,
        debug_overlay,
        render_mode,
        capture_provider,
        capture_context,
        external_tracing,
    } = options;
    let mut controller = if let Some(live) = &live_device {
        native_controller_from_observation(live.observation())?
    } else {
        replay_fixture_controller()?
    };
    if let Some(value) = play_card {
        let face = if value == "first" {
            controller
                .first_owned_face()
                .ok_or_else(|| "exact-recipient fixture has no owned face".to_owned())?
        } else {
            parse_card_face(&value).ok_or_else(|| format!("unrecognized card face {value:?}"))?
        };
        let committed = controller.commit_named(face)?;
        if let Some(live) = live_device.as_mut() {
            live.submit_play(&committed)?;
        }
    }
    let exit_after = exit_after_seconds
        .map(|seconds| {
            if !seconds.is_finite() || seconds < 1.0 {
                Err("exit duration must be finite and at least one second".to_owned())
            } else {
                Ok(Duration::from_secs_f64(seconds))
            }
        })
        .transpose()?;
    let acceptance = AcceptanceOptions {
        screenshot,
        report: acceptance_report,
        exit_after,
        debug_overlay,
    };

    let debug_overlay = DebugOverlay {
        enabled: acceptance.debug_overlay,
    };
    let windowless = render_mode == NativeRenderMode::WindowlessImage;
    let render_surface = NativeRenderSurface::from_mode(render_mode);
    let window_plugin = if windowless {
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                title: "Poche — canonical spatial mirror".to_owned(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }
    };
    let mut default_plugins = DefaultPlugins.set(window_plugin);
    if windowless {
        // Winit is the OS-window/event-loop integration. Windowless workers
        // own a GPU image target and a bounded schedule runner instead. Force
        // pipeline compilation to finish in-band so an early offscreen
        // readback cannot race the renderer and produce a blank artifact.
        default_plugins = default_plugins
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>();
    }
    let mut app = App::new();
    if external_tracing {
        app.add_plugins(default_plugins.disable::<LogPlugin>());
    } else {
        app.add_plugins(default_plugins);
    }
    if windowless {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    }
    app.insert_resource(controller)
        .insert_resource(debug_overlay)
        .insert_resource(render_surface)
        .insert_resource(acceptance)
        .init_resource::<TweenClock>()
        .init_resource::<CameraRig>()
        .init_resource::<LaunchClock>()
        .add_plugins((MeshPickingPlugin, FrameTimeDiagnosticsPlugin::default()))
        .add_systems(
            Startup,
            (setup_native_render_target, setup_native_scene).chain(),
        )
        .add_systems(
            Update,
            (
                keyboard_input,
                camera_input,
                apply_mirrored_transforms,
                draw_spatial_debug,
                update_status,
                poll_live_device,
                acceptance_driver,
            )
                .chain(),
        );
    if capture_provider.is_some() != capture_context.is_some() {
        return Err(
            "native capture provider and projection context must be supplied together".to_owned(),
        );
    }
    let shutdown_provider = capture_provider.clone();
    if let Some(provider) = capture_provider {
        app.insert_resource(provider);
        app.add_systems(Update, native_capture_driver.after(update_status));
    }
    if let Some(context) = capture_context {
        app.insert_resource(context);
    }
    if let Some(live_device) = live_device {
        app.insert_resource(live_device);
    }
    app.run();
    if let Some(provider) = shutdown_provider {
        let _ = provider.shutdown();
    }
    Ok(())
}

/// Exercise the real Bevy render target through the common provider and
/// persistence contracts. This deterministic fixture acceptance is a local
/// renderer prerequisite; certification/authorization is proved separately by
/// the session/device tests and composed by the multi-device puppet.
///
/// # Errors
///
/// Returns launch, provider, render, or shared-pipeline failures.
pub fn run_fixture_capture_acceptance(
    mut options: NativeUiLaunchOptions,
    artifact_root: impl Into<PathBuf>,
) -> Result<PersistedCapture, String> {
    let provider_device = DeviceId::new("native-fixture-renderer")
        .map_err(|_| "invalid fixture provider device".to_owned())?;
    let requester_device = DeviceId::new("native-fixture-requester")
        .map_err(|_| "invalid fixture requester device".to_owned())?;
    let player = PrincipalId::new("native-fixture-player")
        .map_err(|_| "invalid fixture player".to_owned())?;
    let room = RoomId::new("native-fixture-room").map_err(|_| "invalid fixture room".to_owned())?;
    let signature =
        || SignatureBytes::new("55".repeat(64)).map_err(|_| "invalid fixture signature".to_owned());
    let advertisement = UnsignedCaptureProviderAdvertisementWire {
        schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
        room_id: room.clone(),
        membership_epoch: 1,
        player_id: player.clone(),
        provider_device_id: provider_device.clone(),
        provider_kind: CaptureProviderKindWire::NativeBevy,
        representations: vec![CaptureRepresentationWire::Png],
        privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
        consent_policy: CaptureConsentPolicyWire::HarnessOnly,
        max_total_bytes: 8 * 1024 * 1024,
        advertisement_sequence: 1,
        expires_at_unix_ms: 2_000_000_000_000,
        signature_intent: DeviceSignatureIntentWire {
            domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: provider_device.clone(),
        },
    }
    .attach_signature(signature()?)
    .map_err(|_| "invalid fixture capture advertisement".to_owned())?;
    let request = UnsignedCaptureRequestWire {
        schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
        request_id: CaptureRequestId::new("native-fixture-capture")
            .map_err(|_| "invalid fixture capture request ID".to_owned())?,
        room_id: room,
        membership_epoch: 1,
        player_id: player,
        requester_device_id: requester_device.clone(),
        provider_device_id: provider_device,
        observed_revision: 1,
        expires_at_unix_ms: 2_000_000_000_000,
        replay_nonce: "native-fixture-capture-nonce".to_owned(),
        privacy: CapturePrivacyWire::ExactPlayerView,
        provider_kind: CaptureProviderKindWire::NativeBevy,
        representations: vec![CaptureRepresentationWire::Png],
        // Let the provider report the actual physical render-target extent.
        // A 1280x800 logical Windows window can have different physical
        // dimensions under DPI scaling.
        viewport: None,
        label: "native fixture acceptance".to_owned(),
        max_total_bytes: 8 * 1024 * 1024,
        signature_intent: DeviceSignatureIntentWire {
            domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: requester_device,
        },
    }
    .attach_signature(signature()?)
    .map_err(|_| "invalid fixture capture request".to_owned())?;
    let mut provider = NativeCaptureProvider::new(advertisement);
    provider
        .begin_capture(request.clone())
        .map_err(|error| error.to_string())?;
    let mut result_handle = provider.clone();
    options.capture_provider = Some(provider);
    options.capture_context = Some(NativeCaptureContext {
        current_revision: request.observed_revision,
        projection_hash: SemanticHash([1; 32]),
    });
    options.exit_after_seconds.get_or_insert(3.0);
    options.render_mode = NativeRenderMode::WindowlessImage;
    run(options)?;
    let bundle = match result_handle
        .poll_capture(&request.request_id)
        .map_err(|error| error.to_string())?
    {
        CaptureProviderPoll::Ready(bundle) => bundle,
        CaptureProviderPoll::Denied(reason) => {
            return Err(format!("native capture denied: {reason:?}"));
        }
        CaptureProviderPoll::Pending(stage) => {
            return Err(format!("native capture still pending: {stage:?}"));
        }
    };
    CapturePipeline::new(artifact_root)
        .persist(&bundle)
        .map_err(|error| error.to_string())
}

fn setup_native_render_target(
    mut images: ResMut<Assets<Image>>,
    mut surface: ResMut<NativeRenderSurface>,
) {
    let NativeRenderSurface::Windowless {
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

struct RasterGlyph {
    cursor: f32,
    geometry: GlyphGeometry,
    bands: DirectionalBands,
}

struct SlugRaster {
    image: Image,
    design_width: f32,
    design_height: f32,
}

struct SlugTextLayout {
    width: f32,
    height: f32,
    anchor_offset_x: f32,
}

const fn slug_text_color(binding: TextBinding) -> [u8; 3] {
    match binding {
        TextBinding::CardFace(_) | TextBinding::PlayerName(_) => [0, 0, 0],
        TextBinding::PlayerScore(_) => [165, 0, 0],
    }
}

fn slug_text_layout(binding: TextBinding, design_width: f32, design_height: f32) -> SlugTextLayout {
    let (width, height, left_anchored) = match binding {
        TextBinding::CardFace(_) => {
            let scale = (0.044 / design_width)
                .min(0.032 / design_height)
                .max(f32::EPSILON);
            (design_width * scale, design_height * scale, false)
        }
        // A fixed-height score row remains readable from the home camera.
        // Long monospace names may condense horizontally inside their column,
        // but must not shrink the entire run into a few screen pixels.
        TextBinding::PlayerName(_) => {
            let height = 0.026;
            (
                (design_width / design_height * height).min(0.090),
                height,
                true,
            )
        }
        TextBinding::PlayerScore(_) => {
            let height = 0.026;
            (
                (design_width / design_height * height).min(0.032),
                height,
                false,
            )
        }
    };
    SlugTextLayout {
        width,
        height,
        anchor_offset_x: if left_anchored { width * 0.5 } else { 0.0 },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated finite positive Slug bounds are explicitly rasterized into bounded pixel extents"
)]
fn rasterize_slug_text(
    font: &SlugFont<'_>,
    text: &str,
    color: [u8; 3],
) -> Result<SlugRaster, SlugError> {
    const MAXIMUM_RASTER_WIDTH: f32 = 1_024.0;

    let metrics = font.metrics();
    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut cursor = 0.0_f32;
    let mut minimum_x = 0.0_f32;
    let mut maximum_x = 0.0_f32;
    let mut minimum_y = metrics.descender;
    let mut maximum_y = metrics.ascender;
    for character in text.chars() {
        let geometry = font.glyph_geometry(character)?;
        minimum_x = minimum_x.min(cursor + geometry.bounds.min_x);
        maximum_x = maximum_x.max(cursor + geometry.bounds.max_x);
        minimum_y = minimum_y.min(geometry.bounds.min_y);
        maximum_y = maximum_y.max(geometry.bounds.max_y);
        let bands = build_directional_bands(
            &geometry.curves,
            geometry.bounds,
            DEFAULT_BAND_SIZE_FONT_UNITS,
        )?;
        let advance = geometry.advance;
        glyphs.push(RasterGlyph {
            cursor,
            geometry,
            bands,
        });
        cursor += advance;
        maximum_x = maximum_x.max(cursor);
    }

    let content_width = (maximum_x - minimum_x).max(1.0);
    let content_height = (maximum_y - minimum_y).max(1.0);
    if !content_width.is_finite() || !content_height.is_finite() {
        return Err(SlugError::InvalidBounds);
    }
    let nominal_pixels_per_font_unit = SLUG_RASTER_PIXELS_PER_EM / f32::from(metrics.units_per_em);
    let available_width = MAXIMUM_RASTER_WIDTH - 2.0 * SLUG_RASTER_PADDING_PIXELS as f32;
    let pixels_per_font_unit = nominal_pixels_per_font_unit
        .min(available_width / content_width)
        .max(f32::EPSILON);
    let padding_units = SLUG_RASTER_PADDING_PIXELS as f32 / pixels_per_font_unit;
    let canvas_minimum_x = minimum_x - padding_units;
    let canvas_maximum_y = maximum_y + padding_units;
    let width = ((content_width * pixels_per_font_unit).ceil() as u32)
        .saturating_add(SLUG_RASTER_PADDING_PIXELS * 2)
        .max(1);
    let height = ((content_height * pixels_per_font_unit).ceil() as u32)
        .saturating_add(SLUG_RASTER_PADDING_PIXELS * 2)
        .max(1);
    let byte_count = usize::try_from(width)
        .expect("bounded raster width fits usize")
        .checked_mul(usize::try_from(height).expect("bounded raster height fits usize"))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(SlugError::CountOverflow("native Slug raster"))?;
    let mut bytes = vec![0_u8; byte_count];

    for pixel_y in 0..height {
        let sample_y = canvas_maximum_y - (pixel_y as f32 + 0.5) / pixels_per_font_unit;
        for pixel_x in 0..width {
            let sample_x = canvas_minimum_x + (pixel_x as f32 + 0.5) / pixels_per_font_unit;
            let mut coverage = 0.0_f32;
            for glyph in &glyphs {
                let local_x = sample_x - glyph.cursor;
                if local_x < glyph.geometry.bounds.min_x
                    || local_x > glyph.geometry.bounds.max_x
                    || sample_y < glyph.geometry.bounds.min_y
                    || sample_y > glyph.geometry.bounds.max_y
                {
                    continue;
                }
                coverage = coverage.max(coverage_banded(
                    &glyph.geometry.curves,
                    &glyph.bands,
                    Point::new(local_x, sample_y),
                    pixels_per_font_unit,
                )?);
            }
            let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha == 0 {
                continue;
            }
            let pixel_index = usize::try_from(pixel_y * width + pixel_x)
                .expect("bounded raster index fits usize")
                * 4;
            bytes[pixel_index..pixel_index + 3].copy_from_slice(&color);
            bytes[pixel_index + 3] = alpha;
        }
    }

    Ok(SlugRaster {
        image: Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        ),
        design_width: width as f32 / pixels_per_font_unit,
        design_height: height as f32 / pixels_per_font_unit,
    })
}

#[allow(clippy::too_many_lines)]
fn setup_native_scene(
    mut commands: Commands,
    controller: Res<NativeController>,
    surface: Res<NativeRenderSurface>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut camera = commands.spawn((
        Camera3d::default(),
        CameraView::home().transform(),
        TabletopCamera,
    ));
    if let Some(target) = surface.render_target() {
        camera.insert(target);
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-0.5, 1.8, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.85, 0.88, 1.0),
        brightness: 180.0,
        affects_lightmapped_meshes: true,
    });

    let table_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.055, 0.28, 0.16),
        perceptual_roughness: 0.82,
        ..default()
    });
    let seat_material = materials.add(Color::srgb(0.28, 0.18, 0.10));
    let player_material = materials.add(Color::srgb(0.18, 0.43, 0.75));
    let paper_material = materials.add(Color::srgb(0.92, 0.88, 0.72));
    let card_material = materials.add(Color::srgb(0.96, 0.94, 0.86));
    let card_back_material = materials.add(Color::srgb(0.14, 0.20, 0.54));
    let zone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.62, 0.76, 0.16),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let mut roots = HashMap::new();
    for object in &controller.scene.objects {
        let (mesh, material) = match object.id {
            ObjectId::Table => (
                meshes.add(Cuboid::new(
                    millimetres(object.half_extents.x * 2),
                    millimetres(object.half_extents.y * 2),
                    millimetres(object.half_extents.z * 2),
                )),
                table_material.clone(),
            ),
            ObjectId::Seat(_) => (meshes.add(Cylinder::new(0.16, 0.06)), seat_material.clone()),
            ObjectId::Player(_) => (meshes.add(Sphere::new(0.06)), player_material.clone()),
            ObjectId::ScoreSheet => (
                meshes.add(Cuboid::new(
                    millimetres(object.half_extents.x * 2),
                    millimetres(object.half_extents.y.max(1) * 2),
                    millimetres(object.half_extents.z * 2),
                )),
                paper_material.clone(),
            ),
            ObjectId::Zone(_) => (
                meshes.add(Cuboid::new(
                    millimetres(object.half_extents.x * 2),
                    0.004,
                    millimetres(object.half_extents.z * 2),
                )),
                zone_material.clone(),
            ),
            ObjectId::Card(_) => continue,
        };
        let id = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                pose_transform(object.pose),
                CanonicalMirror {
                    id: object.id,
                    pose: object.pose,
                    location: None,
                },
            ))
            .id();
        if object.id == ObjectId::Zone(ZoneId::Play) {
            commands.entity(id).observe(on_drag_drop);
        }
        roots.insert(object.id, id);
    }

    for card in &controller.scene.cards {
        let is_visible = card.face.is_some();
        let mut entity = commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                millimetres(card.half_extents.x * 2),
                millimetres(card.half_extents.y.max(1) * 2),
                millimetres(card.half_extents.z * 2),
            ))),
            MeshMaterial3d(if is_visible {
                card_material.clone()
            } else {
                card_back_material.clone()
            }),
            pose_transform(card.pose),
            CanonicalMirror {
                id: ObjectId::Card(card.id),
                pose: card.pose,
                location: Some(card.location),
            },
            DragPreview::default(),
        ));
        if is_visible
            && matches!(
                card.location,
                CardLocation::Hand { seat, .. } if seat == controller.issuing_seat
            )
        {
            entity
                .insert(DraggableCard(card.id))
                .observe(on_drag_card)
                .observe(on_drag_end);
        }
        let id = entity.id();
        roots.insert(ObjectId::Card(card.id), id);
    }

    let slug_font = SlugFont::parse(FONT_BYTES, 0, '?').expect("checked native font");
    for run in &controller.scene.text {
        let raster = rasterize_slug_text(&slug_font, &run.text, slug_text_color(run.binding))
            .expect("validated Slug text raster");
        let layout = slug_text_layout(run.binding, raster.design_width, raster.design_height);
        let texture = images.add(raster.image);
        let material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let mut translation = point_to_vec3(run.local_pose.translation);
        translation.x += layout.anchor_offset_x;
        let entity = commands
            .spawn((
                Mesh3d(meshes.add(Plane3d::default().mesh().size(layout.width, layout.height))),
                MeshMaterial3d(material),
                Transform::from_translation(translation)
                    .with_rotation(yaw_rotation(run.local_pose.yaw)),
                Pickable::IGNORE,
            ))
            .id();
        let parent = roots
            .get(&run.attached_to.object)
            .copied()
            .expect("validated text attachment parent");
        commands.entity(parent).add_child(entity);
    }
}

fn on_drag_card(
    drag: On<Pointer<Drag>>,
    mut previews: Query<&mut DragPreview>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    if let Ok(mut preview) = previews.get_mut(drag.entity) {
        preview.pixels += drag.delta;
    }
    if let Ok(window) = windows.single() {
        commands
            .entity(window)
            .insert(CursorIcon::System(SystemCursorIcon::Grabbing));
    }
}

fn on_drag_end(
    drag: On<Pointer<DragEnd>>,
    mut previews: Query<&mut DragPreview>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    if let Ok(mut preview) = previews.get_mut(drag.entity) {
        preview.pixels = Vec2::ZERO;
    }
    if let Ok(window) = windows.single() {
        commands
            .entity(window)
            .insert(CursorIcon::System(SystemCursorIcon::Default));
    }
}

fn on_drag_drop(
    mut event: On<Pointer<DragDrop>>,
    cards: Query<&DraggableCard>,
    mut controller: ResMut<NativeController>,
    live: Option<ResMut<NativeLiveDevice>>,
) {
    let Ok(card) = cards.get(event.dropped) else {
        return;
    };
    let result = zone_center_card_bounds(controller.layout(), ZoneId::Play)
        .and_then(|bounds| controller.commit_drag(card.0, bounds).map_err(|_| ()));
    match result {
        Ok(committed) => {
            if let Some(mut live) = live
                && let Err(error) = live.submit_play(&committed)
            {
                controller.last_finding = format!("live play queue rejected: {error}");
            }
        }
        Err(()) => {
            "drag release did not classify as a legal PLAY"
                .clone_into(&mut controller.last_finding);
        }
    }
    event.propagate(false);
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut controller: ResMut<NativeController>,
    mut debug: ResMut<DebugOverlay>,
    live: Option<ResMut<NativeLiveDevice>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.enabled = !debug.enabled;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        if let Some(face) = controller.first_owned_face() {
            match controller.commit_named(face) {
                Ok(committed) => {
                    if let Some(mut live) = live
                        && let Err(error) = live.submit_play(&committed)
                    {
                        controller.last_finding = format!("live play queue rejected: {error}");
                    }
                }
                Err(finding) => {
                    controller.last_finding = format!("keyboard play rejected: {finding}");
                }
            }
        } else {
            "keyboard play rejected: no visible owned card"
                .clone_into(&mut controller.last_finding);
        }
    }
}

fn poll_live_device(
    live: Option<ResMut<NativeLiveDevice>>,
    mut controller: ResMut<NativeController>,
) {
    let Some(mut live) = live else {
        return;
    };
    match live.poll() {
        Ok(true) => match native_controller_from_observation(live.observation()) {
            Ok(mut next) => {
                next.last_finding = format!(
                    "live device synchronized authority revision {}",
                    live.observation().projection.current_revision
                );
                *controller = next;
            }
            Err(error) => controller.last_finding = format!("live projection rejected: {error}"),
        },
        Ok(false) => {}
        Err(error) => controller.last_finding = format!("live device error: {error}"),
    }
}

fn camera_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut rig: ResMut<CameraRig>,
    mut camera: Single<&mut Transform, With<TabletopCamera>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        rig.begin_reset();
    } else {
        let mut movement = Vec2::ZERO;
        movement.y += f32::from(keys.pressed(KeyCode::KeyW));
        movement.y -= f32::from(keys.pressed(KeyCode::KeyS));
        movement.x += f32::from(keys.pressed(KeyCode::KeyD));
        movement.x -= f32::from(keys.pressed(KeyCode::KeyA));

        let mut rotation = Vec2::ZERO;
        rotation.x += f32::from(keys.pressed(KeyCode::ArrowRight));
        rotation.x -= f32::from(keys.pressed(KeyCode::ArrowLeft));
        rotation.y += f32::from(keys.pressed(KeyCode::ArrowUp));
        rotation.y -= f32::from(keys.pressed(KeyCode::ArrowDown));

        let mouse_pan = if mouse_buttons.pressed(MouseButton::Middle) {
            mouse_motion.delta
        } else {
            Vec2::ZERO
        };
        if movement != Vec2::ZERO || rotation != Vec2::ZERO || mouse_pan != Vec2::ZERO {
            rig.cancel_reset();
        }

        let view_forward = Vec3::new(-rig.view.yaw.sin(), 0.0, -rig.view.yaw.cos());
        let view_right = Vec3::new(rig.view.yaw.cos(), 0.0, -rig.view.yaw.sin());
        let movement =
            movement.normalize_or_zero() * CAMERA_MOVE_METRES_PER_SECOND * time.delta_secs();
        rig.view.target += view_right * movement.x + view_forward * movement.y;
        rig.view.target += (view_right * -mouse_pan.x + view_forward * mouse_pan.y)
            * CAMERA_MOUSE_PAN_METRES_PER_PIXEL;
        rig.view.yaw += rotation.x * CAMERA_ROTATE_RADIANS_PER_SECOND * time.delta_secs();
        rig.view.pitch = (rig.view.pitch
            + rotation.y * CAMERA_ROTATE_RADIANS_PER_SECOND * time.delta_secs())
        .clamp(CAMERA_MIN_PITCH, CAMERA_MAX_PITCH);
    }

    rig.advance_reset(time.delta_secs());
    **camera = rig.view.transform();
}

fn apply_mirrored_transforms(
    time: Res<Time>,
    controller: Res<NativeController>,
    mut clock: ResMut<TweenClock>,
    mut mirrors: Query<(&CanonicalMirror, &DragPreview, &mut Transform)>,
) {
    if let Some(committed) = controller.committed {
        if clock.generation == committed.generation {
            clock.elapsed_seconds += time.delta_secs();
        } else {
            clock.generation = committed.generation;
            clock.elapsed_seconds = 0.0;
        }
    }
    for (mirror, preview, mut transform) in &mut mirrors {
        let base_pose = controller
            .scene
            .objects
            .iter()
            .find(|object| object.id == mirror.id)
            .map(|object| object.pose)
            .or_else(|| {
                controller
                    .scene
                    .cards
                    .iter()
                    .find(|card| ObjectId::Card(card.id) == mirror.id)
                    .map(|card| card.pose)
            })
            .unwrap_or(mirror.pose);
        let mut pose = pose_transform(base_pose);
        if let Some(committed) = controller.committed
            && committed.endpoint.object == mirror.id
        {
            let duration = committed.endpoint.duration_milliseconds as f32 / 1_000.0;
            let linear = (clock.elapsed_seconds / duration).clamp(0.0, 1.0);
            let eased = linear * linear * (3.0 - 2.0 * linear);
            let from = point_to_vec3(committed.endpoint.from.translation);
            let to = point_to_vec3(committed.endpoint.to.translation);
            pose.translation = from.lerp(to, eased);
            pose.rotation = yaw_rotation(committed.endpoint.to.yaw);
        } else if preview.pixels != Vec2::ZERO {
            pose.translation += Vec3::new(preview.pixels.x, 30.0, -preview.pixels.y) * 0.000_55;
        }
        *transform = pose;
    }
}

fn draw_spatial_debug(
    mut gizmos: Gizmos,
    debug: Res<DebugOverlay>,
    controller: Res<NativeController>,
    camera: Res<CameraRig>,
) {
    if !debug.enabled {
        return;
    }
    for zone in controller.layout.zones() {
        draw_aabb(
            &mut gizmos,
            zone.outer,
            Color::srgba(0.95, 0.55, 0.12, 0.85),
        );
        draw_aabb(
            &mut gizmos,
            zone.inner,
            Color::srgba(0.12, 0.95, 0.62, 0.95),
        );
    }
    for seat in controller.layout.seats() {
        gizmos.cross(
            point_to_vec3(seat.seat_pose.translation),
            0.06,
            Color::WHITE,
        );
        gizmos.line(
            point_to_vec3(seat.seat_pose.translation),
            point_to_vec3(seat.player_pose.translation),
            Color::srgb(0.7, 0.8, 1.0),
        );
    }
    gizmos.cross(camera.view.target, 0.045, Color::srgb(1.0, 0.84, 0.2));
    gizmos.line(
        camera.view.target,
        camera.view.eye(),
        Color::srgba(1.0, 0.84, 0.2, 0.4),
    );
}

fn update_status(
    controller: Res<NativeController>,
    debug: Res<DebugOverlay>,
    diagnostics: Res<DiagnosticsStore>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
        .unwrap_or_default();
    let visible_faces = controller
        .scene
        .text
        .iter()
        .filter(|run| matches!(run.binding, TextBinding::CardFace(_)))
        .count();
    window.title = format!(
        "Poche | seat {} | {} cards | {} faces | {} | audit {} | {fps:.0} fps",
        controller.issuing_seat.get(),
        controller.scene.cards.len(),
        visible_faces,
        controller.last_finding,
        if debug.enabled { "on" } else { "off" },
    );
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated camera and window values are intentionally quantized into bounded integer evidence metadata"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects these independent renderer resources as system parameters"
)]
fn native_capture_driver(
    mut commands: Commands,
    provider: Option<Res<NativeCaptureProvider>>,
    context: Option<Res<NativeCaptureContext>>,
    controller: Res<NativeController>,
    camera: Res<CameraRig>,
    surface: Res<NativeRenderSurface>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut rendered_frames: Local<u32>,
) {
    let (Some(provider), Some(context)) = (provider, context) else {
        return;
    };
    // A wall-clock delay does not prove that a headless renderer has produced
    // a frame: pipeline compilation and GPU scheduling vary by backend. Count
    // actual application frames after startup while synchronous compilation
    // closes the shader-pipeline race.
    *rendered_frames = rendered_frames.saturating_add(1);
    if *rendered_frames < AUTOMATION_CAPTURE_PREROLL_FRAMES {
        return;
    }
    let Ok(Some(request)) = provider.take_queued() else {
        return;
    };
    if request.observed_revision != context.current_revision {
        let _ = provider.deny(&request.request_id, CaptureDenialReasonWire::StaleRevision);
        return;
    }
    let window = windows.single().ok();
    let Some((width_pixels, height_pixels, scale_milli)) = surface.extent(window) else {
        let _ = provider.deny(
            &request.request_id,
            CaptureDenialReasonWire::ProviderUnavailable,
        );
        return;
    };
    let viewport = poche_protocol::CaptureViewportWire {
        width_pixels,
        height_pixels,
    };
    if request
        .viewport
        .is_some_and(|requested| requested != viewport)
    {
        let _ = provider.deny(
            &request.request_id,
            CaptureDenialReasonWire::UnsupportedFormat,
        );
        return;
    }
    let Ok(scene_hash) = spatial_scene_hash(&controller.scene) else {
        let _ = provider.deny(
            &request.request_id,
            CaptureDenialReasonWire::IntegrityFailure,
        );
        return;
    };
    let eye = camera.view.eye();
    let camera_metadata = CaptureCameraMetadata {
        position_millimetres: [
            (eye.x * 1_000.0).round() as i64,
            (eye.y * 1_000.0).round() as i64,
            (eye.z * 1_000.0).round() as i64,
        ],
        rotation_milliradians: [
            (camera.view.pitch * 1_000.0).round() as i32,
            (camera.view.yaw * 1_000.0).round() as i32,
            0,
        ],
        vertical_fov_millidegrees: 45_000,
    };
    let surface_metadata = CaptureSurfaceMetadata {
        provider_kind: CaptureProviderKindWire::NativeBevy,
        viewport,
        framebuffer_width: viewport.width_pixels,
        framebuffer_height: viewport.height_pixels,
        scale_milli,
        camera: Some(camera_metadata),
    };
    let provider = provider.clone();
    let projection_hash = context.projection_hash;
    let Some(screenshot) = surface.screenshot() else {
        let _ = provider.deny(
            &request.request_id,
            CaptureDenialReasonWire::ProviderUnavailable,
        );
        return;
    };
    commands
        .spawn(screenshot)
        .observe(move |captured: On<ScreenshotCaptured>| {
            let request_id = request.request_id.clone();
            match native_capture_bundle(
                &request,
                projection_hash,
                SemanticHash(scene_hash),
                surface_metadata.clone(),
                &captured.image,
            ) {
                Ok(bundle) => {
                    let _ = provider.complete(&request_id, bundle);
                }
                Err(reason) => {
                    let _ = provider.deny(&request_id, reason);
                }
            }
        });
}

fn native_capture_bundle(
    request: &poche_protocol::CaptureRequestWire,
    projection_hash: poche_protocol::SemanticHash,
    scene_hash: poche_protocol::SemanticHash,
    surface: CaptureSurfaceMetadata,
    image: &Image,
) -> Result<RawCaptureBundle, CaptureDenialReasonWire> {
    let dynamic = image
        .clone()
        .try_into_dynamic()
        .map_err(|_| CaptureDenialReasonWire::ProviderUnavailable)?;
    let rgba = dynamic.to_rgba8();
    if !has_meaningful_render_content(&rgba) {
        return Err(CaptureDenialReasonWire::IntegrityFailure);
    }
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|_| CaptureDenialReasonWire::ProviderUnavailable)?;
    let bytes = cursor.into_inner();
    if u64::try_from(bytes.len()).map_or(true, |length| length > request.max_total_bytes) {
        return Err(CaptureDenialReasonWire::Oversize);
    }
    let token = capture_token(&request.request_id);
    let source_hash = poche_protocol::SemanticHash(*blake3::hash(&bytes).as_bytes());
    Ok(RawCaptureBundle {
        figure_id: format!("native-{token}"),
        caption: request.label.clone(),
        captured_revision: request.observed_revision,
        projection_hash,
        scene_hash: Some(scene_hash),
        surface,
        qualification: CaptureQualification::RuntimeGenerated,
        cancelled: false,
        artifacts: vec![RawCaptureArtifact {
            artifact_id: CaptureArtifactId::new(format!("native-{token}-png"))
                .map_err(|_| CaptureDenialReasonWire::ProviderUnavailable)?,
            representation: CaptureRepresentationWire::Png,
            media_type: "image/png".to_owned(),
            bytes,
            expected_source_hash: Some(source_hash),
        }],
    })
}

fn has_meaningful_render_content(image: &image::RgbaImage) -> bool {
    let Some(background) = image.pixels().next().copied() else {
        return false;
    };
    image
        .pixels()
        .filter(|pixel| {
            pixel.0[..3]
                .iter()
                .zip(&background.0[..3])
                .map(|(channel, base)| u16::from(channel.abs_diff(*base)))
                .sum::<u16>()
                > 18
        })
        .take(257)
        .count()
        > 256
}

fn capture_token(request_id: &poche_protocol::CaptureRequestId) -> String {
    let hash = blake3::hash(request_id.as_str().as_bytes());
    hash.as_bytes()[..8]
        .iter()
        .fold(String::with_capacity(16), |mut output, byte| {
            use std::fmt::Write;

            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects these independent acceptance resources as system parameters"
)]
fn acceptance_driver(
    mut commands: Commands,
    time: Res<Time>,
    options: Res<AcceptanceOptions>,
    controller: Res<NativeController>,
    debug: Res<DebugOverlay>,
    surface: Res<NativeRenderSurface>,
    provider: Option<Res<NativeCaptureProvider>>,
    mut clock: ResMut<LaunchClock>,
    mut exit: MessageWriter<AppExit>,
) {
    let elapsed = clock.started.elapsed();
    clock.first_update.get_or_insert(elapsed);
    if time.delta_secs_f64() > 0.0 {
        clock.frame_samples.push(time.delta_secs_f64() * 1_000.0);
    }
    let after_first_update = elapsed.saturating_sub(clock.first_update.unwrap_or(elapsed));
    if !clock.screenshot_requested
        && after_first_update >= Duration::from_millis(900)
        && let Some(path) = &options.screenshot
    {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(screenshot) = surface.screenshot() {
            commands
                .spawn(screenshot)
                .observe(save_to_disk(path.clone()));
            clock.screenshot_requested = true;
        }
    }
    let Some(exit_after) = options.exit_after else {
        return;
    };
    let should_exit = if let Some(provider) = provider {
        // GPU readback completion, rather than an arbitrary duration, is the
        // semantic wait for capture workers. Retain a bounded fail-safe so a
        // broken backend cannot leave automation alive indefinitely.
        provider.terminal_result_available().unwrap_or(false)
            || elapsed >= exit_after.saturating_add(Duration::from_secs(10))
    } else {
        elapsed >= exit_after
    };
    if !should_exit || clock.report_written {
        return;
    }
    if let Some(path) = &options.report {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut samples = clock.frame_samples.clone();
        samples.sort_by(f64::total_cmp);
        let mean = if samples.is_empty() {
            0.0
        } else {
            samples.iter().sum::<f64>() / samples.len() as f64
        };
        let p95 = samples
            .get(samples.len().saturating_sub(1) * 95 / 100)
            .copied()
            .unwrap_or_default();
        let face_runs = controller
            .scene
            .text
            .iter()
            .filter(|run| matches!(run.binding, TextBinding::CardFace(_)))
            .count();
        let hidden = controller
            .scene
            .cards
            .iter()
            .filter(|card| card.face.is_none())
            .count();
        let report = AcceptanceReport {
            scene_semantic_hash: spatial_scene_hash_hex(&controller.scene)
                .unwrap_or_else(|error| format!("invalid-scene-{error:?}")),
            startup_to_first_update_microseconds: clock
                .first_update
                .unwrap_or_default()
                .as_micros(),
            semantic_commit_microseconds: controller.last_commit_cost.map(|cost| cost.as_micros()),
            semantic_commit_nanoseconds: controller.last_commit_cost.map(|cost| cost.as_nanos()),
            sampled_frames: samples.len(),
            mean_frame_milliseconds: mean,
            p95_frame_milliseconds: p95,
            committed_face: controller.committed.map(|commit| commit.play.face.label()),
            scene_objects: controller.scene.objects.len(),
            scene_cards: controller.scene.cards.len(),
            semantic_text_runs: controller.scene.text.len(),
            visible_face_runs: face_runs,
            hidden_cards_without_face_runs: hidden,
            debug_overlay: debug.enabled,
            screenshot: options
                .screenshot
                .as_ref()
                .map(|path| path.display().to_string()),
            qualification: "real-window startup, semantic input-to-commit, and sampled frame presentation timing; not automated input-to-photon latency",
        };
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            let _ = std::fs::write(path, format!("{json}\n"));
        }
    }
    clock.report_written = true;
    exit.write(AppExit::Success);
}

fn pose_transform(pose: PoseMm) -> Transform {
    Transform::from_translation(point_to_vec3(pose.translation))
        .with_rotation(yaw_rotation(pose.yaw))
}

fn point_to_vec3(point: Point3Mm) -> Vec3 {
    Vec3::new(
        point.x.get() as f32 * METRES_PER_MILLIMETRE,
        point.y.get() as f32 * METRES_PER_MILLIMETRE,
        point.z.get() as f32 * METRES_PER_MILLIMETRE,
    )
}

fn yaw_rotation(yaw: YawMilliDegrees) -> Quat {
    Quat::from_rotation_y(-(yaw.get() as f32 / 1_000.0) * PI / 180.0)
}

fn millimetres(value: u32) -> f32 {
    value as f32 * METRES_PER_MILLIMETRE
}

fn draw_aabb(gizmos: &mut Gizmos, bounds: AabbMm, color: Color) {
    let min = point_to_vec3(bounds.min);
    let max = point_to_vec3(bounds.max);
    gizmos.cube(
        Transform::from_translation((min + max) * 0.5).with_scale(max - min),
        color,
    );
}

fn zone_center_card_bounds(layout: &SpatialLayout, id: ZoneId) -> Result<AabbMm, ()> {
    let zone = layout
        .zones()
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or(())?;
    let center = Point3Mm::new(
        i32::midpoint(zone.inner.min.x.get(), zone.inner.max.x.get()),
        i32::midpoint(zone.inner.min.y.get(), zone.inner.max.y.get()),
        i32::midpoint(zone.inner.min.z.get(), zone.inner.max.z.get()),
    );
    AabbMm::from_center(center, HalfExtentsMm::new(32, 1, 44)).ok_or(())
}

#[cfg(test)]
mod tests {
    use poche_player_client::{
        AdvertisedAction, DeviceObservation, DeviceProfile, LoopbackDeviceTransport,
        PlayerDeviceClient,
    };
    use poche_protocol::{
        CertificateId, ChanceWire, CommandId, CommandPayload, CorrelationId, CountdownToken,
        DeviceCapabilityWire, DeviceCustodyWire, DeviceId, EventId, GameActionWire,
        GamePublicStateWire, HandProjection, InviteProof, MemberProjection, PrincipalId,
        ProjectionEnvelope, ProjectionId, ProjectionPayload, PublicGamePhase, PublicTurnWire,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, RoomId, RoomPhase,
        SIGNATURE_DOMAIN_V1, SemanticHash, SignatureAlgorithm, SignatureBytes, SignatureIntent,
        SignatureMetadata, UnsignedDeviceCertificateWire,
    };
    use poche_runtime::{
        AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
        OracleGameActionSource, OracleSessionGame, RuntimeLoopbackDeviceAdapter, ScriptedClient,
    };
    use poche_session::{GameTurn, InviteRecord, SessionGame, SessionPhase, SessionState};
    use poche_slug::{
        DEFAULT_BAND_SIZE_FONT_UNITS, Point, SlugFont, build_directional_bands,
        coverage_all_curves, coverage_banded,
    };
    use poche_spatial::{CardFace, CardLocation, ObjectId, TextBinding, ZoneId};

    use super::{
        AUTOMATION_RENDER_HEIGHT, AUTOMATION_RENDER_WIDTH, CAMERA_RESET_SECONDS, CameraRig,
        CameraView, FONT_BYTES, NativeController, NativeLiveDevice, NativeRenderMode,
        NativeRenderSurface, NativeUiLaunchOptions, advertised_action_for_play,
        has_meaningful_render_content, native_controller_from_observation, parse_card_face,
        rasterize_slug_text, replay_fixture_controller, slug_packet_for_text, slug_text_color,
        slug_text_layout, zone_center_card_bounds,
    };

    #[test]
    fn render_mode_makes_windowless_automation_explicit() {
        assert_eq!(
            NativeUiLaunchOptions::default().render_mode,
            NativeRenderMode::InteractiveWindow
        );
        assert!(matches!(
            NativeRenderSurface::from_mode(NativeRenderMode::WindowlessImage),
            NativeRenderSurface::Windowless {
                width: AUTOMATION_RENDER_WIDTH,
                height: AUTOMATION_RENDER_HEIGHT,
                target: None,
            }
        ));
        assert!(matches!(
            NativeRenderSurface::from_mode(NativeRenderMode::InteractiveWindow),
            NativeRenderSurface::Windowed
        ));
    }

    #[test]
    fn slug_surface_is_filled_antialiased_and_fitted_to_its_binding() {
        let font = SlugFont::parse(FONT_BYTES, 0, '?').expect("checked font");
        let controller = replay_fixture_controller().expect("fixture controller");
        let card_binding = controller
            .scene
            .text
            .iter()
            .find_map(|run| matches!(run.binding, TextBinding::CardFace(_)).then_some(run.binding))
            .expect("visible card text");
        let raster = rasterize_slug_text(&font, "8♣", slug_text_color(card_binding))
            .expect("filled Slug raster");
        let bytes = raster.image.data.as_ref().expect("resident raster bytes");
        let alphas = bytes.chunks_exact(4).map(|pixel| pixel[3]);
        let (transparent, antialiased, opaque) = alphas.fold(
            (0_usize, 0_usize, 0_usize),
            |(transparent, antialiased, opaque), alpha| match alpha {
                0 => (transparent + 1, antialiased, opaque),
                255 => (transparent, antialiased, opaque + 1),
                _ => (transparent, antialiased + 1, opaque),
            },
        );
        assert!(transparent > 0, "the run needs a transparent background");
        assert!(antialiased > 0, "edge pixels need analytic coverage");
        assert!(opaque > 0, "glyph interiors must be filled, not outlined");

        let card_layout = slug_text_layout(card_binding, raster.design_width, raster.design_height);
        assert!(card_layout.width <= 0.044);
        assert!(card_layout.height <= 0.032);
        assert!(card_layout.anchor_offset_x.abs() < f32::EPSILON);

        let name_binding = controller
            .scene
            .text
            .iter()
            .find_map(|run| {
                matches!(run.binding, TextBinding::PlayerName(_)).then_some(run.binding)
            })
            .expect("player name text");
        let name_raster = rasterize_slug_text(
            &font,
            "A deliberately long player name",
            slug_text_color(name_binding),
        )
        .expect("bounded name raster");
        let name_layout = slug_text_layout(
            name_binding,
            name_raster.design_width,
            name_raster.design_height,
        );
        assert!(name_layout.width <= 0.090);
        assert!((name_layout.height - 0.026).abs() < f32::EPSILON);
        assert!((name_layout.anchor_offset_x - name_layout.width * 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn graphical_evidence_rejects_uniform_readbacks() {
        let blank = image::RgbaImage::from_pixel(64, 64, image::Rgba([18, 24, 30, 255]));
        assert!(!has_meaningful_render_content(&blank));

        let mut rendered = blank;
        for y in 8..32 {
            for x in 8..32 {
                rendered.put_pixel(x, y, image::Rgba([220, 180, 80, 255]));
            }
        }
        assert!(has_meaningful_render_content(&rendered));
    }

    fn live_play_observation() -> DeviceObservation {
        let alice = PrincipalId::new("alice").expect("alice");
        let bob = PrincipalId::new("bob").expect("bob");
        DeviceObservation {
            projection: ProjectionEnvelope {
                protocol_version: 1,
                room_id: RoomId::new("native-live-room").expect("room"),
                session_epoch: 1,
                projection_id: ProjectionId::new("native-projection-9").expect("projection"),
                principal_id: alice.clone(),
                current_revision: 9,
                projection_epoch: 3,
                correlation_id: CorrelationId::new("native-correlation-9").expect("correlation"),
                causation_id: EventId::new("native-event-9").expect("event"),
                payload: ProjectionPayload {
                    phase: RoomPhase::Running,
                    members: vec![
                        MemberProjection {
                            principal_id: alice.clone(),
                            connected: true,
                            seat: Some(0),
                            ready: false,
                            host: true,
                        },
                        MemberProjection {
                            principal_id: bob,
                            connected: true,
                            seat: Some(1),
                            ready: false,
                            host: false,
                        },
                    ],
                    public_game_state: Some(GamePublicStateWire {
                        schema_version: 1,
                        phase: PublicGamePhase::Playing,
                        dealer: Some(1),
                        actor: PublicTurnWire::Player(0),
                        round_index: 0,
                        hand_size: 2,
                        hand_counts: vec![2, 2],
                        trump: Some(51),
                        current_trick: Vec::new(),
                        bids: vec![Some(1), Some(1)],
                        tricks_won: vec![0, 0],
                        scores: vec![0, 0],
                        pot_cents: 50,
                    }),
                    own_hand: Some(HandProjection {
                        player: alice.clone(),
                        grant_epoch: 3,
                        cards: vec![0, 12],
                    }),
                    granted_hands: Vec::new(),
                    public_history: Vec::new(),
                },
                signature: SignatureMetadata {
                    domain_version: SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: alice,
                    signature: SignatureBytes::new("00".repeat(64)).expect("signature"),
                },
            },
            projection_hash: SemanticHash([9; 32]),
            actions: vec![AdvertisedAction {
                id: "play-2-clubs".to_owned(),
                label: "Play 2♣".to_owned(),
                payload: CommandPayload::GameAction {
                    action: GameActionWire::Play { card: 0 },
                },
            }],
            action_templates: Vec::new(),
            chat_tail: Vec::new(),
            capture_providers: Vec::new(),
        }
    }

    fn certified_profile(player: &PrincipalId, label: &str, device_byte: &str) -> DeviceProfile {
        let device_key = device_byte.repeat(32);
        let device_id = DeviceId::new(device_key.clone()).expect("device ID");
        let certificate = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("certificate-{label}"))
                .expect("certificate ID"),
            player_id: player.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_key,
            device_encryption_public_key: "ee".repeat(32),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player.clone(),
            },
        }
        .attach_signature(SignatureBytes::new("00".repeat(64)).expect("signature"))
        .expect("certificate");
        DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: label.to_owned(),
            player_id: player.clone(),
            device_id,
            certificate,
            signing_key_handle: format!("test-key-store:{label}"),
        }
    }

    fn submit_fixture_command(
        authority: &mut InProcessAuthority<OracleSessionGame<2>>,
        client: &ScriptedClient,
        command_id: &str,
        payload: CommandPayload,
    ) {
        let command = client
            .command(&authority.state, command_id, payload)
            .expect("fixture command");
        client
            .submit(&mut authority.transport, command)
            .expect("fixture transport");
        let outcome = authority.drive_all().expect("fixture authority");
        assert_eq!(outcome.len(), 1);
        assert_eq!(outcome[0].disposition, AuthorityDisposition::Applied);
    }

    fn running_play_state() -> (SessionState<OracleSessionGame<2>>, PrincipalId) {
        let alice = PrincipalId::new("11".repeat(32)).expect("alice");
        let bob = PrincipalId::new("22".repeat(32)).expect("bob");
        let room_id = RoomId::new("native-device-room").expect("room");
        let clock = PrincipalId::new("authority-clock").expect("clock");
        let environment = PrincipalId::new("game-environment").expect("environment");
        let mut state = SessionState::pending(room_id, clock, environment.clone());
        state
            .invites
            .push(InviteRecord::new("bob-fixture-invite", u64::MAX).expect("invite"));
        let mut transport = InProcessTransport::new(LoopbackCodec::Typed);
        let alice_client = transport.connect(alice.clone()).expect("Alice connection");
        let bob_client = transport.connect(bob.clone()).expect("Bob connection");
        let environment_client = transport
            .connect(environment)
            .expect("environment connection");
        let mut authority = InProcessAuthority::new(state, transport);
        submit_fixture_command(
            &mut authority,
            &alice_client,
            "create-native-room",
            CommandPayload::CreateRoom,
        );
        submit_fixture_command(
            &mut authority,
            &bob_client,
            "join-bob",
            CommandPayload::RedeemInvite {
                invite: InviteProof::new("bob-fixture-invite").expect("invite proof"),
            },
        );
        submit_fixture_command(
            &mut authority,
            &alice_client,
            "seat-alice",
            CommandPayload::TakeSeat { seat: 0 },
        );
        submit_fixture_command(
            &mut authority,
            &bob_client,
            "seat-bob",
            CommandPayload::TakeSeat { seat: 1 },
        );
        submit_fixture_command(
            &mut authority,
            &alice_client,
            "ready-alice",
            CommandPayload::Ready,
        );
        submit_fixture_command(
            &mut authority,
            &bob_client,
            "ready-bob",
            CommandPayload::Ready,
        );
        submit_fixture_command(
            &mut authority,
            &alice_client,
            "arm-native-countdown",
            CommandPayload::ArmCountdown {
                deadline_tick: 1,
                countdown_token: CountdownToken::new("native-start-token").expect("token"),
            },
        );
        assert_eq!(
            authority
                .advance_clock_to(1)
                .expect("countdown expiry")
                .len(),
            1
        );
        submit_fixture_command(
            &mut authority,
            &environment_client,
            "native-deal",
            CommandPayload::ApplyChance {
                chance: ChanceWire {
                    cards: (0_u8..52).collect(),
                    seed: None,
                    deal_ordinal: None,
                },
            },
        );
        let actor_principal =
            finish_fixture_bidding(&mut authority, &alice_client, &bob_client, alice, bob);
        (authority.state, actor_principal)
    }

    fn finish_fixture_bidding(
        authority: &mut InProcessAuthority<OracleSessionGame<2>>,
        alice_client: &ScriptedClient,
        bob_client: &ScriptedClient,
        alice: PrincipalId,
        bob: PrincipalId,
    ) -> PrincipalId {
        let mut bid_sequence = 0_u8;
        loop {
            let SessionPhase::Running { game } = &authority.state.phase else {
                panic!("fixture must be running");
            };
            if game.public_projection().expect("public projection").phase
                != PublicGamePhase::Bidding
            {
                break;
            }
            let GameTurn::Player(seat) = game.turn() else {
                panic!("bidding must belong to a player");
            };
            let action = game
                .legal_player_actions()
                .into_iter()
                .next()
                .expect("legal bid");
            let client = if seat == 0 { alice_client } else { bob_client };
            submit_fixture_command(
                authority,
                client,
                &format!("native-bid-{bid_sequence}"),
                CommandPayload::GameAction { action },
            );
            bid_sequence = bid_sequence.saturating_add(1);
        }
        let SessionPhase::Running { game } = &authority.state.phase else {
            panic!("fixture must remain running");
        };
        let GameTurn::Player(actor) = game.turn() else {
            panic!("playing must belong to a player");
        };
        if actor == 0 { alice } else { bob }
    }

    #[test]
    fn camera_reset_tweens_back_to_the_registered_home_view() {
        let mut rig = CameraRig::default();
        let home = rig.view;
        rig.view.target += bevy::prelude::Vec3::new(0.4, 0.0, -0.3);
        rig.view.yaw += 1.1;
        rig.view.pitch = 0.3;
        rig.begin_reset();
        rig.advance_reset(CAMERA_RESET_SECONDS / 2.0);
        assert_ne!(rig.view, home);
        assert!(rig.reset.is_some());
        rig.advance_reset(CAMERA_RESET_SECONDS / 2.0);
        assert_eq!(rig.view, CameraView::home());
        assert!(rig.reset.is_none());
    }

    #[test]
    fn card_parser_accepts_dense_unicode_and_cli_spellings() {
        assert_eq!(parse_card_face("51").map(CardFace::code), Some(51));
        assert_eq!(parse_card_face("A♠").map(CardFace::code), Some(51));
        assert_eq!(parse_card_face("jack-spades").map(CardFace::code), Some(48));
        assert_eq!(
            parse_card_face("queen_of_hearts").map(CardFace::code),
            Some(36)
        );
        assert!(parse_card_face("joker").is_none());
    }

    #[test]
    fn selected_font_has_real_suit_outlines_and_buffer_contract() {
        let font = SlugFont::parse(FONT_BYTES, 0, '?').expect("checked font");
        for character in ['♣', '♦', '♥', '♠'] {
            let glyph = font.glyph_geometry(character).expect("suit outline");
            assert!(!glyph.used_fallback, "{character} must not use fallback");
            assert!(!glyph.curves.is_empty());
        }
        let packet = slug_packet_for_text("A♠ Alice 100").expect("packet");
        assert_eq!(packet.instances.len(), 12);
        assert_eq!(packet.metadata_words.len(), packet.instances.len());
        assert_eq!(packet.curve_vec4s.len() % 2, 0);
        assert!(!packet.band_words.is_empty());
    }

    #[test]
    fn native_packet_keeps_cpu_and_banded_coverage_in_parity() {
        let font = SlugFont::parse(FONT_BYTES, 0, '?').expect("checked font");
        let glyph = font.glyph_geometry('♠').expect("spade");
        let bands =
            build_directional_bands(&glyph.curves, glyph.bounds, DEFAULT_BAND_SIZE_FONT_UNITS)
                .expect("bands");
        for x_step in 0..=4 {
            for y_step in 0..=4 {
                let x = glyph.bounds.min_x
                    + (glyph.bounds.max_x - glyph.bounds.min_x) * x_step as f32 / 4.0;
                let y = glyph.bounds.min_y
                    + (glyph.bounds.max_y - glyph.bounds.min_y) * y_step as f32 / 4.0;
                let point = Point::new(x, y);
                let oracle =
                    coverage_all_curves(&glyph.curves, glyph.bounds, point, 0.05).expect("oracle");
                let banded = coverage_banded(&glyph.curves, &bands, point, 0.05).expect("banded");
                assert!((oracle - banded).abs() <= 0.000_1);
            }
        }
    }

    #[test]
    fn typed_and_drag_input_commit_identical_transition_and_tween() {
        let controller = replay_fixture_controller().expect("fixture");
        let face = controller.first_owned_face().expect("owned face");
        let object = controller
            .scene()
            .cards
            .iter()
            .find(|card| {
                card.face == Some(face)
                    && matches!(
                        card.location,
                        CardLocation::Hand { seat, .. } if seat == controller.issuing_seat()
                    )
            })
            .expect("owned object")
            .id;
        let bounds =
            zone_center_card_bounds(controller.layout(), ZoneId::Play).expect("play bounds");
        let mut named = NativeController::try_new(
            controller.layout().clone(),
            controller.scene().clone(),
            controller.issuing_seat(),
        )
        .expect("named");
        let mut dragged = NativeController::try_new(
            controller.layout().clone(),
            controller.scene().clone(),
            controller.issuing_seat(),
        )
        .expect("dragged");
        let named = named.commit_named(face).expect("named play");
        let dragged = dragged.commit_drag(object, bounds).expect("drag play");
        assert_eq!(named.play, dragged.play);
        assert_eq!(named.endpoint, dragged.endpoint);
    }

    #[test]
    fn live_click_and_drag_resolve_only_to_the_exact_advertised_action() {
        let observation = live_play_observation();
        let mut controller =
            native_controller_from_observation(&observation).expect("live native controller");
        let legal = controller
            .scene()
            .cards
            .iter()
            .find(|card| card.face == CardFace::new(0))
            .expect("legal card")
            .id;
        let bounds =
            zone_center_card_bounds(controller.layout(), ZoneId::Play).expect("play bounds");
        let committed = controller.commit_drag(legal, bounds).expect("drag play");
        assert_eq!(
            advertised_action_for_play(&observation, &committed).map(|action| action.id.as_str()),
            Some("play-2-clubs")
        );

        let mut controller =
            native_controller_from_observation(&observation).expect("fresh live controller");
        assert_eq!(
            controller.commit_named(CardFace::new(12).expect("ace clubs")),
            Err("A♣ was not advertised for this exact projection".to_owned())
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the live-device acceptance keeps spatial input, sibling observation, and peer-driven refresh in one shared-room scenario"
    )]
    fn native_spatial_action_commits_through_device_client_and_reaches_sibling() {
        let (state, actor) = running_play_state();
        let room_id = state.room_id.clone();
        let other_player = state
            .members
            .iter()
            .find(|member| member.principal_id != actor)
            .expect("other player")
            .principal_id
            .clone();
        {
            let SessionPhase::Running { game } = &state.phase else {
                panic!("running state");
            };
            let direct_action = game
                .legal_player_actions()
                .into_iter()
                .next()
                .expect("direct legal play");
            let mut direct_transport = InProcessTransport::new(LoopbackCodec::Typed);
            let direct_client = direct_transport
                .connect(actor.clone())
                .expect("direct connection");
            let _other_client = direct_transport
                .connect(other_player.clone())
                .expect("other player connection");
            let mut direct_authority = InProcessAuthority::new(state.clone(), direct_transport);
            submit_fixture_command(
                &mut direct_authority,
                &direct_client,
                "direct-precondition-play",
                CommandPayload::GameAction {
                    action: direct_action,
                },
            );
        }
        let native_profile = certified_profile(&actor, "native-renderer", "33");
        let sibling_profile = certified_profile(&actor, "sibling-cli", "44");
        let other_profile = certified_profile(&other_player, "other-player", "55");
        let adapter = RuntimeLoopbackDeviceAdapter::new(
            state,
            OracleGameActionSource::default(),
            LoopbackCodec::Typed,
        );
        adapter.enroll(&native_profile).expect("enroll native");
        adapter.enroll(&sibling_profile).expect("enroll sibling");
        adapter.enroll(&other_profile).expect("enroll other player");
        let native = PlayerDeviceClient::new(
            native_profile,
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .expect("native client");
        let mut sibling = PlayerDeviceClient::new(
            sibling_profile,
            LoopbackDeviceTransport::new(adapter.clone()),
        )
        .expect("sibling client");
        let mut other =
            PlayerDeviceClient::new(other_profile, LoopbackDeviceTransport::new(adapter))
                .expect("other player client");

        let mut live = NativeLiveDevice::connect(native, room_id.clone()).expect("live device");
        let observation = live.observation().clone();
        let (action_id, face) = observation
            .actions
            .iter()
            .find_map(|action| match action.payload {
                CommandPayload::GameAction {
                    action: GameActionWire::Play { card },
                } => Some((action.id.clone(), CardFace::new(card).expect("card face"))),
                _ => None,
            })
            .expect("advertised play");
        let mut controller =
            native_controller_from_observation(&observation).expect("native controller");
        let committed = controller
            .commit_named(face)
            .expect("native spatial action");
        assert_eq!(
            advertised_action_for_play(&observation, &committed).map(|action| action.id.as_str()),
            Some(action_id.as_str())
        );
        let starting_revision = observation.projection.current_revision;
        let starting_history = observation.projection.payload.public_history.len();
        live.submit_play(&committed).expect("queue device action");
        let mut synchronized = false;
        for _ in 0..10_000 {
            synchronized = live.poll().expect("poll live device");
            if synchronized {
                break;
            }
            std::thread::yield_now();
        }
        assert!(
            synchronized,
            "native device worker must make bounded progress"
        );
        let observed_elsewhere = sibling.observe(&room_id).expect("sibling observation");
        assert_eq!(
            observed_elsewhere.projection.current_revision,
            starting_revision + 1
        );
        assert_eq!(
            observed_elsewhere.projection.payload.public_history.len(),
            starting_history + 1,
            "the sibling must observe the ordinary committed game history"
        );

        let other_observation = other.observe(&room_id).expect("other player turn");
        let other_action = other_observation
            .actions
            .iter()
            .find(|action| matches!(action.payload, CommandPayload::GameAction { .. }))
            .expect("other player advertised action")
            .id
            .clone();
        other
            .invoke(
                &other_observation,
                &other_action,
                CommandId::new("other-player-live-action").expect("other action ID"),
            )
            .expect("other player action");
        let mut observed_peer_progress = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if live.poll().expect("poll peer progress")
                && live.observation().projection.current_revision == starting_revision + 2
            {
                observed_peer_progress = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            observed_peer_progress,
            "native live device must refresh when a peer commits an action"
        );
    }

    #[test]
    fn bevy_mirror_does_not_invent_hidden_text_or_mutate_scene() {
        let mut controller = replay_fixture_controller().expect("fixture");
        let before = controller.scene().clone();
        let face = controller.first_owned_face().expect("owned face");
        controller.commit_named(face).expect("commit");
        assert_eq!(controller.scene(), &before);
        let face_text_objects = before
            .text
            .iter()
            .filter_map(|run| match run.binding {
                TextBinding::CardFace(card) => Some(ObjectId::Card(card)),
                TextBinding::PlayerName(_) | TextBinding::PlayerScore(_) => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert!(before.cards.iter().all(|card| {
            card.face.is_some() == face_text_objects.contains(&ObjectId::Card(card.id))
        }));
        assert_eq!(
            poche_spatial::spatial_scene_hash_hex(&before).expect("canonical scene hash"),
            "d1216416d9fe0fdad1412512a2b5cf273b883843276ec77113931b6e1b4e0576"
        );
    }
}
