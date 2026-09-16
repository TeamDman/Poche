# Table tactility, inspection and money

**Plan status:** Complete for this first-round interaction and money slice; broader game work remains in PLAN-7
**Primary implementation root:** `spacetimedb` worktree, following completed PLAN-8
**Last updated:** 2026-09-16
**Intent audit:** Passed against the latest pickup, scoresheet, coin and speech request and all three subsequent money replies.

## How to update this plan

Use `[ ]` not started, `[~]` active, `[x]` complete and `[!]` blocked. Keep evidence below the affected task. Parallel work is intentional: pickup prediction, inset geometry/outline, world text, and camera inspection. Money follows its decision gate. Do not silently replace an unfinished requirement with a smaller feature.

## Guidance ledger and traceability

| ID | User direction, including qualifications | Task and proof |
| --- | --- | --- |
| T1 | Commit the work already present. | 0: existing commit and remote status |
| T2 | Grabbing a card must not shift it under the pointer as it rises. Ignore the held card as a drag surface; prevent feedback from its changing height. | 1: perspective/top-down grab-plane regressions |
| T3 | Ease local pickup/drop height as peers already do, while preserving direct planar manipulation. | 1: timed local/peer pose tests and captures |
| T4 | The bottom hand bar must represent the actual hand region: a card should not leave the inset while still inside the indicated drop region. Preserve bidirectional inset/world dragging. | 2: one geometry source, edge/resize tests and input captures |
| T5 | Highlight the full card silhouette, including its thickness when viewed from the side, not a box drawn on its top face. | 2: solid outline geometry and low-angle capture |
| T6 | Draw a legible, aligned scoresheet with genuine table structure rather than independently laid-out pipe characters. | 3: fixed cell geometry, grid and image review |
| T7 | Put money in a shared bowl and each player's glass jar and dark-green detached lid. Movable quarters can go lid→bowl and jar↔lid, with gentle pickup lift. Label totals for every container. | 5 after G1/G2: authoritative conservation/permission tests and two-client movement |
| T8 | Use natural self-directed speech when the dealer is also bidding; use first-person thinking while choosing a card. Accepted bid speech must not persist into card play; the durable bid is on the scoresheet. | 3: phase/actor tests |
| T9 | Name tags, including '(you)', need a contrasting backing or outline. | 3: world label contrast capture |
| T10 | Allow closer camera zoom. Clicking the actual scoresheet tweens to orthographic top-down inspection, not a GUI reader. Remember and smoothly restore the preceding view. | 4: camera state tests and before/inspection/restore captures |
| T11 | Moving with middle mouse dismisses inspection. Consume that entire gesture until release so restoration does not also pan. | 4: dismissal/release regression through file control |

## Intent audit evidence

1. Extraction: reread the latest original user message. Kept pickup hit testing, local/peer timing, bar width, silhouette thickness, money containers and accounting, natural speech, label contrast, close zoom, orthographic inspection and gesture consumption as separate requirements.
2. Traceability: every T1–T11 entry has a bounded task and proof. Money accounting is not silently treated as decorative props. PLAN-8 remains completed history, not an excuse to omit the new behavior.
3. Adversarial review: retained dark-green lids, glass jars, movable quarters, totals for all containers, same lift affordance, bid text's limited lifetime and restoration of the previous camera rather than a generic home reset. MMB dismissal consumes the gesture without changing the OS cursor or buttons. Repeated all three passes after the money replies: real quarters/dimes, a full tall jar rather than the proposed $5 cap, and manual ante before dealing are now explicit gates/implementation requirements.

Source limitation: earlier implementation details are carried by PLAN-7/8 and inspected code. The latest request itself was available in full.

## Foundation and boundaries

Commit `87fe165` already implements full-window world rendering, bottom-edge private hand copies, typed pose prediction, world panels, seat clicking and safe bid speech. Existing render readiness, minimize safety, account/rejoin, privacy and authority rules remain required. Card movement changes physical pose, not logical ownership. No solver is implied to have verified renderer behavior.

Use existing windowless file-control puppets. Do not touch the user's clipboard or open visible automation windows. Build in ordinary `target`. Generated screenshots remain ignored; published notes contain no live room codes or credentials.

## Money decision gates

