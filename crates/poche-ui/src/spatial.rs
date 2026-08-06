// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use poche_spatial::{
    CardFace, PlayedCardProjection, PlayerSpatialProjection, RealizationError, SeatId,
    SpatialLayout, SpatialScene, ViewerSpatialProjection, realize_viewer_scene,
};

use crate::{HandPresentation, PresentationModel};

/// Stable adapter failure while turning a UI-safe presentation into a spatial
/// viewer scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationSpatialError {
    /// Public table arrays disagree with the selected layout cardinality.
    TableCardinality,
    /// A required seat is absent or duplicated in public membership data.
    SeatMap,
    /// A hand projection names no seated player or duplicates another hand.
    VisibleHandMap,
    /// An authorized/public card code is outside the standard deck.
    InvalidCard,
    /// Engine-neutral realization rejected the derived projection.
    Realization(RealizationError),
}

/// Realize an exact-recipient [`PresentationModel`] without consulting session
/// authority or another viewer's projection.
///
/// # Errors
///
/// Rejects malformed public cardinalities, member/hand maps, card codes, or a
/// projection that fails the engine-neutral conservation/privacy checks.
pub fn realize_presentation_spatial(
    layout: &SpatialLayout,
    projection_epoch: u64,
    model: &PresentationModel,
) -> Result<SpatialScene, PresentationSpatialError> {
    let player_count = usize::from(layout.id().players());
    let mut member_by_seat = HashMap::new();
    for member in &model.members {
        if let Some(ordinal) = member.seat {
            let seat =
                SeatId::new(ordinal, layout.id()).ok_or(PresentationSpatialError::SeatMap)?;
            if member_by_seat.insert(seat, member).is_some() {
                return Err(PresentationSpatialError::SeatMap);
            }
        }
    }

    let mut visible_by_seat = HashMap::new();
    for hand in model.own_hand.iter().chain(model.granted_hands.iter()) {
        let seat = seat_for_hand(hand, &member_by_seat)?;
        if visible_by_seat.insert(seat, hand).is_some() {
            return Err(PresentationSpatialError::VisibleHandMap);
        }
    }

    let (hand_counts, scores, tricks_won, trump, current_trick) = if let Some(table) = &model.table
    {
        if table.hand_counts.len() != player_count
            || table.scores.len() != player_count
            || table.tricks_won.len() != player_count
        {
            return Err(PresentationSpatialError::TableCardinality);
        }
        let trump = table.trump_code.map(card_face).transpose()?;
        let current_trick = table
            .trick_codes
            .iter()
            .map(|(ordinal, code)| {
                Ok(PlayedCardProjection {
                    seat: SeatId::new(*ordinal, layout.id())
                        .ok_or(PresentationSpatialError::SeatMap)?,
                    face: card_face(*code)?,
                })
            })
            .collect::<Result<Vec<_>, PresentationSpatialError>>()?;
        (
            table.hand_counts.clone(),
            table.scores.clone(),
            table.tricks_won.clone(),
            trump,
            current_trick,
        )
    } else {
        (
            vec![0; player_count],
            vec![0; player_count],
            vec![0; player_count],
            None,
            Vec::new(),
        )
    };

    let mut players = Vec::with_capacity(player_count);
    for ordinal in 0..layout.id().players() {
        let seat = SeatId::new(ordinal, layout.id()).ok_or(PresentationSpatialError::SeatMap)?;
        let member = member_by_seat
            .get(&seat)
            .ok_or(PresentationSpatialError::SeatMap)?;
        let visible_hand = visible_by_seat
            .get(&seat)
            .map(|hand| {
                hand.card_codes
                    .iter()
                    .copied()
                    .map(card_face)
                    .collect::<Result<Vec<_>, PresentationSpatialError>>()
            })
            .transpose()?;
        players.push(PlayerSpatialProjection {
            seat,
            display_name: member.principal.clone(),
            score: scores[usize::from(ordinal)],
            hand_count: hand_counts[usize::from(ordinal)],
            visible_hand,
            tricks_won: tricks_won[usize::from(ordinal)],
        });
    }

    realize_viewer_scene(
        layout,
        &ViewerSpatialProjection {
            projection_epoch,
            players,
            trump,
            current_trick,
            revealed_won_cards: Vec::new(),
        },
    )
    .map_err(PresentationSpatialError::Realization)
}

