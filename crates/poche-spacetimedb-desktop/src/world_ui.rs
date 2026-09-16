// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Readable world surfaces over the public projection. None of these surfaces
//! owns game state: a chair click and a spoken bid use the ordinary UI intents.

use super::{
    CanonicalLayout, HandCamera, PoseDisplay, TabletopCamera, UiAction, UiScreen, UiState,
    card_label_texture, drag_cards, hand_view, may_bid, mm_position, point_to_world,
    sync_player_entities, update_table_camera,
};
use bevy::{
    camera::visibility::RenderLayers,
    clipboard::Clipboard,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use poche_bevy_spacetimedb::{BridgeHandle, BridgeIntent, BridgeModel};
use poche_spacetimedb_client::{ClientSnapshot, RoomCapability};
use poche_spatial::{AabbMm, SpatialLayout, ZoneId};

pub(super) struct WorldUiPlugin;

impl Plugin for WorldUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldInteraction>()
            .init_resource::<PresentationCache>()
            .add_message::<WorldUiCommand>()
            .add_observer(activate_world_ui)
            .add_systems(
                Update,
                (
                    interact_with_world,
                    sync_world_presentation,
                    face_world_labels_towards_camera,
                    scroll_reader,
                )
                    .chain()
                    .after(update_table_camera)
                    .after(sync_player_entities)
                    .before(drag_cards),
            );
    }
}

#[derive(Resource, Default)]
pub(super) struct WorldInteraction {
    pub(super) pointer_over_ui: bool,
    speech_open: bool,
    reading: Option<PanelKind>,
    hovered: Option<String>,
}

impl WorldInteraction {
    pub(super) fn blocks_card_input(&self) -> bool {
        self.reading.is_some() || self.pointer_over_ui
    }

