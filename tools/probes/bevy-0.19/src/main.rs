// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use bevy::prelude::{Component, Transform, Vec3};

#[derive(Component)]
struct SpatialObject(u16);

fn main() {
    let millimetres = Vec3::new(63.0, 1.0, 88.0);
    let transform = Transform::from_translation(millimetres / 1_000.0);
    let object = SpatialObject(1);
    assert_eq!(object.0, 1);
    assert_eq!(transform.translation, Vec3::new(0.063, 0.001, 0.088));

    let mut maximum_round_trip_error_mm = 0_i32;
    for exact_mm in -10_000_i32..=10_000 {
        let rendered_metres = exact_mm as f32 / 1_000.0;
        let recovered_mm = (rendered_metres * 1_000.0).round() as i32;
        maximum_round_trip_error_mm =
            maximum_round_trip_error_mm.max((recovered_mm - exact_mm).abs());
    }

    let conservative_two_metre_epsilon_mm = 2.0_f32 * f32::EPSILON * 1_000.0;
    println!(
        "bevy=0.19.0 features=3d range_mm=-10000..10000 \
         max_integer_round_trip_error_mm={maximum_round_trip_error_mm} \
         conservative_2m_epsilon_mm={conservative_two_metre_epsilon_mm:.9}"
    );
    assert_eq!(maximum_round_trip_error_mm, 0);
}
