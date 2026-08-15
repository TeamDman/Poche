// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::Duration;

use poche_puppet::{
    PuppetErrorCode, PuppetRunOptions, PuppetTransport, TWO_PLAYER_FULL_ROUND, run, run_with_cancel,
};

fn options(root: &std::path::Path, seed: u64) -> PuppetRunOptions {
    PuppetRunOptions {
        scenario: TWO_PLAYER_FULL_ROUND.to_owned(),
        seed,
        artifact_root: root.to_path_buf(),
        per_action_timeout: Duration::from_secs(2),
        whole_run_timeout: Duration::from_secs(10),
        ..PuppetRunOptions::default()
    }
}

#[test]
fn certified_devices_complete_and_witness_the_full_game() {
    let temporary = tempfile::tempdir().expect("temporary artifacts");
    let report = run(&options(temporary.path(), 1)).expect("full puppet game");

    assert_eq!(report.status, "complete");
    assert_eq!(report.final_room_phase, "post_game");
    assert!(report.step_count > 10);
    assert_eq!(report.devices.len(), 8);
    assert!(report.captures.is_empty());
    assert_eq!(report.steps.len(), report.step_count as usize);
    assert!(report.steps.iter().all(|step| {
        step.observed_by.len() == 8
            && step
                .observed_by
                .iter()
                .all(|device| device.revision == step.committed_revision)
    }));
    let artifact_directory = std::path::Path::new(&report.artifact_directory);
    assert!(artifact_directory.join("run.json").is_file());
    assert!(artifact_directory.join("steps.ndjson").is_file());
    assert!(artifact_directory.join("lifecycle.ndjson").is_file());
    assert!(artifact_directory.join("manifest.json").is_file());
    assert!(artifact_directory.join("index.html").is_file());
    assert!(temporary.path().join("catalog.json").is_file());
    assert!(temporary.path().join("index.html").is_file());
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(artifact_directory.join("manifest.json")).expect("manifest bytes"),
    )
    .expect("manifest JSON");
    for file in manifest["files"].as_array().expect("manifest files") {
        let relative = file["path"].as_str().expect("relative path");
        let bytes = std::fs::read(artifact_directory.join(relative)).expect("artifact bytes");
        assert_eq!(file["bytes"].as_u64(), Some(bytes.len() as u64));
        assert_eq!(
            file["blake3"].as_str(),
            Some(blake3::hash(&bytes).to_hex().as_str())
        );
    }
    assert_eq!(
        manifest["executable_revision"].as_str().map(str::is_empty),
        Some(false)
    );
    assert_eq!(manifest["contact_sheet"], "index.html");
    let catalog: serde_json::Value = serde_json::from_slice(
        &std::fs::read(temporary.path().join("catalog.json")).expect("catalog bytes"),
    )
    .expect("catalog JSON");
    assert_eq!(catalog["generated_from_verified_manifests"], true);
    assert_eq!(catalog["runs"].as_array().map(Vec::len), Some(1));
}

#[test]
fn typed_and_ndjson_transports_select_the_same_deterministic_actions() {
    let first = tempfile::tempdir().expect("first artifacts");
    let second = tempfile::tempdir().expect("second artifacts");
    let first = run(&options(first.path(), 17)).expect("first run");
    let mut second_options = options(second.path(), 17);
    second_options.transport = PuppetTransport::LoopbackNdjson;
    let second = run(&second_options).expect("second run");

    assert_eq!(first.final_scores, second.final_scores);
    assert_eq!(first.public_history_hash, second.public_history_hash);
    assert_eq!(first.step_count, second.step_count);
    assert_eq!(
        first
            .steps
            .iter()
            .map(|step| (&step.action_id, step.committed_revision))
            .collect::<Vec<_>>(),
        second
            .steps
            .iter()
            .map(|step| (&step.action_id, step.committed_revision))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cancellation_prevents_partial_publication() {
    let temporary = tempfile::tempdir().expect("temporary artifacts");
    let error = run_with_cancel(&options(temporary.path(), 1), || true)
        .expect_err("cancelled run must fail");
    assert_eq!(error.code(), PuppetErrorCode::Cancelled);
    assert_eq!(
        std::fs::read_dir(temporary.path())
            .expect("artifact root")
            .count(),
        0
    );
}

#[test]
fn zero_run_deadline_is_attributable_and_prevents_partial_publication() {
    let temporary = tempfile::tempdir().expect("temporary artifacts");
    let mut expired = options(temporary.path(), 1);
    expired.whole_run_timeout = Duration::ZERO;
    let error = run(&expired).expect_err("expired run must fail");
    assert_eq!(error.code(), PuppetErrorCode::RunTimeout);
    assert_eq!(
        std::fs::read_dir(temporary.path())
            .expect("artifact root")
            .count(),
        0
    );
}
