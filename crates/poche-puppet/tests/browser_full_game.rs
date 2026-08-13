// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;

use poche_capture::CapturePipeline;
use poche_puppet::{
    PuppetRunOptions, PuppetSurface, PuppetTransport, browser::run_browser_game, run,
};

/// External browser qualification is explicit so ordinary offline tests do
/// not require Edge/Chrome. The harness itself remains headless and creates no
/// visible browser window.
#[test]
#[ignore = "requires an installed Edge or Chrome browser"]
fn isolated_real_browsers_complete_the_game_and_emit_structural_evidence() {
    let result = run_browser_game(41).expect("real browser game");
    assert_eq!(result.summary.browser_contexts, 3);
    assert_eq!(result.summary.terminal_revision, 159);
    assert_eq!(result.summary.public_history_events, 138);
    assert!(result.summary.chat_round_trip);
    assert!(result.summary.disconnect_resume_reconnect);
    assert_eq!(result.summary.console_errors, 0);
    assert_eq!(result.summary.network_failures, 0);
    assert_eq!(
        result
            .checkpoints
            .iter()
            .map(|bundle| bundle.caption.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "bidding browser player view",
            "card-selection browser player view",
            "score-sheet browser player view",
            "terminal browser player view",
            "trick-in-progress browser player view",
            "trick-resolved browser player view",
        ])
    );

    let temporary = tempfile::tempdir().expect("temporary browser artifacts");
    for bundle in result.checkpoints {
        assert_eq!(bundle.artifacts.len(), 4);
        let persisted = CapturePipeline::new(temporary.path())
            .persist(&bundle)
            .expect("shared browser capture pipeline");
        assert_eq!(persisted.manifest.entries.len(), 4);
    }
}

#[test]
#[ignore = "requires an installed Edge or Chrome browser"]
fn unified_web_surface_authorizes_transfers_and_persists_every_representation() {
    let temporary = tempfile::tempdir().expect("temporary signed browser artifacts");
    let report = run(&PuppetRunOptions {
        surface: PuppetSurface::Web,
        transport: PuppetTransport::LoopbackNdjson,
        seed: 41,
        artifact_root: temporary.path().to_path_buf(),
        ..PuppetRunOptions::default()
    })
    .expect("unified signed browser surface");
    assert_eq!(report.status, "complete");
    assert_eq!(report.final_revision, 159);
    assert_eq!(report.public_history_events, 138);
    assert_eq!(report.captures.len(), 6);
    assert!(
        report
            .evidence_boundary
            .contains("parallel real-browser UI")
    );
    for capture in report.captures {
        assert_eq!(capture.status, "complete");
        assert_eq!(capture.provider_kind, "browser_harness");
        assert_eq!(
            capture.representation,
            "png,semantic_html,accessibility_tree_json,layout_json"
        );
        assert_ne!(capture.requester_device_id, capture.provider_device_id);
        assert!(capture.transferred_bytes > 0);
        assert!(capture.transfer_chunks >= 4);
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&capture.manifest_path).expect("browser capture manifest"),
        )
        .expect("valid browser capture manifest");
        assert_eq!(manifest["entries"].as_array().map(Vec::len), Some(4));
    }
}
