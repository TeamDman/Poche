// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Readable world surfaces over the public projection. None of these surfaces
//! owns game state: a chair click and a spoken bid use the ordinary UI intents.

use super::{
    ButtonActivation, CanonicalLayout, HandCamera, PoseDisplay, TabletopCamera, UiAction, UiScreen,
    UiState, card_label_texture, drag_cards, hand_view, may_bid, mm_position, point_to_world,
    room_geometry, sync_player_entities, update_table_camera,
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
            .init_resource::<SheetInspectionRequest>()
            .init_resource::<PresentationCache>()
            .add_message::<WorldUiCommand>()
            .add_observer(activate_world_ui)
            .add_systems(Startup, spawn_world_action_relays)
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SheetInspectionTarget {
    pub center: Vec3,
    pub size: Vec2,
}

#[derive(Resource, Default)]
pub(super) struct SheetInspectionRequest {
    pub pending: Option<SheetInspectionTarget>,
    pub active: bool,
}

pub(super) fn sheet_inspection_target(layout: &SpatialLayout) -> SheetInspectionTarget {
    let paper = layout.score_sheet();
    let mut center = point_to_world(paper.pose.translation);
    center.y += paper.half_extents.y as f32 / 1000.0 + 0.001;
    SheetInspectionTarget {
        center,
        size: Vec2::new(
            paper.half_extents.x as f32 * 0.002,
            paper.half_extents.z as f32 * 0.002,
        ),
    }
}

fn deck_count(snapshot: &ClientSnapshot) -> u8 {
    snapshot
        .game
        .as_ref()
        .filter(|game| matches!(game.phase.as_str(), "bidding" | "playing" | "scoring"))
        .map_or(52, |game| 51_u8.saturating_sub(game.hand_size * 2))
}

pub(super) fn deck_inspection_center(snapshot: &ClientSnapshot) -> Vec3 {
    Vec3::new(
        -0.21,
        0.021 + f32::from(deck_count(snapshot)) * 0.00045 + 0.0025,
        -0.23,
    )
}

impl WorldInteraction {
    pub(super) fn hover_hint(&self) -> Option<&str> {
        self.hovered.as_deref()
    }

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
    Deck,
    Door,
}

impl PanelKind {
    fn title(self) -> &'static str {
        match self {
            Self::RoomCode => "ROOM CODE · click to copy",
            Self::Players => "PLAYERS",
            Self::Activity => "ACTIVITY · newest first",
            Self::ScoreSheet => "POCHE · SCORE SHEET",
            Self::Deck => "DECK · click to deal",
            Self::Door => "EXIT",
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
pub(super) struct WorldPanel {
    kind: PanelKind,
    half_size: Vec2,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
enum WorldActionRelay {
    TakeSeat(u8),
    ReleaseSeat,
    Leave,
}

fn spawn_world_action_relays(mut commands: Commands) {
    // These entities outlive presentation rebuilds. A click queued just before
    // an authority update must still reach the ordinary button handler.
    for (relay, action) in [
        (WorldActionRelay::TakeSeat(0), UiAction::TakeSeat(0)),
        (WorldActionRelay::TakeSeat(1), UiAction::TakeSeat(1)),
        (WorldActionRelay::ReleaseSeat, UiAction::ReleaseSeat),
        (WorldActionRelay::Leave, UiAction::Leave),
    ] {
        commands.spawn((relay, action));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct WorldActionRouter<'w, 's> {
    panels: Query<'w, 's, (&'static WorldPanel, &'static GlobalTransform)>,
    relays: Query<'w, 's, (Entity, &'static WorldActionRelay)>,
    activations: MessageWriter<'w, ButtonActivation>,
    hand_options: Res<'w, super::hand_options::HandViewOptions>,
    selection: Option<Res<'w, super::selection::SelectionState>>,
}

#[derive(Component)]
pub(super) struct WorldUiElement;

#[derive(Component)]
struct WorldReader;

#[derive(Component)]
struct NameTagBackdrop;

#[derive(Component)]
struct ScoreSheetCell;

#[derive(Component)]
struct ScoreSheetRule;

const NAME_TAG_BACKGROUND: [u8; 3] = [15, 28, 25];
const NAME_TAG_FOREGROUND: [u8; 3] = [242, 237, 209];
const SHEET_COLUMNS: [f32; 5] = [0.13, 0.23, 0.12, 0.26, 0.26];
const SHEET_FONT_HEIGHT: f32 = 0.006;

#[derive(Component, Clone, Copy)]
enum WorldUiAction {
    ToggleSpeech,
    CloseReader,
}

#[derive(Message)]
pub(super) struct WorldUiCommand(WorldUiAction);

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

pub(super) fn interact_with_world(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cameras: Query<
        (&Camera, &GlobalTransform, Option<&HandCamera>),
        Or<(With<TabletopCamera>, With<HandCamera>)>,
    >,
    mut router: WorldActionRouter,
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
    mut inspection: ResMut<SheetInspectionRequest>,
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
    if interaction.reading.is_some()
        || state.escape_menu_open
        || interaction.pointer_over_ui
        || router.hand_options.blocks_pointer_input()
        || router
            .selection
            .as_ref()
            .is_some_and(|selection| selection.selecting)
    {
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
                    && hand_view::card_hit(
                        ray,
                        hand.card_position(&poses, &card.card_key).unwrap(),
                        pose.current_rotation,
                    )
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
    for (panel, transform) in &router.panels {
        if inspection.active && panel.kind != PanelKind::ScoreSheet {
            continue;
        }
        let distance = if panel.kind == PanelKind::Door {
            intersect_box(
                ray,
                transform,
                Vec3::new(
                    panel.half_size.x,
                    panel.half_size.y,
                    room_geometry::DOOR_LEAF_SIZE.z * 0.5,
                ),
            )
        } else {
            intersect_panel(ray, transform, panel.half_size)
        };
        if let Some(distance) = distance {
            choose_nearest(&mut closest, distance, WorldHit::Panel(panel.kind));
        }
    }
    if !inspection.active {
        for placement in layout.0.seats() {
            let seat = placement.seat.get();
            let Some(action) = seat_action(&model.snapshot, seat) else {
                continue;
            };
            let relay = if matches!(action, UiAction::ReleaseSeat) {
                WorldActionRelay::ReleaseSeat
            } else {
                WorldActionRelay::TakeSeat(seat)
            };
            let Some((entity, _)) = router
                .relays
                .iter()
                .find(|(_, candidate)| **candidate == relay)
            else {
                continue;
            };
            let center = point_to_world(placement.seat_pose.translation);
            let pick_center = seat_pick_center(&layout.0, seat).unwrap_or(center);
            let distance = ray.intersect_plane(pick_center, InfinitePlane3d::new(Vec3::Y));
            if let Some(distance) = distance {
                let at = ray.get_point(distance) - center;
                let occluded = model
                    .snapshot
                    .members
                    .iter()
                    .filter_map(|member| member.seat)
                    .filter_map(|seat| {
                        layout
                            .0
                            .seats()
                            .iter()
                            .find(|placement| placement.seat.get() == seat)
                    })
                    .any(|placement| {
                        avatar_hit(ray, point_to_world(placement.player_pose.translation))
                            .is_some_and(|hit| hit < distance)
                    });
                if Vec2::new(at.x, at.z).length_squared() <= 0.22_f32.powi(2) && !occluded {
                    choose_nearest(&mut closest, distance, WorldHit::Seat(seat, entity));
                }
            }
        }
    }
    let nearest_card = model
        .snapshot
        .card_poses
        .iter()
        .filter(|card| super::may_manipulate_card(&model.snapshot, card))
        .filter_map(|card| {
            let pose = poses.0.get(&card.card_key)?;
            hand_view::card_hit(
                ray,
                hand_view::visual_position(&poses, &card.card_key)?,
                pose.current_rotation,
            )
        })
        .min_by(f32::total_cmp);
    // A held/owned card placed on the paper stays grabbable. Presentation
    // surfaces behind its face must not claim the same click.
    if let Some(card_distance) = nearest_card.filter(|_| !inspection.active) {
        closest = closest.filter(|(distance, _)| *distance < card_distance);
    }
    let Some((_, target)) = closest else {
        interaction.pointer_over_ui = inspection.active;
        if keys.pressed(KeyCode::KeyZ) {
            interaction.hovered = zone_hover(ray, &layout.0, &model.snapshot);
        }
        return;
    };
    interaction.pointer_over_ui = true;
    interaction.hovered = Some(match target {
        WorldHit::Seat(seat, _) => seat_hover(&model.snapshot, seat),
        WorldHit::Panel(PanelKind::RoomCode) => "Copy room code".into(),
        WorldHit::Panel(PanelKind::Deck) => deck_hover(&model.snapshot),
        WorldHit::Panel(PanelKind::Door) => door_hover(state.confirm_leave).into(),
        WorldHit::Panel(PanelKind::ScoreSheet) if inspection.active => {
            "Click paper again to restore camera · wheel zoom · MMB pan".into()
        }
        WorldHit::Panel(kind) => format!("Read {}", kind.title().to_lowercase()),
    });
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    match target {
        WorldHit::Seat(_, entity) => {
            // Physical affordances use exactly the same typed UI actions as
            // menus and puppets, including the two-activation leave guard.
            router.activations.write(ButtonActivation(entity));
        }
        WorldHit::Panel(PanelKind::Door) => {
            if let Some((entity, _)) = router
                .relays
                .iter()
                .find(|(_, relay)| **relay == WorldActionRelay::Leave)
            {
                router.activations.write(ButtonActivation(entity));
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
        WorldHit::Panel(PanelKind::Deck) => {
            if let Some(room_id) = model.snapshot.room_id() {
                if model.snapshot.game.as_ref().is_some_and(|game| {
                    game.phase == "awaiting-deal" && game.dealer_seat == model.snapshot.own_seat()
                }) {
                    state.status = "Shuffling and dealing…".into();
                    if let Err(error) = bridge.send(BridgeIntent::DealNextRound {
                        room_id: room_id.into(),
                    }) {
                        state.status = error;
                    }
                } else {
                    state.status = next_step(&model.snapshot).join(" ");
                }
            }
        }
        WorldHit::Panel(kind) => open_panel(kind, &layout.0, &mut interaction, &mut inspection),
    }
}

fn open_panel(
    kind: PanelKind,
    layout: &SpatialLayout,
    interaction: &mut WorldInteraction,
    inspection: &mut SheetInspectionRequest,
) {
    if kind == PanelKind::ScoreSheet {
        interaction.reading = None;
        interaction.speech_open = false;
        inspection.pending = Some(sheet_inspection_target(layout));
    } else {
        interaction.reading = Some(kind);
    }
}

#[derive(Clone, Copy)]
enum WorldHit {
    Seat(u8, Entity),
    Panel(PanelKind),
}

fn seat_action(snapshot: &ClientSnapshot, seat: u8) -> Option<UiAction> {
    if snapshot.own_seat() == Some(seat) {
        return Some(UiAction::ReleaseSeat);
    }
    (snapshot.own_seat().is_none()
        && !snapshot
            .members
            .iter()
            .any(|member| member.seat == Some(seat)))
    .then_some(UiAction::TakeSeat(seat))
}

fn seat_hover(snapshot: &ClientSnapshot, seat: u8) -> String {
    if snapshot.own_seat() == Some(seat) {
        "Stand up · releases your seat".into()
    } else {
        format!("Take seat {}", seat + 1)
    }
}

fn door_hover(confirm_leave: bool) -> &'static str {
    if confirm_leave {
        "Click the door again to leave lobby · your seat will be released"
    } else {
        "Leave lobby · click the door"
    }
}

fn deck_hover(snapshot: &ClientSnapshot) -> String {
    let mut lines = vec![format!("{} cards in deck", deck_count(snapshot))];
    if let Some(trump) = snapshot.game.as_ref().and_then(|game| game.trump) {
        lines.push(format!("Trump: {}", public_card_label(trump)));
    }
    lines.extend(next_step(snapshot));
    lines.join("\n")
}

pub(super) fn door_center() -> Vec3 {
    room_geometry::door_center()
}

pub(super) fn seat_pick_center(layout: &SpatialLayout, seat: u8) -> Option<Vec3> {
    layout
        .seats()
        .iter()
        .find(|placement| placement.seat.get() == seat)
        .map(|placement| {
            let center = point_to_world(placement.seat_pose.translation);
            let outward = Vec3::new(center.x, 0., center.z).normalize_or_zero();
            center + outward * 0.19 + Vec3::Y * 0.06
        })
}

fn intersect_box(ray: Ray3d, transform: &GlobalTransform, half_size: Vec3) -> Option<f32> {
    let inverse = transform.affine().inverse();
    let origin = inverse.transform_point3(ray.origin);
    let direction = inverse.transform_vector3(*ray.direction);
    let mut entry = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < 0.000_001 {
            if origin[axis].abs() > half_size[axis] {
                return None;
            }
        } else {
            let near = (-half_size[axis] - origin[axis]) / direction[axis];
            let far = (half_size[axis] - origin[axis]) / direction[axis];
            entry = entry.max(near.min(far));
            exit = exit.min(near.max(far));
            if entry > exit {
                return None;
            }
        }
    }
    Some(entry)
}

fn avatar_hit(ray: Ray3d, center: Vec3) -> Option<f32> {
    // Match the renderer's upright Capsule3d::new(0.12, 0.24).
    let relative = ray.origin - center;
    let direction = *ray.direction;
    let mut hits = Vec::with_capacity(6);
    let a = direction.x * direction.x + direction.z * direction.z;
    let b = relative.x * direction.x + relative.z * direction.z;
    let c = relative.x * relative.x + relative.z * relative.z - 0.12_f32.powi(2);
    let discriminant = b * b - a * c;
    if a > 0.000_001 && discriminant >= 0.0 {
        for distance in [
            (-b - discriminant.sqrt()) / a,
            (-b + discriminant.sqrt()) / a,
        ] {
            if distance >= 0.0 && (relative.y + direction.y * distance).abs() <= 0.12 {
                hits.push(distance);
            }
        }
    }
    for y in [-0.12, 0.12] {
        let from_cap = relative - Vec3::Y * y;
        let b = from_cap.dot(direction);
        let c = from_cap.length_squared() - 0.12_f32.powi(2);
        let discriminant = b * b - c;
        if discriminant >= 0.0 {
            for distance in [-b - discriminant.sqrt(), -b + discriminant.sqrt()] {
                if distance >= 0.0 {
                    hits.push(distance);
                }
            }
        }
    }
    hits.into_iter().min_by(f32::total_cmp)
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
    inspection: Res<SheetInspectionRequest>,
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
            "{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}",
            model.snapshot.members,
            model.snapshot.game,
            model.snapshot.activity,
            model.snapshot.rounds,
            [model.snapshot.payment_due(0), model.snapshot.payment_due(1)],
            state
                .capability
                .as_ref()
                .map_or("", |capability| capability.join_code.as_str()),
            state.room_scene_generation
        )
    } else {
        String::new()
    };
    let controls_content = if room_visible && state.screen == UiScreen::Table && !inspection.active
    {
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
        if room_visible && state.screen == UiScreen::Table && !inspection.active {
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
    painter.score_sheet(sheet_inspection_target(&layout.0), &model.snapshot);
    painter.deck(&model.snapshot);
    painter.door();

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
            NAME_TAG_FOREGROUND,
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
    fn door(&mut self) {
        let center = door_center();
        let root = self
            .commands
            .spawn((
                WorldPresentation,
                WorldPanel {
                    kind: PanelKind::Door,
                    half_size: room_geometry::DOOR_LEAF_SIZE.truncate() * 0.5,
                },
                Pickable::IGNORE,
                Transform::from_translation(center),
                Visibility::default(),
            ))
            .id();
        // A tangible door with a jamb and knob, not another floating notice.
        // The full front/back rectangle is pickable from either player view.
        let leaf = std::iter::once((
            Vec3::ZERO,
            room_geometry::DOOR_LEAF_SIZE,
            Color::srgb(0.11, 0.24, 0.20),
        ));
        let frame = room_geometry::door_frame_parts()
            .into_iter()
            .map(|(position, size)| (position, size, Color::srgb(0.23, 0.19, 0.14)));
        for (position, size, color) in leaf.chain(frame) {
            let piece = self
                .commands
                .spawn((
                    Mesh3d(self.meshes.add(Cuboid::from_size(size))),
                    MeshMaterial3d(self.materials.add(StandardMaterial {
                        base_color: color,
                        perceptual_roughness: 0.85,
                        ..default()
                    })),
                    Transform::from_translation(position),
                    Pickable::IGNORE,
                    RenderLayers::layer(0),
                ))
                .id();
            self.commands.entity(root).add_child(piece);
        }
        for facing in [-1.0, 1.0] {
            let knob = self
                .commands
                .spawn((
                    Mesh3d(self.meshes.add(Sphere::new(0.022))),
                    MeshMaterial3d(self.materials.add(StandardMaterial {
                        base_color: Color::srgb(0.72, 0.59, 0.27),
                        metallic: 0.8,
                        ..default()
                    })),
                    Transform::from_xyz(0.13, -0.04, facing * 0.041),
                    Pickable::IGNORE,
                    RenderLayers::layer(0),
                ))
                .id();
            self.commands.entity(root).add_child(knob);
        }
        self.text(
            root,
            "EXIT",
            Vec3::new(0.0, 0.25, 0.025),
            Vec2::new(0.27, 0.07),
            [240, 238, 211],
        );
        // The room is on the negative-Z side of this perimeter. Keep the
        // inside EXIT lettering readable, rather than showing the back of
        // the outside sign through a two-sided material.
        let inside_sign = self
            .commands
            .spawn((
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                Visibility::default(),
            ))
            .id();
        self.commands.entity(root).add_child(inside_sign);
        self.text(
            inside_sign,
            "EXIT",
            Vec3::new(0.0, 0.25, 0.025),
            Vec2::new(0.27, 0.07),
            [240, 238, 211],
        );
    }

    fn deck(&mut self, snapshot: &ClientSnapshot) {
        let active = snapshot
            .game
            .as_ref()
            .is_some_and(|game| matches!(game.phase.as_str(), "bidding" | "playing" | "scoring"));
        let count = deck_count(snapshot);
        let height = f32::from(count) * 0.00045;
        // The public deck sits clear of the bowl. It represents undealt count,
        // not the private shuffled order or a second source of card identities.
        let center = Vec3::new(-0.21, 0.021 + height * 0.5, -0.23);
        let root = self
            .commands
            .spawn((
                WorldPresentation,
                WorldPanel {
                    kind: PanelKind::Deck,
                    half_size: Vec2::new(0.035, 0.048),
                },
                Transform::from_translation(center + Vec3::Y * height * 0.5)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::default(),
            ))
            .id();
        let stack = self
            .commands
            .spawn((
                Mesh3d(self.meshes.add(Cuboid::new(0.064, 0.088, height))),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: Color::srgb(0.19, 0.27, 0.42),
                    perceptual_roughness: 0.85,
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.0, -height * 0.5),
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(stack);
        if let Some(trump) = snapshot
            .game
            .as_ref()
            .and_then(|game| game.trump)
            .filter(|_| active)
        {
            let card = self
                .commands
                .spawn((
                    Mesh3d(self.meshes.add(Cuboid::new(0.064, 0.002, 0.088))),
                    MeshMaterial3d(self.materials.add(StandardMaterial {
                        base_color: Color::srgb(0.94, 0.92, 0.80),
                        ..default()
                    })),
                    Transform::from_xyz(0.0, 0.0, 0.0015)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    RenderLayers::layer(0),
                ))
                .id();
            self.commands.entity(root).add_child(card);
            hand_view::spawn_card_face(
                self.commands,
                card,
                self.meshes.add(Plane3d::default()),
                &public_card_label(trump),
                self.images,
                self.materials,
                0,
            );
        }
    }

    fn score_sheet(&mut self, target: SheetInspectionTarget, snapshot: &ClientSnapshot) {
        let root = self
            .commands
            .spawn((
                WorldPresentation,
                WorldPanel {
                    kind: PanelKind::ScoreSheet,
                    half_size: target.size * 0.5,
                },
                Transform::from_translation(target.center)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::default(),
            ))
            .id();
        let paper = self
            .commands
            .spawn((
                Mesh3d(
                    self.meshes
                        .add(Cuboid::new(target.size.x, target.size.y, 0.0006)),
                ),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: Color::srgb(0.93, 0.90, 0.76),
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::default(),
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(paper);
        self.text(
            root,
            "POCHE · SCORE SHEET",
            Vec3::new(0.0, target.size.y * 0.425, 0.0006),
            Vec2::new(target.size.x * 0.9, 0.010),
            [30, 31, 27],
        );
        let rows = score_sheet_cells(snapshot);
        let first = sheet_cell_bounds(target.size, 0, 0, rows.len());
        let last = sheet_cell_bounds(target.size, rows.len() - 1, 4, rows.len());
        let ink = self.materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.12, 0.10),
            unlit: true,
            ..default()
        });
        for row in 0..=rows.len() {
            let y = first.max.y - (first.max.y - last.min.y) * row as f32 / rows.len() as f32;
            self.sheet_rule(
                root,
                Vec2::new(0.0, y),
                Vec2::new(last.max.x - first.min.x, 0.00022),
                ink.clone(),
            );
        }
        for column in 0..=SHEET_COLUMNS.len() {
            let x = if column == SHEET_COLUMNS.len() {
                last.max.x
            } else {
                sheet_cell_bounds(target.size, 0, column, rows.len()).min.x
            };
            self.sheet_rule(
                root,
                Vec2::new(x, (first.max.y + last.min.y) * 0.5),
                Vec2::new(0.00022, first.max.y - last.min.y),
                ink.clone(),
            );
        }
        let font_height = SHEET_FONT_HEIGHT.min(target.size.y * 0.60 / rows.len() as f32 * 0.65);
        for (row, cells) in rows.iter().enumerate() {
            for (column, value) in cells.iter().enumerate() {
                let bounds = sheet_cell_bounds(target.size, row, column, rows.len());
                let center = bounds.center();
                let cell = self
                    .commands
                    .spawn((
                        ScoreSheetCell,
                        Transform::from_xyz(center.x, center.y, 0.0006),
                        Visibility::default(),
                    ))
                    .id();
                self.commands.entity(root).add_child(cell);
                let width = bounds.width() - 0.003;
                let maximum_characters = (1..=64_usize)
                    .take_while(|count| *count as f32 * font_height * 0.6 <= width)
                    .last()
                    .unwrap_or(1);
                let label = cell_label(value, maximum_characters);
                self.text(
                    cell,
                    &label,
                    Vec3::ZERO,
                    Vec2::new(width, font_height),
                    [30, 31, 27],
                );
            }
        }
        let pot: u32 = snapshot
            .coins
            .iter()
            .filter(|coin| coin.container == "bowl")
            .map(|coin| u32::from(coin.denomination_cents))
            .sum();
        let phase = format!(
            "Bowl ${}.{:02} · due {}¢ / {}¢",
            pot / 100,
            pot % 100,
            snapshot.payment_due(0),
            snapshot.payment_due(1)
        );
        for (line, label) in [
            phase.as_str(),
            "● missed · 1N made · 2N all tricks",
            "Totals are recorded authority points",
        ]
        .iter()
        .enumerate()
        {
            self.text(
                root,
                label,
                Vec3::new(0.0, -target.size.y * (0.335 + line as f32 * 0.045), 0.0006),
                Vec2::new(target.size.x * 0.9, 0.005),
                [30, 31, 27],
            );
        }
    }

    fn sheet_rule(
        &mut self,
        root: Entity,
        center: Vec2,
        size: Vec2,
        material: Handle<StandardMaterial>,
    ) {
        let rule = self
            .commands
            .spawn((
                ScoreSheetRule,
                Mesh3d(self.meshes.add(Cuboid::new(size.x, size.y, 0.00012))),
                MeshMaterial3d(material),
                Transform::from_xyz(center.x, center.y, 0.00045),
                NotShadowCaster,
                NotShadowReceiver,
                Pickable::IGNORE,
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(rule);
    }

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
        let background = self
            .commands
            .spawn((
                NameTagBackdrop,
                Mesh3d(self.meshes.add(Cuboid::new(width + 0.020, 0.052, 0.002))),
                MeshMaterial3d(self.materials.add(StandardMaterial {
                    base_color: Color::srgb_u8(
                        NAME_TAG_BACKGROUND[0],
                        NAME_TAG_BACKGROUND[1],
                        NAME_TAG_BACKGROUND[2],
                    ),
                    unlit: true,
                    ..default()
                })),
                Transform::default(),
                NotShadowCaster,
                NotShadowReceiver,
                RenderLayers::layer(0),
            ))
            .id();
        self.commands.entity(root).add_child(background);
        self.text(
            root,
            label,
            Vec3::new(0.0, 0.0, 0.0015),
            Vec2::new(width, 0.035),
            color,
        );
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
                Pickable::IGNORE,
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
        PanelKind::Deck => lines.extend(next_step(snapshot)),
        PanelKind::Door => lines.push("Click the door twice to leave lobby.".into()),
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

fn public_card_label(code: u8) -> String {
    const RANKS: [&str; 13] = [
        "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A",
    ];
    const SUITS: [&str; 4] = ["♣", "♦", "♥", "♠"];
    if code >= 52 {
        return "?".into();
    }
    format!(
        "{}{}",
        RANKS[usize::from(code % 13)],
        SUITS[usize::from(code / 13)]
    )
}

fn next_step(snapshot: &ClientSnapshot) -> Vec<String> {
    if snapshot
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .count()
        < 2
    {
        return vec!["Take both seats to begin.".into()];
    }
    let due = [snapshot.payment_due(0), snapshot.payment_due(1)];
    if snapshot
        .game
        .as_ref()
        .is_none_or(|game| game.phase == "scoring")
        && due.iter().any(|value| *value > 0)
    {
        return due
            .iter()
            .enumerate()
            .filter(|(_, amount)| **amount > 0)
            .map(|(seat, amount)| {
                format!(
                    "{}: move {amount}¢ into the bowl.",
                    seat_name(snapshot, seat as u8)
                )
            })
            .collect();
    }
    let Some(game) = &snapshot.game else {
        return vec!["Waiting for the first deal.".into()];
    };
    match game.phase.as_str() {
        "awaiting-deal" => vec![format!(
            "{}: click the deck to shuffle and deal round {}.",
            game.dealer_seat
                .map_or_else(|| "Dealer".into(), |seat| seat_name(snapshot, seat)),
            game.round_index + 1
        )],
        "bidding" => vec![format!(
            "{}: use Speech to announce a bid.",
            game.actor_seat
                .map_or_else(|| "Next player".into(), |seat| seat_name(snapshot, seat))
        )],
        "playing" => vec![format!(
            "{}: place a legal card fully inside PLAY, then release.",
            game.actor_seat
                .map_or_else(|| "Next player".into(), |seat| seat_name(snapshot, seat))
        )],
        "scoring" => vec!["Payments received. Preparing the next round.".into()],
        "finished" => vec![
            "Game complete. Scores are on the sheet.".into(),
            "The bowl is held pending payout.".into(),
        ],
        _ => vec![format!("Authority phase: {}", game.phase)],
    }
}

fn score_sheet_lines(snapshot: &ClientSnapshot) -> Vec<String> {
    // Plain text is useful for diagnostics; the physical paper renders cells
    // and ruled geometry, never independently scaled lines of pipe characters.
    score_sheet_cells(snapshot)
        .iter()
        .map(|row| row.join(" | "))
        .collect()
}

fn score_sheet_cells(snapshot: &ClientSnapshot) -> Vec<[String; 5]> {
    let mut rows = vec![[
        "Round".into(),
        "Dealer".into(),
        "Cards".into(),
        seat_name(snapshot, 0),
        seat_name(snapshot, 1),
    ]];
    let mut totals = [0_u16, 0];
    for round in &snapshot.rounds {
        rows.push([
            (round.round_index + 1).to_string(),
            seat_name(snapshot, round.dealer_seat),
            round.hand_size.to_string(),
            score_cell(round.bids[0], round.tricks_won[0], round.hand_size),
            score_cell(round.bids[1], round.tricks_won[1], round.hand_size),
        ]);
        totals = round.totals;
    }
    if let Some(game) = &snapshot.game {
        let settled = matches!(game.phase.as_str(), "scored" | "finished" | "scoring")
            && game.hand_counts == [0, 0];
        let cells = [0_usize, 1].map(|index| match game.bids[index] {
            None => "—".into(),
            Some(bid) if !settled => bid.to_string(),
            Some(bid) => score_cell(bid, game.tricks_won[index], game.hand_size),
        });
        if game.phase != "finished"
            && !snapshot
                .rounds
                .iter()
                .any(|round| round.round_index == game.round_index)
        {
            rows.push([
                format!("{}", game.round_index + 1),
                game.dealer_seat
                    .map_or_else(|| "—".into(), |seat| seat_name(snapshot, seat)),
                game.hand_size.to_string(),
                cells[0].clone(),
                cells[1].clone(),
            ]);
        }
        totals = game.scores;
    } else {
        rows.push(std::array::from_fn(|_| String::new()));
    }
    // Preserve every authority-recorded round; blank writing space does not
    // invent past or future results. Ink shrinks uniformly for longer sheets.
    rows.extend((rows.len()..7).map(|_| std::array::from_fn(|_| String::new())));
    rows.push([
        "Total".into(),
        "recorded".into(),
        String::new(),
        totals[0].to_string(),
        totals[1].to_string(),
    ]);
    rows
}

fn sheet_cell_bounds(paper: Vec2, row: usize, column: usize, rows: usize) -> Rect {
    let width = paper.x * 0.90;
    let left = -width * 0.5 + width * SHEET_COLUMNS[..column].iter().sum::<f32>();
    let top = paper.y * 0.32;
    let row_height = paper.y * 0.60 / rows as f32;
    Rect::from_corners(
        Vec2::new(left, top - (row + 1) as f32 * row_height),
        Vec2::new(
            left + width * SHEET_COLUMNS[column],
            top - row as f32 * row_height,
        ),
    )
}

fn cell_label(text: &str, maximum: usize) -> String {
    if text.chars().count() <= maximum {
        text.into()
    } else {
        format!(
            "{}…",
            text.chars()
                .take(maximum.saturating_sub(1))
                .collect::<String>()
        )
    }
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
    if !snapshot
        .members
        .iter()
        .any(|member| member.seat == Some(seat))
    {
        return None;
    }
    let due = snapshot.payment_due(seat);
    if snapshot.game.is_none() {
        return (due > 0).then(|| format!("I still owe my {due}¢ ante. Into the bowl!"));
    }
    let game = snapshot.game.as_ref()?;
    if game.phase == "scoring" {
        return (due > 0).then(|| format!("I missed my bid. I owe {due}¢ to the bowl."));
    }
    if game.phase == "awaiting-deal" {
        return (game.dealer_seat == Some(seat)).then(|| "My deal! I'll click the deck.".into());
    }
    if game.phase == "finished" {
        return (game.dealer_seat == Some(seat)).then(|| {
            "The game is over. The scores are on the sheet; payout is still pending.".into()
        });
    }
    if game.phase == "playing" {
        return (game.actor_seat == Some(seat)).then(|| "Hmm, which card should I play?".into());
    }
    if game.phase != "bidding" {
        return None;
    }
    let accepted_bid = game
        .bids
        .get(usize::from(seat))
        .copied()
        .flatten()
        .map(bid_phrase);
    // The scorekeeper role has not yet been elected in the authority schema.
    // The dealer voices the mechanical prompt; this grants no new permission.
    if game.actor_seat == Some(seat) {
        return Some("Hmm, how many tricks should I bid?".into());
    }
    if game.dealer_seat == Some(seat)
        && let Some(actor) = game.actor_seat
    {
        let question = format!("{}, how many tricks?", seat_name(snapshot, actor));
        return Some(accepted_bid.map_or(question.clone(), |bid| format!("{bid}. {question}")));
    }
    accepted_bid
}

fn bid_phrase(tricks: u8) -> String {
    format!(
        "I bid {tricks} {}",
        if tricks == 1 { "trick" } else { "tricks" }
    )
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
            if snapshot
                .game
                .as_ref()
                .is_some_and(|game| game.phase == "bidding")
            {
                small_button(
                    root,
                    if may_bid(snapshot, 0) {
                        "Speech · your bid"
                    } else {
                        "Speech · bids"
                    },
                    WorldUiAction::ToggleSpeech,
                );
            }
            if interaction.speech_open
                && snapshot
                    .game
                    .as_ref()
                    .is_some_and(|game| game.phase == "bidding")
            {
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
    use poche_spacetimedb_client::{CoinView, GameView, HandCardView, MemberView, RoomView};
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
            coins: vec![ante_coin(0), ante_coin(1)],
            ..default()
        }
    }

    fn ante_coin(seat: u8) -> CoinView {
        CoinView {
            coin_key: format!("quarter-{seat}"),
            coin_id: "quarter".into(),
            owner: if seat == 0 { "one" } else { "two" }.into(),
            owner_seat: Some(seat),
            is_own: seat == 0,
            denomination_cents: 25,
            container: "bowl".into(),
            position_mm: [0, 25, 0],
            sequence: 1,
        }
    }

    #[test]
    fn seated_players_voice_only_their_own_unpaid_ante_or_penalty() {
        let mut snapshot = snapshot();
        snapshot.game = None;
        snapshot.coins.clear();
        assert_eq!(
            player_speech(&snapshot, 0).as_deref(),
            Some("I still owe my 25¢ ante. Into the bowl!")
        );
        assert_eq!(
            player_speech(&snapshot, 1).as_deref(),
            Some("I still owe my 25¢ ante. Into the bowl!")
        );
        snapshot.coins.push(ante_coin(0));
        assert_eq!(player_speech(&snapshot, 0), None);
        assert!(player_speech(&snapshot, 1).unwrap().contains("25¢ ante"));
        snapshot.members[1].seat = None;
        assert_eq!(player_speech(&snapshot, 1), None);

        let mut snapshot = self::snapshot();
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "scoring".into();
        game.bids = [Some(0), Some(0)];
        game.tricks_won = [1, 0];
        assert_eq!(
            player_speech(&snapshot, 0).as_deref(),
            Some("I missed my bid. I owe 10¢ to the bowl.")
        );
        assert_eq!(player_speech(&snapshot, 1), None);
        let mut dime = ante_coin(0);
        dime.coin_key = "dime-0".into();
        dime.denomination_cents = 10;
        snapshot.coins.push(dime);
        assert_eq!(player_speech(&snapshot, 0), None);
        snapshot.game.as_mut().unwrap().phase = "awaiting-deal".into();
        snapshot.game.as_mut().unwrap().dealer_seat = Some(1);
        assert_eq!(player_speech(&snapshot, 0), None);
        assert_eq!(
            player_speech(&snapshot, 1).as_deref(),
            Some("My deal! I'll click the deck.")
        );
    }

    #[test]
    fn deck_quantity_is_hover_context_not_an_always_present_name_tag() {
        let mut snapshot = snapshot();
        assert!(deck_hover(&snapshot).contains("49 cards in deck"));
        assert!(deck_hover(&snapshot).contains("Trump: 4♣"));
        assert!(!deck_hover(&snapshot).contains("secret-"));
        snapshot.game = None;
        assert!(deck_hover(&snapshot).starts_with("52 cards in deck"));
        let mut app = presentation_app(snapshot);
        app.update();
        let mut names = app
            .world_mut()
            .query_filtered::<Entity, With<NameTagBackdrop>>();
        assert_eq!(names.iter(app.world()).count(), 2);
    }

    #[test]
    fn only_your_own_seat_can_stand_you_up_and_only_empty_seats_can_seat_you() {
        let mut snapshot = snapshot();
        assert!(matches!(
            seat_action(&snapshot, 0),
            Some(UiAction::ReleaseSeat)
        ));
        assert_eq!(seat_hover(&snapshot, 0), "Stand up · releases your seat");
        assert!(seat_action(&snapshot, 1).is_none());
        snapshot.members[0].seat = None;
        assert!(matches!(
            seat_action(&snapshot, 0),
            Some(UiAction::TakeSeat(0))
        ));
        assert!(seat_action(&snapshot, 1).is_none());
        assert_eq!(seat_hover(&snapshot, 0), "Take seat 1");
    }

    #[test]
    fn physical_door_uses_shared_leave_action_and_changes_confirmation_hint() {
        let mut app = presentation_app(snapshot());
        app.update();
        let mut doors = app.world_mut().query::<(&WorldActionRelay, &UiAction)>();
        let door_actions = doors
            .iter(app.world())
            .filter(|(relay, _)| **relay == WorldActionRelay::Leave)
            .collect::<Vec<_>>();
        assert_eq!(door_actions.len(), 1);
        assert!(matches!(door_actions[0].1, UiAction::Leave));
        assert_eq!(door_hover(false), "Leave lobby · click the door");
        assert!(door_hover(true).contains("again"));
        assert!(door_hover(true).contains("seat will be released"));
        let mut panels = app.world_mut().query::<(&WorldPanel, &Transform)>();
        let (door, transform) = panels
            .iter(app.world())
            .find(|(panel, _)| panel.kind == PanelKind::Door)
            .unwrap();
        assert_eq!(transform.translation, room_geometry::door_center());
        assert_eq!(
            door.half_size,
            room_geometry::DOOR_LEAF_SIZE.truncate() * 0.5
        );
    }

    #[test]
    fn queued_world_actions_survive_a_public_surface_rebuild() {
        let mut app = presentation_app(snapshot());
        app.update();
        let mut relays = app
            .world_mut()
            .query_filtered::<Entity, With<WorldActionRelay>>();
        let queued = relays.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(queued.len(), 4);
        let mut panels = app.world_mut().query_filtered::<Entity, With<WorldPanel>>();
        let old_panels = panels.iter(app.world()).collect::<Vec<_>>();
        app.world_mut()
            .resource_mut::<BridgeModel>()
            .snapshot
            .game
            .as_mut()
            .unwrap()
            .bids[1] = Some(0);
        app.update();
        assert_ne!(panels.iter(app.world()).collect::<Vec<_>>(), old_panels);
        assert_eq!(relays.iter(app.world()).collect::<Vec<_>>(), queued);
        for entity in queued {
            assert!(app.world().get::<UiAction>(entity).is_some());
        }
    }

    #[test]
    fn top_down_door_pick_hits_its_actual_thickness_not_a_parallel_plane() {
        let transform = GlobalTransform::from_translation(door_center());
        let ray = Ray3d::new(door_center() + Vec3::Y, Dir3::NEG_Y);
        let half_size = room_geometry::DOOR_LEAF_SIZE * 0.5;
        assert!(intersect_panel(ray, &transform, half_size.truncate()).is_none());
        assert!((intersect_box(ray, &transform, half_size).unwrap() - 0.56).abs() < 0.000_001);
        let front = Ray3d::new(door_center() + Vec3::Z, Dir3::NEG_Z);
        let back = Ray3d::new(door_center() - Vec3::Z, Dir3::Z);
        assert!(intersect_box(front, &transform, half_size).is_some());
        assert!(intersect_box(back, &transform, half_size).is_some());
        let miss = Ray3d::new(door_center() + Vec3::new(0.201, 1., 0.), Dir3::NEG_Y);
        assert!(intersect_box(miss, &transform, half_size).is_none());
    }

    #[test]
    fn avatar_blocks_the_hidden_stool_centre_but_not_its_exposed_rim() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 2).unwrap()).unwrap();
        for seat in layout.seats() {
            let center = point_to_world(seat.seat_pose.translation);
            let avatar = point_to_world(seat.player_pose.translation);
            let centre_ray = Ray3d::new(center + Vec3::Y, Dir3::NEG_Y);
            let seat_distance = centre_ray
                .intersect_plane(center + Vec3::Y * 0.06, InfinitePlane3d::new(Vec3::Y))
                .unwrap();
            assert!(avatar_hit(centre_ray, avatar).unwrap() < seat_distance);
            let rim = seat_pick_center(&layout, seat.seat.get()).unwrap();
            assert!(
                (Vec2::new(rim.x - center.x, rim.z - center.z).length() - 0.19).abs() < 0.000_001
            );
            assert!(avatar_hit(Ray3d::new(rim + Vec3::Y, Dir3::NEG_Y), avatar).is_none());
        }
    }

    fn presentation_app(mut snapshot: ClientSnapshot) -> App {
        snapshot.rooms.push(RoomView {
            room_id: "public-room".into(),
        });
        let mut app = App::new();
        app.insert_resource(BridgeModel {
            snapshot,
            ..default()
        })
        .insert_resource(UiState {
            screen: UiScreen::Table,
            room_scene_generation: 1,
            ..default()
        })
        .insert_resource(CanonicalLayout(
            registered_layout(TableId::new(1), LayoutId::new(2, 1).unwrap()).unwrap(),
        ))
        .init_resource::<WorldInteraction>()
        .init_resource::<SheetInspectionRequest>()
        .init_resource::<PresentationCache>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<Image>>()
        .add_systems(Startup, spawn_world_action_relays)
        .add_systems(Update, sync_world_presentation);
        app
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
        assert_eq!(
            player_speech(&snapshot, 1).as_deref(),
            Some("Hmm, how many tricks should I bid?")
        );
    }

    #[test]
    fn scoresheet_replaces_bids_only_after_play_is_complete() {
        let mut snapshot = snapshot();
        let game = snapshot.game.as_mut().unwrap();
        game.bids = [Some(0), Some(1)];
        assert_eq!(&score_sheet_cells(&snapshot)[1][3..], ["0", "1"]);
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "scored".into();
        game.hand_counts = [0, 0];
        game.tricks_won = [0, 1];
        game.scores = [10, 21];
        let cells = score_sheet_cells(&snapshot);
        assert_eq!(&cells[1][3..], ["10", "21"]);
        assert_eq!(&cells.last().unwrap()[3..], ["10", "21"]);
        snapshot.game.as_mut().unwrap().tricks_won = [1, 0];
        assert_eq!(&score_sheet_cells(&snapshot)[1][3..], ["●", "●"]);
    }

    #[test]
    fn scoresheet_keeps_history_and_does_not_duplicate_current_scored_row() {
        let mut snapshot = snapshot();
        snapshot.rounds.push(poche_spacetimedb_client::RoundView {
            round_index: 0,
            dealer_seat: 0,
            hand_size: 1,
            bids: [0, 0],
            tricks_won: [1, 0],
            points: [0, 10],
            totals: [0, 10],
            payment_cents: [10, 0],
        });
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "scoring".into();
        game.scores = [0, 10];
        let rows = score_sheet_cells(&snapshot);
        assert_eq!(rows.iter().filter(|row| row[0] == "1").count(), 1);
        assert_eq!(&rows[1][3..], ["●", "10"]);
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "awaiting-deal".into();
        game.round_index = 1;
        game.dealer_seat = Some(1);
        game.hand_size = 2;
        let rows = score_sheet_cells(&snapshot);
        assert_eq!(&rows[1][3..], ["●", "10"]);
        assert_eq!(rows[2][1], "Bob");
        assert_eq!(&rows.last().unwrap()[3..], ["0", "10"]);
        assert!(
            next_step(&snapshot)
                .join(" ")
                .contains("Bob: click the deck")
        );
    }

    #[test]
    fn next_step_explains_payment_instead_of_implying_a_bid_turn() {
        let mut snapshot = snapshot();
        snapshot.game.as_mut().unwrap().phase = "scoring".into();
        snapshot.game.as_mut().unwrap().bids = [Some(0), Some(0)];
        snapshot.game.as_mut().unwrap().tricks_won = [1, 0];
        let text = next_step(&snapshot).join(" ");
        assert!(text.contains("Alice: move"));
        assert!(text.contains("into the bowl"));
        assert!(!text.contains("Speech"));
        assert_eq!(public_card_label(8), "10♣");
        assert_eq!(public_card_label(51), "A♠");
        assert_eq!(deck_count(&snapshot), 49);
    }

    #[test]
    fn finished_scoresheet_does_not_invent_a_fourteenth_round() {
        let mut snapshot = snapshot();
        for index in 0..13 {
            snapshot.rounds.push(poche_spacetimedb_client::RoundView {
                round_index: index,
                dealer_seat: (index % 2) as u8,
                hand_size: 1,
                bids: [0, 0],
                tricks_won: [1, 0],
                points: [0, 10],
                totals: [0, (index + 1) * 10],
                payment_cents: [10, 0],
            });
        }
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "finished".into();
        game.round_index = 13;
        game.hand_size = 0;
        game.scores = [0, 130];
        let rows = score_sheet_cells(&snapshot);
        assert_eq!(rows.len(), 15); // header, 13 real rounds, total
        assert!(!rows.iter().any(|row| row[0] == "14"));
    }

    #[test]
    fn speech_thinks_in_first_person_and_does_not_repeat_old_bids_during_play() {
        let mut snapshot = snapshot();
        snapshot.game.as_mut().unwrap().actor_seat = Some(0);
        snapshot.game.as_mut().unwrap().bids[1] = Some(1);
        assert_eq!(
            player_speech(&snapshot, 0).as_deref(),
            Some("Hmm, how many tricks should I bid?")
        );
        assert_eq!(
            player_speech(&snapshot, 1).as_deref(),
            Some("I bid 1 trick")
        );
        let game = snapshot.game.as_mut().unwrap();
        game.bids[0] = Some(0);
        game.phase = "playing".into();
        game.actor_seat = Some(1);
        assert_eq!(player_speech(&snapshot, 0), None);
        assert_eq!(
            player_speech(&snapshot, 1).as_deref(),
            Some("Hmm, which card should I play?")
        );
        let game = snapshot.game.as_mut().unwrap();
        game.phase = "scoring".into();
        game.tricks_won = [0, 1];
        assert_eq!(player_speech(&snapshot, 0), None);
        assert_eq!(player_speech(&snapshot, 1), None);
    }

    #[test]
    fn clicking_score_sheet_requests_real_paper_inspection_without_a_gui_reader() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 2).unwrap()).unwrap();
        let mut interaction = WorldInteraction::default();
        let mut inspection = SheetInspectionRequest::default();
        open_panel(
            PanelKind::ScoreSheet,
            &layout,
            &mut interaction,
            &mut inspection,
        );
        assert_eq!(interaction.reading, None);
        assert!(!interaction.modal_open());
        let target = inspection.pending.unwrap();
        let paper = layout.score_sheet();
        assert!((target.center.x - point_to_world(paper.pose.translation).x).abs() < 0.000_001);
        assert!((target.center.y - 0.026).abs() < 0.000_001);
        assert!((target.size.x - 0.20).abs() < 0.000_001);
        assert!((target.size.y - 0.29).abs() < 0.000_001);
        open_panel(
            PanelKind::Players,
            &layout,
            &mut interaction,
            &mut inspection,
        );
        assert_eq!(interaction.reading, Some(PanelKind::Players));
    }

    #[test]
    fn rendered_scoresheet_has_aligned_cells_fixed_ink_size_and_real_rules() {
        let mut snapshot = snapshot();
        snapshot.members[0].display_name = "A very long identity name".into();
        let row_count = score_sheet_cells(&snapshot).len();
        let mut app = presentation_app(snapshot);
        app.update();
        let mut cells = app
            .world_mut()
            .query_filtered::<(&Transform, Option<&Children>), With<ScoreSheetCell>>();
        let before = cells
            .iter(app.world())
            .map(|(transform, _)| transform.translation)
            .collect::<Vec<_>>();
        assert_eq!(before.len(), row_count * SHEET_COLUMNS.len());
        for (_, children) in cells.iter(app.world()) {
            if let Some(child) = children.and_then(|children| children.first()) {
                let ink = app.world().get::<Transform>(*child).unwrap();
                assert!(
                    (ink.scale.z - SHEET_FONT_HEIGHT).abs() < 0.000_001,
                    "cell text must not be independently rescaled: {}",
                    ink.scale.z
                );
            }
        }
        let mut rules = app
            .world_mut()
            .query_filtered::<Entity, With<ScoreSheetRule>>();
        assert_eq!(
            rules.iter(app.world()).count(),
            row_count + SHEET_COLUMNS.len() + 2
        );
        let paper = app.world().resource::<CanonicalLayout>().0.clone();
        let size = sheet_inspection_target(&paper).size;
        for row in 0..row_count {
            for column in 0..SHEET_COLUMNS.len() {
                let bounds = sheet_cell_bounds(size, row, column, row_count);
                assert!(
                    before.iter().any(
                        |position| (position.truncate() - bounds.center()).length() < 0.000_001
                    )
                );
                if column + 1 < SHEET_COLUMNS.len() {
                    let next = sheet_cell_bounds(size, row, column + 1, row_count);
                    assert!((bounds.max.x - next.min.x).abs() < 0.000_001);
                }
            }
        }
        app.world_mut()
            .resource_mut::<BridgeModel>()
            .snapshot
            .members[0]
            .display_name = "Jo".into();
        app.update();
        assert_eq!(
            cells
                .iter(app.world())
                .map(|(transform, _)| transform.translation)
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn rendered_names_have_opaque_unlit_backdrops_with_accessible_contrast() {
        let mut app = presentation_app(snapshot());
        app.update();
        let mut backgrounds = app
            .world_mut()
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<NameTagBackdrop>>();
        // Counts are hover context; only the two player names stay visible.
        assert_eq!(backgrounds.iter(app.world()).count(), 2);
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        for handle in backgrounds.iter(app.world()) {
            let material = materials.get(&handle.0).unwrap();
            assert_eq!(material.alpha_mode, AlphaMode::Opaque);
            assert!(material.unlit);
            assert_eq!(
                material.base_color,
                Color::srgb_u8(
                    NAME_TAG_BACKGROUND[0],
                    NAME_TAG_BACKGROUND[1],
                    NAME_TAG_BACKGROUND[2]
                )
            );
        }
        let luminance = |rgb: [u8; 3]| {
            let linear = rgb.map(|channel| {
                let value = f32::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
        };
        assert!(
            (luminance(NAME_TAG_FOREGROUND) + 0.05) / (luminance(NAME_TAG_BACKGROUND) + 0.05)
                >= 4.5
        );
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
        let mut app = presentation_app(snapshot());
        app.update();
        let mut panels = app.world_mut().query_filtered::<Entity, With<WorldPanel>>();
        let before = panels.iter(app.world()).collect::<Vec<_>>();
        let images_before = app.world().resource::<Assets<Image>>().len();
        assert_eq!(before.len(), 6); // three notices, paper, deck and door
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
