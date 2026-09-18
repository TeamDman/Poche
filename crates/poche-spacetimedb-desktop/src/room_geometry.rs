// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Presentation-only room dimensions. Enlarging the room must not enlarge the
//! canonical tabletop, payment zones, or the camera's tabletop focus bounds.

use bevy::prelude::*;

pub(super) const FLOOR_SIZE: Vec3 = Vec3::new(4.2, 0.12, 4.2);
pub(super) const FLOOR_CENTER: Vec3 = Vec3::new(0.0, -0.08, 0.0);
pub(super) const DOOR_LEAF_SIZE: Vec3 = Vec3::new(0.40, 0.88, 0.045);
const DOOR_POST_SIZE: Vec3 = Vec3::new(0.045, 0.94, 0.075);
const DOOR_HEADER_SIZE: Vec3 = Vec3::new(0.485, 0.045, 0.075);

pub(super) fn floor_shape() -> Cuboid {
    Cuboid::from_size(FLOOR_SIZE)
}

pub(super) fn door_center() -> Vec3 {
    Vec3::new(
        -0.82,
        FLOOR_CENTER.y + (FLOOR_SIZE.y + DOOR_POST_SIZE.y) * 0.5,
        FLOOR_CENTER.z + (FLOOR_SIZE.z - DOOR_POST_SIZE.z) * 0.5,
    )
}

pub(super) fn door_frame_parts() -> [(Vec3, Vec3); 3] {
    [
        (Vec3::new(-0.22, 0.0, 0.0), DOOR_POST_SIZE),
        (Vec3::new(0.22, 0.0, 0.0), DOOR_POST_SIZE),
        (Vec3::new(0.0, 0.46, 0.0), DOOR_HEADER_SIZE),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn mesh_bounds(mesh: &Mesh, center: Vec3) -> (Vec3, Vec3) {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("room geometry must have positions");
        };
        positions.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), position| {
                let position = Vec3::from_array(*position) + center;
                (min.min(position), max.max(position))
            },
        )
    }

    #[test]
    fn enlarged_room_floor_preserves_its_top_surface_and_centre() {
        let (min, max) = mesh_bounds(&Mesh::from(floor_shape()), FLOOR_CENTER);
        assert!((max.x - min.x - 4.2).abs() < 0.000_001);
        assert!((max.z - min.z - 4.2).abs() < 0.000_001);
        assert!((max.y + 0.02).abs() < 0.000_001);
        assert!((max.x + min.x).abs() < 0.000_001);
        assert!((max.z + min.z).abs() < 0.000_001);
    }

    #[test]
    fn actual_door_frame_meshes_end_at_floor_perimeter_with_posts_on_surface() {
        let (floor_min, floor_max) = mesh_bounds(&Mesh::from(floor_shape()), FLOOR_CENTER);
        for (part_index, (offset, size)) in door_frame_parts().into_iter().enumerate() {
            let (min, max) =
                mesh_bounds(&Mesh::from(Cuboid::from_size(size)), door_center() + offset);
            assert!((max.z - floor_max.z).abs() < 0.000_001);
            assert!(min.x >= floor_min.x && max.x <= floor_max.x);
            if part_index < 2 {
                assert!((min.y - floor_max.y).abs() < 0.000_001);
            }
        }
        assert!(door_center().z > 2.0);
    }
}