    pub(super) fn modal_open(&self) -> bool {
        self.reading.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PanelKind {
    RoomCode,
    Players,
    Activity,
    ScoreSheet,
}

impl PanelKind {
    fn title(self) -> &'static str {
        match self {
            Self::RoomCode => "ROOM CODE · click to copy",
            Self::Players => "PLAYERS",
            Self::Activity => "ACTIVITY · newest first",
            Self::ScoreSheet => "POCHE · SCORE SHEET",
        }
    }
}

#[derive(Resource, Default)]
struct PresentationCache {
    surfaces: String,
    controls: String,
    hover: Option<String>,
}

#[derive(Component)]
struct WorldPresentation;

#[derive(Component)]
struct WorldControls;

#[derive(Component)]
struct WorldHoverTooltip;

#[derive(Component)]
struct FaceCamera;

#[derive(Component)]
struct WorldPanel {
    kind: PanelKind,
    half_size: Vec2,
}

#[derive(Component)]
struct WorldUiElement;

#[derive(Component)]
struct WorldReader;

#[derive(Component, Clone, Copy)]
enum WorldUiAction {
    ToggleSpeech,
    CloseReader,
}

#[derive(Message)]
struct WorldUiCommand(WorldUiAction);

fn activate_world_ui(
    mut click: On<Pointer<Click>>,
    actions: Query<&WorldUiAction>,
    mut commands: MessageWriter<WorldUiCommand>,
) {
    if click.button == bevy::picking::pointer::PointerButton::Primary
        && let Ok(action) = actions.get(click.entity)
    {
        click.propagate(false);
        commands.write(WorldUiCommand(*action));
    }
}

fn interact_with_world(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cameras: Query<
        (&Camera, &GlobalTransform, Option<&HandCamera>),
        Or<(With<TabletopCamera>, With<HandCamera>)>,
    >,
    panels: Query<(&WorldPanel, &GlobalTransform)>,
    buttons: Query<&Interaction, Or<(With<WorldUiElement>, With<UiAction>)>>,
    layout: Res<CanonicalLayout>,
    model: Res<BridgeModel>,
    mut state: ResMut<UiState>,
    mut interaction: ResMut<WorldInteraction>,
    mut events: MessageReader<WorldUiCommand>,
    mut clipboard: ResMut<Clipboard>,
    bridge: Res<BridgeHandle>,
    poses: Res<PoseDisplay>,
    hand: Res<hand_view::HandProjection>,
) {
    for WorldUiCommand(action) in events.read() {
        match action {
            WorldUiAction::ToggleSpeech => interaction.speech_open = !interaction.speech_open,
            WorldUiAction::CloseReader => interaction.reading = None,
        }
    }
    interaction.pointer_over_ui = buttons.iter().any(|value| *value != Interaction::None);
    interaction.hovered = None;
    if state.screen != UiScreen::Table {
        interaction.speech_open = false;
        interaction.reading = None;
        return;
    }
    if interaction.reading.is_some() && keys.just_pressed(KeyCode::Escape) {
        interaction.reading = None;
        state.escape_menu_open = false;
        return;
    }
    if interaction.reading.is_some() || state.escape_menu_open || interaction.pointer_over_ui {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // The inset is an overlay, not a second place. Its visible card must win
    // over a wall notice behind the same pixel in the table camera.
    for (camera, transform, marker) in &cameras {
        if marker.is_none()
            || !camera.is_active
            || !camera
                .logical_viewport_rect()
                .is_some_and(|rect| rect.contains(cursor))
        {
            continue;
        }
        let Ok(ray) = camera.viewport_to_world(transform, cursor) else {
            continue;
        };
        let over_card = model.snapshot.hand.iter().any(|card| {
            poses.0.get(&card.card_key).is_some_and(|pose| {
                let physical = mm_position(pose.current);
                hand.contains(physical)
                    && hand_view::card_hit(ray, hand.to_inset(physical), pose.current_rotation)
                        .is_some()
            })
        });
        if over_card {
            return;
        }
    }
    let Some((camera, camera_transform, _)) =
        cameras.iter().find(|(_, _, marker)| marker.is_none())
    else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };

    let mut closest: Option<(f32, WorldHit)> = None;
    for (panel, transform) in &panels {
        if let Some(distance) = intersect_panel(ray, transform, panel.half_size) {
            choose_nearest(&mut closest, distance, WorldHit::Panel(panel.kind));
        }
    }
    if model.snapshot.own_seat().is_none() {
        for placement in layout.0.seats() {
            let seat = placement.seat.get();
            if model
                .snapshot
                .members
                .iter()
                .any(|member| member.seat == Some(seat))
            {
                continue;
            }
            let center = point_to_world(placement.seat_pose.translation);
            let distance =
                ray.intersect_plane(center + Vec3::Y * 0.06, InfinitePlane3d::new(Vec3::Y));
            if let Some(distance) = distance {
                let at = ray.get_point(distance) - center;
                if Vec2::new(at.x, at.z).length_squared() <= 0.22_f32.powi(2) {
                    choose_nearest(&mut closest, distance, WorldHit::Seat(seat));
                }
            }
        }
    }
    let nearest_card = model
        .snapshot
        .hand
        .iter()
        .filter_map(|card| {
            let pose = poses.0.get(&card.card_key)?;
            hand_view::card_hit(ray, mm_position(pose.current), pose.current_rotation)
        })
        .min_by(f32::total_cmp);
    // A held/owned card placed on the paper stays grabbable. Presentation
    // surfaces behind its face must not claim the same click.
    if let Some(card_distance) = nearest_card {
        closest = closest.filter(|(distance, _)| *distance < card_distance);
    }
    let Some((_, target)) = closest else {
        if keys.pressed(KeyCode::KeyZ) {
            interaction.hovered = zone_hover(ray, &layout.0, &model.snapshot);
        }
        return;
    };
    interaction.pointer_over_ui = true;
    interaction.hovered = Some(match target {
        WorldHit::Seat(seat) => format!("Take seat {}", seat + 1),
        WorldHit::Panel(PanelKind::RoomCode) => "Copy room code".into(),
        WorldHit::Panel(kind) => format!("Read {}", kind.title().to_lowercase()),
    });
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    match target {
        WorldHit::Seat(seat) => {
            if let Some(room_id) = model.snapshot.room_id() {
                state.status = format!("Requesting seat {}…", seat + 1);
                if let Err(error) = bridge.send(BridgeIntent::TakeSeat {
                    room_id: room_id.into(),
                    seat,
                }) {
                    state.status = error;
                }
            }
        }
        WorldHit::Panel(PanelKind::RoomCode) => {
            state.status = match state
                .capability
                .as_ref()
                .map(|capability| clipboard.set_text(capability.join_code.clone()))
            {
                Some(Ok(())) => "Lobby code copied.".into(),
                _ => "The lobby code could not be copied.".into(),
            };
        }
        WorldHit::Panel(kind) => interaction.reading = Some(kind),
    }
}

#[derive(Clone, Copy)]
enum WorldHit {
    Seat(u8),
    Panel(PanelKind),
}

fn choose_nearest(closest: &mut Option<(f32, WorldHit)>, distance: f32, hit: WorldHit) {
    if closest
        .as_ref()
        .is_none_or(|(previous, _)| distance < *previous)
    {
        *closest = Some((distance, hit));
    }
}

fn intersect_panel(ray: Ray3d, transform: &GlobalTransform, half_size: Vec2) -> Option<f32> {
    let normal = transform.rotation() * Vec3::Z;
    let distance = ray.intersect_plane(transform.translation(), InfinitePlane3d::new(normal))?;
    let local = transform
        .affine()
        .inverse()
        .transform_point3(ray.get_point(distance));
    (local.x.abs() <= half_size.x && local.y.abs() <= half_size.y).then_some(distance)
}

fn zone_hover(ray: Ray3d, layout: &SpatialLayout, snapshot: &ClientSnapshot) -> Option<String> {
    let (zone, _) = layout
        .zones()
        .iter()
        .filter_map(|zone| ray_box_distance(ray, zone.inner).map(|distance| (zone.id, distance)))
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    let name = match zone {
        ZoneId::Deck => "Deck zone".into(),
        ZoneId::Trump => "Trump zone".into(),
        ZoneId::Play => "Play zone".into(),
        ZoneId::Hand(seat) => format!("{}'s hand zone", seat_name(snapshot, seat.get())),
        ZoneId::Won(seat) => format!("{}'s won-trick zone", seat_name(snapshot, seat.get())),
    };
    Some(format!(
        "{name}\nPhysical boundary only. A legal action changes a card's logical location."
    ))
}

fn ray_box_distance(ray: Ray3d, bounds: AabbMm) -> Option<f32> {
    let minimum = point_to_world(bounds.min);
    let maximum = point_to_world(bounds.max);
    let mut entry = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let direction = ray.direction[axis];
        if direction.abs() < 0.000_001 {
            if origin < minimum[axis] || origin > maximum[axis] {
                return None;
            }
        } else {
            let near = (minimum[axis] - origin) / direction;
            let far = (maximum[axis] - origin) / direction;
            entry = entry.max(near.min(far));
            exit = exit.min(near.max(far));
            if entry > exit {
                return None;
            }
        }
    }
    Some(entry)
}

