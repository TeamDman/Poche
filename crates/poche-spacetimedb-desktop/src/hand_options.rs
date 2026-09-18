// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Window-local hand presentation. This never changes shared card poses.
use super::{
    DragState, HandCamera, PocheUiCamera, PoseDisplay, UiScreen, UiState, hand_view,
    handle_buttons, mm_position,
};
use bevy::prelude::*;
use poche_bevy_spacetimedb::BridgeModel;

pub(super) const MIN_SCALE: f32 = 0.75;
pub(super) const MAX_SCALE: f32 = 2.5;
const POPUP_SIZE: Vec2 = Vec2::new(300., 118.);
const MAX_HAND_VIEWPORT_FRACTION: f32 = 0.48;

pub(super) struct HandOptionsPlugin;

impl Plugin for HandOptionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HandViewOptions>().add_systems(
            Update,
            (handle_hand_options, sync_hand_options_popup)
                .chain()
                .before(handle_buttons),
        );
    }
}

#[derive(Resource)]
pub(super) struct HandViewOptions {
    scale: f32,
    popup: Option<Rect>,
    dragging_slider: bool,
    captured_right: bool,
    captured_left: bool,
}

impl Default for HandViewOptions {
    fn default() -> Self {
        Self {
            scale: 1.,
            popup: None,
            dragging_slider: false,
            captured_right: false,
            captured_left: false,
        }
    }
}

impl HandViewOptions {
    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn context_open(&self) -> bool {
        self.popup.is_some()
    }

    pub fn slider_rect(&self) -> Option<Rect> {
        self.popup.map(|popup| {
            Rect::from_corners(
                popup.min + Vec2::new(20., 47.),
                Vec2::new(popup.max.x - 20., popup.min.y + 75.),
            )
        })
    }

    pub fn blocks_pointer_input(&self) -> bool {
        self.context_open() || self.captured_left || self.captured_right
    }

    fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() {
            scale.clamp(MIN_SCALE, MAX_SCALE)
        } else {
            1.
        };
    }

    fn open(&mut self, cursor: Vec2, screen_size: Vec2) {
        let size = POPUP_SIZE.min((screen_size - Vec2::splat(16.)).max(Vec2::splat(1.)));
        let mut min = (cursor - Vec2::new(size.x * 0.5, size.y + 12.))
            .max(Vec2::splat(8.))
            .min((screen_size - size - Vec2::splat(8.)).max(Vec2::splat(8.)));
        // Reserve the largest possible hand viewport, not its current size.
        // This keeps the slider stationary while growing cards beneath it and
        // leaves their ranks visible. On very short windows, stay on screen.
        let above_hand = screen_size.y * (1.0 - MAX_HAND_VIEWPORT_FRACTION) - 12. - size.y;
        min.y = min.y.min(above_hand.max(8.));
        self.popup = Some(Rect::from_corners(min, min + size));
        self.captured_right = true;
    }

    fn close(&mut self) {
        self.popup = None;
        self.dragging_slider = false;
        // Keep gesture captures until their release: dismissing a popup must
        // not turn the same held button into an orbit or card/coin drag.
    }

    fn observe_buttons(&mut self, right: bool, left: bool) {
        self.captured_right &= right;
        self.captured_left &= left;
        self.dragging_slider &= left;
    }

    fn set_slider_from_pointer(&mut self, x: f32) {
        if let Some(rect) = self.slider_rect() {
            let fraction = ((x - rect.min.x) / rect.width().max(1.)).clamp(0., 1.);
            self.set_scale(MIN_SCALE + fraction * (MAX_SCALE - MIN_SCALE));
        }
    }
}

/// Size in physical pixels, with a logical-pixel cap so a high-DPI display
/// does not shrink the hand. The viewport always remains on the bottom edge.
pub(super) fn hand_viewport_size(size: UVec2, dpi: f32, scale: f32) -> UVec2 {
    let dpi = if dpi.is_finite() { dpi.max(0.5) } else { 1. };
    let scale = if scale.is_finite() {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.
    };
    let base_height = (size.y as f32 * 0.25).min(190. * dpi);
    let height = (base_height * scale)
        .min(size.y as f32 * MAX_HAND_VIEWPORT_FRACTION)
        .round();
    #[allow(clippy::cast_sign_loss)] // Height is bounded by nonnegative viewport dimensions.
    UVec2::new((size.x * 3 / 5).max(1), (height as u32).max(1))
}

