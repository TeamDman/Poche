// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Private hand projection. Copies are presentation entities, never new cards.
use super::{
    CARD_WORLD_HEIGHT, CARD_WORLD_THICKNESS, CARD_WORLD_WIDTH, DragState, PocheUiCamera,
    PoseDisplay, SpatialAssets, UiScreen, UiState, animate_and_place_cards, card_label_texture,
    mm_position, point_to_world,
};
use bevy::{camera::visibility::RenderLayers, prelude::*, render::render_resource::Face};
use poche_bevy_spacetimedb::BridgeModel;
use poche_spatial::{ObjectId, SpatialLayout, ZoneId};
use std::collections::HashSet;

pub(super) const VIEW_HEIGHT: f32 = 0.14;
const HAND_LIFT_MM: f32 = 24.0;
const OUTLINE_WIDTH: f32 = 0.0015;
const BOUNDS_EPSILON: f32 = 0.000_001;

pub(super) struct HandViewPlugin;
impl Plugin for HandViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HandProjection>().add_systems(
            Update,
            (
                sync_hand_copies,
                inspect_zones,
                update_outlines,
                hand_drop_hint,
                play_drop_hint,
                observe_hand_input,
            )
                .chain()
                .after(animate_and_place_cards),
        );
    }
}

#[derive(Component)]
pub(super) struct HandCardVisual {
    pub key: String,
}

#[derive(Component)]
pub(super) struct DiagnosticZone(pub ObjectId);

#[derive(Resource, Default)]
pub(super) struct HandProjection {
    pub center: Vec3,
    pub horizontal_scale: f32,
    pub has_hand: bool,
    min: Vec3,
    max: Vec3,
}

impl HandProjection {
    /// One physical hand footprint drives visibility, the drop target and its
    /// indicator. Camera scale only changes presentation, never the world zone.
    pub fn configure(
        &mut self,
        layout: &SpatialLayout,
        seat: Option<u8>,
        count: usize,
        surface_size: UVec2,
        viewport_size: UVec2,
    ) {
        let Some(zone) = layout
            .zones()
            .iter()
            .find(|zone| matches!(zone.id, ZoneId::Hand(id) if Some(id.get()) == seat))
        else {
            *self = Self::default();
            return;
        };
        self.min = point_to_world(zone.inner.min);
        self.max = point_to_world(zone.inner.max);
        self.center = (self.min + self.max) * 0.5;
        // Keep the target centered and comfortably wide. A single transform
        // maps both its edges and every card center, and does not change as a
        // card is played. Individual card meshes retain their original size.
        let view_width = VIEW_HEIGHT * viewport_size.x as f32 / viewport_size.y.max(1) as f32;
        let target_fraction = surface_size.x as f32 * 0.5 / viewport_size.x.max(1) as f32;
        self.horizontal_scale = view_width * target_fraction / (self.max.x - self.min.x);
        self.has_hand = count > 0;
    }

    pub fn to_inset(&self, world: Vec3) -> Vec3 {
        Vec3::new(
            self.center.x + (world.x - self.center.x) * self.horizontal_scale,
            world.y,
            world.z,
        )
    }
    pub fn to_world(&self, inset: Vec3) -> Vec3 {
        Vec3::new(
            self.center.x + (inset.x - self.center.x) / self.horizontal_scale.max(0.01),
            inset.y,
            inset.z,
        )
    }
    pub fn contains(&self, world: Vec3) -> bool {
        self.has_hand
            && world.x >= self.min.x - BOUNDS_EPSILON
            && world.x <= self.max.x + BOUNDS_EPSILON
            && world.z >= self.min.z - BOUNDS_EPSILON
            && world.z <= self.max.z + BOUNDS_EPSILON
    }

    /// The hand is a horizontal interaction footprint while a card is lifted.
    /// Its Y coordinate remains free; a grab offset cannot push the center out
    /// of that footprint while the pointer still lies inside the drop target.
    pub fn clamp_world(&self, world: Vec3) -> Vec3 {
        Vec3::new(
            world.x.clamp(self.min.x, self.max.x),
            world.y,
            world.z.clamp(self.min.z, self.max.z),
        )
    }

    pub fn screen_rect(&self, camera: &Camera, transform: &GlobalTransform) -> Option<Rect> {
        if !self.has_hand || !camera.is_active {
            return None;
        }
        let viewport = camera.logical_viewport_rect()?;
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for x in [self.min.x, self.max.x] {
            for z in [self.min.z, self.max.z] {
                let point = camera
                    .world_to_viewport(transform, self.to_inset(Vec3::new(x, self.center.y, z)))
                    .ok()?;
                min = min.min(point);
                max = max.max(point);
            }
        }
        let min = min.max(viewport.min);
        let max = max.min(viewport.max);
        (min.x <= max.x && min.y <= max.y).then_some(Rect::from_corners(min, max))
    }