| Gate | Evidence and required choice | Consequence |
| --- | --- | --- |
| G1 — closed | User confirmed quarters and dimes, with real game-money transfers. | Shared coin rows are the balances, not decoration. No cash redemption or external payments. |
| G2 — closed for this slice | User specified a tall, narrow banana-pepper jar full of quarters/dimes, with a smaller working portion in the lid, rather than the proposed $5 limit. | Reversible implementation allocation: 100 quarters and 100 dimes ($35), initially $33 in the jar and $2 in the lid. This is a play-money supply parameter, not a Poche rule or a user-specified exact count. |
| G3 — closed | User explicitly chose physical 25¢ ante payments before the first deal, replacing the automatic ante. | Both seats can be occupied without a game/hand yet. The second accepted quarter starts dealing. Tests must pay coins through the real pointer path. |

## [x] 0 — Preserve the completed slice

**Completion notes:** `87fe165` is committed and pushed to `origin/spacetimedb`. Working tree was clean at this request's start.

## [x] 1 — Stabilize and ease pickup

**Completion notes:** Before-fix real ray/pose tests reproduced a stationary-cursor perspective pickup changing X 20→40.28mm and Z 300→314.48mm; the top-down nearest case passed. Dragging now retains the resting plane. Rendered Y eases toward immediate submitted Y, for local and peer poses; X/Z remains responsive. Fifteen prediction/pickup tests passed, including stale-echo, denial, smooth lift/drop and unchanged-write suppression. Windowless real input verified both viewports and exact peer rotation/height.

**Work:** Reproduce resting-plane versus lifted-plane displacement using real camera/ray types. Keep drag mapping independent of held mesh height. Separate desired network height from displayed interpolation, preserving authority rejection and stale-echo rules.

**Validation:** Desktop regression suite, single pickup/release and exact peer height observation in file-control acceptance.

**Completion criterion:** Stationary cursor does not change X/Z on pickup; local height advances smoothly; final acknowledged pose is correct.

## [x] 2 — Align the hand indicator and full-volume highlight

**Completion notes:** `hand_view.rs` derives the bar, cursor routing, inverse projection and clamping from canonical physical hand bounds. Four tests cover both seats, narrow/wide windows, DPI, count changes and thin card sides. Front-culled expanded hulls replace the face strips, with no shadow casting/receiving. GPU hand→world→hand and hover captures were inspected in `target/poche-puppet/hand-interaction.png`.

**Work:** Derive indicator and hit area from the same projection/physical bounds. Keep compressed spacing reversible. Replace top-face strips with silhouette rendering for both world and private inset.

**Validation:** Hand edge and resize unit tests, both viewport drag directions, near-horizontal GPU capture.

**Completion criterion:** Indicator promises exactly the supported hand drop region; outline follows the card volume without covering its face or leaking private data.

## [x] 3 — Correct world information

**Completion notes:** Scoresheet has fixed column/cell bounds, consistent ink size and actual ruled geometry. Name tags have opaque unlit contrast backdrops. Bidding thoughts use first person when appropriate; accepted bid text disappears in playing. Nine world-presentation tests passed, including actual spawned grid/backdrop entities and public/private text boundaries. Orthographic GPU inspection shows aligned rows and columns.

**Work:** Structured scoresheet cells and real grid lines; phase-aware first-person speech; contrast backing on name labels.

**Validation:** World presentation tests for actor/phase, fixed cell boundaries and public projection privacy; inspected close-up capture.

**Completion criterion:** Bids remain on the aligned sheet, not stale speech during play; names are legible against the room.

## [x] 4 — Inspect the real scoresheet with the camera

**Completion notes:** A sheet click requests a camera bookmark, not a reader panel. Exact top-down transforms use a stable yaw-derived up axis. Framing and camera pose ease; switching projection preserves focal-plane scale. Movement restores the previous perspective/tactical view and consumes held input until release. Three unit tests cover both modes, zoom, narrow framing and full MMB consumption; the real pointer/gesture acceptance restores the observed preceding pose. `scoresheet-inspection.png` contains before, paper close-up and restored views. Minimum perspective distance is 0.16m; orthographic scale can reach 0.06.

**Work:** Save previous camera view, tween to close orthographic top-down sheet framing, restore on a consumed camera-movement gesture. Extend zoom range. Scoresheet click no longer creates a reader GUI.

