// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Temporary camera bookmarks for inspecting real objects, never reader GUIs.

use super::{CameraPose, TACTICAL_VIEW_HEIGHT, TableCameraController, TableCameraMode};
use bevy::prelude::*;

#[derive(Debug, Default)]
pub(super) struct InspectionState {
    saved: Option<Bookmark>,
}

#[derive(Clone, Copy, Debug)]
struct Bookmark {
    pose: CameraPose,
    mode: TableCameraMode,
    scale: f32,
    top_down_return_angles: Option<Vec2>,
}

impl TableCameraController {
    pub(super) fn inspecting_sheet(&self) -> bool {
        self.inspection.saved.is_some()
    }

    pub(super) fn inspect_sheet(&mut self, center: Vec3, size: Vec2, aspect: f32) {
        if let Some(saved) = self.inspection.saved.take() {
            self.set_projection(saved.mode);
            self.target = saved.pose;
            self.target_orthographic_scale = saved.scale;
            self.top_down_return_angles = saved.top_down_return_angles;
            return;
        }
        self.inspection.saved = Some(Bookmark {
            pose: self.current,
            mode: self.mode,
            scale: self.current_orthographic_scale,
            top_down_return_angles: self.top_down_return_angles,
        });
        // O can change the inspection angle without creating another camera
        // mode or overwriting the pre-inspection bookmark.
        self.top_down_return_angles.get_or_insert(Vec2::new(
            self.current.yaw,
            self.current.pitch.min(super::CAMERA_MAX_PITCH),
        ));
        self.set_projection(TableCameraMode::Orthographic);
        self.target = CameraPose {
            focus: center,
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            distance: 0.55,
        };
        self.target_orthographic_scale = sheet_fit_scale(size, aspect);
    }

    /// Match apparent size at the focal plane when changing projection. There
    /// are no independent hidden poses for perspective versus orthographic.
    pub(super) fn set_projection(&mut self, mode: TableCameraMode) {
        if self.mode == mode {
            return;
        }
        match mode {
            TableCameraMode::Orthographic => {
                self.current_orthographic_scale =
                    perspective_span(self.current.distance) / TACTICAL_VIEW_HEIGHT;
                self.target_orthographic_scale =
                    perspective_span(self.target.distance) / TACTICAL_VIEW_HEIGHT;
            }
            TableCameraMode::Perspective => {
                self.current.distance =
                    self.current_orthographic_scale * TACTICAL_VIEW_HEIGHT / perspective_span(1.0);
                self.target.distance =
                    self.target_orthographic_scale * TACTICAL_VIEW_HEIGHT / perspective_span(1.0);
            }
        }
        self.mode = mode;
    }
}

fn sheet_fit_scale(size: Vec2, aspect: f32) -> f32 {
    // Fit the whole paper to whichever frame dimension is limiting. Six percent
    // extra span leaves a small border without the former 26% blank margin.
    (size.y.max(size.x / aspect.max(0.1)) * 1.06) / TACTICAL_VIEW_HEIGHT
}

fn perspective_span(distance: f32) -> f32 {
    2.0 * distance * (PerspectiveProjection::default().fov * 0.5).tan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_sheet_click_restores_view_without_requiring_a_pan() {
        let mut camera = TableCameraController::default();
        let before = camera.current;
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 1.6);
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 1.6);
        assert!(
            !camera.inspecting_sheet(),
            "second click must dismiss inspection"
        );
        assert_eq!(camera.target, before);
    }

    #[test]
    fn sheet_fills_at_least_ninety_percent_of_the_limiting_frame_dimension() {
        let mut camera = TableCameraController::default();
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 1.6);
        let height = camera.target_orthographic_scale * TACTICAL_VIEW_HEIGHT;
        assert!(
            0.29 / height >= 0.90,
            "paper occupies too little frame height"
        );
    }

    #[test]
    fn score_inspection_keeps_bookmark_during_pan_zoom_and_restores_on_second_click() {
        let mut camera = TableCameraController::default();
        camera.current.focus = Vec3::new(0.12, 0.02, -0.3);
        camera.current.distance = 1.25;
        let before = camera.current;
        camera.inspect_sheet(Vec3::new(0.265, 0.0265, 0.0), Vec2::new(0.2, 0.29), 1.6);
        assert_eq!(camera.mode, TableCameraMode::Orthographic);
        assert!((camera.target.pitch - std::f32::consts::FRAC_PI_2).abs() < f32::EPSILON);
        assert!(camera.target_orthographic_scale * TACTICAL_VIEW_HEIGHT >= 0.29);
        assert!(camera.target.transform().rotation.is_finite());
        // Pan/zoom is ordinary camera input, not an implicit dismiss gesture.
        camera.target.focus.x += 0.02;
        camera.target_orthographic_scale =
            super::super::zoom_orthographic_scale(camera.target_orthographic_scale, 1.0);
        assert!(camera.inspecting_sheet());
        camera.inspect_sheet(Vec3::ZERO, Vec2::ONE, 1.6);
        assert!(!camera.inspecting_sheet());
        assert_eq!(camera.target, before);
        assert_eq!(camera.mode, TableCameraMode::Perspective);
    }

    #[test]
    fn score_inspection_preserves_orthographic_zoom_and_fits_narrow_windows() {
        let mut camera = TableCameraController::default();
        camera.toggle_projection();
        camera.current_orthographic_scale = 0.42;
        camera.current = camera.target;
        let before = camera.current;
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 0.4);
        let width = camera.target_orthographic_scale * TACTICAL_VIEW_HEIGHT * 0.4;
        assert!(width >= 0.2);
        assert!(0.2 / width >= 0.90);
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 0.4);
        assert_eq!(camera.mode, TableCameraMode::Orthographic);
        assert_eq!(camera.target, before);
        assert!((camera.target_orthographic_scale - 0.42).abs() < f32::EPSILON);
    }

    #[test]
    fn projection_and_top_down_toggles_do_not_create_nested_inspections() {
        let mut camera = TableCameraController::default();
        let before = camera.current;
        camera.inspect_sheet(Vec3::ZERO, Vec2::new(0.2, 0.29), 1.6);
        camera.toggle_projection();
        assert_eq!(camera.mode, TableCameraMode::Perspective);
        assert!((camera.target.pitch - std::f32::consts::FRAC_PI_2).abs() < f32::EPSILON);
        camera.toggle_top_down();
        assert!((camera.target.pitch - before.pitch).abs() < f32::EPSILON);
        assert_eq!(camera.mode, TableCameraMode::Perspective);
        assert!(camera.inspecting_sheet());
        camera.toggle_projection();
        camera.toggle_top_down();
        camera.inspect_sheet(Vec3::ZERO, Vec2::ONE, 1.0);
        assert!(!camera.inspecting_sheet());
        assert_eq!(camera.mode, TableCameraMode::Perspective);
        assert_eq!(camera.target, before);
        assert!(camera.top_down_return_angles.is_none());
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
