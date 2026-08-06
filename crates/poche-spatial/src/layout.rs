// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use crate::{
    AabbMm, HalfExtentsMm, LayoutId, ObjectId, Point3Mm, PoseMm, SceneObject, SceneObjectKind,
    SeatId, TableId, YawMilliDegrees, ZoneId,
};

const LAYOUT_REVISION: u16 = 1;
const DIRECTION_RADIUS: i32 = 720;

/// One registered seat and player-anchor placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeatPlacement {
    /// Seat identity within the layout.
    pub seat: SeatId,
    /// Physical seat anchor outside the table edge.
    pub seat_pose: PoseMm,
    /// Player/token anchor colocated with the seat in v1.
    pub player_pose: PoseMm,
}

/// Nested snap and dead-band volumes for one semantic zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZoneVolume {
    /// Semantic zone represented by this volume.
    pub id: ZoneId,
    /// A card bound completely contained here may snap to the typed zone.
    pub inner: AabbMm,
    /// A card intersecting this outer volume but not exclusively inside the
    /// inner volume is deliberately unresolved.
    pub outer: AabbMm,
}

impl ZoneVolume {
    /// Construct a nested zone volume.
    #[must_use]
    pub const fn new(id: ZoneId, inner: AabbMm, outer: AabbMm) -> Option<Self> {
        if outer.contains_aabb(inner) {
            Some(Self { id, inner, outer })
        } else {
            None
        }
    }
}

/// Stable spatial classification of a dropped object bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZoneClassification {
    /// The complete object is exclusively inside one zone's inner volume.
    Snapped(ZoneId),
    /// The object touches one zone's outer volume but is not safely snapped.
    DeadBand(ZoneId),
    /// The object does not intersect any registered outer volume.
    Free,
    /// The supplied point/bound is outside the checked table-local space.
    OutOfBounds,
    /// The object simultaneously implicates multiple zones. Valid layouts
    /// prevent this for points; a large or malformed dragged bound can still
    /// produce it and must not snap arbitrarily.
    Ambiguous(Vec<ZoneId>),
}

/// A deterministic table-local layout for one supported player count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialLayout {
    table_id: TableId,
    id: LayoutId,
    table: SceneObject,
    score_sheet: SceneObject,
    seats: Vec<SeatPlacement>,
    zones: Vec<ZoneVolume>,
}

impl SpatialLayout {
    /// Construct and validate a complete layout.
    ///
    /// # Errors
    ///
    /// Returns a stable category for missing, duplicated, out-of-layout, or
    /// overlapping geometry.
    pub fn try_new(
        table_id: TableId,
        id: LayoutId,
        table: SceneObject,
        score_sheet: SceneObject,
        seats: Vec<SeatPlacement>,
        zones: Vec<ZoneVolume>,
    ) -> Result<Self, LayoutError> {
        let layout = Self {
            table_id,
            id,
            table,
            score_sheet,
            seats,
            zones,
        };
        layout.validate()?;
        Ok(layout)
    }

    /// Return the stable table identity owning this local frame.
    #[must_use]
    pub const fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Return the registered layout identity.
    #[must_use]
    pub const fn id(&self) -> LayoutId {
        self.id
    }

    /// Return the table object.
    #[must_use]
    pub const fn table(&self) -> SceneObject {
        self.table
    }

    /// Return the score-sheet object.
    #[must_use]
    pub const fn score_sheet(&self) -> SceneObject {
        self.score_sheet
    }

    /// Return ordered seat placements.
    #[must_use]
    pub fn seats(&self) -> &[SeatPlacement] {
        &self.seats
    }

    /// Return zones in stable deck/trump/play then seat hand/won order.
    #[must_use]
    pub fn zones(&self) -> &[ZoneVolume] {
        &self.zones
    }

    /// Produce all non-card objects required by this layout.
    #[must_use]
    pub fn scene_objects(&self) -> Vec<SceneObject> {
        let mut objects = Vec::with_capacity(2 + self.seats.len() * 4 + 3);
        objects.push(self.table);
        objects.push(self.score_sheet);
        for placement in &self.seats {
            objects.push(SceneObject {
                id: ObjectId::Seat(placement.seat),
                kind: SceneObjectKind::Seat,
                pose: placement.seat_pose,
                half_extents: HalfExtentsMm::new(220, 220, 220),
            });
            objects.push(SceneObject {
                id: ObjectId::Player(placement.seat),
                kind: SceneObjectKind::Player,
                pose: placement.player_pose,
                half_extents: HalfExtentsMm::new(120, 240, 120),
            });
        }
        for zone in &self.zones {
            objects.push(SceneObject {
                id: ObjectId::Zone(zone.id),
                kind: SceneObjectKind::Zone,
                pose: pose_at_box_center(zone.inner),
                half_extents: half_extents(zone.inner),
            });
        }
        objects
    }

