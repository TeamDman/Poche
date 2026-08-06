// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use core::fmt;

use crate::MAX_ABS_TABLE_COORDINATE_MM;

/// Exact signed millimetres used by the canonical spatial contract.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Millimeters(i32);

impl Millimeters {
    /// Construct an exact millimetre value.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Return the signed integer value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Debug for Millimeters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}mm", self.0)
    }
}

/// Exact three-dimensional point in table-local millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point3Mm {
    /// Rightward coordinate in table-local space.
    pub x: Millimeters,
    /// Upward coordinate in table-local space.
    pub y: Millimeters,
    /// Forward coordinate in table-local space.
    pub z: Millimeters,
}

impl Point3Mm {
    /// Construct a table-local point.
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            x: Millimeters::new(x),
            y: Millimeters::new(y),
            z: Millimeters::new(z),
        }
    }

    /// Return whether every coordinate is inside the v1 table-local bound.
    #[must_use]
    pub const fn is_within_table_bounds(self) -> bool {
        self.x.get() >= -MAX_ABS_TABLE_COORDINATE_MM
            && self.x.get() <= MAX_ABS_TABLE_COORDINATE_MM
            && self.y.get() >= -MAX_ABS_TABLE_COORDINATE_MM
            && self.y.get() <= MAX_ABS_TABLE_COORDINATE_MM
            && self.z.get() >= -MAX_ABS_TABLE_COORDINATE_MM
            && self.z.get() <= MAX_ABS_TABLE_COORDINATE_MM
    }
}

/// Exact yaw in thousandths of one degree, normalized to `[0, 360_000)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct YawMilliDegrees(u32);

impl YawMilliDegrees {
    /// Number of milli-degrees in one complete rotation.
    pub const FULL_TURN: u32 = 360_000;

    /// Construct a normalized yaw.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value % Self::FULL_TURN)
    }

    /// Return the normalized milli-degree value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Exact table-local pose. Rendering adapters may convert this to floating point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PoseMm {
    /// Translation relative to the table origin.
    pub translation: Point3Mm,
    /// Rotation around the table's upward axis.
    pub yaw: YawMilliDegrees,
}

impl PoseMm {
    /// Construct a pose from a point and yaw.
    #[must_use]
    pub const fn new(translation: Point3Mm, yaw: YawMilliDegrees) -> Self {
        Self { translation, yaw }
    }

    /// Construct a pose only when it is inside the v1 table-local bound.
    #[must_use]
    pub const fn checked(translation: Point3Mm, yaw: YawMilliDegrees) -> Option<Self> {
        if translation.is_within_table_bounds() {
            Some(Self { translation, yaw })
        } else {
            None
        }
    }

    /// Return whether this pose is inside the v1 table-local bound.
    #[must_use]
    pub const fn is_within_table_bounds(self) -> bool {
        self.translation.is_within_table_bounds()
    }
}

/// Non-negative half extents of a simple axis-aligned bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HalfExtentsMm {
    /// Half-width on the local x axis.
    pub x: u32,
    /// Half-height on the local y axis.
    pub y: u32,
    /// Half-depth on the local z axis.
    pub z: u32,
}

impl HalfExtentsMm {
    /// Construct half extents. Zero is permitted for deliberately thin planes.
    #[must_use]
    pub const fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }
}

/// Closed axis-aligned box in table-local integer space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AabbMm {
    /// Inclusive minimum corner.
    pub min: Point3Mm,
    /// Inclusive maximum corner.
    pub max: Point3Mm,
}

impl AabbMm {
    /// Construct a box when every minimum coordinate is at most its maximum.
    #[must_use]
    pub const fn new(min: Point3Mm, max: Point3Mm) -> Option<Self> {
        if min.x.get() <= max.x.get() && min.y.get() <= max.y.get() && min.z.get() <= max.z.get() {
            Some(Self { min, max })
        } else {
            None
        }
    }

