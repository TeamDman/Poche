// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Physical money is a view of the shared ledger. Drag previews move objects;
//! release asks the authority to transfer one existing coin, never a balance.

use super::{
    TabletopCamera, UiScreen, UiState, card_label_texture, drag_cards, mm_position, world_ui,
};
use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    window::PrimaryWindow,
};
use poche_bevy_spacetimedb::{BridgeHandle, BridgeIntent, BridgeModel, BridgeNotice};
use poche_money::{
    BOWL_RADIUS_MM, CoinContainer, JAR_HEIGHT_MM, JAR_RADIUS_MM, LID_RADIUS_MM, container_center_mm,
};
use std::collections::{BTreeMap, HashSet};

pub(super) struct MoneyPlugin;

impl Plugin for MoneyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoneyState>()
            .add_systems(Startup, setup_money_assets)
            .add_systems(
                Update,
                (sync_money, interact_money, animate_money)
                    .chain()
                    .after(world_ui::interact_with_world)
                    .before(drag_cards),
            );
    }
}

#[derive(Resource)]
struct MoneyAssets {
    quarter: Handle<Mesh>,
    dime: Handle<Mesh>,
    silver: Handle<StandardMaterial>,
    quarter_ink: Handle<StandardMaterial>,
    dime_ink: Handle<StandardMaterial>,
    label_mesh: Handle<Mesh>,
    glass: Handle<StandardMaterial>,
    green: Handle<StandardMaterial>,
    ceramic: Handle<StandardMaterial>,
}

fn setup_money_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut ink = |text: &str| {
        let (texture, _) = card_label_texture(&mut images, text, [28, 31, 33]);
        materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };
    let quarter_ink = ink("25¢");
    let dime_ink = ink("10¢");
    commands.insert_resource(MoneyAssets {
        quarter: meshes.add(Cylinder::new(0.012, 0.0018)),
        dime: meshes.add(Cylinder::new(0.009, 0.0014)),
        silver: materials.add(StandardMaterial {
            base_color: Color::srgb(0.74, 0.77, 0.79),
            metallic: 0.65,
            perceptual_roughness: 0.4,
            ..default()
        }),
        quarter_ink,
        dime_ink,
        label_mesh: meshes.add(Rectangle::new(1., 1.)),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgba(0.73, 0.9, 0.91, 0.12),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.15,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        green: materials.add(StandardMaterial {
            base_color: Color::srgb(0.025, 0.16, 0.075),
            perceptual_roughness: 0.55,
            ..default()
        }),
        ceramic: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.86, 0.72),
            perceptual_roughness: 0.5,
            ..default()
        }),
    });
}

#[derive(Resource, Default)]
struct MoneyState {
    context: String,
    targets_updated: f64,
    poses: BTreeMap<String, CoinPose>,
    drag: Option<CoinDrag>,
    pending_commit: Option<(String, u64)>,
    scene_key: String,
    labels_key: String,
}
struct CoinPose {
    current: Vec3,
    target: Vec3,
    sequence: u64,
    last_sent: Option<[i32; 3]>,
}
struct CoinDrag {
    key: String,
    plane_y: f32,
    offset: Vec3,
    sent_at: f64,
}
#[derive(Component)]
struct CoinVisual(String);
#[derive(Component)]
struct MoneyProp;
#[derive(Component)]
struct MoneyLabel;

