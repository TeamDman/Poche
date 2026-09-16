# Diegetic table and hand interaction

**Plan status:** Complete for this interaction slice; broader gameplay remains in PLAN-7
**Primary implementation root:** `spacetimedb` worktree; extends PLAN-7
**Last updated:** 2026-09-16
**Intent audit:** Passed against the latest Tabletop Simulator comparison and five annotated screenshots. Earlier project constraints come from PLAN-7, not reconstructed tool history.

## Update protocol

Use `[ ]` not started, `[~]` active, `[x]` complete, `[!]` blocked. Update headings and completion notes together. This plan is only ready after literally triple checking that no user intent was omitted without explicit direction. Parallel tracks are intentional: hand/input integration, pose reconciliation, canonical seat clearance, and world presentation.

## Guidance ledger and traceability

| ID | Requirement and nuance | Work / proof |
| --- | --- | --- |
| D1 | World fills the window; private hand peeks from the actual bottom edge and stays visible during camera movement. Show full card widths until a large hand requires overlap; rank and suit remain legible. | 1, GPU captures |
| D2 | Hand inset and world are two views of the same cards, anchored to a fixed physical hand zone. Other viewers see backs. Small stack-height offsets avoid fighting; grabbing lifts the card. | 1, two-viewer capture and geometry tests |
| D3 | Movement maps between hand and world projections, in both directions; leaving the hand region hides the inset representation and returning shows it. A thin centered drop indicator appears during dragging. Physical relocation does not itself grant ownership, reveal a face, or bypass logical play rules. | 1, actual pointer-path tests |
| D4 | Prefer contextual nonmodal inspection, not copying TTS's modal tool suite. Hover cards in either view; inspect zones without solid shadow-casting diagnostic volumes. | 1/2, hover and zone captures |
| D5 | Move seats outward so cylinders clear table and zones; do not conceal intersections by lifting seats. Allow near-horizontal camera inspection; retain O tactical view. | 2, versioned layout tests and camera capture |
| D6 | Diagnose Q/E rotation twiddling/snapback, including a single press and repeat, with local prediction and authority echoes. Smooth visuals must still commit the chosen angle. | 3, failing/passing regression and two-device observation |
| D7 | Replace oversized HUD with world surfaces: rulebook-style scorekeeper notepad on table, player and activity posters, click-to-copy room-code billboard. Names above capsules include '(you)'; empty seats clickable. | 4, real action wiring and captures |
| D8 | Bidding is constrained speech: scorekeeper asks the named player, compact safe-chat picker offers legal bid sentences, accepted bid appears above bidder. No large bottom bid strip. Chat/voice/hand-sign examples express future communication direction, not a request for voice infrastructure now. | 4, accepted bid and privacy checks |
| D9 | Use existing ad-hoc file control/windowless puppets, not clipboard or computer-use automation. Review composite captures. Preserve rendering/loading/minimize fixes and account/rejoin flows. | 5, current binaries and acceptance |

## Audit evidence

1. Extraction: separated all hand geometry, bid speech, spatial information, hover, camera, and rotation complaints rather than reducing the request to styling.
2. Traceability: D1–D9 each have a task and observable proof. Server rules and privacy remain established foundation, not redesigned with the UI.
3. Adversarial review: retained bidirectional dragging, edge clipping, many-card overlap, same-world movement, no zone shadows, outward seat motion (not Y workaround), single-tap rotation, and the nonmodal preference. TTS illustrations are references, not instructions to copy a toolbar or use their assets.

## Design boundaries

Bevy remains a projection of typed authority. No DB schema migration, spontaneous card spawning, concealed-face leakage, or logical hand reassignment by visual proximity. Playing into PLAY uses the existing reducer. Existing logical cards can physically leave and return to their owner's hand zone without changing ownership. The first-trick/multi-round limits of PLAN-7 remain explicit.

Keep registered layout revision 1 for old replay hashes; add revision 2 with seat clearance while preserving rule-zone geometry. Sparse screen-space controls remain for camera/menu and accessible close-up reading; diegetic does not mean unreadable text.

## [x] 1 — Make the hand a bottom-edge view of the world

**Completion notes:** `hand_view.rs` uses a full-window world camera and a fixed-zone transparent orthographic inset. Same-pose presentation copies carry private faces; logical ownership is untouched. Added reversible compressed center spacing, opposite corner labels, oriented-card hover hit tests, shared outlines, lift, stack separation and a thin drop band. The schema-6 pointer test physically grabbed, rotated, moved out/back and released a real owned card; its peer observed the exact angle and heights. The inset disappeared outside the physical hand region and returned after re-entry. Reviewed `target/poche-puppet/hand-interaction.png`. The 25-card spacing case is a geometry regression, not a claim of multi-round gameplay.

**Work:** Full-surface table camera, fixed-zone transparent hand projection, crop/spacing, shared hover/lift feedback, reversible coordinate handoff and drop indicator. Avoid camera chasing a dragged card. Exercise real pointer paths through opt-in file control.

