// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Viewer-local camera framing. Never mutates the canonical world or permissions.
use super::*;

pub(super) struct Viewports {
    pub table: URect,
    pub hand: URect,
}

fn fifths(value: u32, numerator: u32) -> u32 {
    // Exact floor(value * numerator / 5), without intermediate u32 overflow.
    value / 5 * numerator + value % 5 * numerator / 5
}

pub(super) fn viewports(size: UVec2, has_hand: bool) -> Viewports {
    let hand_top = fifths(size.y, 3);
    let actions_top = fifths(size.y, 4);
    Viewports {
        table: URect::from_corners(
            UVec2::ZERO,
            UVec2::new(size.x, if has_hand { hand_top } else { actions_top }),
        ),
        hand: URect::from_corners(
            UVec2::new(size.x / 5, hand_top),
            UVec2::new(fifths(size.x, 4), actions_top),
        ),
    }
}

pub(super) fn viewport(rectangle: URect) -> bevy::camera::Viewport {
    bevy::camera::Viewport {
        physical_position: rectangle.min,
        physical_size: rectangle.size(),
        ..default()
    }
}

fn home_from_anchor(anchor: Option<Vec3>) -> CameraView {
    let mut home = CameraView::home();
    if let Some(anchor) = anchor {
        let relative = anchor - home.target;
        if relative.x * relative.x + relative.z * relative.z > f32::EPSILON {
            home.yaw = relative.x.atan2(relative.z);
        }
    }
    home
}

pub(super) fn seat_home(controller: &NativeController) -> CameraView {
    home_from_anchor(controller.issuing_seat.and_then(|seat| {
        controller
            .scene
            .objects
            .iter()
            .find(|object| object.id == ObjectId::Seat(seat))
            .map(|object| point_to_vec3(object.pose.translation))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_hand_and_action_regions_are_bounded_and_nonoverlapping() {
        for width in [0, 1, 9, 10, 320, 360, 1280, 1920, 3840, u32::MAX] {
            for height in [0, 1, 9, 10, 480, 800, 960, 2160, u32::MAX] {
                for has_hand in [false, true] {
                    let layout = viewports(UVec2::new(width, height), has_hand);
                    for rect in [layout.table, layout.hand] {
                        assert!(rect.min.x <= rect.max.x && rect.min.y <= rect.max.y);
                        assert!(rect.max.x <= width && rect.max.y <= height);
                    }
                    if has_hand {
                        assert!(layout.table.max.y <= layout.hand.min.y);
                    }
                    assert!(layout.table.max.y <= fifths(height, 4));
                    assert!(layout.hand.max.y <= fifths(height, 4));
                    if width >= 10 && height >= 10 {
                        assert!(layout.table.size().min_element() > 0);
                        assert!(layout.hand.size().min_element() > 0);
                    }
                }
            }
        }
    }

    #[test]
    fn home_projects_each_seat_below_table_center() {
        let projection = Mat4::perspective_rh(PI / 4.0, 1.6, 0.01, 100.0);
        for anchor in [Vec3::Z, Vec3::NEG_Z, Vec3::X, Vec3::NEG_X] {
            let home = home_from_anchor(Some(anchor));
            let world_to_clip = projection * home.transform().to_matrix().inverse();
            assert!(
                world_to_clip.project_point3(anchor).y
                    < world_to_clip.project_point3(home.target).y
            );
        }
        assert_eq!(home_from_anchor(None), CameraView::home());
    }

    #[test]
    fn only_seat_changes_reframe_and_reset_returns_to_viewers_home() {
        let mut rig = CameraRig::default();
        let seat = Some(SeatId::new(1, poche_spatial::LayoutId::new(2, 1).unwrap()).unwrap());
        let home = home_from_anchor(Some(Vec3::NEG_Z));
        rig.follow_seat(seat, home);
        assert!(rig.reset.is_some());
        rig.advance_reset(CAMERA_RESET_SECONDS);
        assert_eq!(rig.view, home);
        rig.view.target += Vec3::X;
        let manual = rig.view;
        rig.follow_seat(seat, home);
        assert_eq!(rig.view, manual);
        assert!(rig.reset.is_none());
        rig.begin_reset();
        rig.advance_reset(CAMERA_RESET_SECONDS);
        assert_eq!(rig.view, home);
        rig.follow_seat(None, CameraView::home());
        rig.advance_reset(CAMERA_RESET_SECONDS);
        assert_eq!(rig.view, CameraView::home());
    }
}