    /// Classify a point as a zero-sized object.
    #[must_use]
    pub fn classify_point(&self, point: Point3Mm) -> ZoneClassification {
        let Some(bounds) = AabbMm::from_center(point, HalfExtentsMm::new(0, 0, 0)) else {
            return ZoneClassification::OutOfBounds;
        };
        self.classify_bounds(bounds)
    }

    /// Classify a complete dragged object bound against snap and dead-band
    /// volumes. Classification never selects an arbitrary first match.
    #[must_use]
    pub fn classify_bounds(&self, bounds: AabbMm) -> ZoneClassification {
        if !bounds.is_within_table_bounds() {
            return ZoneClassification::OutOfBounds;
        }
        let inner = self
            .zones
            .iter()
            .filter(|zone| zone.inner.contains_aabb(bounds))
            .map(|zone| zone.id)
            .collect::<Vec<_>>();
        let outer = self
            .zones
            .iter()
            .filter(|zone| zone.outer.intersects(bounds))
            .map(|zone| zone.id)
            .collect::<Vec<_>>();

        match (inner.as_slice(), outer.as_slice()) {
            ([inner_id], [outer_id]) if inner_id == outer_id => {
                ZoneClassification::Snapped(*inner_id)
            }
            ([], []) => ZoneClassification::Free,
            ([], [id]) => ZoneClassification::DeadBand(*id),
            _ => ZoneClassification::Ambiguous(unique_zone_ids(inner, outer)),
        }
    }

    fn validate(&self) -> Result<(), LayoutError> {
        if self.id.revision() != LAYOUT_REVISION {
            return Err(LayoutError::UnsupportedRevision);
        }
        if self.table.id != ObjectId::Table || self.table.kind != SceneObjectKind::Table {
            return Err(LayoutError::ObjectKind);
        }
        if self.score_sheet.id != ObjectId::ScoreSheet
            || self.score_sheet.kind != SceneObjectKind::ScoreSheet
        {
            return Err(LayoutError::ObjectKind);
        }
        if !object_is_bounded(self.table) || !object_is_bounded(self.score_sheet) {
            return Err(LayoutError::PoseOutsideTable);
        }
        if self.seats.len() != usize::from(self.id.players()) {
            return Err(LayoutError::SeatCardinality);
        }

        let mut seat_ids = HashSet::new();
        for placement in &self.seats {
            if placement.seat.get() >= self.id.players() || !seat_ids.insert(placement.seat) {
                return Err(LayoutError::SeatCardinality);
            }
            if !placement.seat_pose.is_within_table_bounds()
                || !placement.player_pose.is_within_table_bounds()
            {
                return Err(LayoutError::PoseOutsideTable);
            }
        }

        let expected = expected_zones(self.id);
        let actual = self
            .zones
            .iter()
            .map(|zone| zone.id)
            .collect::<HashSet<_>>();
        if expected != actual || actual.len() != self.zones.len() {
            return Err(LayoutError::ZoneSet);
        }
        for zone in &self.zones {
            if !zone.inner.is_within_table_bounds() || !zone.outer.is_within_table_bounds() {
                return Err(LayoutError::PoseOutsideTable);
            }
            if !zone.outer.contains_aabb(zone.inner) {
                return Err(LayoutError::InnerOutsideOuter);
            }
        }
        for (index, left) in self.zones.iter().enumerate() {
            for right in self.zones.iter().skip(index + 1) {
                if left.inner.intersects(right.inner) {
                    return Err(LayoutError::OverlappingInnerZones);
                }
                if left.outer.intersects(right.outer) {
                    return Err(LayoutError::OverlappingOuterZones);
                }
            }
        }
        Ok(())
    }
}

/// Stable failure category for a registered or custom layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Only layout revision one is currently defined.
    UnsupportedRevision,
    /// A table or score-sheet object has the wrong semantic kind.
    ObjectKind,
    /// A pose or bound exceeds the table-local v1 coordinate limit.
    PoseOutsideTable,
    /// Seat count, identity, or uniqueness does not match the layout ID.
    SeatCardinality,
    /// Required deck/play/hand/won zones are missing or duplicated.
    ZoneSet,
    /// A zone inner volume is not contained by its outer volume.
    InnerOutsideOuter,
    /// Two snap volumes share at least one point.
    OverlappingInnerZones,
    /// Two dead-band volumes share at least one point.
    OverlappingOuterZones,
    /// Arithmetic failed while generating a registered layout.
    GeometryOverflow,
}