**Validation:** Geometry/input boundary tests and ordinary windowless GPU captures showing hand/world correspondence and peer backs. Current hand is often one card; large-hand layout requires an explicit synthetic geometry test rather than claiming a multi-round run.

**Completion criterion:** Drag out/back without cursor discontinuity within a viewport, private face leakage, or a hidden portion of the world; peers observe the same physical pose.

## [x] 2 — Remove misleading spatial intersections

**Completion notes:** Canonical layout revision 2 moves two-player seats to ±900mm (220mm radius, 30mm table clearance); all 2–8-player layouts reject seat/table/zone intersections. Revision 1 remains unchanged, including old play endpoints. All 36 spatial tests passed. Zones have `NotShadowCaster` and `NotShadowReceiver`, are normally hidden, and appear during drag or held Z. Z-hover explains the named volume without capturing input. Perspective reaches 3°; arrow-key orbit and a near-horizontal GPU capture verify inspection, while Space resets smoothly. Ordinary paper/cards retain shadows.

**Work:** Versioned canonical seat clearance, non-shadow-casting contextual zone outlines, lower diagnostic camera pitch. Preserve old layouts and renderer lifecycle.

**Validation:** `cargo test --locked -p poche-spatial`; near-horizontal capture. Verify rendered dimensions match canonical bounds.

**Completion criterion:** Seats clear table and hand zones, zones cast no shadows, cards still cast shadows.

## [x] 3 — Preserve local pose intent through authority echoes

**Completion notes:** Reproduced two failures in real DisplayPose/CardPoseView tests before correction: an unsent 45° tap reset to 0°, and a newer 90° prediction reset to an older 45° echo. Reconciliation now respects active input and unacknowledged sequences, but authoritative ownership/location changes and rejected submissions still win. Release freezes its submitted target. Ten regressions cover those boundaries and unchanged-write suppression; a real Q tap survived late echoes and was observed by the second device. A stationary hold no longer emits repeated equal writes.

**Work:** Reproduce stale snapshot overwrite using real DisplayPose/CardPoseView; reconcile active/unsent and unacknowledged intent; recover on denial or authoritative logical transition.

**Validation:** Desktop regression suite with single step, repeat, release, acknowledgment, stale update and rejection variants; two-client exact angle observation.

**Completion criterion:** One Q/E press commits once and remains stable after acknowledgement; no endless optimistic masking of errors.

## [x] 4 — Put information and actions in the scene

**Completion notes:** `world_ui.rs` supplies filled Slug text on public room/player/activity signs and the table notepad, name labels, accepted bid bubbles and a dealer prompt. Close-up readers support activity scrolling. The sparse Speech → Bid picker submits ordinary typed actions. Actual puppet pointer clicks now take the first stool and submit both bids; a separate disposable stool-click run also verified capsule creation and the public seat event. Five world tests cover privacy, ray intersections, score notation, zone inspection and stable surface/button identities during hover. Wider camera framing and offset speech bubbles keep cards and signs visible. Current-round outcome notation is explicitly separate from authority totals: the server still stops at scoring, per PLAN-7.

**Work:** Separate world presentation module, filled text, boards/paper, capsule labels, click-seat, constrained bid speech and close-up reading. Keep public activity free of hidden hands.

**Validation:** Source-backed scoresheet layout from `docs/main.typ`, rendered text review, seat and bid action dispatch checks.

**Completion criterion:** A player can seat and bid through world/contextual affordances; public information remains readable without the previous full-screen text strips.

## [x] 5 — Verify, document and deliver

**Completion notes:** Final Maincloud acceptance receipt `target/poche-puppet/diegetic-final.json` (v9) passed real stool/bid clicks, hand→world→hand dragging, one Q tap, hidden-face checks, two legal plays, same-identity resume, sibling presence, options and explicit leave. Captures were inspected with view_image. One sample measured 91.98ms authority acknowledgement and 182.53ms exact peer observation; these are test-run observations, not guarantees. Spatial tests: 36 passed; desktop/input/world/prediction tests: 54 passed. Strict desktop all-target Clippy passed with warnings denied. No solver or DB schema/module change was needed. No clipboard, OS pointer, visible automation windows or user processes were used. Player guide now documents the new affordances. Release is the `spacetimedb` commit containing this plan; ignored test captures/receipts stay local rather than committing room codes or account identifiers.

**Work:** Build both game and puppet; focused tests, strict Clippy, windowless two-device acceptance and interaction captures; update player guide and these task notes with actual evidence. Commit/push the authorized branch after review.

**Validation:** `cargo test --locked -p poche-spacetimedb-desktop --lib --bins`; `cargo clippy --locked -p poche-spacetimedb-desktop --all-targets --no-deps -- -D warnings`; `cargo build --locked -p poche-spacetimedb-desktop --bins`; `target/debug/poche-puppet acceptance --server maincloud --output target/poche-puppet/diegetic.png`.

**Risks:** Direct reducer tests do not prove pointer hit testing; add input-path evidence. GPU final screenshots do not prove absence of startup gaps; preserve existing render readiness contract. Test rooms only, no user identities or clipboard. Do not claim comprehensive formal verification from spatial and Rust regressions alone.
