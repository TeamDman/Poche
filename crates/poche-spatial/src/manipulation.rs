// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared physical state, deliberately separate from logical card location.
//! Authentication and lease allocation belong to the device/session layer.
//! This reducer consumes an already authenticated device's lease epoch; callers
//! must never treat a caller-supplied matching epoch as proof of authority.

use crate::{CardLocation, CardObjectId, Point3Mm, PoseMm, YawMilliDegrees};

/// Full orientation in normalized millidegrees. Render using intrinsic YXZ
/// order (yaw, pitch, roll); wire values are integers, not unchecked floats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RotationMilliDegrees {
    pub yaw: YawMilliDegrees,
    pub pitch: YawMilliDegrees,
    pub roll: YawMilliDegrees,
}

/// World pose, independent of the hand/deck/play rules location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ManipulationPose {
    pub translation: Point3Mm,
    pub rotation: RotationMilliDegrees,
}

impl From<PoseMm> for ManipulationPose {
    fn from(pose: PoseMm) -> Self {
        Self {
            translation: pose.translation,
            rotation: RotationMilliDegrees {
                yaw: pose.yaw,
                ..RotationMilliDegrees::default()
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationError {
    OutsideWorld,
    WrongLease,
    StaleSequence,
    StaleLease,
}

/// A physical overlay. It cannot mutate or reveal a logical card: neither
/// logical location nor face is stored here. Each lease has one authorized
/// device writer, allocated externally in accepted session order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardManipulation {
    card: CardObjectId,
    pose: ManipulationPose,
    lease_epoch: u64,
    sequence: u64,
    inside_play: bool,
}

impl CardManipulation {
    /// Initialize from an accepted snapshot, not untrusted pose traffic.
    pub fn new(
        card: CardObjectId,
        pose: ManipulationPose,
        lease_epoch: u64,
    ) -> Result<Self, ManipulationError> {
        if !pose.translation.is_within_table_bounds() {
            return Err(ManipulationError::OutsideWorld);
        }
        Ok(Self {
            card,
            pose,
            lease_epoch,
            sequence: 0,
            inside_play: false,
        })
    }

    #[must_use]
    pub const fn card(&self) -> CardObjectId {
        self.card
    }

    #[must_use]
    pub const fn pose(&self) -> ManipulationPose {
        self.pose
    }

    /// Apply an authenticated, permitted writer's sample. Play-region entry
    /// produces an intent edge, not a logical transition. Merely advancing a
    /// turn while the card stays inside the region produces no new attempt.
    /// `inside_play` must be computed by the receiver from validated geometry.
    pub fn update(
        &mut self,
        lease_epoch: u64,
        sequence: u64,
        pose: ManipulationPose,
        inside_play: bool,
    ) -> Result<bool, ManipulationError> {
        if lease_epoch != self.lease_epoch {
            return Err(ManipulationError::WrongLease);
        }
        if sequence <= self.sequence {
            return Err(ManipulationError::StaleSequence);
        }
        if !pose.translation.is_within_table_bounds() {
            return Err(ManipulationError::OutsideWorld);
        }
        let attempt = inside_play && !self.inside_play;
        self.pose = pose;
        self.sequence = sequence;
        self.inside_play = inside_play;
        Ok(attempt)
    }

    /// Called only after the session accepts a new device's manipulation
    /// lease. Preserve pose and region occupancy: handoff is not a play.
    pub fn change_lease(&mut self, lease_epoch: u64) -> Result<(), ManipulationError> {
        if lease_epoch <= self.lease_epoch {
            return Err(ManipulationError::StaleLease);
        }
        self.lease_epoch = lease_epoch;
        self.sequence = 0;
        Ok(())
    }
}

/// Logical ownership is necessary, but not sufficient, for manipulation:
/// session/device authorization and the current lease are checked separately.
#[must_use]
pub fn hand_owner(location: CardLocation) -> Option<crate::SeatId> {
    match location {
        CardLocation::Hand { seat, .. } => Some(seat),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> CardManipulation {
        CardManipulation::new(
            CardObjectId {
                projection_epoch: 1,
                ordinal: 5,
            },
            ManipulationPose::default(),
            1,
        )
        .unwrap()
    }

    #[test]
    fn physical_play_entry_is_one_intent_not_a_delayed_play() {
        let mut state = state();
        let mut pose = state.pose();
        pose.translation = Point3Mm::new(100, 200, 300);
        pose.rotation.pitch = YawMilliDegrees::new(90_000);
        pose.rotation.roll = YawMilliDegrees::new(45_000);
        assert_eq!(state.update(1, 1, pose, true), Ok(true));
        // Rejected logical play leaves this independently accepted pose intact.
        assert_eq!(state.pose(), pose);
        assert_eq!(state.update(1, 2, pose, true), Ok(false));
        assert_eq!(state.update(1, 3, pose, false), Ok(false));
        assert_eq!(state.update(1, 4, pose, true), Ok(true));
    }

    #[test]
    fn delayed_samples_and_old_devices_cannot_rewind_pose() {
        let mut state = state();
        let pose = state.pose();
        state.update(1, 8, pose, true).unwrap();
        let before = state.clone();
        assert_eq!(
            state.update(1, 7, pose, false),
            Err(ManipulationError::StaleSequence)
        );
        assert_eq!(state, before);
        state.change_lease(2).unwrap();
        assert_eq!(
            state.update(1, 9, pose, false),
            Err(ManipulationError::WrongLease)
        );
        assert_eq!(state.update(2, 1, pose, true), Ok(false));
        assert_eq!(state.change_lease(1), Err(ManipulationError::StaleLease));
    }

    #[test]
    fn invalid_pose_is_atomic_and_does_not_consume_sequence() {
        let mut state = state();
        let before = state.clone();
        let mut invalid = state.pose();
        invalid.translation = Point3Mm::new(i32::MAX, 0, 0);
        assert_eq!(
            state.update(1, 1, invalid, true),
            Err(ManipulationError::OutsideWorld)
        );
        assert_eq!(state, before);
        assert_eq!(state.update(1, 1, before.pose(), false), Ok(false));
    }
}
