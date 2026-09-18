// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local inspection only: selecting a projected centre never issues a reducer.
use super::{
    DragState, HandCamera, PocheUiCamera, PoseDisplay, TabletopCamera, UiScreen, UiState,
    animate_and_place_cards, drag_cards, hand_view::HandProjection, mm_position, money::MoneyState,
    world_ui::WorldInteraction,
};
use bevy::prelude::*;
use poche_bevy_spacetimedb::BridgeModel;
use std::collections::HashSet;

pub(super) struct SelectionPlugin;
impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionState>().add_systems(
            Update,
            (update_selection, draw_selection)
                .chain()
                .after(drag_cards)
                .before(animate_and_place_cards),
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
    selection.summary = format!(
        "{} coins · ${}.{:02} · {} cards\nCentres inside the box{}",
        selection.selected_coin_keys.len(),
        selection.total_cents / 100,
        selection.total_cents % 100,
        selection.selected_card_keys.len(),
        if selection.selecting {
            " · release to select"
        } else {
            " · click empty space to clear"
        }
    );
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
                hand.to_inset(world)
            } else {
                world
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
            selection.summary = format!(
                "{} coins · ${}.{:02} · {} cards\nCentres inside the box · click empty space to clear",
                selection.selected_coin_keys.len(),
                selection.total_cents / 100,
                selection.total_cents % 100,
                selection.selected_card_keys.len()
            );
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
struct SelectionLabel;

fn draw_selection(
    mut commands: Commands,
    selection: Res<SelectionState>,
    cameras: Query<Entity, With<PocheUiCamera>>,
    mut boxes: Query<(Entity, &mut Node), With<Marquee>>,
    mut labels: Query<(Entity, &mut Text), With<SelectionLabel>>,
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
    if selection.summary.is_empty() {
        for (entity, _) in &labels {
            commands.entity(entity).despawn();
        }
    } else if let Ok((_, mut text)) = labels.single_mut() {
        if text.0 != selection.summary {
            text.0.clone_from(&selection.summary);
        }
    } else {
        commands.spawn((
            SelectionLabel,
            Pickable::IGNORE,
            UiTargetCamera(camera),
            Text::new(&selection.summary),
            TextFont::from_font_size(17.),
            TextColor(Color::srgb(0.95, 0.97, 0.89)),
            Node {
                position_type: PositionType::Absolute,
                left: px(18.),
                bottom: px(18.),
                padding: UiRect::all(px(9.)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.02, 0.07, 0.06)),
            GlobalZIndex(22),
        ));
    }
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
        assert!(state.summary.contains("release to select"));
        state.selecting = false;
        count_selection(&mut state, &pieces, rect);
        assert_eq!(state.total_cents, 35);
        assert!(state.summary.contains("click empty space"));
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
}
