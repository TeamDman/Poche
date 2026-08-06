// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use crate::{
    AabbMm, CardObjectId, HalfExtentsMm, LayoutId, ObjectId, PoseMm, SPATIAL_SCHEMA_VERSION,
    SeatId, SurfaceId, SurfaceKind, TableId, TextRunId, ZoneId,
};

/// Whether spatial input is restricted to typed Poche zones or may propose a
/// loose tabletop placement for later audit/governance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabletopMode {
    /// Only actions that resolve to strict typed Poche intent can commit.
    StrictPoche,
    /// Loose placement can be retained as an attempted interaction, but it does
    /// not become a valid Poche state without a checked typed transition.
    AuditLoose,
}

/// Canonical 52-card face code (`suit * 13 + rank`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CardFace(u8);

impl CardFace {
    /// Construct a standard-card face from its dense code.
    #[must_use]
    pub const fn new(code: u8) -> Option<Self> {
        if code < 52 { Some(Self(code)) } else { None }
    }

    /// Return the dense standard-card code.
    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }
}

/// Exact typed location represented by one card endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardLocation {
    /// Ordered deck slot, zero at the bottom.
    Deck { index_from_bottom: u8 },
    /// Ordered private hand slot.
    Hand { seat: SeatId, index_from_left: u8 },
    /// One seat's card in the current trick.
    Play { seat: SeatId },
    /// Card retained in a completed trick stack.
    Won {
        /// Winning seat.
        seat: SeatId,
        /// Stable trick ordinal in the current round.
        trick: u8,
        /// Card slot within that trick.
        index: u8,
    },
}

impl CardLocation {
    /// Return the semantic zone containing this card.
    #[must_use]
    pub const fn zone(self) -> ZoneId {
        match self {
            Self::Deck { .. } => ZoneId::Deck,
            Self::Hand { seat, .. } => ZoneId::Hand(seat),
            Self::Play { .. } => ZoneId::Play,
            Self::Won { seat, .. } => ZoneId::Won(seat),
        }
    }
}

/// Non-card object kind. Card objects use [`CardObject`] so location and
/// viewer-scoped face knowledge cannot be confused with generic scene objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneObjectKind {
    /// Physical table.
    Table,
    /// Player seat anchor.
    Seat,
    /// Player/token anchor.
    Player,
    /// Visible/debuggable semantic zone volume.
    Zone,
    /// Score sheet surface.
    ScoreSheet,
}

/// One non-card semantic object in the realized scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneObject {
    /// Stable semantic identity.
    pub id: ObjectId,
    /// Expected object category.
    pub kind: SceneObjectKind,
    /// Exact endpoint pose.
    pub pose: PoseMm,
    /// Simple renderer/formal bound.
    pub half_extents: HalfExtentsMm,
}

/// One opaque card object realized for an exact viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CardObject {
    /// Opaque projection-epoch object handle.
    pub id: CardObjectId,
    /// Exact typed Poche location.
    pub location: CardLocation,
    /// Exact endpoint pose.
    pub pose: PoseMm,
    /// Simple card bound.
    pub half_extents: HalfExtentsMm,
    /// Face known to this viewer, or `None` for a hidden/back-only object.
    pub face: Option<CardFace>,
}

/// Semantic reason that a text run exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextBinding {
    /// Visible face label for an exact opaque card object.
    CardFace(CardObjectId),
    /// Display name of the player in one seat.
    PlayerName(SeatId),
    /// Displayed score value for one seat.
    PlayerScore(SeatId),
}

/// Semantic text attached to an exact surface. Renderer-specific glyph curves
/// are derived from this record and are never identity or score authority.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextRun {
    /// Stable identity within the scene.
    pub id: TextRunId,
    /// Semantic reason for the text.
    pub binding: TextBinding,
    /// Exact attachment target.
    pub attached_to: SurfaceId,
    /// Pose relative to the owning surface.
    pub local_pose: PoseMm,
    /// Viewer-authorized UTF-8 content such as `A♠` or a score.
    pub text: String,
}

