// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use eframe::egui;
use poche_protocol::{CommandPayload, GameActionWire, RoomPhase};

use crate::{
    ConnectionPresentation, EMBEDDED_REPLAY, HandPresentation, LiveClientPresentation,
    PresentationModel, ReplayDeck, action_label,
};

/// Read-only fixture client used to prove the shared native/web egui path.
pub struct PocheReplayApp {
    deck: ReplayDeck,
    selected: usize,
}

impl PocheReplayApp {
    /// Decode a replay fixture and select its first checkpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayDeck::from_json`] failures.
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(Self {
            deck: ReplayDeck::from_json(json)?,
            selected: 0,
        })
    }

    /// Construct the checked-in static replay.
    ///
    /// # Panics
    ///
    /// Panics only if the repository's checked fixture no longer decodes.
    #[must_use]
    pub fn embedded() -> Self {
        Self::from_json(EMBEDDED_REPLAY).expect("the checked replay fixture must decode")
    }

    fn step(&mut self, offset: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(offset)
            .min(self.deck.checkpoints.len().saturating_sub(1));
    }
}

impl eframe::App for PocheReplayApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("replay-navigation").show(context, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Poche projection replay");
                if ui.button("Previous").clicked() {
                    self.step(-1);
                }
                if ui.button("Next").clicked() {
                    self.step(1);
                }
                let checkpoint = &self.deck.checkpoints[self.selected];
                ui.label(format!(
                    "checkpoint {}/{} · source step {} · viewer {}",
                    self.selected + 1,
                    self.deck.checkpoints.len(),
                    checkpoint.step_index,
                    checkpoint.viewer
                ));
            });
            ui.horizontal_wrapped(|ui| {
                for viewer in ["host", "alice", "bob"] {
                    if ui.button(format!("First {viewer} view")).clicked()
                        && let Some(index) = self.deck.first_for_viewer(viewer)
                    {
                        self.selected = index;
                    }
                }
            });
        });

        egui::CentralPanel::default().show(context, |ui| {
            let checkpoint = &self.deck.checkpoints[self.selected];
            ui.label(format!("fixture: {}", self.deck.fixture_id));
            ui.label(format!("authorization: {}", checkpoint.authorization));
            ui.label(format!("outcome: {}", checkpoint.outcome));
            if !checkpoint.events.is_empty() {
                ui.label(format!("events: {}", checkpoint.events.join(", ")));
            }
            ui.separator();
            render_projection(ui, &checkpoint.presentation);
        });
    }
}

fn render_projection(ui: &mut egui::Ui, model: &PresentationModel) {
    ui.heading(format!(
        "{} · {} · {}",
        model.viewer,
        phase_label(model.room_phase),
        connection_label(model.connection)
    ));

    ui.collapsing("Members", |ui| {
        egui::Grid::new("member-grid").striped(true).show(ui, |ui| {
            ui.strong("Identity");
            ui.strong("Role");
            ui.strong("Seat");
            ui.strong("Ready");
            ui.strong("Connected");
            ui.end_row();
            for member in &model.members {
                ui.label(&member.principal);
                ui.label(member.role);
                ui.label(
                    member
                        .seat
                        .map_or_else(|| "—".to_owned(), |seat| seat.to_string()),
                );
                ui.label(if member.ready { "yes" } else { "no" });
                ui.label(if member.connected { "yes" } else { "no" });
                ui.end_row();
            }
        });
    });

    if let Some(countdown) = model.countdown {
        ui.separator();
        ui.heading("Countdown");
        ui.label(format!(
            "authority tick {} · deadline {} · {} ticks remaining",
            countdown.logical_now,
            countdown.deadline_tick,
            countdown.remaining()
        ));
    }

    if let Some(table) = &model.table {
        ui.separator();
        ui.heading("Public table");
        ui.label(format!(
            "phase {:?} · round {} · actor {} · pot ${:.2}",
            table.phase,
            table.round_index,
            table.actor,
            f64::from(table.pot_cents) / 100.0
        ));
        ui.label(format!("scores: {:?}", table.scores));
        ui.label(format!("cards remaining: {:?}", table.hand_counts));
        ui.label(format!(
            "bids: {:?} · tricks: {:?}",
            table.bids, table.tricks_won
        ));
        if let Some(trump) = &table.trump {
            ui.label(format!("trump: {trump}"));
        }
        if !table.trick.is_empty() {
            ui.label(format!("trick: {:?}", table.trick));
        }
    }

    if let Some(hand) = &model.own_hand {
        render_hand(ui, "Your hand", hand);
    }
    for hand in &model.granted_hands {
        render_hand(ui, "Granted spectator view", hand);
    }

    if !model.legal_actions.is_empty() {
        ui.separator();
        ui.heading("Legal actions");
        ui.horizontal_wrapped(|ui| {
            for action in &model.legal_actions {
                typed_action_button(ui, action);
            }
        });
    }

    if !model.history.is_empty() {
        ui.separator();
        ui.collapsing("Public history", |ui| {
            for item in &model.history {
                ui.label(item);
            }
        });
    }

    if !model.chat.is_empty() {
        ui.separator();
        ui.collapsing("Chat", |ui| {
            for message in &model.chat {
                ui.label(format!("{}: {}", message.principal, message.text));
            }
        });
    }

    for notice in &model.notices {
        ui.label(format!("{}: {}", notice.reason_code, notice.message));
    }
}

