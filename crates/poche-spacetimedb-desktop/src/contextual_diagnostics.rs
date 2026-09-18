// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Read-only local observations of the actual contextual controls. Screen
//! coordinates use logical window pixels, matching the file-control pointer.

use super::{
    CanonicalLayout, EscapeMenuPage, TabletopCamera, UiAction, UiScreen, UiState,
    hand_options::HandViewOptions,
    money::MoneyState,
    selection::{SelectionReadout, SelectionState},
    sound_feedback::SoundFeedback,
    world_ui,
};
use bevy::{prelude::*, transform::TransformSystems};
use poche_bevy_spacetimedb::BridgeModel;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContextualDiagnostics {
    pub money_hover: Option<String>,
    pub hovered_coin_key: Option<String>,
    pub world_hover: Option<String>,
    pub selected_coin_keys: Vec<String>,
    pub selected_card_keys: Vec<String>,
    pub selected_cents: u32,
    pub selecting: bool,
    /// [minimum x, minimum y, maximum x, maximum y], in logical pixels.
    pub selection_box: Option<[f32; 4]>,
    pub selection_readout_text: String,
    pub selection_readout_world_anchor: Option<[f32; 3]>,
    pub selection_readout_screen: Option<[f32; 4]>,
    pub selection_readout_is_world_mesh: bool,
    /// Projected centres, not card faces or a new authoritative object list.
    pub projected_coins: Vec<(String, [f32; 2])>,
    pub hand_scale: f32,
    pub hand_popup: bool,
    pub hand_slider: Option<[f32; 4]>,
    /// The viewer's own visible upper-left rank corners, in logical pixels.
    pub hand_rank_corners: Vec<(String, [f32; 2])>,
    pub hovered_card_key: Option<String>,
    pub door_screen: Option<[f32; 2]>,
    pub own_seat_screen: Option<[f32; 2]>,
    pub help_button_screen: Option<[f32; 2]>,
    pub options_button_screen: Option<[f32; 2]>,
    pub sound_pickup_count: u64,
    pub sound_release_count: u64,
    pub sound_audible_count: u64,
    pub escape_menu_page: String,
}

impl Default for ContextualDiagnostics {
    fn default() -> Self {
        Self {
            money_hover: None,
            hovered_coin_key: None,
            world_hover: None,
            selected_coin_keys: Vec::new(),
            selected_card_keys: Vec::new(),
            selected_cents: 0,
            selecting: false,
            selection_box: None,
            selection_readout_text: String::new(),
            selection_readout_world_anchor: None,
            selection_readout_screen: None,
            selection_readout_is_world_mesh: false,
            projected_coins: Vec::new(),
            hand_scale: 1.0,
            hand_popup: false,
            hand_slider: None,
            hand_rank_corners: Vec::new(),
            hovered_card_key: None,
            door_screen: None,
            own_seat_screen: None,
            help_button_screen: None,
            options_button_screen: None,
            sound_pickup_count: 0,
            sound_release_count: 0,
            sound_audible_count: 0,
            escape_menu_page: "closed".into(),
        }
    }
}

pub(super) struct ContextualDiagnosticsPlugin;

impl Plugin for ContextualDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        // Update owns selection, audio and interaction. PostUpdate samples
        // their finished results and propagated camera transforms before the
        // file-control response is written in Last.
        app.add_systems(
            PostUpdate,
            observe_contextual_controls
                .after(TransformSystems::Propagate)
                .after(bevy::ui::UiSystems::Layout),
        );
    }
}

fn rect_array(rectangle: Rect) -> [f32; 4] {
    [
        rectangle.min.x,
        rectangle.min.y,
        rectangle.max.x,
        rectangle.max.y,
    ]
}

