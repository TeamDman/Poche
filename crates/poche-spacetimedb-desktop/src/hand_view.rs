// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Private hand projection. Copies are presentation entities, never new cards.
use super::{
    CARD_WORLD_HEIGHT, CARD_WORLD_THICKNESS, CARD_WORLD_WIDTH, DragState, PocheUiCamera,
    PoseDisplay, SpatialAssets, UiScreen, UiState, animate_and_place_cards, card_label_texture,
    mm_position, point_to_world,
};
use bevy::{camera::visibility::RenderLayers, prelude::*};
use poche_bevy_spacetimedb::BridgeModel;
use poche_spatial::{ObjectId, SpatialLayout, ZoneId};
use std::collections::HashSet;

pub(super) const VIEW_HEIGHT: f32 = 0.14;
const HAND_LIFT_MM: f32 = 24.0;

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
    pub half_width: f32,
    pub has_hand: bool,
}

impl HandProjection {
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
        (world.x - self.center.x).abs() <= self.half_width
            && (world.z - self.center.z).abs() <= 0.125
    }
}

pub(super) fn hand_center(layout: &SpatialLayout, seat: Option<u8>) -> Vec3 {
    layout
        .zones()
        .iter()
        .find(|zone| matches!(zone.id, ZoneId::Hand(id) if Some(id.get()) == seat))
        .map_or(Vec3::new(0.0, 0.04, 0.52), |zone| {
            (point_to_world(zone.inner.min) + point_to_world(zone.inner.max)) * 0.5
        })
}

pub(super) fn lift_height(resting: f32) -> f32 {
    resting + HAND_LIFT_MM
}

/// Uncompressed cards keep their width; only center spacing compresses when a
/// large hand would run past the inset. This makes rank/suit corners overlap.
pub(super) fn hand_spacing_scale(count: usize, available_width: f32) -> f32 {
    let spread = count.saturating_sub(1) as f32 * 0.072;
    if spread <= 0.0 {
        1.0
    } else {
        ((available_width - CARD_WORLD_WIDTH) / spread).clamp(0.05, 1.0)
    }
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
        let (texture, aspect) = card_label_texture(&mut images, &card.face, [18, 18, 16]);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let (width, height) = corner_label_size(aspect);
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
        spawn_card_labels(
            &mut commands,
            entity,
            assets.card_label_mesh.clone(),
            material,
            (width, height),
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

pub(super) fn spawn_card_labels(
    commands: &mut Commands,
    parent: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    size: (f32, f32),
    layer: usize,
) {
    let (width, height) = size;
    for reverse in [false, true] {
        let sign = if reverse { -1. } else { 1. };
        let label = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(
                    sign * (-CARD_WORLD_WIDTH * 0.5 + width * 0.5 + 0.003),
                    CARD_WORLD_THICKNESS * 0.6,
                    sign * (-CARD_WORLD_HEIGHT * 0.5 + height * 0.5 + 0.004),
                )
                .with_rotation(Quat::from_rotation_y(if reverse {
                    std::f32::consts::PI
                } else {
                    0.
                }))
                .with_scale(Vec3::new(width, 1., height)),
                RenderLayers::layer(layer),
                bevy::light::NotShadowCaster,
            ))
            .id();
        commands.entity(parent).add_child(label);
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
        unlit: true,
        ..default()
    });
    let outline = commands
        .spawn((
            CardOutline(key.into()),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    for (x, z, w, h) in [
        (-CARD_WORLD_WIDTH / 2., 0., 0.0015, CARD_WORLD_HEIGHT),
        (CARD_WORLD_WIDTH / 2., 0., 0.0015, CARD_WORLD_HEIGHT),
        (0., -CARD_WORLD_HEIGHT / 2., CARD_WORLD_WIDTH, 0.0015),
        (0., CARD_WORLD_HEIGHT / 2., CARD_WORLD_WIDTH, 0.0015),
    ] {
        let edge = commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(w, 0.001, h))),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(x, 0.0016, z),
                RenderLayers::layer(layer),
                bevy::light::NotShadowCaster,
            ))
            .id();
        commands.entity(outline).add_child(edge);
    }
    commands.entity(parent).add_child(outline);
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
    cameras: Query<Entity, With<PocheUiCamera>>,
    existing: Query<Entity, With<HandDropHint>>,
) {
    if drag.card_key.is_none() || state.screen != UiScreen::Table {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
    } else if existing.is_empty()
        && let Ok(camera) = cameras.single()
    {
        commands.spawn((
            HandDropHint,
            UiTargetCamera(camera),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: percent(25.),
                width: percent(50.),
                bottom: px(6.),
                height: px(4.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.35, 0.88, 0.78, 0.8)),
        ));
    }
}

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

/// Ray / oriented card rectangle rather than a fixed pixel-radius click target.
pub(super) fn card_hit(ray: Ray3d, position: Vec3, rotation: Quat) -> Option<f32> {
    let inverse = rotation.inverse();
    let origin = inverse * (ray.origin - position);
    let direction = inverse * *ray.direction;
    if direction.y.abs() < 0.00001 {
        return None;
    }
    let distance = -origin.y / direction.y;
    let point = origin + distance * direction;
    (distance >= 0.0
        && point.x.abs() <= CARD_WORLD_WIDTH * 0.55
        && point.z.abs() <= CARD_WORLD_HEIGHT * 0.55)
        .then_some(distance)
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
    fn large_hand_overlaps_centers_without_shrinking_cards_and_mapping_is_reversible() {
        assert!((hand_spacing_scale(3, 0.6) - 1.0).abs() < f32::EPSILON);
        let scale = hand_spacing_scale(25, 0.6);
        assert!(scale * 0.072 < CARD_WORLD_WIDTH);
        let p = HandProjection {
            center: Vec3::new(0., 0.04, 0.52),
            horizontal_scale: scale,
            half_width: 1.,
            has_hand: true,
        };
        let world = Vec3::new(0.35, 0.064, 0.53);
        assert!(p.to_world(p.to_inset(world)).distance(world) < 0.00001);
    }
    #[test]
    fn hover_hits_only_the_oriented_card_not_a_large_screen_circle() {
        let ray = Ray3d::new(Vec3::new(0., 1., 0.), Dir3::NEG_Y);
        assert!(card_hit(ray, Vec3::ZERO, Quat::IDENTITY).is_some());
        assert!(card_hit(ray, Vec3::new(0.08, 0., 0.), Quat::IDENTITY).is_none());
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
