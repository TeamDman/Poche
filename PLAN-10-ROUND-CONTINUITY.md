# Round continuity and table interaction

**Plan status:** Complete for this round-continuity and tabletop interaction slice
**Primary implementation root:** `spacetimedb`, following PLAN-9
**Last updated:** 2026-09-16
**Intent audit:** Passed against the latest resumed-match playtest report.

## How to update this plan

Use `[ ]` not started, `[~]` active, `[x]` complete and `[!]` blocked. Keep decisions and validation beside their task. Parallel owners: authority/round flow, camera, coins, and root world presentation/integration. Generated test identities and captures stay ignored. No OS clipboard or visible automation windows.

## Authoritative guidance and traceability

| ID | User direction | Coverage |
| --- | --- | --- |
| R1 | Remove glass jar/table Z-fighting, separating bottom surfaces. | A |
| R2 | Coins need the same full-volume hover outline as cards. | A |
| R3 | Let a player regrab a coin while it returns after a rejected table drop; do not let a stale return override renewed manipulation. | A |
| R4 | Dropping near the yellow play area's edge was unexpectedly only physical; make the boundary/intended next action understandable. | C |
| R5 | Spread won cards enough to read ranks, especially 10. Put suit below rank towards the card centre. Let the winning player reposition taken cards physically. | B, C |
| R6 | Scoresheet inspection should fill the limiting screen dimension with little margin. Pan and zoom must remain usable. Click sheet again to restore prior view. This supersedes PLAN-9 T11 movement-to-dismiss. | D |
| R7 | Separate projection from angle: I toggles orthographic/perspective; O toggles top-down. Avoid extra hidden camera states. Smooth camera movement remains required. | D |
| R8 | After last trick and required dime payments, rotate dealer, collect/shuffle/deal the next round. The next dealer is responsible for dealing. | B, C |
| R9 | Show an actual deck with public face-up trump on it; private undealt faces remain hidden. | C |
| R10 | Scoresheet totals must update live, including money. Preserve prior round results. | B, C |
| R11 | Explain who should do what next, including payments/dealing; avoid a bid menu that misleadingly suggests a turn exists during scoring. Add a diagnostic world notice. | B, C |
| R12 | Existing resumed matches and working real-money transfers must continue without destructive reset. | B, E |

## Intent audit evidence

1. Extraction: reread the full latest playtest report, retaining jar clearance, interruptible return, rank/suit geometry, won-card authority, payment-driven progress, deck/trump and live totals separately.
2. Traceability: R1–R12 each map to a task and evidence. Camera changes explicitly supersede earlier controls, not user intent. Progress uses existing pure game rules rather than a second rule engine.
3. Adversarial omission: retained the difference between physical movement and logical play, smooth controls, second click restoration, near-full-frame inspection, dealer agency, resumed-state compatibility and public diagnostics without hidden cards.

Source limitation: none for the latest user report. Earlier foundation remains in PLAN-7/8/9 and source.

## Decisions and boundaries

- Existing pinned Bevy and SpacetimeDB architecture stays. Changes are additive where schema is involved; no database deletion.
- Dealer explicitly clicks the deck after payments; no timed auto-deal. This is a reversible interaction choice supporting the user's dealer-responsibility expectation.
- Pure Rust rules decide round score, dealer rotation and deal schedule. Physical coins remain conserved; repeated requests cannot score or charge twice.
- A rejected tabletop coin drop does not invent a new logical money container. Its visual return must be interruptible.
- Deck and trump render only counts and already-public trump identity.
- Final game/payout behavior must be explicitly bounded; do not claim full formal conformance from this interaction test.

## [x] A — Repair coin feedback

**Completion notes:** Reproduced pending-return pickup denial and coplanar jar bottom with actual MoneyState and mesh geometry. Both failed before the fix, with nearby passing cases retained. Glass base/rim now has 1mm clearance. Expanded front-culled cylinder hulls highlight the full coin. Per-request bridge completion correlation lets a new drag start at the displayed position and ignore older return acknowledgements/rejections; accepted bowl payments still win. All 14 money tests and 3 bridge tests passed. Windowless Maincloud acceptance also interrupted a return through real pointer input and observed the fresh movement on the peer, preserving all 400 coins/$70.

**Work:** Separate glass/table surfaces; full silhouette hover; interrupt return with fresh drag sequence and displayed pose.
**Validation:** Focused prediction/geometry tests; real two-client pick/drop/regrab capture; unchanged money conservation.
**Completion criterion:** No coincident jar bottom, clear coin hover, no unpickable return interval or stale snapback after regrab.

## [x] B — Continue rounds through the rules engine

**Completion notes:** Added private round records/member-scoped history view, idempotent scoring and cumulative payment obligations, explicit dealer action, replayable settle/deal log entries and round-specific card IDs. Won-card movement follows the winner rather than original card owner; new tricks no longer reassign old piles. Native authority-helper tests passed all 13 scheduled rounds through Finished and exact log reconstruction; 7 client tests passed. Strict module/client Clippy and additive Maincloud publish passed without deleting data. The renderer-neutral two-client example passed two full rounds, denied unpaid/nondealer/duplicate deals, preserved scores on scoring rejoin, accepted winner-only captured-card movement, and conserved $70 while dealing two-/three-card hands. Final payout is explicitly not automated. The test room was disbanded.