/// Construct the deterministic revision-one layout for 2 through 8 players.
///
/// # Errors
///
/// Returns [`LayoutError::UnsupportedRevision`] for another revision and a
/// stable geometry category if the registered constants cease to validate.
pub fn registered_layout(table_id: TableId, id: LayoutId) -> Result<SpatialLayout, LayoutError> {
    if id.revision() != LAYOUT_REVISION {
        return Err(LayoutError::UnsupportedRevision);
    }

    let table = SceneObject {
        id: ObjectId::Table,
        kind: SceneObjectKind::Table,
        pose: pose(0, 0, 0, 0)?,
        half_extents: HalfExtentsMm::new(650, 20, 650),
    };
    let score_sheet = SceneObject {
        id: ObjectId::ScoreSheet,
        kind: SceneObjectKind::ScoreSheet,
        pose: pose(265, 24, 0, 90_000)?,
        half_extents: HalfExtentsMm::new(100, 1, 145),
    };

    let directions = directions(id.players());
    let mut seats = Vec::with_capacity(directions.len());
    let mut zones = vec![
        zone(ZoneId::Deck, Point3Mm::new(-190, 125, 0), 45, 40, 50, 50)?,
        zone(ZoneId::Trump, Point3Mm::new(-70, 125, 0), 45, 40, 50, 50)?,
        zone(ZoneId::Play, Point3Mm::new(0, 40, 0), 100, 20, 130, 30)?,
    ];
    for (ordinal, direction) in directions.iter().copied().enumerate() {
        let seat = SeatId::new(
            u8::try_from(ordinal).map_err(|_| LayoutError::GeometryOverflow)?,
            id,
        )
        .ok_or(LayoutError::SeatCardinality)?;
        let outward_yaw = u32::try_from(ordinal)
            .map_err(|_| LayoutError::GeometryOverflow)?
            .checked_mul(YawMilliDegrees::FULL_TURN)
            .ok_or(LayoutError::GeometryOverflow)?
            / u32::from(id.players());
        let inward_yaw = (outward_yaw + 180_000) % YawMilliDegrees::FULL_TURN;
        let seat_point = scale_direction(direction, 750, 0)?;
        seats.push(SeatPlacement {
            seat,
            seat_pose: PoseMm::checked(seat_point, YawMilliDegrees::new(inward_yaw))
                .ok_or(LayoutError::PoseOutsideTable)?,
            player_pose: PoseMm::checked(
                Point3Mm::new(seat_point.x.get(), 240, seat_point.z.get()),
                YawMilliDegrees::new(inward_yaw),
            )
            .ok_or(LayoutError::PoseOutsideTable)?,
        });

        zones.push(zone(
            ZoneId::Hand(seat),
            scale_direction(direction, 520, 40)?,
            75,
            20,
            95,
            30,
        )?);
        zones.push(zone(
            ZoneId::Won(seat),
            scale_direction(direction, 300, 40)?,
            45,
            20,
            55,
            30,
        )?);
    }

    SpatialLayout::try_new(table_id, id, table, score_sheet, seats, zones)
}

fn zone(
    id: ZoneId,
    center: Point3Mm,
    inner_horizontal: u32,
    inner_vertical: u32,
    outer_horizontal: u32,
    outer_vertical: u32,
) -> Result<ZoneVolume, LayoutError> {
    let inner = AabbMm::from_center(
        center,
        HalfExtentsMm::new(inner_horizontal, inner_vertical, inner_horizontal),
    )
    .ok_or(LayoutError::GeometryOverflow)?;
    let outer = AabbMm::from_center(
        center,
        HalfExtentsMm::new(outer_horizontal, outer_vertical, outer_horizontal),
    )
    .ok_or(LayoutError::GeometryOverflow)?;
    ZoneVolume::new(id, inner, outer).ok_or(LayoutError::InnerOutsideOuter)
}

fn pose(x: i32, y: i32, z: i32, yaw: u32) -> Result<PoseMm, LayoutError> {
    PoseMm::checked(Point3Mm::new(x, y, z), YawMilliDegrees::new(yaw))
        .ok_or(LayoutError::PoseOutsideTable)
}