fn seat_for_hand(
    hand: &HandPresentation,
    members: &HashMap<SeatId, &crate::MemberPresentation>,
) -> Result<SeatId, PresentationSpatialError> {
    members
        .iter()
        .find_map(|(seat, member)| (member.principal == hand.player).then_some(*seat))
        .ok_or(PresentationSpatialError::VisibleHandMap)
}

fn card_face(code: u8) -> Result<CardFace, PresentationSpatialError> {
    CardFace::new(code).ok_or(PresentationSpatialError::InvalidCard)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use poche_protocol::{
        GamePublicStateWire, HandProjection, MemberProjection, PlayedCardWire, PrincipalId,
        ProjectionPayload, PublicGamePhase, PublicTurnWire, RoomPhase,
    };
    use poche_spatial::{CardFace, LayoutId, TableId, TextBinding, registered_layout};

    use crate::{
        ConnectionPresentation, PresentationInput, PresentationModel, realize_presentation_spatial,
    };

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("principal")
    }

    fn model(
        viewer: &str,
        own: Option<HandProjection>,
        grants: Vec<HandProjection>,
    ) -> PresentationModel {
        PresentationModel::from_input(PresentationInput {
            viewer: viewer.to_owned(),
            projection: ProjectionPayload {
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
                    current_trick: vec![PlayedCardWire { seat: 1, card: 25 }],
                    bids: vec![Some(1), Some(0)],
                    tricks_won: vec![0, 0],
                    scores: vec![10, 7],
                    pot_cents: 50,
                }),
                own_hand: own,
                granted_hands: grants,
                public_history: Vec::new(),
            },
            legal_actions: Vec::new(),
            connection: ConnectionPresentation::Replay,
            countdown: None,
            chat: Vec::new(),
            notices: Vec::new(),
        })
    }

    fn visible_faces(scene: &poche_spatial::SpatialScene) -> HashSet<CardFace> {
        scene.cards.iter().filter_map(|card| card.face).collect()
    }

    #[test]
    fn exact_recipient_spatial_projection_preserves_own_and_granted_visibility_only() {
        let layout = registered_layout(TableId::new(12), LayoutId::new(2, 1).expect("layout ID"))
            .expect("layout");
        let alice_hand = HandProjection {
            player: principal("alice"),
            grant_epoch: 1,
            cards: vec![0, 12],
        };
        let alice = model("alice", Some(alice_hand.clone()), Vec::new());
        let plain_spectator = model("viewer", None, Vec::new());
        let granted_spectator = model("viewer", None, vec![alice_hand]);

        let alice_scene = realize_presentation_spatial(&layout, 4, &alice).expect("Alice scene");
        let plain_scene =
            realize_presentation_spatial(&layout, 4, &plain_spectator).expect("plain scene");
        let granted_scene =
            realize_presentation_spatial(&layout, 4, &granted_spectator).expect("granted scene");

        assert_eq!(visible_faces(&alice_scene), visible_faces(&granted_scene));
        assert_eq!(
            visible_faces(&alice_scene)
                .difference(&visible_faces(&plain_scene))
                .map(|face| face.code())
                .collect::<HashSet<_>>(),
            HashSet::from([0, 12])
        );
        assert_eq!(alice_scene.cards.len(), 52);
        assert!(plain_scene.text.iter().all(|text| {
            !matches!(text.binding, TextBinding::CardFace(_))
                || text.text == "A♠"
                || text.text == "A♦"
        }));
    }
}