#[allow(clippy::too_many_arguments)]
fn sync_money(
    mut commands: Commands,
    model: Res<BridgeModel>,
    bridge: Res<BridgeHandle>,
    state: Res<UiState>,
    assets: Res<MoneyAssets>,
    mut money: ResMut<MoneyState>,
    mut notices: MessageReader<BridgeNotice>,
    coins: Query<(Entity, &CoinVisual)>,
    props: Query<Entity, With<MoneyProp>>,
    labels: Query<Entity, With<MoneyLabel>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let rejected = notices.read().any(|notice| {
        matches!(
            notice,
            BridgeNotice::Command {
                operation: "move_coin",
                result: Err(_),
                ..
            }
        )
    });
    if rejected {
        money.drag = None;
        // A rejected transfer does not undo earlier accepted drag previews.
        // Put that same coin back into its current logical container once.
        if let Some((key, sent)) = money.pending_commit.take()
            && let Some(coin) = model
                .snapshot
                .coins
                .iter()
                .find(|coin| coin.coin_key == key)
        {
            let _ = bridge.send(BridgeIntent::MoveCoin {
                room_id: model.snapshot.room_id().unwrap_or_default().into(),
                coin_id: coin.coin_id.clone(),
                sequence: coin.sequence.max(sent).saturating_add(1),
                container: coin.container.clone(),
                position_mm: coin.position_mm,
                commit: true,
            });
        }
        money.poses.clear();
    }
    if money
        .pending_commit
        .as_ref()
        .is_some_and(|(key, sequence)| {
            model
                .snapshot
                .coins
                .iter()
                .any(|coin| &coin.coin_key == key && coin.sequence >= *sequence)
        })
    {
        money.pending_commit = None;
    }
    let active = state.screen == UiScreen::Table;
    let context = format!(
        "{:?}:{:?}",
        model.snapshot.room_id(),
        model.snapshot.identity
    );
    if money.context != context || !active {
        money.context = context;
        money.drag = None;
        money.pending_commit = None;
        money.poses.clear();
    }
    let wanted: HashSet<_> = model
        .snapshot
        .coins
        .iter()
        .filter(|c| active && (c.owner_seat.is_some() || c.container == "bowl"))
        .map(|c| c.coin_key.as_str())
        .collect();
    money.poses.retain(|key, _| wanted.contains(key.as_str()));
    let mut existing = HashSet::new();
    for (entity, coin) in &coins {
        if wanted.contains(coin.0.as_str()) {
            existing.insert(coin.0.as_str());
        } else {
            commands.entity(entity).despawn();
        }
    }
    for coin in model
        .snapshot
        .coins
        .iter()
        .filter(|coin| wanted.contains(coin.coin_key.as_str()))
    {
        let network = mm_position(coin.position_mm.map(|value| value as f32));
        let held = money
            .drag
            .as_ref()
            .is_some_and(|drag| drag.key == coin.coin_key);
        let pose = money
            .poses
            .entry(coin.coin_key.clone())
            .or_insert(CoinPose {
                current: network,
                target: network,
                sequence: coin.sequence,
                last_sent: None,
            });
        if !held && coin.sequence >= pose.sequence {
            pose.target = network;
            pose.sequence = coin.sequence;
        }
        if existing.contains(coin.coin_key.as_str()) {
            continue;
        }
        let (mesh, ink, diameter) = if coin.denomination_cents == 25 {
            (&assets.quarter, &assets.quarter_ink, 0.018)
        } else {
            (&assets.dime, &assets.dime_ink, 0.014)
        };
        commands
            .spawn((
                CoinVisual(coin.coin_key.clone()),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(assets.silver.clone()),
                Transform::from_translation(network),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(assets.label_mesh.clone()),
                    MeshMaterial3d(ink.clone()),
                    Transform::from_xyz(0., 0.001, 0.)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(diameter, diameter * 0.6, 1.)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            });
    }
    let scene_key = format!(
        "{active}:{:?}",
        model
            .snapshot
            .members
            .iter()
            .map(|m| (&m.identity, m.seat))
            .collect::<Vec<_>>()
    );
    if scene_key != money.scene_key {
        money.scene_key = scene_key;
        for entity in &props {
            commands.entity(entity).despawn();
        }
        if active {
            // A shallow open bowl: floor plus a ring wall, not a sealed cylinder.
            let bowl = center(0, CoinContainer::Bowl);
            spawn_prop(
                &mut commands,
                meshes.add(Cylinder::new(0.067, 0.005)),
                assets.ceramic.clone(),
                bowl + Vec3::Y * 0.001,
            );
            spawn_prop(
                &mut commands,
                meshes.add(Torus::new(0.063, 0.077)),
                assets.ceramic.clone(),
                bowl + Vec3::Y * 0.015,
            );
            for member in &model.snapshot.members {
                let Some(seat) = member.seat else {
                    continue;
                };
                let jar = center(seat, CoinContainer::Jar);
                commands.spawn((
                    MoneyProp,
                    Mesh3d(meshes.add(Cylinder::new(
                        JAR_RADIUS_MM as f32 * 0.001,
                        JAR_HEIGHT_MM as f32 * 0.001,
                    ))),
                    MeshMaterial3d(assets.glass.clone()),
                    Transform::from_translation(jar + Vec3::Y * (JAR_HEIGHT_MM as f32 * 0.0005)),
                    NotShadowCaster,
                ));
                spawn_prop(
                    &mut commands,
                    meshes.add(Torus::new(0.036, 0.041)),
                    assets.glass.clone(),
                    jar + Vec3::Y * 0.18,
                );
                let lid = center(seat, CoinContainer::Lid);
                spawn_prop(
                    &mut commands,
                    meshes.add(Cylinder::new(0.043, 0.004)),
                    assets.green.clone(),
                    lid + Vec3::Y * 0.002,
                );
                spawn_prop(
                    &mut commands,
                    meshes.add(Torus::new(0.04, 0.044)),
                    assets.green.clone(),
                    lid + Vec3::Y * 0.006,
                );
            }
        }
    }
    // Pose previews must not rebuild text textures; only totals/membership do.
    let labels_key = format!(
        "{}:{:?}",
        money.scene_key,
        model
            .snapshot
            .coins
            .iter()
            .map(|c| (&c.coin_key, &c.container))
            .collect::<Vec<_>>()
    );
    if money.labels_key != labels_key {
        money.labels_key = labels_key;
        for entity in &labels {
            commands.entity(entity).despawn();
        }
        if active {
            let total: u32 = model
                .snapshot
                .coins
                .iter()
                .filter(|c| c.container == "bowl")
                .map(|c| u32::from(c.denomination_cents))
                .sum();
            let instruction = if model.snapshot.game.is_none() {
                " · ante 25¢ each"
            } else {
                ""
            };
            spawn_label(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                &format!("Bowl  ${}.{:02}{instruction}", total / 100, total % 100),
                center(0, CoinContainer::Bowl) + Vec3::new(0., 0.08, -0.08),
                0.23,
            );
            for member in &model.snapshot.members {
                let Some(seat) = member.seat else {
                    continue;
                };
                for container in [CoinContainer::Jar, CoinContainer::Lid] {
                    let total: u32 = model
                        .snapshot
                        .coins
                        .iter()
                        .filter(|c| c.owner == member.identity && c.container == container.as_str())
                        .map(|c| u32::from(c.denomination_cents))
                        .sum();
                    let label = format!(
                        "{} · {}  ${}.{:02}",
                        member.display_name,
                        container.as_str(),
                        total / 100,
                        total % 100
                    );
                    let height = if container == CoinContainer::Jar {
                        0.23
                    } else {
                        0.065
                    };
                    spawn_label(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &label,
                        center(seat, container) + Vec3::Y * height,
                        0.22,
                    );
                }
            }
        }
    }
}

