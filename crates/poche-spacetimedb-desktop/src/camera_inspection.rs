// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Temporary camera bookmarks for inspecting real objects, never reader GUIs.

use super::{CameraPose, TACTICAL_VIEW_HEIGHT, TableCameraController, TableCameraMode};
use bevy::prelude::*;

#[derive(Debug, Default)]
pub(super) struct InspectionState {
    saved: Option<Bookmark>,
    consume_gesture: bool,
}

#[derive(Clone, Copy, Debug)]
struct Bookmark {
    pose: CameraPose,
    mode: TableCameraMode,
    scale: f32,
    perspective_target: CameraPose,
    tactical_target: CameraPose,
}

impl TableCameraController {
    pub(super) fn inspecting_sheet(&self) -> bool {
        self.inspection.saved.is_some()
    }

    pub(super) fn inspect_sheet(&mut self, center: Vec3, size: Vec2, aspect: f32) {
        if self.inspecting_sheet() {
            return;
        }
        self.inspection.saved = Some(Bookmark {
            pose: self.current,
            mode: self.mode,
            scale: self.current_orthographic_scale,
            perspective_target: self.perspective_target,
            tactical_target: self.tactical_target,
        });
        if self.mode == TableCameraMode::Perspective {
            // Match scale at the focal plane before changing projection. Then
            // both the actual camera pose and close-up framing ease together.
            self.current_orthographic_scale =
                perspective_span(self.current.distance) / TACTICAL_VIEW_HEIGHT;
        }
        self.mode = TableCameraMode::Tactical;
        self.target = CameraPose {
            focus: center,
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            distance: 0.55,
        };
        self.target_orthographic_scale =
            (size.y.max(size.x / aspect.max(0.1)) * 1.35) / TACTICAL_VIEW_HEIGHT;
    }

    /// Returns true while a dismissal gesture must not also move the camera.
    /// Consumption is local application state, not an OS button release.
    pub(super) fn inspection_input(&mut self, movement: bool, held: bool) -> bool {
        if self.inspection.consume_gesture {
            self.inspection.consume_gesture = held;
            return true;
        }
        if !movement {
            return self.inspecting_sheet();
        }
        let Some(saved) = self.inspection.saved.take() else {
            return false;
        };
        if saved.mode == TableCameraMode::Perspective {
            // Preserve apparent size at the sheet while returning to perspective.
            // Using the old distant pose immediately would be a visible jump.
            self.current.distance =
                self.current_orthographic_scale * TACTICAL_VIEW_HEIGHT / perspective_span(1.0);
        }
        self.mode = saved.mode;
        self.target = saved.pose;
        self.target_orthographic_scale = saved.scale;
        self.perspective_target = saved.perspective_target;
        self.tactical_target = saved.tactical_target;
        self.inspection.consume_gesture = held;
        true
    }
}

fn perspective_span(distance: f32) -> f32 {
    2.0 * distance * (PerspectiveProjection::default().fov * 0.5).tan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_inspection_restores_actual_previous_pose_and_consumes_entire_drag() {
        let mut camera = TableCameraController::default();
        camera.current.focus = Vec3::new(0.12, 0.02, -0.3);
        camera.current.distance = 1.25;
        let before = camera.current;
        camera.inspect_sheet(Vec3::new(0.265, 0.0265, 0.0), Vec2::new(0.2, 0.29), 1.6);
        assert_eq!(camera.mode, TableCameraMode::Tactical);
        assert!((camera.target.pitch - std::f32::consts::FRAC_PI_2).abs() < f32::EPSILON);
        assert!(camera.target_orthographic_scale * TACTICAL_VIEW_HEIGHT >= 0.29);
        assert!(camera.target.transform().rotation.is_finite());
        // Pressing without movement retains inspection. Moving dismisses it.
        assert!(camera.inspection_input(false, true));
        assert!(camera.inspecting_sheet());
        assert!(camera.inspection_input(true, true));
        assert!(!camera.inspecting_sheet());
        assert_eq!(camera.target, before);
        assert_eq!(camera.mode, TableCameraMode::Perspective);
        assert!(camera.inspection_input(true, true));
        assert!(camera.inspection_input(false, false));
        assert!(!camera.inspection_input(true, true));
    }

    #[test]
    fn score_inspection_preserves_tactical_zoom_and_fits_narrow_windows() {
        let mut camera = TableCameraController::default();
        camera.toggle_mode();
        camera.current_orthographic_scale = 0.42;
        camera.current = camera.target;
        let before = camera.current;
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 0.4);
        assert!(camera.target_orthographic_scale * TACTICAL_VIEW_HEIGHT * 0.4 >= 0.2);
        assert!(camera.inspection_input(true, false));
        assert_eq!(camera.mode, TableCameraMode::Tactical);
        assert_eq!(camera.target, before);
        assert!((camera.target_orthographic_scale - 0.42).abs() < f32::EPSILON);
    }

    #[test]
    fn exactly_vertical_camera_has_finite_rotation_and_correct_up() {
        let mut camera = TableCameraController::default();
        camera.inspect_sheet(Vec3::ZERO, Vec2::ONE, 1.0);
        let transform = camera.target.transform();
        assert!(transform.rotation.is_finite());
        assert!(transform.forward().abs_diff_eq(Vec3::NEG_Y, 0.00001));
        assert!(transform.up().abs_diff_eq(Vec3::NEG_Z, 0.00001));
    }
}
