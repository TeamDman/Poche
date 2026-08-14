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
    assert_eq!(report.final_revision, 158);
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
}