fn sync_world_presentation(
    mut commands: Commands,
    model: Res<BridgeModel>,
    state: Res<UiState>,
    layout: Res<CanonicalLayout>,
    interaction: Res<WorldInteraction>,
    existing: Query<Entity, With<WorldPresentation>>,
    existing_controls: Query<Entity, With<WorldControls>>,
    existing_tooltips: Query<Entity, With<WorldHoverTooltip>>,
    mut cache: ResMut<PresentationCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let room_visible = matches!(state.screen, UiScreen::Table | UiScreen::LoadingRoom)
        && model.snapshot.room_id().is_some();
    let surface_content = if room_visible {
        format!(
            "{:?}|{:?}|{:?}|{}|{}",
            model.snapshot.members,
            model.snapshot.game,
            model.snapshot.activity,
            state
                .capability
                .as_ref()
                .map_or("", |capability| capability.join_code.as_str()),
            state.room_scene_generation
        )
    } else {
        String::new()
    };
    let controls_content = if room_visible && state.screen == UiScreen::Table {
        format!(
            "{}|{}|{:?}|{}",
            surface_content,
            model.snapshot.hand.len(),
            interaction.reading,
            interaction.speech_open
        )
    } else {
        String::new()
    };
    let hovered = if room_visible && state.screen == UiScreen::Table {
        interaction.hovered.clone()
    } else {
        None
    };
    if cache.hover != hovered {
        cache.hover.clone_from(&hovered);
        for entity in &existing_tooltips {
            commands.entity(entity).despawn();
        }
        if let Some(hint) = hovered {
            commands.spawn((
                WorldHoverTooltip,
                Pickable::IGNORE,
                Text::new(hint),
                TextFont::from_font_size(15.),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(18.),
                    bottom: px(18.),
                    max_width: percent(34.),
                    padding: UiRect::all(px(8.)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.035, 0.03, 0.90)),
                GlobalZIndex(21),
            ));
        }
    }
    if cache.controls != controls_content {
        cache.controls = controls_content;
        for entity in &existing_controls {
            commands.entity(entity).despawn();
        }
        if room_visible && state.screen == UiScreen::Table {
            spawn_table_controls(&mut commands, &model.snapshot, &state, &interaction);
        }
    }
    // Hover, opening a reader and navigating speech options never recreate GPU
    // textures. Only an accepted public projection (not a card pose) changes
    // the actual world surfaces.
    if cache.surfaces == surface_content {
        return;
    }
    cache.surfaces = surface_content;
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if !room_visible {
        return;
    }
    let mut painter = WorldPainter {
        commands: &mut commands,
        meshes: &mut meshes,
        materials: &mut materials,
        images: &mut images,
    };

    for (kind, anchor, size) in [
        (
            PanelKind::RoomCode,
            Vec3::new(-0.98, 0.34, -0.18),
            Vec2::new(0.62, 0.20),
        ),
        (
            PanelKind::Players,
            Vec3::new(0.99, 0.39, -0.40),
            Vec2::new(0.63, 0.30),
        ),
        (
            PanelKind::Activity,
            Vec3::new(1.02, 0.29, 0.33),
            Vec2::new(0.72, 0.38),
        ),
    ] {
        let lines = panel_lines(kind, &model.snapshot, state.capability.as_ref());
        painter.panel(kind, anchor, size, &lines, true);
    }
    let sheet = layout.0.score_sheet();
    let mut sheet_position = point_to_world(sheet.pose.translation);
    sheet_position.y += sheet.half_extents.y as f32 / 1000.0 + 0.001;
    painter.panel(
        PanelKind::ScoreSheet,
        sheet_position,
        Vec2::new(
            sheet.half_extents.x as f32 * 0.002,
            sheet.half_extents.z as f32 * 0.002,
        ),
        &panel_lines(PanelKind::ScoreSheet, &model.snapshot, None),
        false,
    );

    for placement in layout.0.seats() {
        let seat = placement.seat.get();
        if !model
            .snapshot
            .members
            .iter()
            .any(|member| member.seat == Some(seat))
        {
            let available = model.snapshot.own_seat().is_none();
            painter.label(
                point_to_world(placement.seat_pose.translation) + Vec3::Y * 0.17,
                &format!(
                    "Seat {}{}",
                    seat + 1,
                    if available {
                        " · click to sit"
                    } else {
                        " · empty"
                    }
                ),
                0.34,
                [242, 224, 144],
            );
        }
    }

    for member in &model.snapshot.members {
        let Some(seat) = member.seat else { continue };
        let Some(placement) = layout
            .0
            .seats()
            .iter()
            .find(|place| place.seat.get() == seat)
        else {
            continue;
        };
        let anchor = point_to_world(placement.player_pose.translation);
        painter.label(
            anchor + Vec3::Y * 0.30,
            &format!(
                "{}{}",
                member.display_name,
                if member.is_self { " (you)" } else { "" }
            ),
            0.34,
            [242, 237, 209],
        );
        if let Some(speech) = player_speech(&model.snapshot, seat) {
            painter.bubble(anchor + Vec3::new(0.4, 0.44, 0.0), &speech);
        }
    }
}

