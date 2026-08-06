// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    AabbMm, AnimationEasing, AnimationEndpoint, CardFace, CardLocation, CardObject, CardObjectId,
    ObjectId, PoseMm, SeatId, SpatialLayout, SpatialScene, ZoneClassification, ZoneId,
    realization::card_pose,
};

const DEFAULT_TWEEN_MILLISECONDS: u32 = 300;

/// Stable evidence explaining why an intended spatial play did not resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionFinding {
    /// Scene and layout identify different table-local projections.
    LayoutMismatch,
    /// No visible owned card has the requested face.
    MissingCard,
    /// The issuing hand contains concealed objects, so face selection cannot
    /// safely guess which one was named.
    HiddenCard,
    /// The visible face belongs to another seat (including a granted hand).
    UnauthorizedCard,
    /// Multiple owned objects claim the same visible face.
    DuplicateCard,
    /// A dragged opaque object does not occur in the exact viewer scene.
    UnknownObject,
    /// The drop snapped to a typed zone other than the play zone.
    WrongZone,
    /// The drop intersected a zone dead band and deliberately did not snap.
    DeadBand,
    /// The drop implicated more than one zone.
    Ambiguous,
    /// The drop was inside the table-local space but outside every zone.
    Free,
    /// The drop exceeded the checked table-local space.
    OutOfBounds,
    /// The owned-card intent is not in the reducer-supplied legal action set.
    IllegalAction,
    /// The semantic endpoint could not be reconstructed from the layout.
    Geometry,
}

/// Replayable semantic record for one resolved hand-to-play movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpatialPlayRecord {
    /// Opaque object selected in this projection epoch.
    pub object: CardObjectId,
    /// Viewer-authorized face converted to the typed game action.
    pub face: CardFace,
    /// Checked source location.
    pub source: CardLocation,
    /// Checked destination location.
    pub destination: CardLocation,
    /// Exact committed source pose. No intermediate frames are recorded.
    pub source_pose: PoseMm,
    /// Nominal local presentation duration.
    pub duration_milliseconds: u32,
    /// Stable local easing identifier.
    pub easing: AnimationEasing,
}

/// Successful semantic resolution shared by typed and drag input adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedCardPlay {
    /// Dense standard-deck face selected for the canonical game action.
    pub face: CardFace,
    /// Replayable spatial sidecar record.
    pub record: SpatialPlayRecord,
}

/// Resolve a named/typed face only against the issuing seat's visible hand.
///
/// # Errors
///
/// Returns stable evidence for hidden, missing, duplicated, or unauthorized
/// face knowledge without mutating the scene.
pub fn resolve_card_play(
    layout: &SpatialLayout,
    scene: &SpatialScene,
    issuing_seat: SeatId,
    face: CardFace,
) -> Result<ResolvedCardPlay, InteractionFinding> {
    validate_layout(layout, scene)?;
    let face_matches = scene
        .cards
        .iter()
        .filter(|card| card.face == Some(face))
        .collect::<Vec<_>>();
    let owned = face_matches
        .iter()
        .copied()
        .filter(
            |card| matches!(card.location, CardLocation::Hand { seat, .. } if seat == issuing_seat),
        )
        .collect::<Vec<_>>();
    match owned.as_slice() {
        [card] => resolved_play(layout, **card, issuing_seat),
        [_, _, ..] => Err(InteractionFinding::DuplicateCard),
        [] if !face_matches.is_empty() => Err(InteractionFinding::UnauthorizedCard),
        [] if scene.cards.iter().any(|card| {
            card.face.is_none()
                && matches!(card.location, CardLocation::Hand { seat, .. } if seat == issuing_seat)
        }) =>
        {
            Err(InteractionFinding::HiddenCard)
        }
        [] => Err(InteractionFinding::MissingCard),
    }
}

