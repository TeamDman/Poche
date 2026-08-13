// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{collections::BTreeSet, path::Path, time::Duration};

use poche_puppet::{PuppetRunOptions, PuppetSurface, TWO_PLAYER_FULL_ROUND, run};

/// GPU qualification is explicit so ordinary/offline unit tests remain
/// portable. Run with a selected backend, for example:
///
/// `$env:WGPU_BACKEND='dx12'; cargo test --locked -p poche-puppet --offline
/// --test native_full_game -- --ignored --nocapture`
#[test]
#[ignore = "requires a working GPU backend"]
fn authorized_windowless_native_capture_survives_the_full_game() {
    let temporary = tempfile::tempdir().expect("temporary native artifacts");
    let report = run(&PuppetRunOptions {
        scenario: TWO_PLAYER_FULL_ROUND.to_owned(),
        surface: PuppetSurface::Native,
        seed: 29,
        artifact_root: temporary.path().to_path_buf(),
        per_action_timeout: Duration::from_secs(3),
        whole_run_timeout: Duration::from_mins(2),
        ..PuppetRunOptions::default()
    })
    .expect("native full-game puppet");

    assert_eq!(report.status, "complete");
    assert_eq!(report.final_room_phase, "post_game");
    assert_eq!(report.devices.len(), 8);
    let expected_labels = BTreeSet::from([
        "bidding",
        "card-selection",
        "scoring",
        "terminal",
        "trick-in-progress",
        "trick-resolved",
    ]);
    assert_eq!(
        report
            .captures
            .iter()
            .map(|capture| capture.label.as_str())
            .collect::<BTreeSet<_>>(),
        expected_labels
    );
    let mut previous_revision = 0;
    for capture in &report.captures {
        assert!(capture.windowless);
        assert_eq!(capture.requested_revision, capture.captured_revision);
        assert!(capture.captured_revision > previous_revision);
        previous_revision = capture.captured_revision;
        assert!(capture.transferred_bytes > 0);
        assert!(capture.transfer_chunks > 0);
        assert!(Path::new(&capture.manifest_path).is_file());
    }
    let terminal = report
        .captures
        .iter()
        .find(|capture| capture.label == "terminal")
        .expect("terminal native capture");
    assert_eq!(terminal.captured_revision, report.final_revision);

    let run_directory = Path::new(&report.artifact_directory);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(run_directory.join("manifest.json")).expect("run manifest bytes"),
    )
    .expect("run manifest JSON");
    let files = manifest["files"].as_array().expect("manifest files");
    assert!(files.len() >= 2 + report.captures.len() * 2);
    for file in files {
        let relative = file["path"].as_str().expect("portable relative path");
        assert!(!relative.contains('\\'));
        let bytes = std::fs::read(run_directory.join(relative)).expect("manifest artifact");
        assert_eq!(file["bytes"].as_u64(), Some(bytes.len() as u64));
        assert_eq!(
            file["blake3"].as_str(),
            Some(blake3::hash(&bytes).to_hex().as_str())
        );
        if Path::new(relative)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        }
    }
}
