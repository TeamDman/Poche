// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use crate::{
    CardFace, CardLocation, CardObject, CardObjectId, HalfExtentsMm, ObjectId, Point3Mm, PoseMm,
    SPATIAL_SCHEMA_VERSION, SceneError, SeatId, SpatialLayout, SpatialScene, SurfaceId,
    SurfaceKind, TextBinding, TextRun, TextRunId, YawMilliDegrees,
};

const STANDARD_DECK_SIZE: usize = 52;
const MAX_HAND_SIZE: u8 = 7;
const CARD_HALF_EXTENTS: HalfExtentsMm = HalfExtentsMm::new(32, 1, 44);

/// Exact public and viewer-authorized data for one seated player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSpatialProjection {
    /// Stable seat within the selected layout.
    pub seat: SeatId,
    /// Public display name.
    pub display_name: String,
    /// Public cumulative game score.
    pub score: u16,
    /// Public number of cards still held.
    pub hand_count: u8,
    /// Exact viewer-authorized hand, or `None` when faces are concealed.
    pub visible_hand: Option<Vec<CardFace>>,
    /// Public number of completed tricks captured this round.
    pub tricks_won: u8,
}

/// Public card currently in the incomplete trick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayedCardProjection {
    /// Seat that played this card.
    pub seat: SeatId,
    /// Public face.
    pub face: CardFace,
}

/// Optional public face reconstruction for a card in a completed trick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RevealedWonCard {
    /// Seat that captured the trick.
    pub winner: SeatId,
    /// Zero-based trick ordinal captured by that seat.
    pub trick: u8,
    /// Card index within the complete trick.
    pub index: u8,
    /// Publicly reconstructed face.
    pub face: CardFace,
}

/// Minimal exact-recipient input to spatial realization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerSpatialProjection {
    /// Privacy epoch used to issue unlinkable card object handles.
    pub projection_epoch: u64,
    /// One complete row for every seat in the layout.
    pub players: Vec<PlayerSpatialProjection>,
    /// Face-up round trump, absent before a deal or after the game.
    pub trump: Option<CardFace>,
    /// Public cards in the current incomplete trick.
    pub current_trick: Vec<PlayedCardProjection>,
    /// Publicly reconstructable faces for already captured cards. Omitted
    /// entries still realize as backs; omission cannot reveal a secret.
    pub revealed_won_cards: Vec<RevealedWonCard>,
}

/// Stable failure category for viewer-safe realization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealizationError {
    /// Seats are missing, duplicated, or outside the layout.
    PlayerSet,
    /// Public or visible hand cardinality is impossible.
    HandCardinality,
    /// Current trick seats are duplicated or outside the layout.
    CurrentTrick,
    /// A revealed captured-card slot is invalid or duplicated.
    RevealedWonCard,
    /// Public counts cannot partition one standard 52-card deck.
    CardConservation,
    /// The projection repeats a face that is already known elsewhere.
    DuplicateKnownFace,
    /// A deterministic card/text pose exceeds the spatial contract.
    Geometry,
    /// The resulting scene violates a structural/privacy invariant.
    Scene(SceneError),
}

/// Stable score-sheet interpretation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreSheetError {
    /// The scene itself is structurally invalid.
    Scene(SceneError),
    /// A seat has zero or multiple score cells.
    ScoreCardinality,
    /// A score cell is not a canonical unsigned decimal value.
    InvalidScore,
}

/// Realize one exact-recipient projection into a deterministic 52-card scene.
///
/// This function has no authority-state input. A concealed hand is represented
/// by opaque card objects with no face value and no face text.
///
/// # Errors
///
/// Returns a stable projection, conservation, geometry, or scene error without
/// partially returning a scene.
pub fn realize_viewer_scene(
    layout: &SpatialLayout,
    projection: &ViewerSpatialProjection,
) -> Result<SpatialScene, RealizationError> {
    validate_projection(layout, projection)?;

    let players = ordered_players(layout, projection)?;
    let cards = realize_cards(layout, projection, &players)?;
    let text = realize_text(layout, &players, &cards)?;

    let scene = SpatialScene {
        schema_version: SPATIAL_SCHEMA_VERSION,
        table_id: layout.table_id(),
        layout: layout.id(),
        projection_epoch: projection.projection_epoch,
        objects: layout.scene_objects(),
        cards,
        text,
    };
    scene.validate().map_err(RealizationError::Scene)?;
    Ok(scene)
}