/// Resolve a drag release through the same owned-card semantic play.
///
/// # Errors
///
/// Returns a stable finding for an unknown/hidden/unowned object or any
/// non-exclusive play-zone classification without mutating the scene.
pub fn resolve_drag_play(
    layout: &SpatialLayout,
    scene: &SpatialScene,
    issuing_seat: SeatId,
    object: CardObjectId,
    released_bounds: AabbMm,
) -> Result<ResolvedCardPlay, InteractionFinding> {
    validate_layout(layout, scene)?;
    let card = scene
        .cards
        .iter()
        .find(|card| card.id == object)
        .ok_or(InteractionFinding::UnknownObject)?;
    let CardLocation::Hand { seat, .. } = card.location else {
        return Err(InteractionFinding::UnauthorizedCard);
    };
    if seat != issuing_seat {
        return Err(InteractionFinding::UnauthorizedCard);
    }
    if card.face.is_none() {
        return Err(InteractionFinding::HiddenCard);
    }
    match layout.classify_bounds(released_bounds) {
        ZoneClassification::Snapped(ZoneId::Play) => resolved_play(layout, *card, issuing_seat),
        ZoneClassification::Snapped(_) => Err(InteractionFinding::WrongZone),
        ZoneClassification::DeadBand(_) => Err(InteractionFinding::DeadBand),
        ZoneClassification::Ambiguous(_) => Err(InteractionFinding::Ambiguous),
        ZoneClassification::Free => Err(InteractionFinding::Free),
        ZoneClassification::OutOfBounds => Err(InteractionFinding::OutOfBounds),
    }
}

/// Reconstruct the presentation tween from a semantic sidecar record and the
/// registered layout. Intermediate frames remain renderer-local.
///
/// # Errors
///
/// Rejects records that are not a same-seat hand-to-play transition or whose
/// destination cannot be realized in the supplied layout.
pub fn reconstruct_animation_endpoint(
    layout: &SpatialLayout,
    record: SpatialPlayRecord,
) -> Result<AnimationEndpoint, InteractionFinding> {
    let CardLocation::Hand { seat: source, .. } = record.source else {
        return Err(InteractionFinding::Geometry);
    };
    let CardLocation::Play { seat: destination } = record.destination else {
        return Err(InteractionFinding::Geometry);
    };
    if source != destination {
        return Err(InteractionFinding::Geometry);
    }
    let to = card_pose(layout, record.destination, 1).map_err(|_| InteractionFinding::Geometry)?;
    Ok(AnimationEndpoint {
        object: ObjectId::Card(record.object),
        from: record.source_pose,
        to,
        duration_milliseconds: record.duration_milliseconds,
        easing: record.easing,
    })
}

fn validate_layout(layout: &SpatialLayout, scene: &SpatialScene) -> Result<(), InteractionFinding> {
    if layout.id() == scene.layout && layout.table_id() == scene.table_id {
        Ok(())
    } else {
        Err(InteractionFinding::LayoutMismatch)
    }
}

fn resolved_play(
    layout: &SpatialLayout,
    card: CardObject,
    issuing_seat: SeatId,
) -> Result<ResolvedCardPlay, InteractionFinding> {
    let Some(face) = card.face else {
        return Err(InteractionFinding::HiddenCard);
    };
    let source = card.location;
    if !matches!(source, CardLocation::Hand { seat, .. } if seat == issuing_seat) {
        return Err(InteractionFinding::UnauthorizedCard);
    }
    let destination = CardLocation::Play { seat: issuing_seat };
    let record = SpatialPlayRecord {
        object: card.id,
        face,
        source,
        destination,
        source_pose: card.pose,
        duration_milliseconds: DEFAULT_TWEEN_MILLISECONDS,
        easing: AnimationEasing::SmoothStep,
    };
    reconstruct_animation_endpoint(layout, record)?;
    Ok(ResolvedCardPlay { face, record })
}

#[cfg(test)]
mod tests {
    use crate::{
        AabbMm, CardFace, CardLocation, HalfExtentsMm, LayoutId, PlayerSpatialProjection, SeatId,
        TableId, ViewerSpatialProjection, ZoneId, realize_viewer_scene, registered_layout,
    };

    use super::{
        InteractionFinding, reconstruct_animation_endpoint, resolve_card_play, resolve_drag_play,
    };