/// Presentation-only easing identifier. It cannot alter semantic endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationEasing {
    /// Constant-velocity interpolation.
    Linear,
    /// Deterministic smooth start/end interpolation.
    SmoothStep,
}

/// Optional reconstructable animation descriptor between committed endpoint
/// poses. Intermediate frames remain local renderer output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnimationEndpoint {
    /// Object being presented as moving.
    pub object: ObjectId,
    /// Previous committed endpoint.
    pub from: PoseMm,
    /// New committed endpoint.
    pub to: PoseMm,
    /// Nominal presentation duration.
    pub duration_milliseconds: u32,
    /// Deterministic easing identifier.
    pub easing: AnimationEasing,
}

/// Exact viewer-scoped scene produced by one layout and projection epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialScene {
    /// Spatial contract version.
    pub schema_version: u16,
    /// Stable identity of the table-local coordinate frame.
    pub table_id: TableId,
    /// Registered deterministic layout.
    pub layout: LayoutId,
    /// Viewer/privacy epoch used by opaque card handles.
    pub projection_epoch: u64,
    /// Non-card scene objects.
    pub objects: Vec<SceneObject>,
    /// Card objects visible as backs or authorized faces.
    pub cards: Vec<CardObject>,
    /// Semantic text runs; hidden faces have no face-text run.
    pub text: Vec<TextRun>,
}

/// Stable structural problem in a spatial realization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneError {
    /// Unknown spatial schema version.
    UnknownSchema,
    /// Duplicate semantic object identity.
    DuplicateObject,
    /// Duplicate text-run identity.
    DuplicateText,
    /// Generic object kind does not match its semantic identity.
    ObjectKind,
    /// Card handle belongs to another projection epoch.
    CardEpoch,
    /// Referenced seat is outside the layout.
    SeatOutsideLayout,
    /// Text refers to a missing object or card.
    MissingAttachment,
    /// Text binding and target surface disagree.
    InvalidAttachment,
    /// A card face text run exists without viewer-authorized face knowledge.
    HiddenFaceText,
    /// A card's known face is not represented exactly once as text.
    MissingFaceText,
    /// An endpoint pose or bound exceeds the v1 table-local coordinate limit.
    PoseOutsideTable,
}

impl SpatialScene {
    /// Validate structural and viewer-privacy invariants of the scene.
    ///
    /// # Errors
    ///
    /// Returns the first stable structural category that fails.
    pub fn validate(&self) -> Result<(), SceneError> {
        if self.schema_version != SPATIAL_SCHEMA_VERSION {
            return Err(SceneError::UnknownSchema);
        }

        let mut object_ids = HashSet::new();
        for object in &self.objects {
            if !object_ids.insert(object.id) {
                return Err(SceneError::DuplicateObject);
            }
            if !object_kind_matches_id(*object) {
                return Err(SceneError::ObjectKind);
            }
            validate_object_seat(object.id, self.layout)?;
            if !object.pose.is_within_table_bounds()
                || AabbMm::from_center(object.pose.translation, object.half_extents).is_none()
            {
                return Err(SceneError::PoseOutsideTable);
            }
        }
        for card in &self.cards {
            let id = ObjectId::Card(card.id);
            if !object_ids.insert(id) {
                return Err(SceneError::DuplicateObject);
            }
            if card.id.projection_epoch != self.projection_epoch {
                return Err(SceneError::CardEpoch);
            }
            validate_location_seats(card.location, self.layout)?;
            if !card.pose.is_within_table_bounds()
                || AabbMm::from_center(card.pose.translation, card.half_extents).is_none()
            {
                return Err(SceneError::PoseOutsideTable);
            }
        }

        let cards_by_id = self
            .cards
            .iter()
            .map(|card| (card.id, card))
            .collect::<Vec<_>>();
        let mut text_ids = HashSet::new();
        let mut face_text_cards = HashSet::new();
        for text in &self.text {
            if !text_ids.insert(text.id) {
                return Err(SceneError::DuplicateText);
            }
            if text.text.is_empty() || !object_ids.contains(&text.attached_to.object) {
                return Err(SceneError::MissingAttachment);
            }
            if !text.local_pose.is_within_table_bounds() {
                return Err(SceneError::PoseOutsideTable);
            }
            match text.binding {
                TextBinding::CardFace(card_id) => {
                    let Some((_, card)) = cards_by_id.iter().find(|(id, _)| *id == card_id) else {
                        return Err(SceneError::MissingAttachment);
                    };
                    if text.attached_to.object != ObjectId::Card(card_id)
                        || text.attached_to.kind != SurfaceKind::Face
                    {
                        return Err(SceneError::InvalidAttachment);
                    }
                    if card.face.is_none() {
                        return Err(SceneError::HiddenFaceText);
                    }
                    if !face_text_cards.insert(card_id) {
                        return Err(SceneError::DuplicateText);
                    }
                }
                TextBinding::PlayerName(seat) | TextBinding::PlayerScore(seat) => {
                    validate_seat(seat, self.layout)?;
                    if text.attached_to.object != ObjectId::ScoreSheet
                        || text.attached_to.kind != SurfaceKind::Face
                    {
                        return Err(SceneError::InvalidAttachment);
                    }
                }
            }
        }

        if self
            .cards
            .iter()
            .any(|card| card.face.is_some() && !face_text_cards.contains(&card.id))
        {
            return Err(SceneError::MissingFaceText);
        }
        Ok(())
    }
}