fn scale_direction(direction: (i32, i32), radius: i32, y: i32) -> Result<Point3Mm, LayoutError> {
    let x = direction
        .0
        .checked_mul(radius)
        .ok_or(LayoutError::GeometryOverflow)?
        / DIRECTION_RADIUS;
    let z = direction
        .1
        .checked_mul(radius)
        .ok_or(LayoutError::GeometryOverflow)?
        / DIRECTION_RADIUS;
    Ok(Point3Mm::new(x, y, z))
}

fn directions(players: u8) -> &'static [(i32, i32)] {
    match players {
        2 => &[(0, 720), (0, -720)],
        3 => &[(0, 720), (-624, -360), (624, -360)],
        4 => &[(0, 720), (-720, 0), (0, -720), (720, 0)],
        5 => &[(0, 720), (-685, 222), (-423, -582), (423, -582), (685, 222)],
        6 => &[
            (0, 720),
            (-624, 360),
            (-624, -360),
            (0, -720),
            (624, -360),
            (624, 360),
        ],
        7 => &[
            (0, 720),
            (-563, 449),
            (-702, -160),
            (-312, -649),
            (312, -649),
            (702, -160),
            (563, 449),
        ],
        8 => &[
            (0, 720),
            (-509, 509),
            (-720, 0),
            (-509, -509),
            (0, -720),
            (509, -509),
            (720, 0),
            (509, 509),
        ],
        _ => &[],
    }
}

fn expected_zones(layout: LayoutId) -> HashSet<ZoneId> {
    let mut zones = HashSet::from([ZoneId::Deck, ZoneId::Trump, ZoneId::Play]);
    for ordinal in 0..layout.players() {
        let seat = SeatId::new(ordinal, layout).expect("ordinal bounded by layout");
        zones.insert(ZoneId::Hand(seat));
        zones.insert(ZoneId::Won(seat));
    }
    zones
}

fn object_is_bounded(object: SceneObject) -> bool {
    object.pose.is_within_table_bounds()
        && AabbMm::from_center(object.pose.translation, object.half_extents).is_some()
}

fn pose_at_box_center(bounds: AabbMm) -> PoseMm {
    let x = midpoint(bounds.min.x.get(), bounds.max.x.get());
    let y = midpoint(bounds.min.y.get(), bounds.max.y.get());
    let z = midpoint(bounds.min.z.get(), bounds.max.z.get());
    PoseMm::new(Point3Mm::new(x, y, z), YawMilliDegrees::new(0))
}

fn half_extents(bounds: AabbMm) -> HalfExtentsMm {
    HalfExtentsMm::new(
        half_extent(bounds.min.x.get(), bounds.max.x.get()),
        half_extent(bounds.min.y.get(), bounds.max.y.get()),
        half_extent(bounds.min.z.get(), bounds.max.z.get()),
    )
}

fn midpoint(min: i32, max: i32) -> i32 {
    min + (max - min) / 2
}

fn half_extent(min: i32, max: i32) -> u32 {
    u32::try_from((max - min) / 2).expect("validated AABB has non-negative span")
}

fn unique_zone_ids(mut inner: Vec<ZoneId>, outer: Vec<ZoneId>) -> Vec<ZoneId> {
    for id in outer {
        if !inner.contains(&id) {
            inner.push(id);
        }
    }
    inner
}

#[cfg(test)]
mod tests {
    use crate::{AabbMm, HalfExtentsMm, LayoutId, Point3Mm, TableId, ZoneId};

    use super::{LayoutError, SpatialLayout, ZoneClassification, registered_layout};

    fn every_registered_layout() -> Vec<SpatialLayout> {
        (2..=8)
            .map(|players| {
                registered_layout(
                    TableId::new(u64::from(players)),
                    LayoutId::new(players, 1).expect("registered layout ID"),
                )
                .expect("registered layout")
            })
            .collect()
    }

    #[test]
    fn layouts_for_two_through_eight_have_complete_disjoint_zone_sets() {
        for layout in every_registered_layout() {
            assert_eq!(layout.seats().len(), usize::from(layout.id().players()));
            assert_eq!(
                layout.zones().len(),
                3 + usize::from(layout.id().players()) * 2
            );
            for (index, left) in layout.zones().iter().enumerate() {
                for right in layout.zones().iter().skip(index + 1) {
                    assert!(!left.inner.intersects(right.inner));
                    assert!(!left.outer.intersects(right.outer));
                }
            }
        }
    }

