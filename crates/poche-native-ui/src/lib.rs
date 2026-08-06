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
    collections::HashMap,
    f32::consts::PI,
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorIcon, PrimaryWindow, SystemCursorIcon, WindowPlugin},
};
use poche_slug::{
    DEFAULT_BAND_SIZE_FONT_UNITS, GlyphGeometry, SlugError, SlugFont, build_directional_bands,
    build_gpu_glyph_metadata,
};
use poche_spatial::{
    AabbMm, AnimationEndpoint, CardFace, CardLocation, CardObjectId, HalfExtentsMm, ObjectId,
    Point3Mm, PoseMm, ResolvedCardPlay, SeatId, SpatialLayout, SpatialScene, TextBinding,
    YawMilliDegrees, ZoneId, reconstruct_animation_endpoint, resolve_card_play, resolve_drag_play,
    spatial_scene_hash_hex,
};
use poche_ui::embedded_spatial_fixture;
use serde::Serialize;

/// Explicit OFL-licensed font consumed by Slug and native UI text.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/CaskaydiaCove-Regular.ttf");

const METRES_PER_MILLIMETRE: f32 = 0.001;
const SLUG_FONT_UNIT_METRES: f32 = 0.000_028;
const SLUG_CURVE_STEPS: u32 = 7;

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

