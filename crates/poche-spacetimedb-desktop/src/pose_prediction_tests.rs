// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![allow(clippy::float_cmp)] // Exact copies of bounded integer network values are the contract under test.
use super::*;

fn network_pose(sequence: u64, angle: i32) -> CardPoseView {
    CardPoseView {
        card_key: "room:alice:card-0-0".into(),
        card_id: "card-0-0".into(),
        owner: "alice".into(),
        owner_seat: 0,
        logical_location: "hand".into(),
        position_mm: [20, 40, 300],
        rotation_mdeg: [0, angle, 0],
        sequence,
    }
}

fn display_pose(network: &CardPoseView) -> DisplayPose {
    DisplayPose::from_network(network)
}

#[test]
fn rotation_snapback_unsent_single_tap_survives_unchanged_network_snapshot() {
    let network = network_pose(6, 0);
    let mut display = display_pose(&network);
    display.rotation_mdeg[1] = rotate_mdeg(0, card_rotation_direction(true, false), 45_000);
    // Q was pressed between 50 ms publishes. Another bridge change republishes
    // exactly the previous pose before this tap has been transmitted.
    display.reconcile(&network, true);
    assert_eq!(display.rotation_mdeg, [0, 45_000, 0]);
}

#[test]
fn rotation_snapback_delayed_echo_does_not_undo_newer_sent_rotation_after_release() {
    let network = network_pose(6, 0);
    let mut display = display_pose(&network);
    display.rotation_mdeg = [0, 90_000, 0];
    display.sequence = 8;
    display.reconcile(&network_pose(7, 45_000), false);
    assert_eq!(display.rotation_mdeg, [0, 90_000, 0]);
    assert_eq!(display.sequence, 8);
}

#[test]
fn rotation_snapback_current_acknowledgement_updates_remote_card_normally() {
    let mut display = display_pose(&network_pose(6, 0));
    display.reconcile(&network_pose(7, 45_000), false);
    assert_eq!(display.rotation_mdeg, [0, 45_000, 0]);
    assert_eq!(display.sequence, 7);
}

#[test]
fn rotation_snapback_repeated_taps_survive_each_older_echo_and_final_release() {
    let mut display = display_pose(&network_pose(0, 0));
    let mut drag = DragState::default();
    let start = Instant::now();
    for step in 0..4_u32 {
        let now = start + ROTATION_REPEAT_DELAY * step;
        assert!(drag.rotation_step_due(1, now));
        display.rotation_mdeg[1] = rotate_mdeg(display.rotation_mdeg[1], 1, 45_000);
        display.submitted(u64::from(step) + 1);
        display.reconcile(
            &network_pose(u64::from(step), i32::try_from(step).unwrap() * 45_000),
            true,
        );
    }
    drag.reset_rotation_repeat();
    display.current = [70.0, 40.0, 350.0];
    display.submitted(5);
    display.reconcile(&network_pose(3, 135_000), false);
    assert_eq!(display.rotation_mdeg, [0, 180_000, 0]);
    assert_eq!(display.target, [70.0, 40.0, 350.0]);
    let mut acknowledged = network_pose(5, 180_000);
    acknowledged.position_mm = [70, 40, 350];
    display.reconcile(&acknowledged, false);
    assert_eq!(display.rotation_mdeg, [0, 180_000, 0]);
    assert_eq!(display.target, [70.0, 40.0, 350.0]);
}

#[test]
fn rotation_snapback_authoritative_play_overrides_active_hand_prediction() {
    let mut display = display_pose(&network_pose(7, 0));
    display.rotation_mdeg = [0, 90_000, 0];
    display.submitted(10);
    let mut played = network_pose(9, 180_000);
    played.logical_location = "play".into();
    played.position_mm = [0, 10, 0];
    display.reconcile(&played, true);
    assert_eq!(display.rotation_mdeg, [0, 180_000, 0]);
    assert_eq!(display.target, [0.0, 10.0, 0.0]);
    assert_eq!(display.logical_location, "play");
}

#[test]
fn rotation_snapback_rejected_pose_returns_to_authority_instead_of_sticking() {
    let network = network_pose(7, 0);
    let mut display = display_pose(&network);
    display.current = [50.0, 40.0, 300.0];
    display.rotation_mdeg = [0, 90_000, 0];
    display.submitted(8);
    let mut poses = PoseDisplay(HashMap::from([(network.card_key.clone(), display)]));
    poses.reject_predictions(std::slice::from_ref(&network));
    let display = &poses.0[&network.card_key];
    assert_eq!(display.rotation_mdeg, [0, 0, 0]);
    assert_eq!(display.target, [20.0, 40.0, 300.0]);
    assert_eq!(display.sequence, 7);
}

#[test]
fn stationary_hold_stops_publishing_but_rotation_and_release_still_publish() {
    let network = network_pose(7, 0);
    let mut display = display_pose(&network);
    assert!(!display.needs_submission(&network));

    display.set_drag_height(hand_view::lift_height(40.0));
    assert!(display.needs_submission(&network));
    display.submitted(8);
    // Many frames and publish deadlines may pass before an acknowledgement.
    // Holding still must not produce the previous 20 unchanged writes/second.
    for _ in 0..100 {
        display.reconcile(&network, true);
        assert!(!display.needs_submission(&network));
    }

    display.rotation_mdeg[1] = 45_000;
    assert!(display.needs_submission(&network));
    display.submitted(9);
    assert!(!display.needs_submission(&network));

    display.set_drag_height(40.0);
    assert!(display.needs_submission(&network));
    display.submitted(10);
    assert!(!display.needs_submission(&network));
    display.reconcile(&network_pose(10, 45_000), false);
    assert!(!display.needs_submission(&network_pose(10, 45_000)));
}