struct WorldPainter<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
    images: &'a mut Assets<Image>,
}

impl WorldPainter<'_, '_, '_> {
    fn panel(
        &mut self,
        kind: PanelKind,
        anchor: Vec3,
        size: Vec2,
        lines: &[String],
        billboard: bool,
    ) {
        let mut root = self.commands.spawn((
            WorldPresentation,
            WorldPanel {
                kind,
                half_size: size * 0.5,
            },
            Transform::from_translation(anchor).with_rotation(if billboard {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
            }),
            Visibility::default(),
        ));
        if billboard {
            root.insert(FaceCamera);
        }
        let root = root.id();
        let paper = kind == PanelKind::ScoreSheet;
        let background = self
            .commands
            .spawn((
                Mesh3d(self.meshes.add(Cuboid::new(
                    size.x,
                    size.y,
                    if paper { 0.0006 } else { 0.012 },
                ))),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: if paper {
                        Color::srgb(0.93, 0.90, 0.76)
                    } else {
                        Color::srgb(0.055, 0.075, 0.072)
                    },
                    unlit: !paper,
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::default(),
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(background);
        let row_height = size.y / (lines.len() + 2) as f32;
        for (row, line) in lines.iter().enumerate() {
            self.text(
                root,
                line,
                Vec3::new(0.0, size.y * 0.5 - row_height * (row as f32 + 1.0), 0.008),
                Vec2::new(size.x * 0.90, row_height * 0.75),
                if paper { [30, 31, 27] } else { [232, 230, 205] },
            );
        }
    }

    fn label(&mut self, anchor: Vec3, label: &str, width: f32, color: [u8; 3]) {
        let root = self
            .commands
            .spawn((
                WorldPresentation,
                FaceCamera,
                Transform::from_translation(anchor),
                Visibility::default(),
            ))
            .id();
        self.text(root, label, Vec3::ZERO, Vec2::new(width, 0.035), color);
    }

    fn bubble(&mut self, anchor: Vec3, speech: &str) {
        let root = self
            .commands
            .spawn((
                WorldPresentation,
                FaceCamera,
                Transform::from_translation(anchor),
                Visibility::default(),
            ))
            .id();
        let lines = wrap_text(speech, 28);
        let height = 0.045 * lines.len() as f32 + 0.026;
        let panel = self
            .commands
            .spawn((
                Mesh3d(self.meshes.add(Cuboid::new(0.49, height, 0.004))),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: Color::srgb(0.96, 0.93, 0.80),
                    unlit: true,
                    ..default()
                })),
                Transform::default(),
                NotShadowCaster,
                NotShadowReceiver,
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(panel);
        for (row, line) in lines.iter().enumerate() {
            self.text(
                root,
                line,
                Vec3::new(0.0, height * 0.5 - 0.03 - row as f32 * 0.045, 0.003),
                Vec2::new(0.45, 0.032),
                [26, 30, 26],
            );
        }
    }

    fn text(&mut self, root: Entity, text: &str, position: Vec3, bounds: Vec2, color: [u8; 3]) {
        if text.is_empty() {
            return;
        }
        let (texture, aspect) = card_label_texture(self.images, text, color);
        let height = bounds.y.min(bounds.x / aspect.max(0.01));
        let label = self
            .commands
            .spawn((
                Mesh3d(self.meshes.add(Plane3d::default())),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(texture),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(height * aspect, 1.0, height)),
                NotShadowCaster,
                NotShadowReceiver,
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(label);
    }
}

fn face_world_labels_towards_camera(
    camera: Query<&Transform, (With<TabletopCamera>, Without<FaceCamera>)>,
    mut labels: Query<&mut Transform, With<FaceCamera>>,
) {
    let Ok(camera) = camera.single() else { return };
    for mut transform in &mut labels {
        let position = transform.translation;
        let direction = position - camera.translation;
        if direction.length_squared() > f32::EPSILON {
            transform.look_to(direction, *camera.up());
        }
    }
}

fn scroll_reader(
    scroll: Res<AccumulatedMouseScroll>,
    interaction: Res<WorldInteraction>,
    mut readers: Query<(&mut ScrollPosition, &ComputedNode), With<WorldReader>>,
) {
    if !interaction.modal_open() {
        return;
    }
    let delta = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * 32.0,
        MouseScrollUnit::Pixel => scroll.delta.y,
    };
    for (mut position, computed) in &mut readers {
        let maximum = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        position.y = (position.y - delta).clamp(0.0, maximum);
    }
}