pub(super) fn observe_contextual_controls(
    mut state: ResMut<UiState>,
    model: Res<BridgeModel>,
    money: Res<MoneyState>,
    interaction: Res<world_ui::WorldInteraction>,
    selection: Res<SelectionState>,
    readout: Res<SelectionReadout>,
    hand: Res<HandViewOptions>,
    hand_input: Res<super::hand_view::HandInputDiagnostics>,
    sound: Res<SoundFeedback>,
    layout: Res<CanonicalLayout>,
    cameras: Query<(&Camera, &GlobalTransform), With<TabletopCamera>>,
    buttons: Query<(&UiAction, &UiGlobalTransform, &ComputedNode), With<Button>>,
    readout_meshes: Query<(&Mesh3d, Option<&Node>), With<super::selection::SelectionLabel>>,
) {
    let in_table = state.screen == UiScreen::Table;
    let mut result = ContextualDiagnostics {
        hand_scale: hand.scale(),
        hand_popup: in_table && hand.context_open(),
        hand_slider: in_table
            .then(|| hand.slider_rect())
            .flatten()
            .map(rect_array),
        hand_rank_corners: hand_input.corners.clone(),
        hovered_card_key: in_table.then(|| hand_input.hovered_key.clone()).flatten(),
        sound_pickup_count: sound.pickup_count,
        sound_release_count: sound.release_count,
        sound_audible_count: sound.audible_count,
        escape_menu_page: if state.escape_menu_open {
            match state.escape_menu_page {
                EscapeMenuPage::Main => "main",
                EscapeMenuPage::Options => "options",
                EscapeMenuPage::Help => "help",
            }
        } else {
            "closed"
        }
        .into(),
        ..default()
    };
    if in_table {
        if state.escape_menu_open {
            for (action, transform, node) in &buttons {
                if node.size().min_element() <= 0.0 {
                    continue;
                }
                let point = (transform.translation * node.inverse_scale_factor()).to_array();
                match action {
                    UiAction::OpenHelp => result.help_button_screen = Some(point),
                    UiAction::OpenOptions => result.options_button_screen = Some(point),
                    _ => {}
                }
            }
        }
        // Legacy observation remains empty: container counting is selection-only.
        result.hovered_coin_key = money.hovered_coin_key().map(str::to_owned);
        result.world_hover = interaction.hover_hint().map(str::to_owned);
        result.selected_coin_keys = selection.selected_coin_keys.iter().cloned().collect();
        result.selected_card_keys = selection.selected_card_keys.iter().cloned().collect();
        result.selected_coin_keys.sort();
        result.selected_card_keys.sort();
        result.selected_cents = selection.total_cents;
        result.selecting = selection.selecting;
        result.selection_box = selection.rectangle.map(rect_array);
        result.selection_readout_text.clone_from(&readout.text);
        result.selection_readout_world_anchor = readout.anchor.map(|p| p.to_array());
        result.selection_readout_screen = readout.screen_rect.map(rect_array);
        result.selection_readout_is_world_mesh = readout_meshes
            .single()
            .is_ok_and(|(_, node)| node.is_none());
        result.projected_coins = selection
            .projected_coins
            .iter()
            .map(|(key, point)| (key.clone(), point.to_array()))
            .collect();
        result
            .projected_coins
            .sort_by(|left, right| left.0.cmp(&right.0));
        if let Ok((camera, transform)) = cameras.single()
            && camera.is_active
        {
            result.door_screen = project_point(camera, transform, world_ui::door_center());
            result.own_seat_screen = model
                .snapshot
                .own_seat()
                .and_then(|seat| world_ui::seat_pick_center(&layout.0, seat))
                .and_then(|point| project_point(camera, transform, point));
        }
    }
    if state.contextual != result {
        state.contextual = result;
    }
}

fn project_point(camera: &Camera, transform: &GlobalTransform, point: Vec3) -> Option<[f32; 2]> {
    camera
        .world_to_viewport(transform, point)
        .ok()
        .filter(|point| {
            point.is_finite()
                && camera
                    .logical_viewport_rect()
                    .is_some_and(|rect| rect.contains(*point))
        })
        .map(|point| point.to_array())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_defaults_do_not_imply_a_hover_selection_or_open_menu() {
        let diagnostic = ContextualDiagnostics::default();
        assert!((diagnostic.hand_scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(diagnostic.escape_menu_page, "closed");
        assert!(diagnostic.money_hover.is_none());
        assert!(diagnostic.world_hover.is_none());
        assert!(diagnostic.selected_coin_keys.is_empty());
        assert!(!diagnostic.hand_popup);
        assert_eq!(diagnostic.sound_audible_count, 0);
    }

    #[test]
    fn contextual_rectangles_and_observations_round_trip_without_hidden_faces() {
        let rectangle = Rect::from_corners(Vec2::new(10., 20.), Vec2::new(90., 110.));
        let diagnostic = ContextualDiagnostics {
            selection_box: Some(rect_array(rectangle)),
            selected_coin_keys: vec!["public-coin".into()],
            selected_cents: 25,
            ..default()
        };
        assert_eq!(diagnostic.selection_box, Some([10., 20., 90., 110.]));
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert!(!json.contains("face"));
        assert_eq!(
            serde_json::from_str::<ContextualDiagnostics>(&json).unwrap(),
            diagnostic
        );
    }
}