fn object_kind_matches_id(object: SceneObject) -> bool {
    matches!(
        (object.kind, object.id),
        (SceneObjectKind::Table, ObjectId::Table)
            | (SceneObjectKind::Seat, ObjectId::Seat(_))
            | (SceneObjectKind::Player, ObjectId::Player(_))
            | (SceneObjectKind::Zone, ObjectId::Zone(_))
            | (SceneObjectKind::ScoreSheet, ObjectId::ScoreSheet)
    )
}

fn validate_object_seat(id: ObjectId, layout: LayoutId) -> Result<(), SceneError> {
    match id {
        ObjectId::Seat(seat)
        | ObjectId::Player(seat)
        | ObjectId::Zone(ZoneId::Hand(seat) | ZoneId::Won(seat)) => validate_seat(seat, layout),
        ObjectId::Table | ObjectId::ScoreSheet | ObjectId::Zone(ZoneId::Deck | ZoneId::Play) => {
            Ok(())
        }
        ObjectId::Card(_) => Err(SceneError::ObjectKind),
    }
}

fn validate_location_seats(location: CardLocation, layout: LayoutId) -> Result<(), SceneError> {
    match location {
        CardLocation::Hand { seat, .. }
        | CardLocation::Play { seat }
        | CardLocation::Won { seat, .. } => validate_seat(seat, layout),
        CardLocation::Deck { .. } => Ok(()),
    }
}

