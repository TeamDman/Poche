// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local inspection only: selecting a projected centre never issues a reducer.
use super::{
    DragState, HandCamera, PocheUiCamera, PoseDisplay, TabletopCamera, UiScreen, UiState,
    animate_and_place_cards, drag_cards, hand_view::HandProjection, mm_position, money::MoneyState,
    world_ui::WorldInteraction,
};
use bevy::{
    camera::visibility::RenderLayers,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    transform::TransformSystems,
};
use poche_bevy_spacetimedb::BridgeModel;
use std::collections::HashSet;

pub(super) struct SelectionPlugin;
impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionState>()
            .init_resource::<SelectionReadout>()
            .add_systems(
                Update,
                (update_selection, draw_selection)
                    .chain()
                    .after(drag_cards)
                    .before(animate_and_place_cards),
            )
            .add_systems(
                PostUpdate,
                draw_selection_readout
                    .after(bevy::camera::CameraUpdateSystems)
                    .before(TransformSystems::Propagate),
            );
    }
}

#[derive(Resource, Default)]
pub(super) struct SelectionState {
    pub selected_coin_keys: HashSet<String>,
    pub selected_card_keys: HashSet<String>,
    pub selecting: bool,
    pub total_cents: u32,
    pub rectangle: Option<Rect>,
    pub summary: String,
    pub projected_coins: Vec<(String, Vec2)>,
    origin: Option<Vec2>,
    context: Option<String>,
}

impl SelectionState {
    pub(super) fn clear(&mut self) {
        self.selected_coin_keys.clear();
        self.selected_card_keys.clear();
        self.selecting = false;
        self.total_cents = 0;
        self.rectangle = None;
        self.summary.clear();
        self.origin = None;
    }
}

#[derive(Clone)]
struct Piece {
    key: String,
    coin_cents: Option<u8>,
    points: Vec<Vec2>,
}

fn centre_inside(rectangle: Rect, point: Vec2) -> bool {
    point.is_finite()
        && point.x >= rectangle.min.x
        && point.x <= rectangle.max.x
        && point.y >= rectangle.min.y
        && point.y <= rectangle.max.y
}

fn count_selection(selection: &mut SelectionState, pieces: &[Piece], rectangle: Rect) {
    selection.selected_coin_keys.clear();
    selection.selected_card_keys.clear();
    selection.total_cents = 0;
    for piece in pieces
        .iter()
        .filter(|p| p.points.iter().any(|p| centre_inside(rectangle, *p)))
    {
        if let Some(cents) = piece.coin_cents {
            if selection.selected_coin_keys.insert(piece.key.clone()) {
                selection.total_cents += u32::from(cents);
            }
        } else {
            selection.selected_card_keys.insert(piece.key.clone());
        }
    }
    selection.summary = selection_summary(selection);
}

fn selection_summary(selection: &SelectionState) -> String {
    let coins = selection.selected_coin_keys.len();
    let cards = selection.selected_card_keys.len();
    let money = format!(
        "{coins} {} · ${}.{:02}",
        if coins == 1 { "coin" } else { "coins" },
        selection.total_cents / 100,
        selection.total_cents % 100
    );
    if cards == 0 {
        money
    } else {
        format!(
            "{money} · {cards} {}",
            if cards == 1 { "card" } else { "cards" }
        )
    }
}