/// Interpret the typed score cells rendered on the score sheet.
///
/// # Errors
///
/// Rejects an invalid scene, missing/duplicate score cells, non-decimal text,
/// leading zeroes, or a row outside the selected layout.
pub fn interpret_score_sheet(scene: &SpatialScene) -> Result<Vec<(SeatId, u16)>, ScoreSheetError> {
    scene.validate().map_err(ScoreSheetError::Scene)?;
    let mut scores = Vec::with_capacity(usize::from(scene.layout.players()));
    for ordinal in 0..scene.layout.players() {
        let Some(seat) = SeatId::new(ordinal, scene.layout) else {
            return Err(ScoreSheetError::ScoreCardinality);
        };
        let cells = scene
            .text
            .iter()
            .filter(|text| text.binding == TextBinding::PlayerScore(seat))
            .collect::<Vec<_>>();
        if cells.len() != 1 {
            return Err(ScoreSheetError::ScoreCardinality);
        }
        let cell = &cells[0].text;
        let score = cell
            .parse::<u16>()
            .map_err(|_| ScoreSheetError::InvalidScore)?;
        if score.to_string() != *cell {
            return Err(ScoreSheetError::InvalidScore);
        }
        scores.push((seat, score));
    }
    Ok(scores)
}

fn realize_cards(
    layout: &SpatialLayout,
    projection: &ViewerSpatialProjection,
    players: &[&PlayerSpatialProjection],
) -> Result<Vec<CardObject>, RealizationError> {
    let mut cards = Vec::with_capacity(STANDARD_DECK_SIZE);
    if let Some(face) = projection.trump {
        push_card(
            &mut cards,
            projection.projection_epoch,
            CardLocation::Trump,
            card_pose(layout, CardLocation::Trump, 1)?,
            Some(face),
        )?;
    }
    realize_hands(layout, projection.projection_epoch, players, &mut cards)?;
    realize_current_trick(layout, projection, &mut cards)?;
    realize_won_cards(layout, projection, players, &mut cards)?;
    realize_deck(layout, projection.projection_epoch, &mut cards)?;
    Ok(cards)
}

fn realize_hands(
    layout: &SpatialLayout,
    projection_epoch: u64,
    players: &[&PlayerSpatialProjection],
    cards: &mut Vec<CardObject>,
) -> Result<(), RealizationError> {
    for player in players {
        for index in 0..player.hand_count {
            let face = player
                .visible_hand
                .as_ref()
                .and_then(|hand| hand.get(usize::from(index)))
                .copied();
            let location = CardLocation::Hand {
                seat: player.seat,
                index_from_left: index,
            };
            push_card(
                cards,
                projection_epoch,
                location,
                card_pose(layout, location, player.hand_count)?,
                face,
            )?;
        }
    }
    Ok(())
}

fn realize_current_trick(
    layout: &SpatialLayout,
    projection: &ViewerSpatialProjection,
    cards: &mut Vec<CardObject>,
) -> Result<(), RealizationError> {
    for played in &projection.current_trick {
        let location = CardLocation::Play { seat: played.seat };
        push_card(
            cards,
            projection.projection_epoch,
            location,
            card_pose(layout, location, 1)?,
            Some(played.face),
        )?;
    }
    Ok(())
}

fn realize_won_cards(
    layout: &SpatialLayout,
    projection: &ViewerSpatialProjection,
    players: &[&PlayerSpatialProjection],
    cards: &mut Vec<CardObject>,
) -> Result<(), RealizationError> {
    for player in players {
        for trick in 0..player.tricks_won {
            for index in 0..layout.id().players() {
                let location = CardLocation::Won {
                    seat: player.seat,
                    trick,
                    index,
                };
                let face = projection
                    .revealed_won_cards
                    .iter()
                    .find(|revealed| {
                        revealed.winner == player.seat
                            && revealed.trick == trick
                            && revealed.index == index
                    })
                    .map(|revealed| revealed.face);
                push_card(
                    cards,
                    projection.projection_epoch,
                    location,
                    card_pose(layout, location, 1)?,
                    face,
                )?;
            }
        }
    }
    Ok(())
}