fn validate_seat(seat: SeatId, layout: LayoutId) -> Result<(), SceneError> {
    if seat.get() < layout.players() {
        Ok(())
    } else {
        Err(SceneError::SeatOutsideLayout)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CardObjectId, HalfExtentsMm, LayoutId, ObjectId, Point3Mm, PoseMm, SPATIAL_SCHEMA_VERSION,
        SeatId, SurfaceId, SurfaceKind, TableId, TextRunId, YawMilliDegrees, ZoneId,
    };

    use super::{
        CardFace, CardLocation, CardObject, SceneError, SceneObject, SceneObjectKind, SpatialScene,
        TextBinding, TextRun,
    };

    fn pose(x: i32, y: i32, z: i32) -> PoseMm {
        PoseMm::new(Point3Mm::new(x, y, z), YawMilliDegrees::new(0))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one literal scene keeps the complete two-player privacy fixture auditable"
    )]
    fn fixture() -> SpatialScene {
        let layout = LayoutId::new(2, 1).expect("two-player layout");
        let seat_zero = SeatId::new(0, layout).expect("seat zero");
        let seat_one = SeatId::new(1, layout).expect("seat one");
        let own_card = CardObjectId {
            projection_epoch: 7,
            ordinal: 0,
        };
        let hidden_card = CardObjectId {
            projection_epoch: 7,
            ordinal: 1,
        };
        let played_card = CardObjectId {
            projection_epoch: 7,
            ordinal: 2,
        };
        let deck_card = CardObjectId {
            projection_epoch: 7,
            ordinal: 3,
        };
        let flat = HalfExtentsMm::new(32, 1, 44);
        SpatialScene {
            schema_version: SPATIAL_SCHEMA_VERSION,
            table_id: TableId::new(3),
            layout,
            projection_epoch: 7,
            objects: vec![
                SceneObject {
                    id: ObjectId::Table,
                    kind: SceneObjectKind::Table,
                    pose: pose(0, 0, 0),
                    half_extents: HalfExtentsMm::new(600, 20, 600),
                },
                SceneObject {
                    id: ObjectId::Seat(seat_zero),
                    kind: SceneObjectKind::Seat,
                    pose: pose(0, 0, 700),
                    half_extents: HalfExtentsMm::new(250, 250, 250),
                },
                SceneObject {
                    id: ObjectId::Seat(seat_one),
                    kind: SceneObjectKind::Seat,
                    pose: pose(0, 0, -700),
                    half_extents: HalfExtentsMm::new(250, 250, 250),
                },
                SceneObject {
                    id: ObjectId::Zone(ZoneId::Hand(seat_zero)),
                    kind: SceneObjectKind::Zone,
                    pose: pose(0, 30, 430),
                    half_extents: HalfExtentsMm::new(240, 20, 90),
                },
                SceneObject {
                    id: ObjectId::Zone(ZoneId::Hand(seat_one)),
                    kind: SceneObjectKind::Zone,
                    pose: pose(0, 30, -430),
                    half_extents: HalfExtentsMm::new(240, 20, 90),
                },
                SceneObject {
                    id: ObjectId::Zone(ZoneId::Deck),
                    kind: SceneObjectKind::Zone,
                    pose: pose(-250, 30, 0),
                    half_extents: HalfExtentsMm::new(50, 30, 60),
                },
                SceneObject {
                    id: ObjectId::Zone(ZoneId::Play),
                    kind: SceneObjectKind::Zone,
                    pose: pose(0, 30, 0),
                    half_extents: HalfExtentsMm::new(180, 30, 180),
                },
                SceneObject {
                    id: ObjectId::ScoreSheet,
                    kind: SceneObjectKind::ScoreSheet,
                    pose: pose(270, 25, 0),
                    half_extents: HalfExtentsMm::new(100, 1, 150),
                },
            ],
            cards: vec![
                CardObject {
                    id: own_card,
                    location: CardLocation::Hand {
                        seat: seat_zero,
                        index_from_left: 0,
                    },
                    pose: pose(0, 55, 430),
                    half_extents: flat,
                    face: CardFace::new(51),
                },
                CardObject {
                    id: hidden_card,
                    location: CardLocation::Hand {
                        seat: seat_one,
                        index_from_left: 0,
                    },
                    pose: pose(0, 55, -430),
                    half_extents: flat,
                    face: None,
                },
                CardObject {
                    id: played_card,
                    location: CardLocation::Play { seat: seat_one },
                    pose: pose(0, 55, 0),
                    half_extents: flat,
                    face: CardFace::new(11),
                },
                CardObject {
                    id: deck_card,
                    location: CardLocation::Deck {
                        index_from_bottom: 0,
                    },
                    pose: pose(-250, 55, 0),
                    half_extents: flat,
                    face: None,
                },
            ],
            text: vec![
                TextRun {
                    id: TextRunId(0),
                    binding: TextBinding::CardFace(own_card),
                    attached_to: SurfaceId {
                        object: ObjectId::Card(own_card),
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(0, 2, 0),
                    text: "A♠".to_owned(),
                },
                TextRun {
                    id: TextRunId(1),
                    binding: TextBinding::CardFace(played_card),
                    attached_to: SurfaceId {
                        object: ObjectId::Card(played_card),
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(0, 2, 0),
                    text: "K♣".to_owned(),
                },
                TextRun {
                    id: TextRunId(2),
                    binding: TextBinding::PlayerName(seat_zero),
                    attached_to: SurfaceId {
                        object: ObjectId::ScoreSheet,
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(-50, 2, 50),
                    text: "Alice".to_owned(),
                },
                TextRun {
                    id: TextRunId(3),
                    binding: TextBinding::PlayerScore(seat_zero),
                    attached_to: SurfaceId {
                        object: ObjectId::ScoreSheet,
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(50, 2, 50),
                    text: "10".to_owned(),
                },
                TextRun {
                    id: TextRunId(4),
                    binding: TextBinding::PlayerName(seat_one),
                    attached_to: SurfaceId {
                        object: ObjectId::ScoreSheet,
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(-50, 2, -50),
                    text: "Bob".to_owned(),
                },
                TextRun {
                    id: TextRunId(5),
                    binding: TextBinding::PlayerScore(seat_one),
                    attached_to: SurfaceId {
                        object: ObjectId::ScoreSheet,
                        kind: SurfaceKind::Face,
                    },
                    local_pose: pose(50, 2, -50),
                    text: "7".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn two_player_fixture_carries_spatial_cards_scores_names_and_two_hands() {
        let scene = fixture();
        assert_eq!(scene.cards.len(), 4);
        assert!(scene.validate().is_ok());
        assert_eq!(
            scene
                .cards
                .iter()
                .filter(|card| matches!(card.location, CardLocation::Hand { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn hidden_card_has_no_face_text_for_this_viewer() {
        let scene = fixture();
        let hidden = scene
            .cards
            .iter()
            .find(|card| card.face.is_none())
            .expect("hidden card");
        assert!(
            !scene
                .text
                .iter()
                .any(|text| { text.binding == TextBinding::CardFace(hidden.id) })
        );
    }

    #[test]
    fn face_text_on_a_hidden_card_is_rejected() {
        let mut scene = fixture();
        let hidden = scene
            .cards
            .iter()
            .find(|card| card.face.is_none())
            .expect("hidden card")
            .id;
        scene.text.push(TextRun {
            id: TextRunId(9),
            binding: TextBinding::CardFace(hidden),
            attached_to: SurfaceId {
                object: ObjectId::Card(hidden),
                kind: SurfaceKind::Face,
            },
            local_pose: pose(0, 2, 0),
            text: "?".to_owned(),
        });
        assert_eq!(scene.validate(), Err(SceneError::HiddenFaceText));
    }

    #[test]
    fn proximity_cannot_reassign_text_to_another_touching_card() {
        let mut scene = fixture();
        let own = scene.cards[0];
        scene.cards[1].pose = own.pose;
        assert!(scene.validate().is_ok());
        assert_eq!(
            scene.text[0].attached_to.object,
            ObjectId::Card(own.id),
            "attachment is semantic even when bounds occupy the same pose"
        );
    }

    #[test]
    fn known_face_without_exact_text_attachment_is_rejected() {
        let mut scene = fixture();
        scene.text.remove(0);
        assert_eq!(scene.validate(), Err(SceneError::MissingFaceText));
    }

    #[test]
    fn card_handle_from_another_privacy_epoch_is_rejected() {
        let mut scene = fixture();
        scene.cards[0].id.projection_epoch += 1;
        assert_eq!(scene.validate(), Err(SceneError::CardEpoch));
    }

    #[test]
    fn face_text_attached_to_the_score_sheet_is_rejected() {
        let mut scene = fixture();
        scene.text[0].attached_to.object = ObjectId::ScoreSheet;
        assert_eq!(scene.validate(), Err(SceneError::InvalidAttachment));
    }
}