pub(super) fn update_selection(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<
        (&Camera, &GlobalTransform, Option<&HandCamera>),
        Or<(With<TabletopCamera>, With<HandCamera>)>,
    >,
    state: Res<UiState>,
    model: Res<BridgeModel>,
    poses: Res<PoseDisplay>,
    hand: Res<HandProjection>,
    money: Res<MoneyState>,
    drag: Res<DragState>,
    interaction: Res<WorldInteraction>,
    hand_options: Res<super::hand_options::HandViewOptions>,
    mut selection: ResMut<SelectionState>,
) {
    let context = model.snapshot.room_id().map(str::to_owned);
    if selection.context != context {
        selection.clear();
        selection.context = context;
    }
    if state.screen != UiScreen::Table
        || state.escape_menu_open
        || interaction.modal_open()
        || hand_options.blocks_pointer_input()
    {
        selection.clear();
        return;
    }
    let Some(cursor) = windows.single().ok().and_then(Window::cursor_position) else {
        if selection.origin.is_some() {
            selection.clear();
        }
        return;
    };
    // Normal object picking has first refusal on the initial press. A primary
    // drag only becomes a marquee if neither a piece nor a world/UI action won.
    if mouse.just_pressed(MouseButton::Left) {
        if drag.card_key.is_some()
            || money.held_coin().is_some()
            || interaction.blocks_card_input()
            || mouse.pressed(MouseButton::Right)
            || mouse.pressed(MouseButton::Middle)
        {
            selection.clear();
            return;
        }
        selection.clear();
        selection.origin = Some(cursor);
    }
    if (mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle))
        && selection.origin.is_some()
    {
        selection.clear();
    }
    if let Some(origin) = selection.origin {
        selection.selecting |= cursor.distance(origin) >= 5.;
        if selection.selecting {
            selection.rectangle = Some(Rect::from_corners(origin.min(cursor), origin.max(cursor)));
        }
    }
    let mut pieces = Vec::new();
    selection.projected_coins.clear();
    for (camera, transform, inset) in &cameras {
        if !camera.is_active {
            continue;
        }
        let Some(viewport) = camera.logical_viewport_rect() else {
            continue;
        };
        let project = |world| {
            camera
                .world_to_viewport(transform, world)
                .ok()
                .filter(|p| centre_inside(viewport, *p))
        };
        if inset.is_none() {
            for coin in &model.snapshot.coins {
                if let Some(position) = money.displayed_coin_position(&coin.coin_key)
                    && let Some(point) = project(Vec3::from_array(position))
                {
                    selection
                        .projected_coins
                        .push((coin.coin_key.clone(), point));
                    pieces.push(Piece {
                        key: coin.coin_key.clone(),
                        coin_cents: Some(coin.denomination_cents),
                        points: vec![point],
                    });
                }
            }
        }
        for card in &model.snapshot.card_poses {
            let Some(pose) = poses.0.get(&card.card_key) else {
                continue;
            };
            let world = mm_position(pose.current);
            let position = if inset.is_some() {
                if !model
                    .snapshot
                    .hand
                    .iter()
                    .any(|c| c.card_key == card.card_key)
                    || !hand.contains(world)
                {
                    continue;
                }
                let Some(position) = hand.card_position(&poses, &card.card_key) else {
                    continue;
                };
                position
            } else {
                let Some(position) = super::hand_view::visual_position(&poses, &card.card_key)
                else {
                    continue;
                };
                position
            };
            if let Some(point) = project(position) {
                pieces.push(Piece {
                    key: card.card_key.clone(),
                    coin_cents: None,
                    points: vec![point],
                });
            }
        }
    }
    if selection.selecting {
        if let Some(rectangle) = selection.rectangle {
            count_selection(&mut selection, &pieces, rectangle);
        }
    } else if selection.origin.is_none() {
        // Keep a released selection by object identity, but discard objects that
        // left the room/deal. Do not reveal or count private card identities.
        selection
            .selected_coin_keys
            .retain(|key| model.snapshot.coins.iter().any(|c| c.coin_key == *key));
        selection
            .selected_card_keys
            .retain(|key| model.snapshot.card_poses.iter().any(|c| c.card_key == *key));
        selection.total_cents = model
            .snapshot
            .coins
            .iter()
            .filter(|coin| selection.selected_coin_keys.contains(&coin.coin_key))
            .map(|coin| u32::from(coin.denomination_cents))
            .sum();
        if !selection.summary.is_empty() {
            selection.summary = selection_summary(&selection);
        }
    }
    if mouse.just_released(MouseButton::Left) {
        selection.origin = None;
        if selection.selecting {
            selection.selecting = false;
            if let Some(rectangle) = selection.rectangle {
                count_selection(&mut selection, &pieces, rectangle);
            }
        }
    }
}

#[derive(Component)]
struct Marquee;
#[derive(Component)]
pub(super) struct SelectionLabel {
    text: String,
    size: Vec2,
}

/// The count is a world mesh, not a screen HUD. A live marquee positions it at
/// a corner; on release its world anchor remains fixed as the camera moves.
#[derive(Resource, Default)]
pub(super) struct SelectionReadout {
    pub anchor: Option<Vec3>,
    pub screen_rect: Option<Rect>,
    pub text: String,
    last_rectangle: Option<Rect>,
}

