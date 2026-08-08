// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_protocol::{
    GameActionWire, ProjectionPayload, PublicGameEventWire, PublicGamePhase, PublicTurnWire,
    RoomPhase,
};

/// Transport-independent status rendered beside one viewer projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionPresentation {
    /// A fixture or local replay with no live transport.
    #[default]
    Replay,
    /// The viewer has an authoritative live stream.
    Connected,
    /// The viewer is retrying with its durable membership.
    Reconnecting,
    /// The viewer has no current authority stream.
    Disconnected,
}

/// Authority-owned countdown values supplied to pure presentation code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountdownPresentation {
    pub logical_now: u64,
    pub deadline_tick: u64,
}

impl CountdownPresentation {
    /// Saturating display estimate; this never triggers the transition.
    #[must_use]
    pub const fn remaining(self) -> u64 {
        self.deadline_tick.saturating_sub(self.logical_now)
    }
}

/// Escaped-at-render-time chat supplied separately from persistent replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatPresentation {
    pub principal: String,
    pub text: String,
}

/// A policy denial or recoverable client error safe for this viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticePresentation {
    pub reason_code: String,
    pub message: String,
}

/// Complete input to the pure presentation reduction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationInput {
    pub viewer: String,
    pub projection: ProjectionPayload,
    pub legal_actions: Vec<GameActionWire>,
    pub connection: ConnectionPresentation,
    pub countdown: Option<CountdownPresentation>,
    pub chat: Vec<ChatPresentation>,
    pub notices: Vec<NoticePresentation>,
}

/// Render-ready member row derived only from public member projection data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberPresentation {
    pub principal: String,
    /// Human-facing name; authorization continues to use `principal`.
    pub display_name: String,
    pub role: &'static str,
    pub seat: Option<u8>,
    pub ready: bool,
    pub connected: bool,
}

/// Render-ready public table state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePresentation {
    pub phase: PublicGamePhase,
    pub actor: String,
    pub round_index: u16,
    pub hand_size: u8,
    pub hand_counts: Vec<u8>,
    /// Exact public trump code retained for typed downstream projections.
    pub trump_code: Option<u8>,
    pub trump: Option<String>,
    /// Exact public current-trick cards retained beside their labels.
    pub trick_codes: Vec<(u8, u8)>,
    pub trick: Vec<(u8, String)>,
    pub bids: Vec<Option<u8>>,
    pub tricks_won: Vec<u8>,
    pub scores: Vec<u16>,
    pub pot_cents: u32,
}

/// One private hand already authorized into the viewer's projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandPresentation {
    pub player: String,
    pub grant_epoch: u64,
    /// Exact card codes already authorized to this viewer.
    pub card_codes: Vec<u8>,
    pub cards: Vec<String>,
}

/// Deterministic, renderer-neutral model for one exact viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationModel {
    pub viewer: String,
    /// Human-facing name for the exact recipient.
    pub viewer_display_name: String,
    pub room_phase: RoomPhase,
    pub connection: ConnectionPresentation,
    pub members: Vec<MemberPresentation>,
    pub table: Option<TablePresentation>,
    pub own_hand: Option<HandPresentation>,
    pub granted_hands: Vec<HandPresentation>,
    pub legal_actions: Vec<GameActionWire>,
    pub history: Vec<String>,
    pub countdown: Option<CountdownPresentation>,
    pub chat: Vec<ChatPresentation>,
    pub notices: Vec<NoticePresentation>,
}

impl PresentationModel {
    /// Reduce an exact-recipient projection and public supplements into a UI model.
    #[must_use]
    pub fn from_input(input: PresentationInput) -> Self {
        let projection = input.projection;
        let viewer_display_name = input.viewer.clone();
        Self {
            viewer: input.viewer,
            viewer_display_name,
            room_phase: projection.phase,
            connection: input.connection,
            members: projection
                .members
                .into_iter()
                .map(|member| MemberPresentation {
                    principal: member.principal_id.as_str().to_owned(),
                    display_name: member.principal_id.as_str().to_owned(),
                    role: if member.host {
                        "coordinator"
                    } else if member.seat.is_some() {
                        "player"
                    } else {
                        "spectator"
                    },
                    seat: member.seat,
                    ready: member.ready,
                    connected: member.connected,
                })
                .collect(),
            table: projection.public_game_state.map(|game| {
                let trump_code = game.trump;
                let trick_codes = game
                    .current_trick
                    .iter()
                    .map(|played| (played.seat, played.card))
                    .collect::<Vec<_>>();
                TablePresentation {
                    phase: game.phase,
                    actor: actor_label(game.actor),
                    round_index: game.round_index,
                    hand_size: game.hand_size,
                    hand_counts: game.hand_counts,
                    trump_code,
                    trump: trump_code.map(card_label),
                    trick: trick_codes
                        .iter()
                        .map(|(seat, card)| (*seat, card_label(*card)))
                        .collect(),
                    trick_codes,
                    bids: game.bids,
                    tricks_won: game.tricks_won,
                    scores: game.scores,
                    pot_cents: game.pot_cents,
                }
            }),
            own_hand: projection.own_hand.map(hand_presentation),
            granted_hands: projection
                .granted_hands
                .into_iter()
                .map(hand_presentation)
                .collect(),
            legal_actions: input.legal_actions,
            history: projection
                .public_history
                .iter()
                .map(history_label)
                .collect(),
            countdown: input.countdown,
            chat: input.chat,
            notices: input.notices,
        }
    }
}

