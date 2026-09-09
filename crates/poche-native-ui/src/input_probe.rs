// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windowless pointer acceptance helper, enabled only by `input-probe`.
//! Uses a caller-owned authenticated live device, never a mutation shortcut.
use super::*;
use bevy::picking::{
    backend::{PointerHits, ray::RayMap},
    mesh_picking::{MeshPickingSettings, ray_cast::RayCastVisibility},
    pointer::{Location, PointerAction, PointerId, PointerInput, PointerLocation},
};

/// Move the first own-hand card through hand/table camera coordinates while
/// applying yaw, pitch and roll. Returns the live device after its signed pose is observed.
/// This exercises picking, observers and transport, not GPU rendering or OS mouse input.
///
/// # Errors
/// Returns missing-hand/camera, rejected-motion, logical-mutation or bounded
/// observation-timeout failures. Does not retry rejected writes.
pub fn drag_across_viewports(live: NativeLiveDevice) -> Result<NativeLiveDevice, String> {
    let controller = native_controller_from_observation(live.observation())?;
    let face = controller
        .first_owned_face()
        .ok_or("probe requires an own hand")?;
    let card = controller
        .scene
        .cards
        .iter()
        .find(|card| card.face == Some(face))
        .ok_or("probe card missing")?
        .clone();
    let identity = live
        .observation()
        .physical_hands
        .iter()
        .find(|card| card.face == Some(face.code()))
        .ok_or("probe requires physical identity")?
        .id
        .clone();
    let initial_revision = live.observation().projection.current_revision;
    let initial_transform = controller
        .physical_poses
        .get(&ObjectId::Card(card.id))
        .map_or_else(
            || {
                pose_transform(
                    controller
                        .physical_defaults
                        .get(&ObjectId::Card(card.id))
                        .copied()
                        .unwrap_or(card.pose),
                )
            },
            |pose| {
                Transform::from_translation(Vec3::from_array(
                    pose.position_mm.map(|v| v as f32 / 1000.0),
                ))
                .with_rotation(Quat::from_euler(
                    EulerRot::YXZ,
                    (pose.rotation_millidegrees[0] as f32 / 1000.0).to_radians(),
                    (pose.rotation_millidegrees[1] as f32 / 1000.0).to_radians(),
                    (pose.rotation_millidegrees[2] as f32 / 1000.0).to_radians(),
                ))
            },
        );
    let target = RenderTarget::Image(Handle::<Image>::default().into());
    let mut app = App::new();
    app.add_plugins((
        bevy::app::TaskPoolPlugin::default(),
        PickingPlugin,
        InteractionPlugin,
        MeshPickingPlugin,
    ));
    let half_extents = Vec3::new(
        millimetres(card.half_extents.x),
        millimetres(card.half_extents.y.max(1)),
        millimetres(card.half_extents.z),
    );
    let mut meshes = Assets::<Mesh>::default();
    let mesh = meshes.add(Cuboid::from_size(half_extents * 2.0));
    app.insert_resource(meshes)
        .insert_resource(MeshPickingSettings {
            ray_cast_visibility: RayCastVisibility::Any,
            ..default()
        })
        .add_systems(
            PreUpdate,
            image_target_pointer_rays
                .after(RayMap::repopulate)
                .in_set(bevy::picking::PickingSystems::ProcessInput),
        );
    app.insert_resource(controller)
        .insert_resource(live)
        .init_resource::<ButtonInput<KeyCode>>();
    let entity = app
        .world_mut()
        .spawn((
            DraggableCard(card.id),
            DragPreview::default(),
            Pickable::default(),
            Mesh3d(mesh),
            bevy::camera::primitives::Aabb::from_min_max(-half_extents, half_extents),
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            CanonicalMirror {
                id: ObjectId::Card(card.id),
                pose: card.pose,
                location: Some(card.location),
            },
            GlobalTransform::from(initial_transform),
        ))
        .observe(on_drag_start)
        .observe(on_drag_card)
        .observe(on_drag_end)
        .id();
    for (order, viewport, transform) in [
        (0, None, CameraView::home().transform()),
        (
            1,
            Some(bevy::camera::Viewport {
                physical_position: UVec2::new(200, 360),
                physical_size: UVec2::new(400, 120),
                ..default()
            }),
            Transform::from_translation(initial_transform.translation + Vec3::Y * 0.5)
                .looking_at(initial_transform.translation, Vec3::NEG_Z),
        ),
    ] {
        let size = viewport
            .as_ref()
            .map_or(UVec2::new(800, 600), |viewport| viewport.physical_size);
        let mut projection = Projection::Perspective(PerspectiveProjection::default());
        projection.update(size.x as f32, size.y as f32);
        let mut camera = Camera {
            order,
            viewport,
            ..default()
        };
        camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: UVec2::new(800, 600),
            scale_factor: 1.0,
        });
        camera.computed.clip_from_view = projection.get_clip_from_view();
        let mut camera_entity =
            app.world_mut()
                .spawn((camera, GlobalTransform::from(transform), target.clone()));
        if order == 0 {
            camera_entity.insert(TabletopCamera);
        } else {
            camera_entity.insert(HandCamera);
        }
    }
    let location = |position| Location {
        target: target.normalize(None).unwrap(),
        position,
    };
    let initial_location = location(Vec2::new(400.0, 420.0));
    app.world_mut().spawn((
        PointerId::Mouse,
        PointerLocation::new(initial_location.clone()),
    ));
    app.update();
    if !app
        .world_mut()
        .resource_mut::<Messages<PointerHits>>()
        .drain()
        .any(|hits| hits.picks.iter().any(|(hit, _)| *hit == entity))
    {
        return Err("initial hand-camera ray did not pick the card".to_owned());
    }
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        initial_location,
        PointerAction::Press(PointerButton::Primary),
    ));
    app.update();
    let mut previous = Vec2::new(400.0, 420.0);
    for (position, expected_camera, modifier) in [
        (Vec2::new(410.0, 410.0), 1, KeyCode::ShiftLeft),
        (Vec2::new(420.0, 350.0), 0, KeyCode::ControlLeft),
        (Vec2::new(430.0, 330.0), 0, KeyCode::AltLeft),
    ] {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(modifier);
        let mut cameras = app
            .world_mut()
            .query::<(&Camera, &GlobalTransform, &RenderTarget)>();
        let selected = select_drag_camera(cameras.iter(app.world()), &location(position), None)
            .map(|(camera, _)| camera.order);
        if selected != Some(expected_camera) {
            return Err("probe did not cross the intended camera boundary".to_owned());
        }
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location(position),
            PointerAction::Move {
                delta: position - previous,
            },
        ));
        app.update();
        if app
            .world()
            .get::<DragPreview>(entity)
            .is_none_or(|preview| preview.pointer != Some(PointerId::Mouse))
        {
            return Err("pointer pipeline did not retain the dragged card".to_owned());
        }
        previous = position;
    }
    let preview = app
        .world()
        .get::<DragPreview>(entity)
        .ok_or("gesture disappeared")?;
    let expected_position = preview
        .physical_origin
        .ok_or("drag did not submit movement")?
        .to_array()
        .map(|v| (v * 1000.0).round() as i32);
    let expected_rotation = preview
        .physical_rotation
        .ok_or("drag did not submit rotation")?;
    let (yaw, pitch, roll) = initial_transform.rotation.to_euler(EulerRot::YXZ);
    let rotated_once_per_axis = [yaw, pitch, roll]
        .map(|angle| ((angle.to_degrees() * 1000.0).round() as i32 + 5000).rem_euclid(360000));
    if expected_rotation != rotated_once_per_axis {
        return Err("pointer gesture did not apply each rotation axis exactly once".to_owned());
    }
    if expected_position[1] != (initial_transform.translation.y * 1000.0).round() as i32 {
        return Err("planar drag changed the card's height".to_owned());
    }
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location(previous),
        PointerAction::Release(PointerButton::Primary),
    ));
    app.update();
    if !app
        .world()
        .get::<Pickable>(entity)
        .is_some_and(|pickable| pickable.is_hoverable && pickable.should_block_lower)
    {
        return Err("pointer release did not restore card picking".to_owned());
    }
    let mut live = app
        .world_mut()
        .remove_resource::<NativeLiveDevice>()
        .ok_or("device disappeared")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        live.poll()?;
        if live.observation().projection.current_revision != initial_revision {
            return Err("physical drag changed logical revision".to_owned());
        }
        if live
            .observation()
            .physical_hands
            .iter()
            .find(|card| card.id == identity)
            .and_then(|card| card.pose.as_ref())
            .is_some_and(|pose| {
                pose.position_mm == expected_position
                    && pose.rotation_millidegrees == expected_rotation
            })
        {
            return Ok(live);
        }
        if Instant::now() >= deadline {
            return Err("drag pose did not reach its own observation".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
