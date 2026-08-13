// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;

use poche_capture::CapturePipeline;
use poche_puppet::browser::run_browser_game;

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