    pub fn contains_cursor(
        &self,
        cursor: Vec2,
        camera: &Camera,
        transform: &GlobalTransform,
    ) -> bool {
        self.screen_rect(camera, transform)
            .is_some_and(|rect| rect.contains(cursor))
    }
}

pub(super) fn lift_height(resting: f32) -> f32 {
    resting + HAND_LIFT_MM
}

fn sync_hand_copies(
    mut commands: Commands,
    state: Res<UiState>,
    model: Res<BridgeModel>,
    projection: Res<HandProjection>,
    poses: Res<PoseDisplay>,
    assets: Res<SpatialAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut copies: Query<(Entity, &HandCardVisual, &mut Transform, &mut Visibility)>,
) {
    let wanted: HashSet<_> = model
        .snapshot
        .hand
        .iter()
        .map(|c| c.card_key.as_str())
        .collect();
    let mut existing = HashSet::new();
    for (entity, card, mut transform, mut visibility) in &mut copies {
        if state.screen != UiScreen::Table || !wanted.contains(card.key.as_str()) {
            commands.entity(entity).despawn();
            continue;
        }
        existing.insert(card.key.clone());
        if let Some(pose) = poses.0.get(&card.key) {
            let world = mm_position(pose.current);
            *visibility = if projection.contains(world) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            transform.translation =
                projection.to_inset(world) + Vec3::Y * stack_offset(&poses, &card.key);
            transform.rotation = pose.current_rotation;
        }
    }
    if state.screen != UiScreen::Table {
        return;
    }
    for card in &model.snapshot.hand {
        if existing.contains(&card.card_key) {
            continue;
        }
        let Some(pose) = poses.0.get(&card.card_key) else {
            continue;
        };
        let entity = commands
            .spawn((
                HandCardVisual {
                    key: card.card_key.clone(),
                },
                Mesh3d(assets.card_mesh.clone()),
                MeshMaterial3d(assets.card_face_material.clone()),
                Transform::from_translation(projection.to_inset(mm_position(pose.current)))
                    .with_rotation(pose.current_rotation),
                RenderLayers::layer(1),
                bevy::light::NotShadowCaster,
                if projection.contains(mm_position(pose.current)) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            ))
            .id();
        spawn_card_face(
            &mut commands,
            entity,
            assets.card_label_mesh.clone(),
            &card.face,
            &mut images,
            &mut materials,
            1,
        );
        spawn_outline(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            &card.card_key,
            1,
        );
    }
}

#[derive(Component)]
struct CardOutline(String);

fn face_parts(face: &str) -> Vec<&str> {
    let Some((index, suit)) = face.char_indices().last() else {
        return vec![face];
    };
    if matches!(suit, '♣' | '♦' | '♥' | '♠') && index > 0 {
        vec![&face[..index], &face[index..]]
    } else {
        vec![face]
    }
}

/// Separate rank and suit so the entire ten fits in the exposed corner strip.
pub(super) fn spawn_card_face(
    commands: &mut Commands,
    parent: Entity,
    mesh: Handle<Mesh>,
    face: &str,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
    layer: usize,
) {
    let color = if face == "P" {
        [224, 232, 248]
    } else if face.ends_with(['♥', '♦']) {
        [160, 24, 25]
    } else {
        [18, 18, 16]
    };
    for (row, part) in face_parts(face).iter().enumerate() {
        let (texture, aspect) = card_label_texture(images, part, color);
        let material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let height = 0.012_f32.min(0.019 / aspect.max(0.01));
        let width = height * aspect;
        for reverse in [false, true] {
            let sign = if reverse { -1.0 } else { 1.0 };
            let label = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(
                        sign * (-CARD_WORLD_WIDTH * 0.5 + 0.003 + width * 0.5),
                        CARD_WORLD_THICKNESS * 0.6,
                        sign * (-CARD_WORLD_HEIGHT * 0.5
                            + 0.004
                            + height * 0.5
                            + row as f32 * 0.014),
                    )
                    .with_rotation(Quat::from_rotation_y(if reverse {
                        std::f32::consts::PI
                    } else {
                        0.0
                    }))
                    .with_scale(Vec3::new(width, 1.0, height)),
                    RenderLayers::layer(layer),
                    bevy::light::NotShadowCaster,
                ))
                .id();
            commands.entity(parent).add_child(label);
        }
    }
}