fn handle_hand_options(
    mut options: ResMut<HandViewOptions>,
    state: Res<UiState>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<HandCamera>>,
    model: Res<BridgeModel>,
    poses: Res<PoseDisplay>,
    hand: Res<hand_view::HandProjection>,
    drag: Res<DragState>,
) {
    options.observe_buttons(
        mouse.pressed(MouseButton::Right),
        mouse.pressed(MouseButton::Left),
    );
    if state.screen != UiScreen::Table || state.escape_menu_open {
        options.close();
        return;
    }
    if options.context_open() && keys.just_pressed(KeyCode::Escape) {
        options.close();
        keys.clear_just_pressed(KeyCode::Escape);
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if options.context_open() {
        if mouse.just_pressed(MouseButton::Left) {
            options.captured_left = true;
            if options
                .slider_rect()
                .is_some_and(|rect| rect.contains(cursor))
            {
                options.dragging_slider = true;
            } else if !options.popup.is_some_and(|rect| rect.contains(cursor)) {
                options.close();
            }
        }
        if options.dragging_slider {
            options.set_slider_from_pointer(cursor.x);
        }
        return;
    }
    if !mouse.just_pressed(MouseButton::Right)
        || mouse.pressed(MouseButton::Left)
        || drag.card_key.is_some()
    {
        return;
    }
    let Ok((camera, transform)) = cameras.single() else {
        return;
    };
    if !camera.is_active
        || !camera
            .logical_viewport_rect()
            .is_some_and(|rect| rect.contains(cursor))
    {
        return;
    }
    let Ok(ray) = camera.viewport_to_world(transform, cursor) else {
        return;
    };
    let hits_visible_private_card = model.snapshot.hand.iter().any(|card| {
        let Some(pose) = poses.0.get(&card.card_key) else {
            return false;
        };
        let world = mm_position(pose.current);
        hand.contains(world)
            && hand_view::card_hit(
                ray,
                hand.card_position(&poses, &card.card_key).unwrap(),
                pose.current_rotation,
            )
            .is_some()
    });
    if hits_visible_private_card {
        options.open(cursor, Vec2::new(window.width(), window.height()));
    }
}

#[derive(Component)]
struct HandOptionsPopup;

fn sync_hand_options_popup(
    mut commands: Commands,
    options: Res<HandViewOptions>,
    cameras: Query<Entity, With<PocheUiCamera>>,
    existing: Query<Entity, With<HandOptionsPopup>>,
    mut previous: Local<Option<(Option<Rect>, u32)>>,
) {
    let current = (options.popup, options.scale.to_bits());
    if previous.as_ref() == Some(&current) {
        return;
    }
    *previous = Some(current);
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let (Some(rect), Ok(camera)) = (options.popup, cameras.single()) else {
        return;
    };
    let fraction = (options.scale - MIN_SCALE) / (MAX_SCALE - MIN_SCALE);
    commands
        .spawn((
            HandOptionsPopup,
            UiTargetCamera(camera),
            Pickable::IGNORE,
            GlobalZIndex(95),
            Node {
                position_type: PositionType::Absolute,
                left: px(rect.min.x),
                top: px(rect.min.y),
                width: px(rect.width()),
                height: px(rect.height()),
                ..default()
            },
            BackgroundColor(Color::srgb(0.035, 0.075, 0.08)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Pickable::IGNORE,
                Text::new(format!("Hand size · {:.0}%", options.scale * 100.)),
                TextFont::from_font_size(19.),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(20.),
                    top: px(12.),
                    ..default()
                },
            ));
            panel.spawn((
                Pickable::IGNORE,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(20.),
                    right: px(20.),
                    top: px(58.),
                    height: px(6.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.25, 0.39, 0.39)),
            ));
            panel.spawn((
                Pickable::IGNORE,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(13. + (rect.width() - 40.) * fraction),
                    top: px(49.),
                    width: px(14.),
                    height: px(24.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.52, 0.90, 0.78)),
            ));
            panel.spawn((
                Pickable::IGNORE,
                Text::new("This window only · Esc or click away"),
                TextFont::from_font_size(13.),
                TextColor(Color::srgb(0.72, 0.82, 0.80)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(20.),
                    top: px(89.),
                    ..default()
                },
            ));
        });
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Exact dyadic clamp endpoints, not approximate arithmetic.
mod tests {
    use super::*;