fn realize_deck(
    layout: &SpatialLayout,
    projection_epoch: u64,
    cards: &mut Vec<CardObject>,
) -> Result<(), RealizationError> {
    let deck_count = STANDARD_DECK_SIZE
        .checked_sub(cards.len())
        .ok_or(RealizationError::CardConservation)?;
    let location_count =
        u8::try_from(deck_count).map_err(|_| RealizationError::CardConservation)?;
    for index in 0..deck_count {
        let index_from_bottom =
            u8::try_from(index).map_err(|_| RealizationError::CardConservation)?;
        let location = CardLocation::Deck { index_from_bottom };
        push_card(
            cards,
            projection_epoch,
            location,
            card_pose(layout, location, location_count)?,
            None,
        )?;
    }
    Ok(())
}

fn realize_text(
    layout: &SpatialLayout,
    players: &[&PlayerSpatialProjection],
    cards: &[CardObject],
) -> Result<Vec<TextRun>, RealizationError> {
    let mut text = Vec::new();
    for card in cards {
        if let Some(face) = card.face {
            text.push(TextRun {
                id: next_text_id(&text)?,
                binding: TextBinding::CardFace(card.id),
                attached_to: SurfaceId {
                    object: ObjectId::Card(card.id),
                    kind: SurfaceKind::Face,
                },
                local_pose: local_text_pose(0, 2, 0)?,
                text: face.label(),
            });
        }
    }
    for (row, player) in players.iter().enumerate() {
        let row = i32::try_from(row).map_err(|_| RealizationError::Geometry)?;
        let rows = i32::from(layout.id().players());
        let z = (2 * row + 1 - rows) * 14;
        text.push(TextRun {
            id: next_text_id(&text)?,
            binding: TextBinding::PlayerName(player.seat),
            attached_to: SurfaceId {
                object: ObjectId::ScoreSheet,
                kind: SurfaceKind::Face,
            },
            local_pose: local_text_pose(-45, 2, z)?,
            text: player.display_name.clone(),
        });
        text.push(TextRun {
            id: next_text_id(&text)?,
            binding: TextBinding::PlayerScore(player.seat),
            attached_to: SurfaceId {
                object: ObjectId::ScoreSheet,
                kind: SurfaceKind::Face,
            },
            local_pose: local_text_pose(55, 2, z)?,
            text: player.score.to_string(),
        });
    }
    Ok(text)
}

fn validate_projection(
    layout: &SpatialLayout,
    projection: &ViewerSpatialProjection,
) -> Result<(), RealizationError> {
    let players = ordered_players(layout, projection)?;
    let mut known_faces = HashSet::new();
    if let Some(trump) = projection.trump
        && !known_faces.insert(trump)
    {
        return Err(RealizationError::DuplicateKnownFace);
    }

    let mut accounted = usize::from(projection.trump.is_some());
    for player in &players {
        if player.hand_count > MAX_HAND_SIZE {
            return Err(RealizationError::HandCardinality);
        }
        accounted += usize::from(player.hand_count);
        if let Some(hand) = &player.visible_hand {
            if hand.len() != usize::from(player.hand_count) {
                return Err(RealizationError::HandCardinality);
            }
            for face in hand {
                if !known_faces.insert(*face) {
                    return Err(RealizationError::DuplicateKnownFace);
                }
            }
        }
        accounted += usize::from(player.tricks_won) * usize::from(layout.id().players());
    }

    let mut trick_seats = HashSet::new();
    if projection.current_trick.len() >= usize::from(layout.id().players()) {
        return Err(RealizationError::CurrentTrick);
    }
    for played in &projection.current_trick {
        if played.seat.get() >= layout.id().players() || !trick_seats.insert(played.seat) {
            return Err(RealizationError::CurrentTrick);
        }
        if !known_faces.insert(played.face) {
            return Err(RealizationError::DuplicateKnownFace);
        }
    }
    accounted += projection.current_trick.len();

    let mut won_slots = HashSet::new();
    for revealed in &projection.revealed_won_cards {
        let Some(player) = players.iter().find(|player| player.seat == revealed.winner) else {
            return Err(RealizationError::RevealedWonCard);
        };
        if revealed.trick >= player.tricks_won
            || revealed.index >= layout.id().players()
            || !won_slots.insert((revealed.winner, revealed.trick, revealed.index))
        {
            return Err(RealizationError::RevealedWonCard);
        }
        if !known_faces.insert(revealed.face) {
            return Err(RealizationError::DuplicateKnownFace);
        }
    }

    if projection.trump.is_none() && accounted != 0 {
        return Err(RealizationError::CardConservation);
    }
    if accounted > STANDARD_DECK_SIZE {
        return Err(RealizationError::CardConservation);
    }
    Ok(())
}