fn panel_lines(
    kind: PanelKind,
    snapshot: &ClientSnapshot,
    capability: Option<&RoomCapability>,
) -> Vec<String> {
    let mut lines = vec![kind.title().into()];
    match kind {
        PanelKind::RoomCode => lines.push(
            capability
                .map_or("Not available", |value| value.join_code.as_str())
                .into(),
        ),
        PanelKind::Players => {
            for member in &snapshot.members {
                lines.push(format!(
                    "{}{} · {}{}",
                    member.display_name,
                    if member.is_self { " (you)" } else { "" },
                    member
                        .seat
                        .map_or_else(|| "standing".into(), |seat| format!("seat {}", seat + 1)),
                    if member.connected { "" } else { " · away" }
                ));
            }
        }
        PanelKind::Activity => {
            for entry in snapshot.activity.iter().rev().take(6) {
                lines.extend(wrap_text(
                    &format!("#{} {}", entry.sequence + 1, entry.summary),
                    44,
                ));
            }
            if snapshot.activity.is_empty() {
                lines.push("No public actions yet.".into());
            }
        }
        PanelKind::ScoreSheet => {
            lines.extend(score_sheet_lines(snapshot));
        }
    }
    lines
}

fn seat_name(snapshot: &ClientSnapshot, seat: u8) -> String {
    snapshot
        .members
        .iter()
        .find(|member| member.seat == Some(seat))
        .map_or_else(
            || format!("Seat {}", seat + 1),
            |member| member.display_name.clone(),
        )
}