    #[test]
    fn hand_scale_has_finite_bounds_without_mutating_world_geometry() {
        let mut options = HandViewOptions::default();
        options.set_scale(0.);
        assert_eq!(options.scale(), MIN_SCALE);
        options.set_scale(10.);
        assert_eq!(options.scale(), MAX_SCALE);
        options.set_scale(f32::NAN);
        assert_eq!(options.scale(), 1.);
    }

    #[test]
    fn high_dpi_and_slider_scale_increase_pixels_instead_of_hitting_190_pixel_cap() {
        assert_eq!(hand_viewport_size(UVec2::new(1200, 800), 1., 1.).y, 190);
        assert_eq!(hand_viewport_size(UVec2::new(2400, 1600), 2., 1.).y, 380);
        assert_eq!(hand_viewport_size(UVec2::new(3840, 2160), 1., 2.5).y, 475);
        assert!(hand_viewport_size(UVec2::new(390, 844), 1., 2.5).y <= 406);
    }

    #[test]
    fn dismissing_context_retains_entire_right_gesture_without_orbiting() {
        let mut options = HandViewOptions::default();
        options.open(Vec2::new(500., 740.), Vec2::new(1180., 760.));
        options.close();
        assert!(!options.context_open());
        assert!(options.blocks_pointer_input());
        options.observe_buttons(true, false);
        assert!(options.blocks_pointer_input());
        options.observe_buttons(false, false);
        assert!(!options.blocks_pointer_input());
    }

    #[test]
    fn slider_uses_clamped_popup_coordinates_and_captures_dismissal_click() {
        let mut options = HandViewOptions::default();
        options.open(Vec2::new(5., 5.), Vec2::new(390., 844.));
        let rect = options.popup.unwrap();
        assert!(rect.min.cmpge(Vec2::ZERO).all());
        assert!(rect.max.cmple(Vec2::new(390., 844.)).all());
        let slider = options.slider_rect().unwrap();
        options.set_slider_from_pointer(slider.max.x);
        assert_eq!(options.scale(), MAX_SCALE);
        options.set_slider_from_pointer(slider.min.x);
        assert_eq!(options.scale(), MIN_SCALE);
        options.captured_left = true;
        options.close();
        options.observe_buttons(false, true);
        assert!(options.blocks_pointer_input());
        options.observe_buttons(false, false);
        assert!(!options.blocks_pointer_input());
    }

    #[test]
    fn popup_stays_above_largest_hand_and_slider_does_not_move_when_resizing() {
        for screen in [Vec2::new(1280., 720.), Vec2::new(3840., 2160.)] {
            let mut options = HandViewOptions::default();
            options.open(Vec2::new(screen.x * 0.5, screen.y - 12.), screen);
            let popup = options.popup.unwrap();
            let slider = options.slider_rect();
            assert!(popup.max.y <= screen.y * (1.0 - MAX_HAND_VIEWPORT_FRACTION) - 12.);
            assert!(popup.min.cmpge(Vec2::ZERO).all());
            assert!(popup.max.cmple(screen).all());
            for scale in [MIN_SCALE, 1., 2., MAX_SCALE] {
                options.set_scale(scale);
                assert_eq!(options.popup, Some(popup));
                assert_eq!(options.slider_rect(), slider);
            }
        }
    }

    #[test]
    fn popup_remains_inside_short_window_when_hand_clearance_cannot_fit() {
        let mut options = HandViewOptions::default();
        let screen = Vec2::new(320., 180.);
        options.open(Vec2::new(160., 176.), screen);
        let popup = options.popup.unwrap();
        assert!(popup.min.cmpge(Vec2::ZERO).all());
        assert!(popup.max.cmple(screen).all());
    }
}
