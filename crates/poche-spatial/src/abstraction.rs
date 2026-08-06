// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    CardLocation, PlayedCardProjection, PlayerSpatialProjection, RevealedWonCard, SceneError,
    SeatId, SpatialLayout, SpatialScene, TextBinding, ViewerSpatialProjection,
    interpret_score_sheet,
};

const STANDARD_DECK_SIZE: usize = 52;

/// Stable failure while interpreting a canonical viewer scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbstractionError {
    /// Scene and registered layout identify different local frames.
    LayoutMismatch,
    /// The spatial scene violates its structural/privacy contract.
    Scene(SceneError),
    /// The scene does not contain exactly one standard deck partition.
    CardConservation,
    /// Card slot indices are duplicated, missing, or non-contiguous.
    CardSlots,
    /// A hand mixes visible and concealed faces.
    MixedHandVisibility,
    /// Trump/current-play locations lack their required public faces.
    MissingPublicFace,
    /// Deck cards expose faces not represented by the viewer projection.
    UnexpectedDeckFace,
    /// A player name cell is absent or duplicated.
    PlayerName,
    /// Score-sheet text cannot be interpreted canonically.
    PlayerScore,
}

/// Interpret a canonical viewer-scoped spatial scene back into its typed
/// projection.
///
/// # Errors
///
/// Rejects layout mismatch, structural/privacy errors, noncanonical card
/// partitions/slots, mixed hand visibility, or malformed text cells.
pub fn abstract_viewer_scene(
    layout: &SpatialLayout,
    scene: &SpatialScene,
) -> Result<ViewerSpatialProjection, AbstractionError> {
    if layout.id() != scene.layout || layout.table_id() != scene.table_id {
        return Err(AbstractionError::LayoutMismatch);
    }
    if scene.cards.len() != STANDARD_DECK_SIZE {
        return Err(AbstractionError::CardConservation);
    }
    scene.validate().map_err(AbstractionError::Scene)?;
    let (players, revealed_won_cards) = abstract_players(layout, scene)?;

    let trump_cards = scene
        .cards
        .iter()
        .filter(|card| card.location == CardLocation::Trump)
        .collect::<Vec<_>>();
    let trump = match trump_cards.as_slice() {
        [] => None,
        [card] => Some(card.face.ok_or(AbstractionError::MissingPublicFace)?),
        _ => return Err(AbstractionError::CardSlots),
    };
    let current_trick = scene
        .cards
        .iter()
        .filter_map(|card| match card.location {
            CardLocation::Play { seat } => Some(
                card.face
                    .map(|face| PlayedCardProjection { seat, face })
                    .ok_or(AbstractionError::MissingPublicFace),
            ),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    validate_deck(scene)?;
    Ok(ViewerSpatialProjection {
        projection_epoch: scene.projection_epoch,
        players,
        trump,
        current_trick,
        revealed_won_cards,
    })
}

fn abstract_players(
    layout: &SpatialLayout,
    scene: &SpatialScene,
) -> Result<(Vec<PlayerSpatialProjection>, Vec<RevealedWonCard>), AbstractionError> {
    let scores = interpret_score_sheet(scene).map_err(|_| AbstractionError::PlayerScore)?;
    let mut players = Vec::with_capacity(usize::from(layout.id().players()));
    let mut revealed_won_cards = Vec::new();
    for ordinal in 0..layout.id().players() {
        let seat = SeatId::new(ordinal, layout.id()).ok_or(AbstractionError::CardSlots)?;
        let names = scene
            .text
            .iter()
            .filter_map(|text| match text.binding {
                TextBinding::PlayerName(candidate) if candidate == seat => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if names.len() != 1 {
            return Err(AbstractionError::PlayerName);
        }

        let mut hand = scene
            .cards
            .iter()
            .filter_map(|card| match card.location {
                CardLocation::Hand {
                    seat: candidate,
                    index_from_left,
                } if candidate == seat => Some((index_from_left, card.face)),
                _ => None,
            })
            .collect::<Vec<_>>();
        hand.sort_by_key(|(index, _)| *index);
        require_contiguous(hand.iter().map(|(index, _)| *index))?;
        let visible = hand.iter().filter(|(_, face)| face.is_some()).count();
        let visible_hand = match visible {
            0 => None,
            count if count == hand.len() => Some(
                hand.iter()
                    .map(|(_, face)| face.ok_or(AbstractionError::MixedHandVisibility))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            _ => return Err(AbstractionError::MixedHandVisibility),
        };

        let mut won = scene
            .cards
            .iter()
            .filter_map(|card| match card.location {
                CardLocation::Won {
                    seat: candidate,
                    trick,
                    index,
                } if candidate == seat => Some((trick, index, card.face)),
                _ => None,
            })
            .collect::<Vec<_>>();
        won.sort_by_key(|(trick, index, _)| (*trick, *index));
        let tricks_won = validate_won_slots(layout, &won)?;
        for (trick, index, face) in won {
            if let Some(face) = face {
                revealed_won_cards.push(RevealedWonCard {
                    winner: seat,
                    trick,
                    index,
                    face,
                });
            }
        }

        players.push(PlayerSpatialProjection {
            seat,
            display_name: names[0].to_owned(),
            score: scores[usize::from(ordinal)].1,
            hand_count: u8::try_from(hand.len()).map_err(|_| AbstractionError::CardSlots)?,
            visible_hand,
            tricks_won,
        });
    }
    Ok((players, revealed_won_cards))
}

fn validate_deck(scene: &SpatialScene) -> Result<(), AbstractionError> {
    let mut deck = scene
        .cards
        .iter()
        .filter_map(|card| match card.location {
            CardLocation::Deck { index_from_bottom } => Some((index_from_bottom, card.face)),
            _ => None,
        })
        .collect::<Vec<_>>();
    deck.sort_by_key(|(index, _)| *index);
    require_contiguous(deck.iter().map(|(index, _)| *index))?;
    if deck.iter().any(|(_, face)| face.is_some()) {
        return Err(AbstractionError::UnexpectedDeckFace);
    }
    Ok(())
}

fn validate_won_slots(
    layout: &SpatialLayout,
    won: &[(u8, u8, Option<crate::CardFace>)],
) -> Result<u8, AbstractionError> {
    if won.is_empty() {
        return Ok(0);
    }
    let per_trick = usize::from(layout.id().players());
    if !won.len().is_multiple_of(per_trick) {
        return Err(AbstractionError::CardSlots);
    }
    let tricks = won.len() / per_trick;
    for (ordinal, (trick, index, _)) in won.iter().enumerate() {
        if usize::from(*trick) != ordinal / per_trick || usize::from(*index) != ordinal % per_trick
        {
            return Err(AbstractionError::CardSlots);
        }
    }
    u8::try_from(tricks).map_err(|_| AbstractionError::CardSlots)
}

fn require_contiguous(indices: impl IntoIterator<Item = u8>) -> Result<(), AbstractionError> {
    for (expected, actual) in indices.into_iter().enumerate() {
        if usize::from(actual) != expected {
            return Err(AbstractionError::CardSlots);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        CardFace, LayoutId, PlayerSpatialProjection, SeatId, TableId, ViewerSpatialProjection,
        abstract_viewer_scene, realize_viewer_scene, registered_layout,
    };

    #[test]
    fn abstraction_round_trips_canonical_realizations_for_every_layout_cardinality() {
        for players in 2..=8 {
            let id = LayoutId::new(players, 1).expect("layout ID");
            let layout = registered_layout(TableId::new(u64::from(players)), id).expect("layout");
            let projection = ViewerSpatialProjection {
                projection_epoch: 99,
                players: (0..players)
                    .map(|ordinal| PlayerSpatialProjection {
                        seat: SeatId::new(ordinal, id).expect("seat"),
                        display_name: format!("player-{ordinal}"),
                        score: u16::from(ordinal) * 10,
                        hand_count: 1,
                        visible_hand: (ordinal == 0)
                            .then(|| vec![CardFace::new(ordinal).expect("distinct visible face")]),
                        tricks_won: 0,
                    })
                    .collect(),
                trump: Some(CardFace::new(40).expect("trump")),
                current_trick: Vec::new(),
                revealed_won_cards: Vec::new(),
            };
            let scene = realize_viewer_scene(&layout, &projection).expect("scene");
            assert_eq!(abstract_viewer_scene(&layout, &scene), Ok(projection));
        }
    }
}