fn center(seat: u8, container: CoinContainer) -> Vec3 {
    Vec3::from_array(container_center_mm(seat, container).map(|v| v as f32 * 0.001))
}
fn spawn_prop(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
) {
    commands.spawn((
        MoneyProp,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position),
    ));
}
fn spawn_label(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    text: &str,
    position: Vec3,
    width: f32,
) {
    let (texture, aspect) = card_label_texture(images, text, [243, 239, 220]);
    let ink = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let backing = materials.add(StandardMaterial {
        base_color: Color::srgb(0.025, 0.055, 0.045),
        unlit: true,
        ..default()
    });
    let height = width / aspect.max(1.0);
    commands
        .spawn((
            MoneyLabel,
            Transform::from_translation(position),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(Rectangle::new(width + 0.012, height + 0.008))),
                MeshMaterial3d(backing),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            parent.spawn((
                Mesh3d(meshes.add(Rectangle::new(width, height))),
                MeshMaterial3d(ink),
                Transform::from_xyz(0., 0., 0.0002),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn interact_money(
    model: Res<BridgeModel>,
    bridge: Res<BridgeHandle>,
    mut state: ResMut<UiState>,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<TabletopCamera>>,
    mut money: ResMut<MoneyState>,
    mut interaction: ResMut<world_ui::WorldInteraction>,
    inspection: Res<world_ui::SheetInspectionRequest>,
    developer: Option<Res<super::file_control::DeveloperControlActive>>,
    hand_projection: Res<super::hand_view::HandProjection>,
    card_poses: Res<super::PoseDisplay>,
    hand_camera: Query<(&Camera, &GlobalTransform), With<super::HandCamera>>,
) {
    if state.screen != UiScreen::Table {
        state.money_pick_targets.clear();
        state.money_bowl_screen = None;
        state.money_jar_screen = None;
        state.money_lid_screen = None;
        return;
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return;
    };
    let Some(seat) = model.snapshot.own_seat() else {
        money.drag = None;
        state.money_pick_targets.clear();
        return;
    };
    let project = |position| {
        camera
            .world_to_viewport(camera_transform, position)
            .ok()
            .map(|p| p.to_array())
    };
    state.money_bowl_screen = project(center(seat, CoinContainer::Bowl));
    state.money_jar_screen = project(center(seat, CoinContainer::Jar));
    state.money_lid_screen = project(center(seat, CoinContainer::Lid));
    if developer.is_some() && time.elapsed_secs_f64() - money.targets_updated > 0.1 {
        state.money_pick_targets.clear();
        money.targets_updated = time.elapsed_secs_f64();
        for coin in model
            .snapshot
            .coins
            .iter()
            .filter(|c| c.is_own && c.container != "bowl")
        {
            if let Some(pose) = money.poses.get(&coin.coin_key)
                && let Some(screen) = project(pose.current)
            {
                let ray = camera
                    .viewport_to_world(camera_transform, Vec2::from_array(screen))
                    .ok();
                let nearest = ray.and_then(|ray| nearest_coin(&model, &money, ray));
                if nearest
                    .as_ref()
                    .is_some_and(|(_, key, _)| key == &coin.coin_key)
                {
                    state
                        .money_pick_targets
                        .push(super::file_control::FileControlCoinTarget {
                            coin_id: coin.coin_id.clone(),
                            screen,
                            container: coin.container.clone(),
                            denomination_cents: coin.denomination_cents,
                        });
                }
            }
        }
    }
    let cursor = windows.single().ok().and_then(Window::cursor_position);
    let ray = cursor.and_then(|point| camera.viewport_to_world(camera_transform, point).ok());
    let enabled = state.screen == UiScreen::Table && !state.escape_menu_open && !inspection.active;
    let inset_card = cursor.is_some_and(|cursor| {
        hand_camera
            .single()
            .ok()
            .is_some_and(|(camera, transform)| {
                camera
                    .viewport_to_world(transform, cursor)
                    .ok()
                    .is_some_and(|ray| {
                        model.snapshot.hand.iter().any(|card| {
                            card_poses.0.get(&card.card_key).is_some_and(|pose| {
                                let world = mm_position(pose.current);
                                hand_projection.contains(world)
                                    && super::hand_view::card_hit(
                                        ray,
                                        hand_projection.to_inset(world),
                                        pose.current_rotation,
                                    )
                                    .is_some()
                            })
                        })
                    })
            })
    });
    if enabled
        && !inset_card
        && money.pending_commit.is_none()
        && !interaction.blocks_card_input()
        && mouse.just_pressed(MouseButton::Left)
        && let Some(ray) = ray
    {
        let nearest_card = card_poses
            .0
            .values()
            .filter_map(|pose| {
                super::hand_view::card_hit(ray, mm_position(pose.current), pose.current_rotation)
            })
            .min_by(f32::total_cmp);
        let candidate = nearest_coin(&model, &money, ray).filter(|(distance, _, _)| {
            nearest_card.is_none_or(|card_distance| *distance < card_distance)
        });
        if let Some((_, key, point)) = candidate {
            let pose = money.poses.get_mut(&key).expect("picked coin exists");
            let plane_y = pose.current.y;
            let offset = pose.current - point;
            pose.target.y = plane_y + 0.024;
            money.drag = Some(CoinDrag {
                key,
                plane_y,
                offset,
                sent_at: -1.0,
            });
        }
    }
    let Some(mut drag) = money.drag.take() else {
        return;
    };
    interaction.pointer_over_ui = true;
    let Some(coin) = model
        .snapshot
        .coins
        .iter()
        .find(|coin| coin.coin_key == drag.key)
    else {
        return;
    };
    let Some(pose) = money.poses.get_mut(&drag.key) else {
        return;
    };
    if enabled
        && let Some(ray) = ray
        && let Some(distance) =
            ray.intersect_plane(Vec3::Y * drag.plane_y, InfinitePlane3d::new(Vec3::Y))
    {
        let position = ray.get_point(distance) + drag.offset;
        pose.target.x = position.x;
        pose.target.z = position.z;
        pose.current.x = position.x;
        pose.current.z = position.z;
    }
    let release = !enabled || !mouse.pressed(MouseButton::Left);
    let mut container = coin.container.clone();
    if release {
        // Hit-test the stationary table plane, not a lifted coin or glass surface.
        let position = ray
            .and_then(|r| {
                r.intersect_plane(Vec3::Y * 0.02, InfinitePlane3d::new(Vec3::Y))
                    .map(|d| r.get_point(d))
            })
            .unwrap_or(pose.target);
        if enabled {
            container = drop_container(seat, position).map_or(container, |c| c.as_str().into());
        }
        pose.target.y = drag.plane_y;
    }
    let position = pose.target.to_array().map(|v| (v * 1000.).round() as i32);
    let due = release || time.elapsed_secs_f64() - drag.sent_at >= 0.05;
    let mut submitted = false;
    if due && (release || pose.last_sent != Some(position)) {
        pose.sequence = pose.sequence.max(coin.sequence).saturating_add(1);
        let result = bridge.send(BridgeIntent::MoveCoin {
            room_id: model.snapshot.room_id().unwrap_or_default().into(),
            coin_id: coin.coin_id.clone(),
            sequence: pose.sequence,
            container,
            position_mm: position,
            commit: release,
        });
        if let Err(error) = result {
            state.status = error;
            pose.target = Vec3::from_array(coin.position_mm.map(|v| v as f32 * 0.001));
        } else {
            submitted = true;
            pose.last_sent = Some(position);
            drag.sent_at = time.elapsed_secs_f64();
        }
    }
    if release {
        let sequence = pose.sequence;
        money.pending_commit = submitted.then(|| (drag.key.clone(), sequence));
    } else {
        money.drag = Some(drag);
    }
}

fn drop_container(seat: u8, point: Vec3) -> Option<CoinContainer> {
    [CoinContainer::Jar, CoinContainer::Lid, CoinContainer::Bowl]
        .into_iter()
        .find(|container| {
            let radius = match container {
                CoinContainer::Jar => JAR_RADIUS_MM,
                CoinContainer::Lid => LID_RADIUS_MM,
                CoinContainer::Bowl => BOWL_RADIUS_MM,
            } as f32
                * 0.001;
            (point - center(seat, *container)).xz().length() <= radius
        })
}

fn nearest_coin(
    model: &BridgeModel,
    money: &MoneyState,
    ray: Ray3d,
) -> Option<(f32, String, Vec3)> {
    model
        .snapshot
        .coins
        .iter()
        .filter(|c| c.is_own && c.container != "bowl")
        .filter_map(|coin| {
            let pose = money.poses.get(&coin.coin_key)?;
            let radius = if coin.denomination_cents == 25 {
                0.012
            } else {
                0.009
            };
            let distance = ray.intersect_plane(pose.current, InfinitePlane3d::new(Vec3::Y))?;
            let point = ray.get_point(distance);
            ((point - pose.current).xz().length() <= radius).then_some((
                distance,
                coin.coin_key.clone(),
                point,
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

fn animate_money(
    time: Res<Time>,
    mut money: ResMut<MoneyState>,
    mut coins: Query<(&CoinVisual, &mut Transform)>,
    mut labels: Query<&mut Transform, (With<MoneyLabel>, Without<CoinVisual>)>,
    camera: Query<
        &Transform,
        (
            With<TabletopCamera>,
            Without<CoinVisual>,
            Without<MoneyLabel>,
        ),
    >,
) {
    let alpha = 1. - (-18. * time.delta_secs()).exp();
    for (coin, mut transform) in &mut coins {
        if let Some(pose) = money.poses.get_mut(&coin.0) {
            pose.current = pose.current.lerp(pose.target, alpha);
            transform.translation = pose.current;
        }
    }
    if let Ok(camera) = camera.single() {
        for mut label in &mut labels {
            label.rotation = camera.rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_container_is_a_distinct_drop_target_for_both_seats() {
        for seat in 0..2 {
            for container in [CoinContainer::Jar, CoinContainer::Lid, CoinContainer::Bowl] {
                assert_eq!(
                    drop_container(seat, center(seat, container)),
                    Some(container)
                );
            }
        }
        assert_eq!(drop_container(0, Vec3::new(1., 0.02, 1.)), None);
    }
}