/// Render the complete live-client shell and return at most one typed command.
///
/// The caller submits the returned payload through the normal authority path;
/// rendering never mutates session or game state directly.
#[must_use]
pub fn render_live_client(
    ui: &mut egui::Ui,
    live: &LiveClientPresentation,
) -> Option<CommandPayload> {
    ui.heading("Live room client");
    ui.label(format!("identity: {}", live.projection.viewer));
    ui.label(format!("room: {}", live.room_id));
    if let Some(code) = &live.room_code {
        ui.label(format!("join code: {code}"));
    }
    if !live.hand_requests.is_empty() {
        ui.collapsing("Pending hand requests", |ui| {
            for request in &live.hand_requests {
                ui.label(format!(
                    "{} requests {} hand ({})",
                    request.recipient.as_str(),
                    request.player.as_str(),
                    request.request_id.as_str()
                ));
            }
        });
    }
    if !live.hand_grants.is_empty() {
        ui.collapsing("Active hand grants", |ui| {
            for grant in &live.hand_grants {
                ui.label(format!(
                    "{} may view {} hand from epoch {}",
                    grant.recipient.as_str(),
                    grant.player.as_str(),
                    grant.grant_epoch
                ));
            }
        });
    }
    if let Some(href) = &live.transcript_href {
        ui.hyperlink_to("Export canonical transcript", href);
    }
    if let Some(href) = &live.replay_href {
        ui.hyperlink_to("Replay exact projection history", href);
    }
    let mut selected = None;
    ui.horizontal_wrapped(|ui| {
        for control in &live.controls {
            if ui.button(&control.label).clicked() && selected.is_none() {
                selected = Some(control.payload.clone());
            }
        }
    });
    ui.separator();
    render_projection(ui, &live.projection);
    selected
}

fn render_hand(ui: &mut egui::Ui, heading: &str, hand: &HandPresentation) {
    ui.separator();
    ui.heading(format!("{heading}: {}", hand.player));
    ui.horizontal_wrapped(|ui| {
        for card in &hand.cards {
            ui.label(egui::RichText::new(card).monospace().strong());
        }
    });
}

fn typed_action_button(ui: &mut egui::Ui, action: &GameActionWire) {
    let _response = ui.add_enabled(false, egui::Button::new(action_label(action)));
}

const fn phase_label(phase: RoomPhase) -> &'static str {
    match phase {
        RoomPhase::Lobby => "lobby",
        RoomPhase::Countdown => "countdown",
        RoomPhase::Running => "running",
        RoomPhase::Paused => "paused",
        RoomPhase::PostGame => "post-game",
        RoomPhase::Closed => "closed",
    }
}

const fn connection_label(connection: ConnectionPresentation) -> &'static str {
    match connection {
        ConnectionPresentation::Replay => "static replay",
        ConnectionPresentation::Connected => "connected",
        ConnectionPresentation::Reconnecting => "reconnecting",
        ConnectionPresentation::Disconnected => "disconnected",
    }
}

#[cfg(test)]
mod tests {
    use super::PocheReplayApp;

    #[test]
    fn checked_fixture_constructs_the_egui_app() {
        let _app = PocheReplayApp::embedded();
    }
}