    #[test]
    fn classify_every_zone_center_as_exactly_its_semantic_zone() {
        for layout in every_registered_layout() {
            for zone in layout.zones() {
                let center = Point3Mm::new(
                    i32::midpoint(zone.inner.min.x.get(), zone.inner.max.x.get()),
                    i32::midpoint(zone.inner.min.y.get(), zone.inner.max.y.get()),
                    i32::midpoint(zone.inner.min.z.get(), zone.inner.max.z.get()),
                );
                assert_eq!(
                    layout.classify_point(center),
                    ZoneClassification::Snapped(zone.id)
                );
            }
        }
    }

    #[test]
    fn classify_inner_boundary_as_snap_outer_boundary_as_dead_band_and_outside_as_free() {
        let layout = every_registered_layout().remove(0);
        let play = layout
            .zones()
            .iter()
            .find(|zone| zone.id == ZoneId::Play)
            .expect("play zone");
        assert_eq!(
            layout.classify_point(play.inner.max),
            ZoneClassification::Snapped(ZoneId::Play)
        );
        assert_eq!(
            layout.classify_point(play.outer.max),
            ZoneClassification::DeadBand(ZoneId::Play)
        );
        assert_eq!(
            layout.classify_point(Point3Mm::new(9_000, 9_000, 9_000)),
            ZoneClassification::Free
        );
        assert_eq!(
            layout.classify_point(Point3Mm::new(10_001, 0, 0)),
            ZoneClassification::OutOfBounds
        );
    }

    #[test]
    fn classify_standard_card_bound_only_when_completely_inside_one_inner_volume() {
        let layout = every_registered_layout().remove(6);
        let hand = layout
            .zones()
            .iter()
            .find(|zone| matches!(zone.id, ZoneId::Hand(_)))
            .expect("hand zone");
        let center = Point3Mm::new(
            i32::midpoint(hand.inner.min.x.get(), hand.inner.max.x.get()),
            i32::midpoint(hand.inner.min.y.get(), hand.inner.max.y.get()),
            i32::midpoint(hand.inner.min.z.get(), hand.inner.max.z.get()),
        );
        let card = AabbMm::from_center(center, HalfExtentsMm::new(32, 1, 44)).expect("card bound");
        assert_eq!(
            layout.classify_bounds(card),
            ZoneClassification::Snapped(hand.id)
        );

        let crossing = AabbMm::from_center(hand.inner.max, HalfExtentsMm::new(1, 1, 1))
            .expect("crossing bound");
        assert_eq!(
            layout.classify_bounds(crossing),
            ZoneClassification::DeadBand(hand.id)
        );
    }

    #[test]
    fn invalid_revision_is_rejected_without_fallback() {
        let id = LayoutId::new(2, 2).expect("syntactically valid future ID");
        assert_eq!(
            registered_layout(TableId::new(1), id),
            Err(LayoutError::UnsupportedRevision)
        );
    }

    #[test]
    fn overlapping_outer_regions_are_rejected_and_large_inputs_are_ambiguous() {
        let layout = every_registered_layout().remove(0);
        let broad = AabbMm::from_center(Point3Mm::new(0, 40, 0), HalfExtentsMm::new(500, 1, 500))
            .expect("broad in-bounds input");
        let ZoneClassification::Ambiguous(candidates) = layout.classify_bounds(broad) else {
            panic!("an input spanning multiple exclusive zones must be ambiguous");
        };
        assert!(candidates.len() > 1);

        let mut zones = layout.zones().to_vec();
        zones[1].outer = AabbMm::new(
            Point3Mm::new(
                zones[0].outer.min.x.get().min(zones[1].outer.min.x.get()),
                zones[0].outer.min.y.get().min(zones[1].outer.min.y.get()),
                zones[0].outer.min.z.get().min(zones[1].outer.min.z.get()),
            ),
            Point3Mm::new(
                zones[0].outer.max.x.get().max(zones[1].outer.max.x.get()),
                zones[0].outer.max.y.get().max(zones[1].outer.max.y.get()),
                zones[0].outer.max.z.get().max(zones[1].outer.max.z.get()),
            ),
        )
        .expect("combined bounds");
        assert_eq!(
            SpatialLayout::try_new(
                layout.table_id(),
                layout.id(),
                layout.table(),
                layout.score_sheet(),
                layout.seats().to_vec(),
                zones,
            ),
            Err(LayoutError::OverlappingOuterZones)
        );
    }

    #[test]
    fn scene_object_projection_has_one_object_per_semantic_identity() {
        for layout in every_registered_layout() {
            let objects = layout.scene_objects();
            let unique = objects
                .iter()
                .map(|object| object.id)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(objects.len(), unique.len());
        }
    }
}