    fn fixture() -> (crate::SpatialLayout, crate::SpatialScene, SeatId) {
        let id = LayoutId::new(2, 1).expect("layout ID");
        let seat = SeatId::new(0, id).expect("seat");
        let other = SeatId::new(1, id).expect("other seat");
        let layout = registered_layout(TableId::new(20), id).expect("layout");
        let projection = ViewerSpatialProjection {
            projection_epoch: 3,
            players: vec![
                PlayerSpatialProjection {
                    seat,
                    display_name: "Alice".to_owned(),
                    score: 0,
                    hand_count: 2,
                    visible_hand: Some(vec![face(48), face(51)]),
                    tricks_won: 0,
                },
                PlayerSpatialProjection {
                    seat: other,
                    display_name: "Bob".to_owned(),
                    score: 0,
                    hand_count: 1,
                    visible_hand: Some(vec![face(9)]),
                    tricks_won: 0,
                },
            ],
            trump: Some(face(1)),
            current_trick: Vec::new(),
            revealed_won_cards: Vec::new(),
        };
        let scene = realize_viewer_scene(&layout, &projection).expect("scene");
        (layout, scene, seat)
    }

    fn face(code: u8) -> CardFace {
        CardFace::new(code).expect("card face")
    }

    fn center_bound(layout: &crate::SpatialLayout, zone: ZoneId) -> AabbMm {
        let zone = layout
            .zones()
            .iter()
            .find(|candidate| candidate.id == zone)
            .expect("zone");
        let center = crate::Point3Mm::new(
            i32::midpoint(zone.inner.min.x.get(), zone.inner.max.x.get()),
            i32::midpoint(zone.inner.min.y.get(), zone.inner.max.y.get()),
            i32::midpoint(zone.inner.min.z.get(), zone.inner.max.z.get()),
        );
        AabbMm::from_center(center, HalfExtentsMm::new(32, 1, 44)).expect("card bound")
    }

    #[test]
    fn interaction_named_and_drag_paths_resolve_the_same_typed_play_and_tween() {
        let (layout, scene, seat) = fixture();
        let named = resolve_card_play(&layout, &scene, seat, face(48)).expect("named play");
        let dragged = resolve_drag_play(
            &layout,
            &scene,
            seat,
            named.record.object,
            center_bound(&layout, ZoneId::Play),
        )
        .expect("drag play");
        assert_eq!(named, dragged);
        let endpoint = reconstruct_animation_endpoint(&layout, named.record).expect("endpoint");
        assert_eq!(endpoint.from, named.record.source_pose);
        assert_eq!(endpoint.object, crate::ObjectId::Card(named.record.object));
    }

    #[test]
    fn interaction_hidden_missing_unowned_and_non_play_drops_fail_stably() {
        let (layout, mut scene, seat) = fixture();
        let owned = scene
            .cards
            .iter()
            .find(|card| {
                matches!(card.location, CardLocation::Hand { seat: owner, .. } if owner == seat)
            })
            .expect("owned card")
            .id;
        assert_eq!(
            resolve_drag_play(
                &layout,
                &scene,
                seat,
                owned,
                center_bound(&layout, ZoneId::Deck),
            ),
            Err(InteractionFinding::WrongZone)
        );
        assert_eq!(
            resolve_card_play(&layout, &scene, seat, face(9)),
            Err(InteractionFinding::UnauthorizedCard)
        );
        assert_eq!(
            resolve_card_play(&layout, &scene, seat, face(10)),
            Err(InteractionFinding::MissingCard)
        );
        let duplicate_face = scene
            .cards
            .iter()
            .find(|card| card.id == owned)
            .and_then(|card| card.face)
            .expect("visible owned face");
        let second_owned = scene
            .cards
            .iter_mut()
            .find(|card| {
                card.id != owned
                    && matches!(card.location, CardLocation::Hand { seat: card_seat, .. } if card_seat == seat)
            })
            .expect("second owned card");
        second_owned.face = Some(duplicate_face);
        assert_eq!(
            resolve_card_play(&layout, &scene, seat, duplicate_face),
            Err(InteractionFinding::DuplicateCard)
        );
        let broad = AabbMm::from_center(
            crate::Point3Mm::new(0, 40, 0),
            HalfExtentsMm::new(500, 1, 500),
        )
        .expect("broad bound");
        assert_eq!(
            resolve_drag_play(&layout, &scene, seat, owned, broad),
            Err(InteractionFinding::Ambiguous)
        );
        scene
            .cards
            .iter_mut()
            .find(|card| card.id == owned)
            .expect("owned card")
            .face = None;
        assert_eq!(
            resolve_drag_play(
                &layout,
                &scene,
                seat,
                owned,
                center_bound(&layout, ZoneId::Play),
            ),
            Err(InteractionFinding::HiddenCard)
        );
    }
}