**Work:** Record scored rounds once, track cumulative payment obligations, advance after required dimes, dealer-authorized next deal, shuffle/recollect, permit winner-only physical won-card moves. Update client/bridge and bindings.
**Validation:** Pure multi-round tests, additive schema build, live wrong-dealer/repeated-payment rejection, resumed scoring state and round-two play.
**Completion criterion:** Last trick → explained outstanding payments → next dealer → explicit deal → next bidding phase, with conserved money and cumulative scores.

## [x] C — Make the table explain itself

**Completion notes:** Added a physical deck with public trump, an actionable next-step world notice, phase-specific speech, multi-round scoresheet with live bowl/payment totals, separate vertical rank/suit labels, and wider won-card spacing. Kept the full-card spatial acceptance rule, with explicit held-card feedback explaining edge drops and physical-only movement. Windowless acceptance completed 2 rounds, clicked the deck to deal round 2, paid both rounds' missed-bid dimes, and observed the next dealer ready for round 3. Inspected `round-continuity.png` and `scoresheet-inspection.png` using the image viewer. The deck label, payment/dealer notices and live ruled scoresheet are visible. No solver runtime is claimed for these rendering checks.

**Work:** Deck/trump click affordance; next-step world board; phase-appropriate speech; multi-round ruled scoresheet/live pot; stacked rank/suit labels and readable won-card spacing. Explain full-card play-zone acceptance with visible feedback.
**Validation:** Presentation tests and real pointer captures through first and second rounds; no private information in public panels.
**Completion criterion:** Players can see what to do next without opening diagnostics or guessing which bid menu is active.

## [x] D — Revise scoresheet and camera controls

**Completion notes:** Reproduced old second-click failure and 74% frame usage; six revised controller tests passed. Inspection fits 94% of the limiting dimension, retains pan/zoom and its bookmark, and restores on second click. I changes projection without hidden saved poses; O changes angle independently. Removed the unconditional pitch clamp that forced vertical inspection back to 78 degrees. Windowless acceptance clicked the actual paper, panned, toggled I/O and restored the saved view with a second click; the before/inspection/restored capture was visually checked.

**Work:** Independent I projection/O angle; persistent inspection pan/zoom; second click restore; tight aspect-aware framing. Remove old MMB consume behavior.
**Validation:** Failing-old/passing-new controller tests, real sheet click, wheel/pan, second click, I/O combinations via windowless input.
**Completion criterion:** Inspection fills the viewport appropriately and never unexpectedly dismisses on movement.

## [x] E — Validate and release

**Completion notes:** Desktop 93 tests and 3 puppet comparison tests passed, including a reproduced between-round resume failure: the loading gate incorrectly expected the next hand's cards before dealing. It now accepts cleared cards in AwaitingDeal/Finished, retaining the active-deal readiness check. Four native authority-helper tests, 7 client tests and 3 bridge tests passed. The final windowless Maincloud run passed all 18 checks in report schema v12, including two rounds, coin regrab, camera input, winner-only captured-opponent-card dragging through real pointer input, same-identity resume, sibling disconnect and explicit leave. The pointer test compares canonical card fields while excluding only the viewer-relative `is_own` flag, with failing-neighbour regressions for pose/sequence/authority differences. Visually inspected the won-card, coin, round-continuity and scoresheet contact sheets. Strict changed-crate Clippy passes with `--no-deps`; an unrestricted invocation encounters a pre-existing missing-panic-doc lint in `poche-spatial/src/physical_identity.rs`. No unrelated dependency code was changed. Additive module publication succeeded without resetting persisted rooms. Implementation commit `42de30f` was pushed successfully to `origin/spacetimedb`; this follow-up records the completed release evidence.

Validation commands run from the repository root:

```powershell
cargo test --locked -p poche-spacetimedb-desktop --lib --bins
cargo test --locked -p poche-spacetimedb-module --test round_flow
cargo test --locked -p poche-spacetimedb-client -p poche-bevy-spacetimedb --lib
cargo clippy --locked -p poche-spacetimedb-desktop -p poche-bevy-spacetimedb -p poche-spacetimedb-client -p poche-spacetimedb-module --all-targets --no-deps -- -D warnings
cargo build --locked -p poche-spacetimedb-desktop --bins
target\debug\poche-puppet.exe acceptance --server maincloud --output target\poche-puppet\round-continuity-final.png
```

The final receipt is `target/poche-puppet/round-continuity-final.json`; captures and generated credentials remain ignored. Pure helper tests cover all 13 scheduled rounds, while the real network/rendered-input run covers 2 complete rounds and the third-round dealer boundary. These are different evidence scopes, not a full live-game or adversarial-security proof. Final payout remains outside this slice.

**Work:** Targeted suites/lint, module build and additive publish, two-client windowless round-two acceptance and visual review. Update player guide, commit and push verified changes.
**Validation:** `cargo test --locked -p poche-spacetimedb-desktop --lib --bins`; focused pure/client tests; strict changed-crate Clippy; module build; `poche-puppet acceptance --server maincloud` after compatible publish.
**Completion criterion:** Evidence for all R1–R12 or an explicit remaining limitation; no claim of whole-game/formal completion without that evidence.

## Risks

- Cumulative pot is not per-round paid amount: explicit obligations prevent charging old misses again.
- Old drag updates must not target new-deal cards: round-specific identifiers and authority checks.
- Camera projection and angle are independent: test their combinations and bookmark restoration.
- Real pointer tests must use current displayed objects, not only direct reducer commands.