**Validation:** Saved perspective/tactical mode and zoom, dismissal, held-MMB suppression and release tests; actual click/drag screenshots.

**Completion criterion:** Sheet remains the real world object. Restore returns to the preceding view without unintended panning or OS input manipulation.

## [x] 5 — Represent shared money after the accounting gate

**Completion notes:** Added `poche-money`, private coin storage/member-scoped view, typed client/Bevy movement, and rendered glass jars/green lids/bowl with derived totals. Initial inventory is 200 individually conserved coins per seated identity. Both 25¢ payments gate the first deal; a missed bid owes a 10¢ dime. Owned jar↔lid transfers, rejection, first-free pile placement, rejoin inventory and abandonment refunds are implemented. Legacy already-dealt rooms materialize their prior automatic antes without resetting cards. Additive Maincloud publish succeeded (one private table and member view; no data deletion). Six pure money tests and eight focused client/bridge tests passed; module test source passes strict Clippy/check, but native execution cannot link SpacetimeDB WASM host imports. The real Maincloud reducer tests replace that unavailable native execution: both ante pointer paths, 25¢/no-deal intermediate state, 50¢/dealt state, rejected wrong denomination/overpayment, jar↔lid transfers, 60¢ missed-bid payment and unchanged $70 combined inventory all passed. Renderer-neutral two-client test also passed and disbanded its own room.

**Decisions:** Pure `poche-money` owns denominations, inventory, container geometry and payment guards. The private DB coin table is exposed only through an active-room member view. Existing coins move; transfers cannot mint or duplicate money. Jar/lid ownership follows identity across rejoining. The first-deal slice accepts an exact quarter ante and an exact dime owed after a miss; bowl withdrawals await settlement and are not a general-purpose withdrawal affordance. Abandoning the deal refunds the bowl, consistent with the existing whole-deal abandonment behavior. Additive DB schema only; no database reset.

**Work:** Close G1/G2 first. Define authoritative coin identity, denomination, owner, physical pose and logical container with conservation and permissions. Add glass jars, detached green lids, bowl and amount labels. Reuse stable pickup/drag behavior. Preserve existing databases without destructive migration.

**Validation:** Reducer conservation/rejected transfer tests, module/client schema parity, real two-client coin movement and totals, deployment compatibility review.

**Completion criterion:** All participants observe the same money and movements; displayed totals have an explicit accounting meaning consistent with the selected policy.

## [x] 6 — Verify and hand off

**Completion notes:** Desktop suite: 71 tests passed. Desktop, client/bridge, and pure-money/module strict all-target Clippy passed. Both desktop binaries built. Maincloud windowless receipt `target/poche-puppet/tactility-final.json` (v10) passed all 12 independent checks, including account resume, sibling presence and leave after the new money/card/camera paths. Captured `manual-antes.png`, `missed-bid-payment.png`, `hand-interaction.png` and `scoresheet-inspection.png`; inspected images with view_image. A sampled card update took 63.13ms authority acknowledgement and 147.02ms exact peer observation, not a latency guarantee. Review also fixed failed-send pickup lockout, cross-room pending state, coin picking through the hand overlay and bowl-floor intersection; diagnostic coin targets are opt-in and throttled rather than quadratic work in ordinary player frames. Player guide and file-control schema 8 document the change. No solver or comprehensive multi-round claim is made. Source is committed in the release commit containing this plan; built binaries, private test receipts and captures remain ignored.

**Work:** Build game and puppet, run targeted tests and strict lint, inspect composite captures, document controls and any still-open money gate, commit verified changes. Do not claim the whole plan complete while task 5 is unresolved.

**Validation:** From the worktree: `cargo test --locked -p poche-spacetimedb-desktop --lib --bins`; `cargo clippy --locked -p poche-spacetimedb-desktop --all-targets --no-deps -- -D warnings`; `cargo build --locked -p poche-spacetimedb-desktop --bins`; `target/debug/poche-puppet acceptance --server maincloud --output target/poche-puppet/tactility.png`.

**Risks:** Pure tests do not prove picking/render integration. Interpolation must not suppress authoritative denial. Container props must not imply a different pot from the rule engine. Camera inspection must not create degenerate top-down transforms or regress loading/minimize safety.
