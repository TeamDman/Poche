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
    render::render_resource::Face,
    window::PrimaryWindow,
};
use poche_bevy_spacetimedb::{BridgeHandle, BridgeIntent, BridgeModel, BridgeNotice};
use poche_money::{
    BOWL_RADIUS_MM, CoinContainer, JAR_HEIGHT_MM, JAR_RADIUS_MM, LID_RADIUS_MM, container_center_mm,
};
use poche_spacetimedb_client::{ClientSnapshot, CoinView};
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
    quarter_outline: Handle<Mesh>,
    dime_outline: Handle<Mesh>,
    outline: Handle<StandardMaterial>,
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
        quarter: meshes.add(coin_shape(25)),
        dime: meshes.add(coin_shape(10)),
        quarter_outline: meshes.add(coin_outline_shape(25)),
        dime_outline: meshes.add(coin_outline_shape(10)),
        outline: materials.add(StandardMaterial {
            base_color: Color::srgb(1., 0.83, 0.18),
            cull_mode: Some(Face::Front),
            unlit: true,
            ..default()
        }),
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
pub(super) struct MoneyState {
    context: String,
    targets_updated: f64,
    poses: BTreeMap<String, CoinPose>,
    drag: Option<CoinDrag>,
    hover_key: Option<String>,
    pending_commits: BTreeMap<String, PendingCoinCommit>,
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
#[derive(Clone, Copy)]
struct PendingCoinCommit {
    sequence: u64,
    repair: bool,
}

impl CoinPose {
    fn reconcile(&mut self, coin: &CoinView, held: bool) {
        let locked = !coin.is_own || coin.container == "bowl";
        if locked || (!held && coin.sequence >= self.sequence) {
            self.target = mm_position(coin.position_mm.map(|value| value as f32));
        }
        // Keep locally reserved sequences even across stale echoes/rejections.
        self.sequence = self.sequence.max(coin.sequence);
    }
}

impl MoneyState {
    pub(super) fn displayed_coin_position(&self, key: &str) -> Option<[f32; 3]> {
        self.poses.get(key).map(|pose| pose.current.to_array())
    }

    pub(super) fn held_coin(&self) -> Option<(&str, [f32; 3], u64)> {
        let drag = self.drag.as_ref()?;
        let pose = self.poses.get(&drag.key)?;
        Some((drag.key.as_str(), pose.current.to_array(), pose.sequence))
    }

    fn begin_drag(&mut self, key: String, point: Vec3) -> bool {
        let Some(pose) = self.poses.get_mut(&key) else {
            return false;
        };
        let plane_y = pose.current.y;
        let offset = pose.current - point;
        // Interrupt the visual return where it is displayed, not at its old
        // target. In-flight commits remain tracked but never disable picking.
        pose.target = pose.current + Vec3::Y * 0.024;
        pose.last_sent = None;
        self.drag = Some(CoinDrag {
            key,
            plane_y,
            offset,
            sent_at: -1.0,
        });
        true
    }

