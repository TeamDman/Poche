// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use core::fmt;

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
}

#[cfg(test)]
mod tests {
    use super::{AabbMm, Point3Mm, YawMilliDegrees};

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
}