fn draw_selection(
    mut commands: Commands,
    selection: Res<SelectionState>,
    cameras: Query<Entity, With<PocheUiCamera>>,
    mut boxes: Query<(Entity, &mut Node), With<Marquee>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let rectangle = selection.rectangle.filter(|_| selection.selecting);
    if let Some(rectangle) = rectangle {
        let node = Node {
            position_type: PositionType::Absolute,
            left: px(rectangle.min.x),
            top: px(rectangle.min.y),
            width: px(rectangle.width()),
            height: px(rectangle.height()),
            border: UiRect::all(px(1.5)),
            ..default()
        };
        if let Ok((_, mut current)) = boxes.single_mut() {
            *current = node;
        } else {
            commands.spawn((
                Marquee,
                Pickable::IGNORE,
                UiTargetCamera(camera),
                node,
                BorderColor::all(Color::srgb(0.4, 0.95, 0.78)),
                BackgroundColor(Color::srgba(0.2, 0.9, 0.7, 0.10)),
                GlobalZIndex(21),
            ));
        }
    } else {
        for (entity, _) in &boxes {
            commands.entity(entity).despawn();
        }
    }
}

fn readout_rect(selection: Rect, size: Vec2, viewport: Rect) -> Rect {
    let margin = 8.;
    let above = selection.min.y - size.y - margin;
    let y = if above >= viewport.min.y + margin {
        above
    } else {
        selection.min.y + margin
    };
    let available_min = viewport.min + Vec2::splat(margin);
    let available_max = (viewport.max - size - Vec2::splat(margin)).max(available_min);
    let min = Vec2::new(selection.min.x, y).clamp(available_min, available_max);
    Rect::from_corners(min, min + size)
}

fn on_view_plane(
    camera: &Camera,
    transform: &GlobalTransform,
    screen: Vec2,
    point: Vec3,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(transform, screen).ok()?;
    let distance = ray.intersect_plane(point, InfinitePlane3d::new(*transform.forward()))?;
    Some(ray.get_point(distance))
}