fn ordered_players<'a>(
    layout: &SpatialLayout,
    projection: &'a ViewerSpatialProjection,
) -> Result<Vec<&'a PlayerSpatialProjection>, RealizationError> {
    let mut ordered = Vec::with_capacity(usize::from(layout.id().players()));
    for ordinal in 0..layout.id().players() {
        let seat = SeatId::new(ordinal, layout.id()).expect("layout ordinal");
        let matches = projection
            .players
            .iter()
            .filter(|player| player.seat == seat)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].display_name.is_empty() {
            return Err(RealizationError::PlayerSet);
        }
        ordered.push(matches[0]);
    }
    if ordered.len() != projection.players.len() {
        return Err(RealizationError::PlayerSet);
    }
    Ok(ordered)
}

fn push_card(
    cards: &mut Vec<CardObject>,
    projection_epoch: u64,
    location: CardLocation,
    pose: PoseMm,
    face: Option<CardFace>,
) -> Result<(), RealizationError> {
    let ordinal = u8::try_from(cards.len()).map_err(|_| RealizationError::CardConservation)?;
    cards.push(CardObject {
        id: CardObjectId {
            projection_epoch,
            ordinal,
        },
        location,
        pose,
        half_extents: CARD_HALF_EXTENTS,
        face,
    });
    Ok(())
}

pub(crate) fn card_pose(
    layout: &SpatialLayout,
    location: CardLocation,
    location_count: u8,
) -> Result<PoseMm, RealizationError> {
    let zone = layout
        .zones()
        .iter()
        .find(|zone| zone.id == location.zone())
        .ok_or(RealizationError::Geometry)?;
    let center = Point3Mm::new(
        i32::midpoint(zone.inner.min.x.get(), zone.inner.max.x.get()),
        i32::midpoint(zone.inner.min.y.get(), zone.inner.max.y.get()),
        i32::midpoint(zone.inner.min.z.get(), zone.inner.max.z.get()),
    );
    let (x, y, z) = match location {
        CardLocation::Deck { index_from_bottom } => (
            center.x.get(),
            center.y.get() + i32::from(index_from_bottom) - i32::from(location_count) / 2,
            center.z.get(),
        ),
        CardLocation::Trump => (center.x.get(), center.y.get(), center.z.get()),
        CardLocation::Hand {
            index_from_left, ..
        } => {
            let offset = (2 * i32::from(index_from_left) + 1 - i32::from(location_count)) * 7;
            (center.x.get() + offset, center.y.get(), center.z.get())
        }
        CardLocation::Play { seat } => {
            let placement = layout
                .seats()
                .iter()
                .find(|placement| placement.seat == seat)
                .ok_or(RealizationError::Geometry)?;
            (
                center.x.get() + placement.seat_pose.translation.x.get() * 50 / 750,
                center.y.get(),
                center.z.get() + placement.seat_pose.translation.z.get() * 50 / 750,
            )
        }
        CardLocation::Won { trick, index, .. } => (
            center.x.get() + (i32::from(index) - i32::from(layout.id().players()) / 2) * 2,
            center.y.get() + i32::from(trick),
            center.z.get(),
        ),
    };
    PoseMm::checked(Point3Mm::new(x, y, z), YawMilliDegrees::new(0))
        .ok_or(RealizationError::Geometry)
}

fn local_text_pose(x: i32, y: i32, z: i32) -> Result<PoseMm, RealizationError> {
    PoseMm::checked(Point3Mm::new(x, y, z), YawMilliDegrees::new(0))
        .ok_or(RealizationError::Geometry)
}