    /// Return whether a point lies in the closed box.
    #[must_use]
    pub const fn contains(self, point: Point3Mm) -> bool {
        point.x.get() >= self.min.x.get()
            && point.x.get() <= self.max.x.get()
            && point.y.get() >= self.min.y.get()
            && point.y.get() <= self.max.y.get()
            && point.z.get() >= self.min.z.get()
            && point.z.get() <= self.max.z.get()
    }

    /// Construct a bound from an exact center and unsigned half extents.
    #[must_use]
    pub fn from_center(center: Point3Mm, half_extents: HalfExtentsMm) -> Option<Self> {
        let x = i32::try_from(half_extents.x).ok()?;
        let y = i32::try_from(half_extents.y).ok()?;
        let z = i32::try_from(half_extents.z).ok()?;
        let min = Point3Mm::new(
            center.x.get().checked_sub(x)?,
            center.y.get().checked_sub(y)?,
            center.z.get().checked_sub(z)?,
        );
        let max = Point3Mm::new(
            center.x.get().checked_add(x)?,
            center.y.get().checked_add(y)?,
            center.z.get().checked_add(z)?,
        );
        if min.is_within_table_bounds() && max.is_within_table_bounds() {
            Self::new(min, max)
        } else {
            None
        }
    }

    /// Return whether this box completely contains another closed box.
    #[must_use]
    pub const fn contains_aabb(self, other: Self) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// Return whether two closed boxes share at least one point.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.min.x.get() <= other.max.x.get()
            && self.max.x.get() >= other.min.x.get()
            && self.min.y.get() <= other.max.y.get()
            && self.max.y.get() >= other.min.y.get()
            && self.min.z.get() <= other.max.z.get()
            && self.max.z.get() >= other.min.z.get()
    }

    /// Return whether both corners are inside the v1 table-local bound.
    #[must_use]
    pub const fn is_within_table_bounds(self) -> bool {
        self.min.is_within_table_bounds() && self.max.is_within_table_bounds()
    }
}

#[cfg(test)]
mod tests {
    use super::{AabbMm, HalfExtentsMm, Point3Mm, PoseMm, YawMilliDegrees};

    #[test]
    fn yaw_is_normalized_without_floating_point() {
        assert_eq!(YawMilliDegrees::new(450_000).get(), 90_000);
    }

    #[test]
    fn closed_integer_box_rejects_inverted_bounds() {
        assert!(AabbMm::new(Point3Mm::new(1, 0, 0), Point3Mm::new(0, 0, 0)).is_none());
    }

    #[test]
    fn closed_integer_box_contains_its_boundary() {
        let bounds =
            AabbMm::new(Point3Mm::new(-1, 0, -1), Point3Mm::new(1, 0, 1)).expect("valid bounds");
        assert!(bounds.contains(Point3Mm::new(1, 0, 1)));
        assert!(!bounds.contains(Point3Mm::new(2, 0, 1)));
    }

    #[test]
    fn checked_pose_and_centered_bounds_enforce_the_table_local_limit() {
        assert!(
            PoseMm::checked(Point3Mm::new(10_000, 0, -10_000), YawMilliDegrees::new(0)).is_some()
        );
        assert!(PoseMm::checked(Point3Mm::new(10_001, 0, 0), YawMilliDegrees::new(0)).is_none());
        assert!(
            AabbMm::from_center(Point3Mm::new(9_990, 0, 0), HalfExtentsMm::new(10, 0, 0)).is_some()
        );
        assert!(
            AabbMm::from_center(Point3Mm::new(9_991, 0, 0), HalfExtentsMm::new(10, 0, 0)).is_none()
        );
    }

    #[test]
    fn closed_boxes_treat_a_shared_boundary_as_an_intersection() {
        let left = AabbMm::new(Point3Mm::new(-2, 0, -2), Point3Mm::new(0, 2, 2)).expect("left box");
        let touching =
            AabbMm::new(Point3Mm::new(0, 0, -2), Point3Mm::new(2, 2, 2)).expect("touching box");
        let separate =
            AabbMm::new(Point3Mm::new(1, 0, -2), Point3Mm::new(2, 2, 2)).expect("separate box");
        assert!(left.intersects(touching));
        assert!(!left.intersects(separate));
        assert!(left.contains_aabb(left));
    }
}