pub(super) fn draw_selection_readout(
    mut commands: Commands,
    selection: Res<SelectionState>,
    mut readout: ResMut<SelectionReadout>,
    cameras: Query<(&Camera, &Transform), (With<TabletopCamera>, Without<SelectionLabel>)>,
    mut labels: Query<(Entity, &SelectionLabel, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    readout.screen_rect = None;
    if selection.summary.is_empty() {
        for (entity, _, _) in &labels {
            commands.entity(entity).despawn();
        }
        *readout = SelectionReadout::default();
        return;
    }
    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    let Some(viewport) = camera
        .logical_viewport_rect()
        .filter(|r| r.size().min_element() > 32.)
    else {
        return;
    };
    let camera_global = GlobalTransform::from(*camera_transform);
    let existing = labels.single().ok();
    let changed = existing.is_none_or(|(_, label, _)| label.text != selection.summary);
    let texture = changed
        .then(|| super::card_label_texture(&mut images, &selection.summary, [242, 247, 226]));
    let natural_size = texture
        .as_ref()
        .map(|(_, aspect)| Vec2::new(18. * aspect + 16., 34.))
        .or_else(|| existing.map(|(_, label, _)| label.size))
        .unwrap();
    // Keep the readout readable even in a narrow window, without overflowing.
    let size = natural_size
        * ((viewport.size() - Vec2::splat(16.)) / natural_size)
            .min_element()
            .min(1.);
    let Some(rectangle) = selection.rectangle else {
        return;
    };
    if readout.anchor.is_none() || selection.selecting || readout.last_rectangle != Some(rectangle)
    {
        let screen = readout_rect(rectangle, size, viewport).center();
        // A floating plane in front of the table keeps the readout out of the
        // surface. Unlike a HUD, the released anchor is a fixed world point.
        let depth = (camera_transform.translation.length() * 0.65).max(0.15);
        let point = camera_transform.translation + *camera_transform.forward() * depth;
        readout.anchor = on_view_plane(camera, &camera_global, screen, point);
        readout.last_rectangle = Some(rectangle);
    }
    let Some(anchor) = readout.anchor else { return };
    let Ok(screen) = camera.world_to_viewport(&camera_global, anchor) else {
        return;
    };
    let Some(neighbour) = on_view_plane(camera, &camera_global, screen + Vec2::Y, anchor) else {
        return;
    };
    let units_per_pixel = neighbour.distance(anchor);
    if !units_per_pixel.is_finite() || units_per_pixel <= 0. {
        return;
    }
    let transform = Transform::from_translation(anchor)
        .with_rotation(camera_transform.rotation)
        .with_scale(Vec3::new(size.x, size.y, 1.) * units_per_pixel);
    if let Some((texture, aspect)) = texture {
        for (entity, _, _) in &labels {
            commands.entity(entity).despawn();
        }
        let text_height = natural_size.y - 16.;
        let root = commands
            .spawn((
                SelectionLabel {
                    text: selection.summary.clone(),
                    size: natural_size,
                },
                Mesh3d(meshes.add(Rectangle::new(1., 1.))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.02, 0.07, 0.06),
                    unlit: true,
                    ..default()
                })),
                transform,
                Pickable::IGNORE,
                NotShadowCaster,
                NotShadowReceiver,
                RenderLayers::layer(0),
            ))
            .id();
        let text = commands
            .spawn((
                Mesh3d(meshes.add(Rectangle::new(
                    text_height * aspect / natural_size.x,
                    text_height / natural_size.y,
                ))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(texture),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(0., 0., 0.5),
                Pickable::IGNORE,
                NotShadowCaster,
                NotShadowReceiver,
                RenderLayers::layer(0),
            ))
            .id();
        commands.entity(root).add_child(text);
    } else if let Ok((_, _, mut current)) = labels.single_mut() {
        *current = transform;
    }
    readout.text.clone_from(&selection.summary);
    readout.screen_rect = Some(Rect::from_center_size(screen, size));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centre_selection_includes_boundary_not_merely_touching_volume() {
        let rect = Rect::from_corners(Vec2::new(10., 20.), Vec2::new(30., 40.));
        assert!(centre_inside(rect, Vec2::new(10., 40.)));
        assert!(!centre_inside(rect, Vec2::new(9.99, 30.)));
        assert!(!centre_inside(rect, Vec2::NAN));
    }
    #[test]
    fn preview_counts_mixed_money_and_deduplicates_inset_cards() {
        let mut state = SelectionState {
            selecting: true,
            ..default()
        };
        let rect = Rect::from_corners(Vec2::ZERO, Vec2::splat(100.));
        let pieces = vec![
            Piece {
                key: "q".into(),
                coin_cents: Some(25),
                points: vec![Vec2::ONE],
            },
            Piece {
                key: "d".into(),
                coin_cents: Some(10),
                points: vec![Vec2::ONE],
            },
            Piece {
                key: "card".into(),
                coin_cents: None,
                points: vec![Vec2::ONE],
            },
            Piece {
                key: "card".into(),
                coin_cents: None,
                points: vec![Vec2::ONE],
            },
        ];
        count_selection(&mut state, &pieces, rect);
        assert_eq!(state.total_cents, 35);
        assert_eq!(state.selected_coin_keys.len(), 2);
        assert_eq!(state.selected_card_keys.len(), 1);
        assert_eq!(state.summary, "2 coins · $0.35 · 1 card");
        state.selecting = false;
        count_selection(&mut state, &pieces, rect);
        assert_eq!(state.total_cents, 35);
        assert_eq!(state.summary, "2 coins · $0.35 · 1 card");
    }
    #[test]
    fn dragging_in_either_direction_selects_the_same_centres() {
        let a = Vec2::new(20., 40.);
        let b = Vec2::new(80., 10.);
        let first = Rect::from_corners(a.min(b), a.max(b));
        let second = Rect::from_corners(b.min(a), b.max(a));
        assert_eq!(first, second);
        assert!(centre_inside(first, Vec2::new(50., 25.)));
    }

    #[test]
    fn floating_count_hugs_selection_corner_and_stays_in_view() {
        let viewport = Rect::from_corners(Vec2::ZERO, Vec2::new(1180., 780.));
        let box_rect = Rect::from_corners(Vec2::new(100., 160.), Vec2::new(600., 500.));
        let label = readout_rect(box_rect, Vec2::new(240., 34.), viewport);
        assert!((label.min.x - box_rect.min.x).abs() < f32::EPSILON);
        assert!((label.max.y + 8. - box_rect.min.y).abs() < f32::EPSILON);
        let at_edge = readout_rect(
            Rect::from_corners(Vec2::new(1100., 0.), Vec2::new(1180., 80.)),
            Vec2::new(240., 34.),
            viewport,
        );
        assert!(viewport.contains(at_edge.min) && viewport.contains(at_edge.max));
    }
}