pub(super) fn spawn_outline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent: Entity,
    key: &str,
    layer: usize,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(1., 0.83, 0.18),
        // Expanded backfaces peek around the card's silhouette. Front faces
        // are culled so the hull cannot cover its private/public face texture.
        cull_mode: Some(Face::Front),
        unlit: true,
        ..default()
    });
    let outline = commands
        .spawn((
            CardOutline(key.into()),
            Mesh3d(meshes.add(Cuboid::from_size(outline_size()))),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Hidden,
            RenderLayers::layer(layer),
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(parent).add_child(outline);
}

fn outline_size() -> Vec3 {
    Vec3::new(CARD_WORLD_WIDTH, CARD_WORLD_THICKNESS, CARD_WORLD_HEIGHT)
        + Vec3::splat(2.0 * OUTLINE_WIDTH)
}

fn update_outlines(drag: Res<DragState>, mut outlines: Query<(&CardOutline, &mut Visibility)>) {
    for (card, mut visible) in &mut outlines {
        *visible = if drag.hover_key.as_deref() == Some(card.0.as_str())
            || drag.card_key.as_deref() == Some(card.0.as_str())
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[derive(Component)]
struct HandDropHint;

#[derive(Component)]
struct PlayDropHint;

fn play_drop_hint(
    mut commands: Commands,
    drag: Res<DragState>,
    model: Res<BridgeModel>,
    poses: Res<PoseDisplay>,
    layout: Res<super::CanonicalLayout>,
    state: Res<UiState>,
    mut existing: Query<(Entity, &mut Text), With<PlayDropHint>>,
) {
    let message = drag.card_key.as_ref().and_then(|key| {
        if state.screen != UiScreen::Table {
            return None;
        }
        let pose = poses.0.get(key)?;
        if !model.snapshot.hand.iter().any(|card| card.card_key == *key) {
            return Some("Repositioning a taken card · this does not play it again.");
        }
        let inside = super::is_play_drop(
            &layout.0,
            [pose.current[0], drag.resting_height, pose.current[2]],
        );
        let turn = model.snapshot.game.as_ref().is_some_and(|game| {
            game.phase == "playing" && game.actor_seat == model.snapshot.own_seat()
        });
        Some(match (inside, turn) {
            (true, true) => "Release to attempt PLAY · the authority checks follow suit.",
            (false, true) => {
                "Physical move only · fit the whole card inside yellow PLAY to play it."
            }
            (_, false) => "Physical move only · it is not your card-playing turn.",
        })
    });
    match message {
        None => {
            for (entity, _) in &existing {
                commands.entity(entity).despawn();
            }
        }
        Some(message) => {
            if let Some((_, mut text)) = existing.iter_mut().next() {
                if text.0 != message {
                    text.0 = message.into();
                }
            } else {
                commands.spawn((
                    PlayDropHint,
                    Pickable::IGNORE,
                    Text::new(message),
                    TextFont::from_font_size(17.),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(18.),
                        top: px(72.),
                        padding: UiRect::all(px(8.)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.02, 0.04, 0.03)),
                    GlobalZIndex(22),
                ));
            }
        }
    }
}

fn observe_hand_input(
    drag: Res<DragState>,
    copies: Query<&Visibility, With<HandCardVisual>>,
    mut state: ResMut<UiState>,
) {
    let visible = copies
        .iter()
        .filter(|visibility| **visibility != Visibility::Hidden)
        .count();
    if state.held_card_key != drag.card_key || state.visible_hand_copies != visible {
        state.held_card_key.clone_from(&drag.card_key);
        state.visible_hand_copies = visible;
    }
}

fn hand_drop_hint(
    mut commands: Commands,
    drag: Res<DragState>,
    state: Res<UiState>,
    projection: Res<HandProjection>,
    cameras: Query<Entity, With<PocheUiCamera>>,
    hand_cameras: Query<(&Camera, &GlobalTransform), With<super::HandCamera>>,
    mut existing: Query<(Entity, &mut Node), With<HandDropHint>>,
) {
    let rect = hand_cameras
        .single()
        .ok()
        .and_then(|(camera, transform)| projection.screen_rect(camera, transform));
    if drag.card_key.is_none() || state.screen != UiScreen::Table || rect.is_none() {
        for (entity, _) in &existing {
            commands.entity(entity).despawn();
        }
    } else if let Some(rect) = rect {
        let node = Node {
            position_type: PositionType::Absolute,
            left: px(rect.min.x),
            width: px(rect.width()),
            top: px(rect.max.y - 8.),
            height: px(4.),
            ..default()
        };
        if let Ok((_, mut current)) = existing.single_mut() {
            *current = node;
        } else if let Ok(camera) = cameras.single() {
            commands.spawn((
                HandDropHint,
                UiTargetCamera(camera),
                Pickable::IGNORE,
                node,
                BackgroundColor(Color::srgba(0.35, 0.88, 0.78, 0.8)),
            ));
        }
    }
}

#[cfg(test)]
pub(super) fn corner_label_size(aspect: f32) -> (f32, f32) {
    let h = (CARD_WORLD_HEIGHT * 0.22).min(CARD_WORLD_WIDTH * 0.85 / aspect.max(0.01));
    (h * aspect, h)
}

pub(super) fn stack_offset(poses: &PoseDisplay, key: &str) -> f32 {
    let Some(pose) = poses.0.get(key) else {
        return 0.;
    };
    poses
        .0
        .iter()
        .filter(|(other_key, other)| {
            other_key.as_str() < key
                && (pose.current[0] - other.current[0]).abs() < CARD_WORLD_WIDTH * 1000.
                && (pose.current[2] - other.current[2]).abs() < CARD_WORLD_HEIGHT * 1000.
                && (pose.current[1] - other.current[1]).abs() < 2.
        })
        .count() as f32
        * 0.0022
}

/// Ray / full oriented card bound, including its thin sides.
pub(super) fn card_hit(ray: Ray3d, position: Vec3, rotation: Quat) -> Option<f32> {
    let inverse = rotation.inverse();
    let origin = inverse * (ray.origin - position);
    let direction = inverse * *ray.direction;
    ray_box_entry(
        origin,
        direction,
        Vec3::new(CARD_WORLD_WIDTH, CARD_WORLD_THICKNESS, CARD_WORLD_HEIGHT) * 0.5,
    )
}

fn ray_box_entry(origin: Vec3, direction: Vec3, half_size: Vec3) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < f32::EPSILON {
            if origin[axis].abs() > half_size[axis] {
                return None;
            }
        } else {
            let first = (-half_size[axis] - origin[axis]) / direction[axis];
            let second = (half_size[axis] - origin[axis]) / direction[axis];
            near = near.max(first.min(second));
            far = far.min(first.max(second));
        }
    }
    (near <= far).then_some(near)
}