fn score_sheet_lines(snapshot: &ClientSnapshot) -> Vec<String> {
    let player0 = truncate_text(&seat_name(snapshot, 0), 10);
    let player1 = truncate_text(&seat_name(snapshot, 1), 10);
    let mut lines = vec![format!("Round | Dealer | Cards | {player0} | {player1}")];
    let Some(game) = &snapshot.game else {
        lines.push("Waiting for the first deal".into());
        lines.push("Total |       |       | 0 | 0".into());
        return lines;
    };
    let settled = matches!(game.phase.as_str(), "scored" | "finished" | "scoring")
        && game.hand_counts == [0, 0];
    let cells = [0_usize, 1].map(|index| match game.bids[index] {
        None => "—".into(),
        Some(bid) if !settled => bid.to_string(),
        Some(bid) => score_cell(bid, game.tricks_won[index], game.hand_size),
    });
    lines.push(format!(
        "{} | {} | {} | {} | {}",
        game.round_index + 1,
        game.dealer_seat.map_or_else(
            || "—".into(),
            |seat| truncate_text(&seat_name(snapshot, seat), 10)
        ),
        game.hand_size,
        cells[0],
        cells[1]
    ));
    lines.push(format!(
        "Recorded total |       |       | {} | {}",
        game.scores[0], game.scores[1]
    ));
    lines.push(format!(
        "Pot ${}.{:02} · {}",
        game.pot_cents / 100,
        game.pot_cents % 100,
        game.phase
    ));
    lines.push("Bid → ● missed · 1N made · 2N all tricks".into());
    if game.phase == "scoring" {
        lines.push("Outcome shown; authority scoring is pending.".into());
    }
    lines
}

fn score_cell(bid: u8, tricks: u8, hand_size: u8) -> String {
    if bid != tricks {
        "●".into()
    } else if tricks == hand_size {
        format!("2{bid}")
    } else {
        format!("1{bid}")
    }
}

fn player_speech(snapshot: &ClientSnapshot, seat: u8) -> Option<String> {
    let game = snapshot.game.as_ref()?;
    let accepted_bid = game
        .bids
        .get(usize::from(seat))
        .copied()
        .flatten()
        .map(bid_phrase);
    // The scorekeeper role has not yet been elected in the authority schema.
    // The dealer voices the mechanical prompt; this grants no new permission.
    if game.phase == "bidding"
        && game.dealer_seat == Some(seat)
        && let Some(actor) = game.actor_seat
    {
        let question = format!("{}, how many tricks?", seat_name(snapshot, actor));
        return Some(accepted_bid.map_or(question.clone(), |bid| format!("{bid}. {question}")));
    }
    if game.phase == "playing" && game.actor_seat == Some(seat) {
        return Some("Choosing a card…".into());
    }
    accepted_bid
}

fn bid_phrase(tricks: u8) -> String {
    format!(
        "I bid {tricks} {}",
        if tricks == 1 { "trick" } else { "tricks" }
    )
}

fn truncate_text(text: &str, maximum: usize) -> String {
    let mut result = text.chars().take(maximum).collect::<String>();
    if text.chars().count() > maximum {
        result.push('…');
    }
    result
}