fn next_text_id(text: &[TextRun]) -> Result<TextRunId, RealizationError> {
    u16::try_from(text.len())
        .map(TextRunId)
        .map_err(|_| RealizationError::Geometry)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{
        CardFace, CardLocation, LayoutId, ObjectId, SeatId, TableId, TextBinding,
        ZoneClassification, interpret_score_sheet, realize_viewer_scene, registered_layout,
    };

    use super::{PlayedCardProjection, PlayerSpatialProjection, ViewerSpatialProjection};

    fn face(code: u8) -> CardFace {
        CardFace::new(code).expect("standard face")
    }

    fn fixture(reveal_second_hand: bool) -> (crate::SpatialLayout, ViewerSpatialProjection) {
        let layout_id = LayoutId::new(2, 1).expect("layout ID");
        let seat_zero = SeatId::new(0, layout_id).expect("seat zero");
        let seat_one = SeatId::new(1, layout_id).expect("seat one");
        let layout = registered_layout(TableId::new(9), layout_id).expect("layout");
        let projection = ViewerSpatialProjection {
            projection_epoch: 17,
            players: vec![
                PlayerSpatialProjection {
                    seat: seat_zero,
                    display_name: "Alice".to_owned(),
                    score: 10,
                    hand_count: 2,
                    visible_hand: Some(vec![face(0), face(51)]),
                    tricks_won: 0,
                },
                PlayerSpatialProjection {
                    seat: seat_one,
                    display_name: "Bob".to_owned(),
                    score: 7,
                    hand_count: 2,
                    visible_hand: reveal_second_hand.then(|| vec![face(1), face(2)]),
                    tricks_won: 0,
                },
            ],
            trump: Some(face(13)),
            current_trick: vec![PlayedCardProjection {
                seat: seat_one,
                face: face(25),
            }],
            revealed_won_cards: Vec::new(),
        };
        (layout, projection)
    }

    #[test]
    fn realization_partitions_all_52_objects_and_registered_endpoints_classify() {
        let (layout, projection) = fixture(false);
        let scene = realize_viewer_scene(&layout, &projection).expect("realized scene");
        assert_eq!(scene.cards.len(), 52);
        assert_eq!(
            scene
                .cards
                .iter()
                .filter(|card| matches!(card.location, CardLocation::Deck { .. }))
                .count(),
            46
        );
        for card in scene
            .cards
            .iter()
            .filter(|card| !matches!(card.location, CardLocation::Deck { .. }))
        {
            let bounds = crate::AabbMm::from_center(card.pose.translation, card.half_extents)
                .expect("card bound");
            assert_eq!(
                layout.classify_bounds(bounds),
                ZoneClassification::Snapped(card.location.zone())
            );
        }
    }

    #[test]
    fn visibility_adds_only_exact_authorized_hand_faces_and_text() {
        let (layout, concealed) = fixture(false);
        let (_, granted) = fixture(true);
        let concealed_scene = realize_viewer_scene(&layout, &concealed).expect("concealed scene");
        let granted_scene = realize_viewer_scene(&layout, &granted).expect("granted scene");
        let concealed_faces = concealed_scene
            .cards
            .iter()
            .filter_map(|card| card.face)
            .collect::<HashSet<_>>();
        let granted_faces = granted_scene
            .cards
            .iter()
            .filter_map(|card| card.face)
            .collect::<HashSet<_>>();
        assert_eq!(
            concealed_faces,
            HashSet::from([face(0), face(51), face(13), face(25)])
        );
        assert_eq!(
            granted_faces
                .difference(&concealed_faces)
                .copied()
                .collect::<HashSet<_>>(),
            HashSet::from([face(1), face(2)])
        );
        for card in concealed_scene
            .cards
            .iter()
            .filter(|card| card.face.is_none())
        {
            assert!(!concealed_scene.text.iter().any(|text| {
                text.binding == TextBinding::CardFace(card.id)
                    || text.attached_to.object == ObjectId::Card(card.id)
            }));
        }
    }

    #[test]
    fn text_attachment_score_sheet_round_trip_is_typed_not_proximity_based() {
        let (layout, projection) = fixture(false);
        let scene = realize_viewer_scene(&layout, &projection).expect("scene");
        let scores = interpret_score_sheet(&scene).expect("score sheet");
        assert_eq!(scores[0].1, 10);
        assert_eq!(scores[1].1, 7);
        assert!(scene.text.iter().any(|text| text.text == "A♠"));
    }
}