#[test]
fn returning_to_old_authority_pose_still_publishes_over_newer_inflight_pose() {
    let network = network_pose(7, 0);
    let mut display = display_pose(&network);
    display.rotation_mdeg[1] = 45_000;
    display.submitted(8);
    display.rotation_mdeg[1] = 0;
    // Equal to the old authority is not equal to the pending Q tap: E must send.
    assert!(display.needs_submission(&network));
}

#[test]
fn new_authority_sequence_is_not_masked_by_duplicate_submission_suppression() {
    for sequence in [8, 9] {
        let mut display = display_pose(&network_pose(7, 0));
        display.rotation_mdeg[1] = 45_000;
        display.submitted(8);
        let authority = network_pose(sequence, 90_000);
        // A competing same-player device committed something else. It must not
        // be hidden behind our cached payload even at the same sequence.
        assert!(display.needs_submission(&authority));
        display.reconcile(&authority, false);
        assert_eq!(display.rotation_mdeg[1], 90_000);
        assert!(!display.needs_submission(&authority));
    }
}

#[test]
fn submillimeter_pointer_noise_does_not_publish_identical_quantized_poses() {
    let network = network_pose(7, 0);
    let mut display = display_pose(&network);
    display.current[0] += 0.49;
    assert!(!display.needs_submission(&network));
    display.current[0] += 0.02;
    assert!(display.needs_submission(&network));
}

fn pickup_after_unchanged_ray(ray: Ray3d, card: [f32; 3]) -> [f32; 3] {
    let anchor = Vec3::from_array(cursor_ray_to_world_mm(ray, card[1]).unwrap());
    let offset = Vec3::from_array(card) - anchor;
    let next = cursor_ray_to_world_mm(ray, drag_plane_height(card[1])).unwrap();
    (Vec3::from_array(next) + offset).to_array()
}

#[test]
fn pickup_jump_perspective_ray_keeps_planar_position_with_unchanged_cursor() {
    let card = [20.0, 40.0, 300.0];
    let eye = Vec3::new(1.0, 1.2, 1.0);
    let ray = Ray3d::new(eye, Dir3::new(mm_position(card) - eye).unwrap());
    let next = pickup_after_unchanged_ray(ray, card);
    assert!((next[0] - card[0]).abs() < 0.001, "X jumped: {next:?}");
    assert!((next[2] - card[2]).abs() < 0.001, "Z jumped: {next:?}");
}

#[test]
fn pickup_jump_topdown_ray_is_the_nearest_passing_case() {
    let card = [20.0, 40.0, 300.0];
    let ray = Ray3d::new(Vec3::new(0.02, 1.2, 0.3), Dir3::NEG_Y);
    let next = pickup_after_unchanged_ray(ray, card);
    assert!((next[0] - card[0]).abs() < 0.001);
    assert!((next[2] - card[2]).abs() < 0.001);
}

#[test]
fn pickup_jump_local_lift_moves_partway_up_without_planar_delay() {
    let mut display = display_pose(&network_pose(7, 0));
    display.set_drag_height(hand_view::lift_height(40.0));
    display.current[0] = 80.0;
    display.advance_display(0.25, true);
    assert!(display.current[1] > 40.0 && display.current[1] < 64.0);
    assert_eq!(display.current[0], 80.0);
}

#[test]
fn pickup_jump_lift_and_drop_send_final_height_while_both_clients_ease() {
    let network = network_pose(7, 0);
    let mut local = display_pose(&network);
    let mut peer = display_pose(&network);
    local.set_drag_height(64.0);
    local.submitted(8);
    assert_eq!(local.submission_position_mm(), [20, 64, 300]);
    assert_eq!(local.current[1], 40.0);
    let mut accepted = network.clone();
    accepted.sequence = 8;
    accepted.position_mm = local.submission_position_mm();
    peer.reconcile(&accepted, false);
    local.advance_display(0.25, true);
    peer.advance_display(0.25, false);
    assert_eq!(local.current, peer.current);
    assert_eq!(local.current[1], 46.0);
    assert!(!local.needs_submission(&network));

    local.set_drag_height(40.0);
    local.submitted(9);
    assert_eq!(local.submission_position_mm(), [20, 40, 300]);
    assert_eq!(local.current[1], 46.0);
    accepted.sequence = 9;
    accepted.position_mm = local.submission_position_mm();
    peer.reconcile(&accepted, false);
    local.advance_display(0.25, false);
    peer.advance_display(0.25, false);
    assert_eq!(local.current, peer.current);
    assert_eq!(local.current[1], 44.5);
    assert!(!local.needs_submission(&accepted));
}

#[test]
fn pickup_jump_lift_uses_same_fixed_plane_throughout_visual_tween() {
    let card = [20.0, 40.0, 300.0];
    let eye = Vec3::new(1.0, 1.2, 1.0);
    let ray = Ray3d::new(eye, Dir3::new(mm_position(card) - eye).unwrap());
    let mut display = display_pose(&network_pose(7, 0));
    display.set_drag_height(64.0);
    for _ in 0..20 {
        display.advance_display(0.25, true);
        let position = pickup_after_unchanged_ray(ray, card);
        assert!((position[0] - card[0]).abs() < 0.001);
        assert!((position[2] - card[2]).abs() < 0.001);
    }
    assert!(display.current[1] > 63.0);
}