fn inspect_zones(
    state: Res<UiState>,
    keys: Res<ButtonInput<KeyCode>>,
    drag: Res<DragState>,
    mut zones: Query<(&DiagnosticZone, &mut Visibility)>,
) {
    for (zone, mut visible) in &mut zones {
        // Hold Z to inspect, without changing the active interaction tool.
        *visible = if state.screen == UiScreen::Table
            && (keys.pressed(KeyCode::KeyZ)
                || (drag.card_key.is_some()
                    && matches!(zone.0, ObjectId::Zone(ZoneId::Hand(_) | ZoneId::Play))))
        {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_and_suit_are_separate_vertical_labels_not_a_wide_single_row() {
        assert_eq!(face_parts("10♣"), ["10", "♣"]);
        assert_eq!(face_parts("A♦"), ["A", "♦"]);
        assert_eq!(face_parts("P"), ["P"]);
    }
    use bevy::camera::{
        CameraProjection, ComputedCameraValues, OrthographicProjection, RenderTargetInfo,
        ScalingMode, Viewport,
    };
    use poche_spatial::{LayoutId, TableId, registered_layout};

    fn projected_hand(
        size: UVec2,
        seat: u8,
        count: usize,
        scale_factor: f32,
    ) -> (HandProjection, Camera, GlobalTransform) {
        let viewport_size = UVec2::new((size.x * 3 / 5).max(1), (size.y / 4).clamp(1, 190));
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 2).unwrap()).unwrap();
        let mut hand = HandProjection::default();
        hand.configure(&layout, Some(seat), count, size, viewport_size);
        let mut projection = OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            near: 0.01,
            far: 10.0,
            ..OrthographicProjection::default_3d()
        };
        projection.update(viewport_size.x as f32, viewport_size.y as f32);
        let camera = Camera {
            viewport: Some(Viewport {
                physical_position: UVec2::new(
                    (size.x - viewport_size.x) / 2,
                    size.y - viewport_size.y,
                ),
                physical_size: viewport_size,
                ..default()
            }),
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: size,
                    scale_factor,
                }),
                ..default()
            },
            ..default()
        };
        let up = super::super::hand_camera_up(Some(seat));
        let focus = hand.center + up * VIEW_HEIGHT * 0.44;
        let transform = Transform::from_translation(focus + Vec3::Y * 0.7)
            .looking_at(focus, up)
            .into();
        (hand, camera, transform)
    }

    #[test]
    fn projected_drop_edges_match_physical_zone_after_resize_and_for_both_seats() {
        for size in [
            UVec2::new(1180, 760),
            UVec2::new(390, 844),
            UVec2::new(2560, 1080),
        ] {
            for seat in [0, 1] {
                for dpi in [1.0, 2.0] {
                    let (hand, camera, transform) = projected_hand(size, seat, 1, dpi);
                    let rect = hand.screen_rect(&camera, &transform).unwrap();
                    for x in [rect.min.x, rect.center().x, rect.max.x] {
                        for y in [rect.min.y, rect.center().y, rect.max.y] {
                            let cursor = Vec2::new(x, y);
                            assert!(hand.contains_cursor(cursor, &camera, &transform));
                            let ray = camera.viewport_to_world(&transform, cursor).unwrap();
                            let distance = (hand.center.y - ray.origin.y) / ray.direction.y;
                            let world = hand.to_world(ray.origin + *ray.direction * distance);
                            assert!(
                                hand.contains(world),
                                "{size:?} seat{seat} dpi{dpi} cursor{cursor:?} mapped{world:?}"
                            );
                            assert!(
                                hand.to_world(hand.to_inset(world)).distance(world) < 0.000_001
                            );
                            // A nonzero grab offset cannot cause loss of the inset
                            // when a drag reaches the indicator's visible edge.
                            assert!(
                                hand.contains(
                                    hand.clamp_world(world + Vec3::new(0.03, 0.04, 0.03))
                                )
                            );
                        }
                    }
                    assert!(!hand.contains_cursor(
                        Vec2::new(rect.min.x - 1., rect.center().y),
                        &camera,
                        &transform
                    ));
                    assert!(!hand.contains_cursor(
                        Vec2::new(rect.max.x + 1., rect.center().y),
                        &camera,
                        &transform
                    ));
                }
            }
        }
    }

    #[test]
    fn playing_a_card_does_not_resize_the_physical_drop_target() {
        let size = UVec2::new(1180, 760);
        let (one, camera, transform) = projected_hand(size, 0, 1, 1.);
        let (seven, _, _) = projected_hand(size, 0, 7, 1.);
        assert_eq!(
            one.screen_rect(&camera, &transform),
            seven.screen_rect(&camera, &transform)
        );
        assert!(!one.contains(one.center + Vec3::X * 0.076));
        assert!(one.contains(one.center + Vec3::X * 0.075 + Vec3::Y * 0.03));
    }

    #[test]
    fn outline_hull_extends_beyond_every_side_without_covering_the_card_center() {
        let card = Vec3::new(CARD_WORLD_WIDTH, CARD_WORLD_THICKNESS, CARD_WORLD_HEIGHT) * 0.5;
        let hull = outline_size() * 0.5;
        for axis in 0..3 {
            let outside_axis = (axis + 1) % 3;
            let mut origin = Vec3::ZERO;
            origin[axis] = 1.0;
            origin[outside_axis] = (card[outside_axis] + hull[outside_axis]) * 0.5;
            let mut direction = Vec3::ZERO;
            direction[axis] = -1.0;
            assert!(ray_box_entry(origin, direction, card).is_none());
            assert!(ray_box_entry(origin, direction, hull).is_some());
            // At the center the real card's near face hides the expanded
            // hull's back face (front faces are culled by its material).
            assert!(1.0 - card[axis] < 1.0 + hull[axis]);
        }
    }
    #[test]
    fn hover_hits_only_the_oriented_card_not_a_large_screen_circle() {
        let ray = Ray3d::new(Vec3::new(0., 1., 0.), Dir3::NEG_Y);
        assert!(card_hit(ray, Vec3::ZERO, Quat::IDENTITY).is_some());
        assert!(card_hit(ray, Vec3::new(0.08, 0., 0.), Quat::IDENTITY).is_none());
        assert!(
            card_hit(
                Ray3d::new(Vec3::new(1., 0., 0.), Dir3::NEG_X),
                Vec3::ZERO,
                Quat::IDENTITY
            )
            .is_some()
        );
        assert!(
            card_hit(
                Ray3d::new(Vec3::new(0., -1., 0.), Dir3::NEG_Y),
                Vec3::ZERO,
                Quat::IDENTITY
            )
            .is_none()
        );
    }
}