    fn finish_move(
        &mut self,
        snapshot: &ClientSnapshot,
        notice: &BridgeNotice,
    ) -> Option<BridgeIntent> {
        let BridgeNotice::CoinMoveFinished {
            room_id,
            coin_id,
            sequence,
            commit: true,
            result,
        } = notice
        else {
            return None;
        };
        if snapshot.room_id() != Some(room_id.as_str()) {
            return None;
        }
        let coin = snapshot
            .coins
            .iter()
            .find(|coin| coin.is_own && coin.coin_id == *coin_id)?;
        let pending = self.pending_commits.get(&coin.coin_key)?;
        if pending.sequence != *sequence {
            return None;
        }
        let pending = self.pending_commits.remove(&coin.coin_key)?;
        if result.is_ok()
            || self
                .drag
                .as_ref()
                .is_some_and(|drag| drag.key == coin.coin_key)
        {
            return None;
        }
        let pose = self.poses.get_mut(&coin.coin_key)?;
        if pose.sequence > *sequence {
            return None; // A newer gesture already owns the prediction.
        }
        if pending.repair || coin.container == "bowl" {
            // Never create an infinite repair loop or try to withdraw money
            // which the authority has already accepted into the bowl.
            pose.target = mm_position(coin.position_mm.map(|value| value as f32));
            return None;
        }
        // Rejecting a final transfer leaves earlier accepted previews intact.
        // Commit the existing coin back to its authoritative logical container
        // once; this changes no balance and cannot mint or withdraw a coin.
        let next = pose.sequence.max(coin.sequence).saturating_add(1);
        pose.sequence = next;
        self.pending_commits.insert(
            coin.coin_key.clone(),
            PendingCoinCommit {
                sequence: next,
                repair: true,
            },
        );
        Some(BridgeIntent::MoveCoin {
            room_id: room_id.clone(),
            coin_id: coin.coin_id.clone(),
            sequence: next,
            container: coin.container.clone(),
            position_mm: coin.position_mm,
            commit: true,
        })
    }
}
#[derive(Component)]
struct CoinVisual(String);
#[derive(Component)]
struct CoinOutline(String);
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
    let active = state.screen == UiScreen::Table;
    let context = format!(
        "{:?}:{:?}",
        model.snapshot.room_id(),
        model.snapshot.identity
    );
    if money.context != context || !active {
        money.context = context;
        money.drag = None;
        money.pending_commits.clear();
        money.poses.clear();
    }
    for notice in notices.read() {
        if active && let Some(intent) = money.finish_move(&model.snapshot, notice) {
            let _ = bridge.send(intent);
        }
    }
    let wanted: HashSet<_> = model
        .snapshot
        .coins
        .iter()
        .filter(|c| active && (c.owner_seat.is_some() || c.container == "bowl"))
        .map(|c| c.coin_key.as_str())
        .collect();
    money.poses.retain(|key, _| wanted.contains(key.as_str()));
    money
        .pending_commits
        .retain(|key, _| wanted.contains(key.as_str()));
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
        if (!coin.is_own || coin.container == "bowl")
            && money
                .drag
                .as_ref()
                .is_some_and(|drag| drag.key == coin.coin_key)
        {
            money.drag = None;
        }
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
        pose.reconcile(coin, held);
        if existing.contains(coin.coin_key.as_str()) {
            continue;
        }
        let (mesh, ink, outline, diameter) = if coin.denomination_cents == 25 {
            (
                &assets.quarter,
                &assets.quarter_ink,
                &assets.quarter_outline,
                0.018,
            )
        } else {
            (&assets.dime, &assets.dime_ink, &assets.dime_outline, 0.014)
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
                    CoinOutline(coin.coin_key.clone()),
                    Mesh3d(outline.clone()),
                    MeshMaterial3d(assets.outline.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    NotShadowCaster,
                    NotShadowReceiver,
                    Pickable::IGNORE,
                ));
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
                    Transform::from_translation(jar_body_center(seat)),
                    NotShadowCaster,
                ));
                spawn_prop(
                    &mut commands,
                    meshes.add(Torus::new(0.036, 0.041)),
                    assets.glass.clone(),
                    jar + Vec3::Y * (JAR_HEIGHT_MM as f32 * 0.001 + JAR_GLASS_CLEARANCE),
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
fn jar_body_center(seat: u8) -> Vec3 {
    center(seat, CoinContainer::Jar)
        + Vec3::Y * (JAR_HEIGHT_MM as f32 * 0.0005 + JAR_GLASS_CLEARANCE)
}
const JAR_GLASS_CLEARANCE: f32 = 0.001;
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
    card_drag: Res<super::DragState>,
    hand_camera: Query<(&Camera, &GlobalTransform), With<super::HandCamera>>,
) {
    money.hover_key = None;
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
    let enabled = state.screen == UiScreen::Table
        && !state.escape_menu_open
        && !inspection.active
        && card_drag.card_key.is_none();
    let inset_card = cursor.is_some_and(|cursor| {
        hand_camera
            .single()
            .ok()
            .is_some_and(|(camera, transform)| {
                if !camera.is_active
                    || camera
                        .logical_viewport_rect()
                        .is_none_or(|rect| !rect.contains(cursor))
                {
                    return false;
                }
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
    let candidate = if enabled
        && !inset_card
        && !interaction.blocks_card_input()
        && let Some(ray) = ray
    {
        let nearest_card = card_poses
            .0
            .values()
            .filter_map(|pose| {
                super::hand_view::card_hit(ray, mm_position(pose.current), pose.current_rotation)
            })
            .min_by(f32::total_cmp);
        nearest_coin(&model, &money, ray).filter(|(distance, _, _)| {
            nearest_card.is_none_or(|card_distance| *distance < card_distance)
        })
    } else {
        None
    };
    if let Some((_, key, _)) = &candidate {
        money.hover_key = Some(key.clone());
        interaction.pointer_over_ui = true;
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some((_, key, point)) = candidate
    {
        // Picking includes the cylindrical side/cap, but drag mapping uses
        // the fixed center-height plane so pickup cannot shift X/Z.
        let point = ray
            .and_then(|ray| {
                let center = money.poses.get(&key)?.current;
                let distance = ray.intersect_plane(center, InfinitePlane3d::new(Vec3::Y))?;
                Some(ray.get_point(distance))
            })
            .unwrap_or(point);
        money.begin_drag(key, point);
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
        if submitted {
            money.pending_commits.insert(
                drag.key.clone(),
                PendingCoinCommit {
                    sequence,
                    repair: false,
                },
            );
        }
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
            let distance = coin_hit(ray, pose.current, coin.denomination_cents)?;
            let point = ray.get_point(distance);
            Some((distance, coin.coin_key.clone(), point))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

fn coin_shape(denomination: u8) -> Cylinder {
    if denomination == 25 {
        Cylinder::new(0.012, 0.0018)
    } else {
        Cylinder::new(0.009, 0.0014)
    }
}

fn coin_outline_shape(denomination: u8) -> Cylinder {
    let shape = coin_shape(denomination);
    Cylinder::new(shape.radius + 0.0012, shape.half_height * 2. + 0.0024)
}

/// Intersect the actual finite cylinder, including its narrow side. The hover
/// hull is presentation only and must not make adjacent coins steal a click.
fn coin_hit(ray: Ray3d, center: Vec3, denomination: u8) -> Option<f32> {
    let shape = coin_shape(denomination);
    let origin = ray.origin - center;
    let direction = *ray.direction;
    let mut nearest = f32::INFINITY;
    if direction.y.abs() > 1e-7 {
        for y in [-shape.half_height, shape.half_height] {
            let t = (y - origin.y) / direction.y;
            if t >= 0. && (origin + direction * t).xz().length_squared() <= shape.radius.powi(2) {
                nearest = nearest.min(t);
            }
        }
    }
    let a = direction.xz().length_squared();
    let b = 2. * origin.xz().dot(direction.xz());
    let c = origin.xz().length_squared() - shape.radius.powi(2);
    let discriminant = b * b - 4. * a * c;
    if a > 1e-10 && discriminant >= 0. {
        for sign in [-1., 1.] {
            let t = (-b + sign * discriminant.sqrt()) / (2. * a);
            if t >= 0. && (origin.y + direction.y * t).abs() <= shape.half_height {
                nearest = nearest.min(t);
            }
        }
    }
    nearest.is_finite().then_some(nearest)
}

fn animate_money(
    time: Res<Time>,
    mut money: ResMut<MoneyState>,
    mut coins: Query<(&CoinVisual, &mut Transform)>,
    mut outlines: Query<(&CoinOutline, &mut Visibility)>,
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
    for (coin, mut visible) in &mut outlines {
        *visible = if money.hover_key.as_deref() == Some(coin.0.as_str())
            || money.drag.as_ref().is_some_and(|drag| drag.key == coin.0)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
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
    fn coin_outline_encloses_top_bottom_and_side_without_covering_the_face() {
        for denomination in [10, 25] {
            let body = coin_shape(denomination);
            let hull = coin_outline_shape(denomination);
            assert!(hull.radius > body.radius);
            assert!(hull.half_height > body.half_height);
            // Inverted hull's back surface is behind the opaque face, from
            // either direction. Only the expanded silhouette remains visible.
            assert!(-hull.half_height < -body.half_height);
        }
    }

    #[test]
    fn coin_picking_covers_top_and_side_but_not_outline_only_space() {
        for denomination in [10, 25] {
            let body = coin_shape(denomination);
            assert!(coin_hit(Ray3d::new(Vec3::Y, Dir3::NEG_Y), Vec3::ZERO, denomination).is_some());
            assert!(coin_hit(Ray3d::new(Vec3::X, Dir3::NEG_X), Vec3::ZERO, denomination).is_some());
            assert!(
                coin_hit(
                    Ray3d::new(Vec3::new(body.radius + 0.0001, 1., 0.), Dir3::NEG_Y),
                    Vec3::ZERO,
                    denomination
                )
                .is_none()
            );
        }
    }

    #[test]
    fn returning_coin_is_picked_at_displayed_pose_not_its_network_destination() {
        let money = returning_coin();
        let mut model = BridgeModel::default();
        model
            .snapshot
            .coins
            .push(poche_spacetimedb_client::CoinView {
                coin_key: "quarter".into(),
                coin_id: "q0".into(),
                owner: "self".into(),
                owner_seat: Some(0),
                is_own: true,
                denomination_cents: 25,
                container: "lid".into(),
                position_mm: [-220, 25, 430],
                sequence: 7,
            });
        let displayed = money.poses["quarter"].current;
        let ray = Ray3d::new(displayed + Vec3::Y, Dir3::NEG_Y);
        assert_eq!(nearest_coin(&model, &money, ray).unwrap().1, "quarter");
        let destination = mm_position(model.snapshot.coins[0].position_mm.map(|v| v as f32));
        assert!(
            nearest_coin(
                &model,
                &money,
                Ray3d::new(destination + Vec3::Y, Dir3::NEG_Y)
            )
            .is_none()
        );
        model.snapshot.coins[0].container = "bowl".into();
        assert!(
            nearest_coin(&model, &money, ray).is_none(),
            "paid coins remain authority-locked"
        );
    }

    fn returning_coin() -> MoneyState {
        let displayed = Vec3::new(-0.1, 0.06, 0.3);
        MoneyState {
            poses: BTreeMap::from([(
                "quarter".into(),
                CoinPose {
                    current: displayed,
                    target: center(0, CoinContainer::Lid) + Vec3::Y * 0.005,
                    sequence: 7,
                    last_sent: Some([-100, 60, 300]),
                },
            )]),
            ..default()
        }
    }

    #[test]
    fn returning_coin_can_be_regrabbed_before_commit_acknowledgement() {
        let mut money = returning_coin();
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 7,
                repair: false,
            },
        );
        let displayed = money.poses["quarter"].current;
        assert!(money.begin_drag("quarter".into(), displayed));
        assert_eq!(
            money.drag.as_ref().unwrap().plane_y.to_bits(),
            displayed.y.to_bits()
        );
        assert_eq!(money.poses["quarter"].target.xz(), displayed.xz());
    }

    fn coin_snapshot() -> ClientSnapshot {
        ClientSnapshot {
            rooms: vec![poche_spacetimedb_client::RoomView {
                room_id: "room".into(),
            }],
            coins: vec![CoinView {
                coin_key: "quarter".into(),
                coin_id: "q0".into(),
                owner: "self".into(),
                owner_seat: Some(0),
                is_own: true,
                denomination_cents: 25,
                container: "lid".into(),
                position_mm: [-220, 25, 430],
                sequence: 7,
            }],
            ..default()
        }
    }

    fn rejected(sequence: u64) -> BridgeNotice {
        BridgeNotice::CoinMoveFinished {
            room_id: "room".into(),
            coin_id: "q0".into(),
            sequence,
            commit: true,
            result: Err("invalid payment".into()),
        }
    }

    #[test]
    fn old_return_acknowledgement_cannot_pull_a_regrabbed_coin_from_the_cursor() {
        let mut money = returning_coin();
        let snapshot = coin_snapshot();
        let displayed = money.poses["quarter"].current;
        money.begin_drag("quarter".into(), displayed);
        let held_target = money.poses["quarter"].target;
        money
            .poses
            .get_mut("quarter")
            .unwrap()
            .reconcile(&snapshot.coins[0], true);
        assert_eq!(money.poses["quarter"].target, held_target);
        assert_eq!(money.poses["quarter"].current, displayed);
    }

    #[test]
    fn old_rejection_does_not_cancel_regrab_even_before_its_first_preview() {
        let mut money = returning_coin();
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 7,
                repair: false,
            },
        );
        let displayed = money.poses["quarter"].current;
        money.begin_drag("quarter".into(), displayed);
        assert!(money.finish_move(&coin_snapshot(), &rejected(7)).is_none());
        assert_eq!(money.drag.as_ref().unwrap().key, "quarter");
        assert_eq!(money.poses["quarter"].current, displayed);
    }

    #[test]
    fn old_rejection_cannot_replace_a_newer_release_prediction() {
        let mut money = returning_coin();
        money.poses.get_mut("quarter").unwrap().sequence = 9;
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 9,
                repair: false,
            },
        );
        assert!(money.finish_move(&coin_snapshot(), &rejected(7)).is_none());
        assert_eq!(money.pending_commits["quarter"].sequence, 9);
    }

    #[test]
    fn old_pose_echo_after_second_release_is_ignored_but_latest_ack_is_applied() {
        let mut money = returning_coin();
        let mut snapshot = coin_snapshot();
        let pose = money.poses.get_mut("quarter").unwrap();
        pose.sequence = 9;
        pose.target = Vec3::new(-0.15, 0.049, 0.37);
        let desired = pose.target;
        pose.reconcile(&snapshot.coins[0], false);
        assert_eq!(pose.target, desired);
        assert_eq!(pose.sequence, 9);
        snapshot.coins[0].sequence = 9;
        pose.reconcile(&snapshot.coins[0], false);
        assert_eq!(pose.target, mm_position([-220., 25., 430.]));
        assert_eq!(pose.sequence, 9);
    }

    #[test]
    fn final_rejected_transfer_repairs_only_that_coin_once_with_a_fresh_sequence() {
        let mut money = returning_coin();
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 7,
                repair: false,
            },
        );
        let snapshot = coin_snapshot();
        let Some(BridgeIntent::MoveCoin {
            coin_id,
            sequence,
            container,
            commit,
            ..
        }) = money.finish_move(&snapshot, &rejected(7))
        else {
            panic!("one corrective commit")
        };
        assert_eq!(coin_id, "q0");
        assert_eq!(sequence, 8);
        assert_eq!(container, "lid");
        assert!(commit);
        assert!(money.finish_move(&snapshot, &rejected(8)).is_none());
        assert!(money.pending_commits.is_empty());
        assert_eq!(
            money.poses["quarter"].target,
            mm_position([-220., 25., 430.])
        );
    }

    #[test]
    fn another_rooms_rejection_cannot_repair_active_room_coins() {
        let mut money = returning_coin();
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 7,
                repair: false,
            },
        );
        let mut snapshot = coin_snapshot();
        snapshot.rooms[0].room_id = "different-room".into();
        assert!(money.finish_move(&snapshot, &rejected(7)).is_none());
        assert_eq!(money.pending_commits["quarter"].sequence, 7);
    }

    #[test]
    fn authoritative_bowl_payment_overrides_a_regrab_and_never_repairs_to_a_wallet() {
        let mut money = returning_coin();
        money.pending_commits.insert(
            "quarter".into(),
            PendingCoinCommit {
                sequence: 7,
                repair: false,
            },
        );
        let mut snapshot = coin_snapshot();
        snapshot.coins[0].container = "bowl".into();
        snapshot.coins[0].position_mm = [-240, 25, 0];
        money
            .poses
            .get_mut("quarter")
            .unwrap()
            .reconcile(&snapshot.coins[0], true);
        assert_eq!(money.poses["quarter"].target, mm_position([-240., 25., 0.]));
        assert!(money.finish_move(&snapshot, &rejected(7)).is_none());
    }

    #[test]
    fn settled_coin_can_be_grabbed_without_pending_commit() {
        let mut money = returning_coin();
        let displayed = money.poses["quarter"].current;
        money.poses.get_mut("quarter").unwrap().target = displayed;
        assert!(money.begin_drag("quarter".into(), displayed));
        assert_eq!(money.drag.as_ref().unwrap().offset, Vec3::ZERO);
    }

    #[test]
    fn jar_bottom_is_separated_from_table_but_keeps_lowest_coins_inside() {
        for seat in 0..2 {
            let bottom = jar_body_center(seat).y - JAR_HEIGHT_MM as f32 * 0.0005;
            let table = center(seat, CoinContainer::Jar).y;
            let first_coin = poche_money::coin_rest_pose(seat, CoinContainer::Jar, 0, 25);
            assert!(
                bottom > table + 0.0001,
                "jar bottom {bottom} is coplanar with table {table}"
            );
            assert!(bottom < first_coin[1] as f32 * 0.001 - 0.0009);
        }
    }
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
