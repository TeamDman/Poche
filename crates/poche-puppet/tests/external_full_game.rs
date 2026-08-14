// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::Duration;

use poche_puppet::{
    EXTERNAL_DEVICES_FULL_GAME, PuppetRunOptions, PuppetSurface, PuppetTransport, run,
};

/// This is deterministic and uses loopback networking only, but remains an
/// explicit acceptance because it crosses hundreds of real signed HTTP
/// requests and is intentionally slower than ordinary unit tests.
#[test]
#[ignore = "full external socket acceptance"]
fn certified_graphical_siblings_take_over_one_complete_hosted_game() {
    let temporary = tempfile::tempdir().expect("temporary external artifacts");
    let report = run(&PuppetRunOptions {
        scenario: EXTERNAL_DEVICES_FULL_GAME.to_owned(),
        surface: PuppetSurface::Headless,
        transport: PuppetTransport::HttpLoopback,
        seed: 73,
        artifact_root: temporary.path().to_path_buf(),
        per_action_timeout: Duration::from_secs(5),
        whole_run_timeout: Duration::from_mins(2),
        ..PuppetRunOptions::default()
    })
    .expect("external full-game puppet");

    assert_eq!(report.status, "complete");
    assert_eq!(report.final_room_phase, "post_game");
    assert_eq!(report.final_revision, 160);
    assert_eq!(report.public_history_events, 138);
    assert_eq!(report.devices.len(), 5);
    assert_eq!(
        report
            .devices
            .iter()
            .map(|device| device.final_revision)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([report.final_revision])
    );
    assert!(
        report
            .steps
            .iter()
            .any(|step| step.acting_device == "alice-browser")
    );
    assert!(
        report
            .steps
            .iter()
            .any(|step| step.acting_device == "bob-native")
    );
    assert!(report.steps.iter().all(|step| step.observed_by.len() == 5));
    assert!(
        report
            .steps
            .iter()
            .any(|step| step.action_id == "room-reconnect" && step.acting_device == "bob-native")
    );
    assert_eq!(report.lifecycle.len(), 4);
    assert!(!report.lifecycle[1].member_connected);
    assert!(!report.lifecycle[2].member_connected);
    assert!(!report.lifecycle[3].member_connected);
}

#[test]
#[ignore = "requires a working GPU backend and full external socket acceptance"]
fn authorized_native_relay_captures_the_same_hosted_game() {
    let temporary = tempfile::tempdir().expect("temporary external native artifacts");
    let report = run(&PuppetRunOptions {
        scenario: EXTERNAL_DEVICES_FULL_GAME.to_owned(),
        surface: PuppetSurface::Native,
        transport: PuppetTransport::HttpLoopback,
        seed: 74,
        artifact_root: temporary.path().to_path_buf(),
        per_action_timeout: Duration::from_secs(10),
        whole_run_timeout: Duration::from_mins(3),
        ..PuppetRunOptions::default()
    })
    .expect("external native full-game puppet");

    assert_eq!(report.status, "complete");
    assert_eq!(report.final_room_phase, "post_game");
    assert_eq!(report.final_revision, 160);
    assert_eq!(report.public_history_events, 138);
    assert_eq!(report.lifecycle.len(), 4);
    assert_eq!(report.captures.len(), 6);
    assert_eq!(
        report
            .captures
            .iter()
            .map(|capture| capture.label.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "bidding",
            "card-selection",
            "score-sheet",
            "terminal",
            "trick-in-progress",
            "trick-resolved",
        ])
    );
    for capture in &report.captures {
        assert_eq!(capture.status, "complete");
        assert!(capture.windowless);
        assert_eq!(capture.requested_revision, capture.captured_revision);
        assert_eq!(capture.provider_kind, "native_bevy_external_relay");
        assert!(capture.transferred_bytes > 0);
        assert!(capture.transfer_chunks > 0);
        assert!(std::path::Path::new(&capture.manifest_path).is_file());
    }
    assert_eq!(
        report
            .captures
            .iter()
            .find(|capture| capture.label == "terminal")
            .expect("terminal capture")
            .captured_revision,
        report.final_revision
    );
}

#[test]
#[ignore = "requires a supported headless browser and full external socket acceptance"]
fn authorized_browser_relay_captures_the_same_hosted_game_without_a_window() {
    let temporary = tempfile::tempdir().expect("temporary external browser artifacts");
    let report = run(&PuppetRunOptions {
        scenario: EXTERNAL_DEVICES_FULL_GAME.to_owned(),
        surface: PuppetSurface::Web,
        transport: PuppetTransport::HttpLoopback,
        seed: 75,
        artifact_root: temporary.path().to_path_buf(),
        per_action_timeout: Duration::from_secs(10),
        whole_run_timeout: Duration::from_mins(3),
        ..PuppetRunOptions::default()
    })
    .expect("external browser full-game puppet");

    assert_eq!(report.status, "complete");
    assert_eq!(report.final_room_phase, "post_game");
    assert_eq!(report.final_revision, 160);
    assert_eq!(report.public_history_events, 138);
    assert_eq!(report.lifecycle.len(), 4);
    assert_eq!(report.captures.len(), 6);
    for capture in &report.captures {
        assert_eq!(capture.status, "complete");
        assert!(capture.windowless);
        assert_eq!(capture.requested_revision, capture.captured_revision);
        assert_eq!(capture.provider_kind, "browser_harness_external_relay");
        assert_eq!(
            capture.representation,
            "png,semantic_html,accessibility_tree_json,layout_json"
        );
        assert!(capture.transferred_bytes > 0);
        assert!(capture.transfer_chunks > 0);
        assert!(std::path::Path::new(&capture.manifest_path).is_file());
    }
    assert_eq!(
        report
            .captures
            .iter()
            .find(|capture| capture.label == "terminal")
            .expect("terminal capture")
            .captured_revision,
        report.final_revision
    );
}