fn wrap_text(text: &str, maximum: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > maximum {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn spawn_table_controls(
    commands: &mut Commands,
    snapshot: &ClientSnapshot,
    state: &UiState,
    interaction: &WorldInteraction,
) {
    commands
        .spawn((
            WorldControls,
            Node {
                position_type: PositionType::Absolute,
                top: px(16.),
                left: px(16.),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.),
                ..default()
            },
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            small_button(
                root,
                if may_bid(snapshot, 0) {
                    "Speech · your bid"
                } else {
                    "Speech · bids"
                },
                WorldUiAction::ToggleSpeech,
            );
            if interaction.speech_open {
                root.spawn((
                    Node {
                        padding: UiRect::all(px(10.)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6.),
                        max_width: px(290.),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.025, 0.045, 0.04)),
                ))
                .with_children(|picker| {
                    picker.spawn((
                        Text::new("Say → Bid"),
                        TextFont::from_font_size(17.),
                        TextColor(Color::srgb(0.96, 0.90, 0.63)),
                    ));
                    let legal = snapshot.game.as_ref().map_or_else(Vec::new, |game| {
                        (0..=game.hand_size)
                            .filter(|bid| may_bid(snapshot, *bid))
                            .collect::<Vec<_>>()
                    });
                    if legal.is_empty() {
                        picker.spawn((
                            Text::new("Bids are available when it is your turn."),
                            TextFont::from_font_size(16.),
                        ));
                    }
                    for tricks in legal {
                        picker
                            .spawn((
                                Button,
                                WorldUiElement,
                                UiAction::Bid(tricks),
                                Node {
                                    padding: UiRect::axes(px(12.), px(8.)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.055, 0.32, 0.25)),
                            ))
                            .with_children(|button| {
                                button.spawn((
                                    Text::new(bid_phrase(tricks)),
                                    TextFont::from_font_size(18.),
                                ));
                            });
                    }
                });
            }
        });
    if let Some(kind) = interaction.reading {
        let lines = if kind == PanelKind::Activity {
            std::iter::once(kind.title().to_owned())
                .chain(
                    snapshot
                        .activity
                        .iter()
                        .rev()
                        .map(|event| format!("#{} {}", event.sequence + 1, event.summary)),
                )
                .collect::<Vec<_>>()
        } else {
            panel_lines(kind, snapshot, state.capability.as_ref())
        };
        commands
            .spawn((
                WorldControls,
                WorldReader,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(12.),
                    right: percent(12.),
                    top: percent(14.),
                    max_height: percent(70.),
                    padding: UiRect::all(px(24.)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(18.),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.065, 0.075, 0.065)),
                GlobalZIndex(30),
            ))
            .with_children(|reader| {
                reader.spawn((
                    Text::new(lines.join("\n")),
                    TextFont::from_font_size(21.),
                    TextColor(Color::srgb(0.95, 0.93, 0.79)),
                ));
                small_button(reader, "Put down · Esc", WorldUiAction::CloseReader);
            });
    }
}