#[derive(Component)]
struct SlugRun {
    binding: TextBinding,
    glyphs: Vec<GlyphGeometry>,
    packet: SlugGpuPacket,
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

/// Run the real Bevy window with arguments from the current process.
///
/// # Errors
///
/// Returns argument or fixture failures before the event loop starts.
pub fn run_from_env() -> Result<(), String> {
    let mut controller = replay_fixture_controller()?;
    let mut acceptance = AcceptanceOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--play-card" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--play-card requires a face".to_owned())?;
                let face = if value == "first" {
                    controller
                        .first_owned_face()
                        .ok_or_else(|| "exact-recipient fixture has no owned face".to_owned())?
                } else {
                    parse_card_face(&value)
                        .ok_or_else(|| format!("unrecognized card face {value:?}"))?
                };
                controller.commit_named(face)?;
            }
            "--debug-overlay" => acceptance.debug_overlay = true,
            "--screenshot" => {
                acceptance.screenshot =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        "--screenshot requires an output path".to_owned()
                    })?));
            }
            "--acceptance-report" => {
                acceptance.report =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        "--acceptance-report requires an output path".to_owned()
                    })?));
            }
            "--exit-after-seconds" => {
                let seconds = args
                    .next()
                    .ok_or_else(|| "--exit-after-seconds requires a number".to_owned())?
                    .parse::<f64>()
                    .map_err(|error| format!("invalid exit duration: {error}"))?;
                if !seconds.is_finite() || seconds < 1.0 {
                    return Err("exit duration must be finite and at least one second".to_owned());
                }
                acceptance.exit_after = Some(Duration::from_secs_f64(seconds));
            }
            "--help" | "-h" => {
                println!(
                    "poche-native-ui [--play-card FACE|first] [--debug-overlay] [--screenshot PATH] \
                     [--acceptance-report PATH] [--exit-after-seconds N]\n\
                     Keyboard: P play first owned card; F3 toggle spatial audit overlay"
                );
                return Ok(());
            }
            unknown => return Err(format!("unknown native UI option {unknown:?}")),
        }
    }

    let debug_overlay = DebugOverlay {
        enabled: acceptance.debug_overlay,
    };
    App::new()
        .insert_resource(controller)
        .insert_resource(debug_overlay)
        .insert_resource(acceptance)
        .init_resource::<TweenClock>()
        .init_resource::<LaunchClock>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Poche — canonical spatial mirror".to_owned(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((MeshPickingPlugin, FrameTimeDiagnosticsPlugin::default()))
        .add_systems(Startup, setup_native_scene)
        .add_systems(
            Update,
            (
                keyboard_input,
                apply_mirrored_transforms,
                draw_slug_text,
                draw_spatial_debug,
                update_status,
                acceptance_driver,
            )
                .chain(),
        )
        .run();
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn setup_native_scene(
    mut commands: Commands,
    controller: Res<NativeController>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.55, 1.65).looking_at(Vec3::new(0.0, 0.08, 0.0), Vec3::Y),
    ));
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
        let glyphs = run
            .text
            .chars()
            .map(|character| slug_font.glyph_geometry(character))
            .collect::<Result<Vec<_>, _>>()
            .expect("validated Slug text");
        let packet = slug_packet_for_text(&run.text).expect("validated Slug packet");
        let entity = commands
            .spawn((
                Transform::from_translation(point_to_vec3(run.local_pose.translation))
                    .with_rotation(yaw_rotation(run.local_pose.yaw)),
                SlugRun {
                    binding: run.binding,
                    glyphs,
                    packet,
                },
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
) {
    let Ok(card) = cards.get(event.dropped) else {
        return;
    };
    let result = zone_center_card_bounds(controller.layout(), ZoneId::Play)
        .and_then(|bounds| controller.commit_drag(card.0, bounds).map_err(|_| ()));
    if result.is_err() {
        "drag release did not classify as a legal PLAY".clone_into(&mut controller.last_finding);
    }
    event.propagate(false);
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut controller: ResMut<NativeController>,
    mut debug: ResMut<DebugOverlay>,
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.enabled = !debug.enabled;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        if let Some(face) = controller.first_owned_face() {
            if let Err(finding) = controller.commit_named(face) {
                controller.last_finding = format!("keyboard play rejected: {finding}");
            }
        } else {
            "keyboard play rejected: no visible owned card"
                .clone_into(&mut controller.last_finding);
        }
    }
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
        let mut pose = pose_transform(mirror.pose);
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

fn draw_slug_text(mut gizmos: Gizmos, runs: Query<(&SlugRun, &GlobalTransform)>) {
    for (run, transform) in &runs {
        let mut cursor = 0.0_f32;
        let color = match run.binding {
            TextBinding::CardFace(_) => Color::srgb(0.06, 0.05, 0.04),
            TextBinding::PlayerName(_) => Color::srgb(0.08, 0.10, 0.17),
            TextBinding::PlayerScore(_) => Color::srgb(0.55, 0.05, 0.04),
        };
        debug_assert_eq!(run.glyphs.len(), run.packet.instances.len());
        for geometry in &run.glyphs {
            for curve in &geometry.curves {
                let mut previous = slug_point(transform, cursor, curve.p0.x, curve.p0.y);
                for step in 1..=SLUG_CURVE_STEPS {
                    let t = step as f32 / SLUG_CURVE_STEPS as f32;
                    let inverse = 1.0 - t;
                    let x = inverse * inverse * curve.p0.x
                        + 2.0 * inverse * t * curve.p1.x
                        + t * t * curve.p2.x;
                    let y = inverse * inverse * curve.p0.y
                        + 2.0 * inverse * t * curve.p1.y
                        + t * t * curve.p2.y;
                    let point = slug_point(transform, cursor, x, y);
                    gizmos.line(previous, point, color);
                    previous = point;
                }
            }
            cursor += geometry.advance * SLUG_FONT_UNIT_METRES;
        }
    }
}

fn draw_spatial_debug(
    mut gizmos: Gizmos,
    debug: Res<DebugOverlay>,
    controller: Res<NativeController>,
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
}

fn update_status(
    controller: Res<NativeController>,
    debug: Res<DebugOverlay>,
    diagnostics: Res<DiagnosticsStore>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
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

fn acceptance_driver(
    mut commands: Commands,
    time: Res<Time>,
    options: Res<AcceptanceOptions>,
    controller: Res<NativeController>,
    debug: Res<DebugOverlay>,
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
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        clock.screenshot_requested = true;
    }
    let Some(exit_after) = options.exit_after else {
        return;
    };
    if elapsed < exit_after || clock.report_written {
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

fn slug_point(transform: &GlobalTransform, cursor: f32, x: f32, y: f32) -> Vec3 {
    transform.transform_point(Vec3::new(
        cursor + x * SLUG_FONT_UNIT_METRES,
        0.001,
        -y * SLUG_FONT_UNIT_METRES,
    ))
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
    use poche_slug::{
        DEFAULT_BAND_SIZE_FONT_UNITS, Point, SlugFont, build_directional_bands,
        coverage_all_curves, coverage_banded,
    };
    use poche_spatial::{CardFace, CardLocation, ObjectId, TextBinding, ZoneId};

    use super::{
        FONT_BYTES, NativeController, parse_card_face, replay_fixture_controller,
        slug_packet_for_text, zone_center_card_bounds,
    };

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
            "0a6de46fd21791260bb57c8516ce9ef5e1666e03e17d29cf6f8cda746c07baee"
        );
    }
}