fn hand_presentation(hand: poche_protocol::HandProjection) -> HandPresentation {
    let card_codes = hand.cards;
    HandPresentation {
        player: hand.player.as_str().to_owned(),
        grant_epoch: hand.grant_epoch,
        cards: card_codes.iter().copied().map(card_label).collect(),
        card_codes,
    }
}

fn actor_label(actor: PublicTurnWire) -> String {
    match actor {
        PublicTurnWire::Chance => "chance".to_owned(),
        PublicTurnWire::Player(seat) => format!("player {seat}"),
        PublicTurnWire::Environment => "environment".to_owned(),
        PublicTurnWire::Finished => "finished".to_owned(),
    }
}

fn history_label(event: &PublicGameEventWire) -> String {
    match event {
        PublicGameEventWire::GameStarted { command_id } => {
            format!("game started ({})", command_id.as_str())
        }
        PublicGameEventWire::PlayerAction {
            command_id,
            seat,
            action,
            ..
        } => format!(
            "seat {seat} {} ({})",
            action_label(action),
            command_id.as_str()
        ),
        PublicGameEventWire::RoundScored {
            command_id,
            scores,
            terminal,
            ..
        } => format!(
            "round scored {scores:?}, terminal={terminal} ({})",
            command_id.as_str()
        ),
    }
}

/// Stable human-readable label for one typed player action.
#[must_use]
pub fn action_label(action: &GameActionWire) -> String {
    match action {
        GameActionWire::Bid { tricks } => format!("bid {tricks}"),
        GameActionWire::Play { card } => format!("play {}", card_label(*card)),
    }
}

/// Convert a canonical card code in `0..52` to rank/suit notation.
#[must_use]
pub fn card_label(card: u8) -> String {
    const RANKS: [&str; 13] = [
        "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A",
    ];
    const SUITS: [&str; 4] = ["C", "D", "H", "S"];
    let rank = RANKS.get(usize::from(card % 13)).copied().unwrap_or("?");
    let suit = SUITS.get(usize::from(card / 13)).copied().unwrap_or("?");
    format!("{rank}{suit}")
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        GamePublicStateWire, HandProjection, MemberProjection, PrincipalId, ProjectionPayload,
        PublicGamePhase, PublicTurnWire, RoomPhase,
    };

    use super::{ConnectionPresentation, PresentationInput, PresentationModel, card_label};

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("test principal")
    }

    #[test]
    fn card_labels_cover_the_dense_standard_deck() {
        assert_eq!(card_label(0), "2C");
        assert_eq!(card_label(12), "AC");
        assert_eq!(card_label(13), "2D");
        assert_eq!(card_label(51), "AS");
    }

    #[test]
    fn presentation_contains_only_hands_already_in_the_projection() {
        let projection = ProjectionPayload {
            phase: RoomPhase::Running,
            members: vec![
                MemberProjection {
                    principal_id: principal("alice"),
                    connected: true,
                    seat: Some(0),
                    ready: false,
                    host: true,
                },
                MemberProjection {
                    principal_id: principal("bob"),
                    connected: true,
                    seat: Some(1),
                    ready: false,
                    host: false,
                },
            ],
            public_game_state: Some(GamePublicStateWire {
                schema_version: 1,
                phase: PublicGamePhase::Playing,
                dealer: Some(0),
                actor: PublicTurnWire::Player(0),
                round_index: 0,
                hand_size: 2,
                hand_counts: vec![2, 2],
                trump: Some(51),
                current_trick: Vec::new(),
                bids: vec![None, None],
                tricks_won: vec![0, 0],
                scores: vec![0, 0],
                pot_cents: 50,
            }),
            own_hand: Some(HandProjection {
                player: principal("alice"),
                grant_epoch: 0,
                cards: vec![0, 12],
            }),
            granted_hands: Vec::new(),
            public_history: Vec::new(),
        };
        let input = PresentationInput {
            viewer: "alice".to_owned(),
            projection,
            legal_actions: Vec::new(),
            connection: ConnectionPresentation::Replay,
            countdown: None,
            chat: Vec::new(),
            notices: Vec::new(),
        };
        let model = PresentationModel::from_input(input.clone());

        assert_eq!(
            model.own_hand.as_ref().expect("own hand").cards,
            ["2C", "AC"]
        );
        assert!(model.granted_hands.is_empty());
        assert!(!format!("{model:?}").contains("bob-hand"));
        assert_eq!(model, PresentationModel::from_input(input));
    }
}