fn small_button(parent: &mut ChildSpawnerCommands, label: &str, action: WorldUiAction) {
    parent
        .spawn((
            Button,
            WorldUiElement,
            action,
            Node {
                padding: UiRect::axes(px(11.), px(8.)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.045, 0.16, 0.14)),
        ))
        .with_children(|button| {
            button.spawn((Text::new(label), TextFont::from_font_size(17.)));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_spacetimedb_client::{GameView, HandCardView, MemberView, RoomView};
    use poche_spatial::{LayoutId, TableId, registered_layout};

    fn snapshot() -> ClientSnapshot {
        ClientSnapshot {
            identity: Some("one".into()),
            members: vec![
                MemberView {
                    identity: "one".into(),
                    display_name: "Alice".into(),
                    seat: Some(0),
                    connected: true,
                    is_self: true,
                },
                MemberView {
                    identity: "two".into(),
                    display_name: "Bob".into(),
                    seat: Some(1),
                    connected: true,
                    is_self: false,
                },
            ],
            game: Some(GameView {
                phase: "bidding".into(),
                actor_seat: Some(1),
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
                trump: Some(2),
                action_count: 0,
            }),
            hand: vec![HandCardView {
                card_key: "secret-key".into(),
                card_id: "secret-id".into(),
                face: "A♠".into(),
            }],
            ..default()
        }
    }

    #[test]
    fn public_world_surfaces_never_include_private_card_faces_or_identifiers() {
        let snapshot = snapshot();
        for kind in [
            PanelKind::Players,
            PanelKind::Activity,
            PanelKind::ScoreSheet,
        ] {
            let text = panel_lines(kind, &snapshot, None).join("\n");
            assert!(!text.contains("A♠"));
            assert!(!text.contains("secret-"));
        }
        assert_eq!(
            player_speech(&snapshot, 0).as_deref(),
            Some("Bob, how many tricks?")
        );
        assert_eq!(player_speech(&snapshot, 1), None);
    }

    #[test]
    fn scoresheet_replaces_bids_only_after_play_is_complete() {
        let mut snapshot = snapshot();
        let game = snapshot.game.as_mut().unwrap();
        game.bids = [Some(0), Some(1)];
        assert!(score_sheet_lines(&snapshot)[1].ends_with("| 0 | 1"));
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "scored".into();
        game.hand_counts = [0, 0];
        game.tricks_won = [0, 1];
        game.scores = [10, 21];
        let lines = score_sheet_lines(&snapshot);
        assert!(lines[1].ends_with("| 10 | 21"));
        assert!(lines[2].ends_with("| 10 | 21"));
        snapshot.game.as_mut().unwrap().tricks_won = [1, 0];
        assert!(score_sheet_lines(&snapshot)[1].ends_with("| ● | ●"));
    }

    #[test]
    fn ray_hits_rotated_world_poster_only_inside_its_surface() {
        let transform = GlobalTransform::from(
            Transform::from_xyz(2., 1., 0.)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        );
        let hit = Ray3d::new(Vec3::new(4., 1., 0.), Dir3::NEG_X);
        assert_eq!(
            intersect_panel(hit, &transform, Vec2::new(0.5, 0.3)),
            Some(2.)
        );
        let miss = Ray3d::new(Vec3::new(4., 1.31, 0.), Dir3::NEG_X);
        assert!(intersect_panel(miss, &transform, Vec2::new(0.5, 0.3)).is_none());
    }

    #[test]
    fn zone_inspection_names_the_real_volume_and_explains_the_logical_boundary() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 1).unwrap()).unwrap();
        let hand = layout
            .zones()
            .iter()
            .find(|zone| matches!(zone.id, ZoneId::Hand(seat) if seat.get() == 0))
            .unwrap();
        let center = (point_to_world(hand.inner.min) + point_to_world(hand.inner.max)) * 0.5;
        let ray = Ray3d::new(center + Vec3::Y, Dir3::NEG_Y);
        let label = zone_hover(ray, &layout, &snapshot()).unwrap();
        assert!(label.starts_with("Alice's hand zone"));
        assert!(label.contains("Physical boundary only"));
        assert!(label.contains("legal action"));
        assert!(!label.contains("A♠"));
        assert!(ray_box_distance(Ray3d::new(center + Vec3::Y, Dir3::Y), hand.inner).is_none());
        assert!(ray_box_distance(Ray3d::new(center + Vec3::X, Dir3::NEG_Y), hand.inner).is_none());
        assert_eq!(
            ray_box_distance(Ray3d::new(center, Dir3::NEG_Y), hand.inner),
            Some(0.0)
        );
    }

    #[test]
    fn hovering_and_opening_speech_preserve_world_entities_and_textures() {
        let mut snapshot = snapshot();
        snapshot.rooms.push(RoomView {
            room_id: "public-room".into(),
        });
        let state = UiState {
            screen: UiScreen::Table,
            room_scene_generation: 1,
            ..default()
        };
        let mut app = App::new();
        app.insert_resource(BridgeModel {
            snapshot,
            ..default()
        })
        .insert_resource(state)
        .insert_resource(CanonicalLayout(
            registered_layout(TableId::new(1), LayoutId::new(2, 1).unwrap()).unwrap(),
        ))
        .init_resource::<WorldInteraction>()
        .init_resource::<PresentationCache>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<Image>>()
        .add_systems(Update, sync_world_presentation);
        app.update();
        let mut panels = app.world_mut().query_filtered::<Entity, With<WorldPanel>>();
        let before = panels.iter(app.world()).collect::<Vec<_>>();
        let images_before = app.world().resource::<Assets<Image>>().len();
        assert_eq!(before.len(), 4);
        let mut buttons = app
            .world_mut()
            .query_filtered::<Entity, With<WorldUiElement>>();
        let buttons_before = buttons.iter(app.world()).collect::<Vec<_>>();
        app.world_mut().resource_mut::<WorldInteraction>().hovered = Some("Take seat 2".into());
        app.update();
        assert_eq!(
            buttons.iter(app.world()).collect::<Vec<_>>(),
            buttons_before
        );
        {
            let mut interaction = app.world_mut().resource_mut::<WorldInteraction>();
            interaction.speech_open = true;
            interaction.reading = Some(PanelKind::Activity);
        }
        app.update();
        let after = panels.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(after, before);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), images_before);

        // An accepted bid really does change the ink on the sheet and speech.
        app.world_mut()
            .resource_mut::<BridgeModel>()
            .snapshot
            .game
            .as_mut()
            .unwrap()
            .bids[1] = Some(1);
        app.update();
        assert_ne!(panels.iter(app.world()).collect::<Vec<_>>(), before);
    }
}
