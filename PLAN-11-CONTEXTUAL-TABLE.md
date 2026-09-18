# Contextual table information and direct interaction

**Plan status:** Complete
**Primary implementation root:** `spacetimedb`, following completed PLAN-10
**Last updated:** 2026-09-17
**Intent audit:** Passed against the latest quiet-table, selection, sound and resizable-hand request.

## How to update this plan

Use `[ ]` not started, `[~]` active, `[x]` complete and `[!]` blocked. Keep evidence beside each task. Parallel owners: world affordances/speech, money inspection/stacking, hand settings/menus, and root selection/audio/integration. Preserve user intent and existing data. Windowless file-control validation must not use the OS clipboard or visible automation windows.

## Authoritative guidance and traceability

| ID | User direction | Coverage |
| --- | --- | --- |
| C1 | Reduce always-present visual information. Jar and lid totals appear on hovering their cylinders, not when the pointer is on a coin. Remove persistent money ownership/amount labels, including the bowl. Spatial placement identifies the owner. | A |
| C2 | Primary-button drag on empty screen creates a 2D selection rectangle. Show live coin count/value before release; release selects encompassed pieces. User considers centroid or full enclosure preferable to mere intersection, without insisting on one. | D |
| C3 | Put larger quarters below dimes in jars. | A |
| C4 | Unpaid 25¢ ante should prompt first-person speech above each owing player instead of a bowl label. Preserve clear actionable progression. | B |
| C5 | Deck card quantity should be contextual on hover, not permanent. | B |
| C6 | Add pickup and put-down sound effects. | D |
| C7 | Bottom-screen cards are too small on this display. Right-clicking an inset card should open a slider popup to change its display size. Preserve the physical/shared card dimensions and the hand/world mapping. | C |
| C8 | Move instructions out of Options into a separate Help entry. | C |
| C9 | Replace Esc-menu Stand up with clicking one's own physical seat and a Stand up hover tag. Add a physical door with Leave hover tag to leave, instead of an Esc-menu Leave lobby button. | B, C |
| C10 | Preserve working camera reset, bidding, physical payment, game rules and peer synchronization while changing presentation. | E |

## Intent audit evidence

1. Extraction: reread the latest original message and separated context labels, coin precedence, selection preview/release, containment alternatives, coin size ordering, ante speech, deck hover, sound, display scaling and diegetic navigation.
2. Traceability: every C1–C10 entry has a task and validation boundary. Existing plans remain foundation. C9 supersedes the earlier Esc-menu placement, not its authority checks or leave confirmation/terminal screen.
3. Adversarial omission: retained pre-release money counting, the distinction between coins and container hover, selection alternatives as a choice rather than a mandate, screen-size-only scaling and right-click gesture ownership. No tool mode or bulk money-transfer authority is inferred.

Source limitation: none for the latest message. Earlier behavior is documented in PLAN-7 through PLAN-10.

## Decisions and scope

- Use projected centres inside the rectangle, with a live preview. Select coins and card pieces; count public coin denominations without exposing hidden card faces. A piece shown both in the world and the inset is counted once. Counting through the glass jar includes its projected coin centres, not just the topmost visible coins.
- Selection is local inspection, not a new authoritative object move or multi-coin payment. It starts on empty space, not on an existing draggable object or UI control. A click on empty space clears it; contextual feedback explains the centre rule.
- Retain ordinary camera gestures. RMB on a private inset card owns that complete gesture to open its size popup, rather than also rotating the world camera.
- Keep server-owned logical containers and all conserved money. Any jar restacking changes poses, not denominations, balances or identity; existing held previews must not be reset incidentally.
- Add short locally generated sound cues, without third-party audio licensing. Windowless tests record cue events without audible output. Local sound can be disabled in Options.
- Seat and door use existing typed actions and existing leave confirmation. A click must not silently abandon a lobby; retain the left-room intermediary screen.
- This is a presentation/input slice, not payout implementation, a general group-transform editor, an audio asset library or a change to the formal game rules.

## [x] A — Inspect money contextually and order jar coins

**Completion notes:** Coin hover wins over container hover, including coins behind transparent glass. Initial placement sorts denomination before ID. Exact-old-layout migration is pose-only and skips any moved or lifted jar coin; nine `poche-money` tests pass, including conservation and idempotence. Maincloud publication succeeded without a schema reset. Final live acceptance verified exact hover counts/values and coin precedence. Viewed the final `contextual-inspection.png`: cursor-adjacent 17-pixel text replaces tiny world labels and remains readable at the default distance.

**Work:** Remove persistent totals. Hover the actual jar/lid/bowl bounds for public count/value, with coin-hover precedence. Keep coin outlines/interruptible return. Order quarters beneath dimes in canonical jar placement, preserving identities and value.

**Validation:** Pure hover/stacking/conservation tests and windowless before/hover/coin-hover captures, including existing-room compatibility where layout changes require authority work.

**Completion criterion:** Idle table has no money labels; container hover supplies totals without fighting coin selection; quarters occupy the lower jar layers.

## [x] B — Put actions and prompts on world objects

**Completion notes:** Deck quantity is hover-only. Owing players speak first-person ante/penalty prompts; the dealer speaks the next-deal prompt. Own-seat and door clicks use stable typed-action relays, retaining leave confirmation and the terminal screen. Regression tests cover capsule occlusion of the stool, top-down picking of the door's thickness, and queued actions surviving a scene rebuild. Final rebuilt live acceptance passed stand/leave through ordinary pointer input and confirmed the other viewer's membership update.

**Work:** Hover-only deck quantity, first-person ante/payment speech, own-seat Stand up, physical door Leave/confirmation. Avoid duplicate permanent next-step prompts while preserving discoverability of dealing/payment.

**Validation:** Presentation/picking tests; ordinary pointer hover and clicks in the running windowless client; server peer observes stand/leave through the existing command path.

**Completion criterion:** Players can discover and perform the infrequent actions in the world without permanent numeric labels.

## [x] C — Resize the private hand and separate Help

**Completion notes:** RMB on an inset card opens a local 75–250% scale slider, bounded to 48% of window height. DPI-aware sizing preserves physical zones and shared poses. Popup gestures retain capture through button release; actual mouse tests verified no camera rotation or authoritative pose change. Help and Options have separate real menu buttons; seat/leave menu entries are removed. Screenshot review moved the popup above the maximum hand area so enlarged ranks remain visible. Final image inspection verified the enlarged rank below the unobstructed slider; normal/4K/short-window placement tests pass.

**Work:** Right-click inset-card size popup with slider; responsive, bounded, local hand scale. Preserve projection roundtrips, bottom-edge anchoring and hit tests. Options holds settings; Help holds instructions. Remove the two replaced Esc actions.

**Validation:** Scale/viewport/gesture tests and real RMB/slider/drag captures at small and large hand sizes. Help/Esc navigation remains predictable.

**Completion criterion:** The user can enlarge their hand without changing shared card dimensions or accidentally orbiting the camera.

## [x] D — Add selection/counting and tactile sound

**Completion notes:** Projected-centre selection previews and retains coin/card outlines, deduplicates inset cards, counts public denominations and issues no reducer. The first real pointer test counted 400 coins/$70 before release and preserved all authoritative poses. Original generated paper/coin PCM cues use Bevy audio; three sound tests verify decoding and one pickup/drop per local gesture, not network echoes. Windowless live tests observed cue counts without opening an audio output device. Speaker output is not claimed as listened-to evidence.

**Work:** Local screen-space marquee with live selected outlines/count/value and persistent selection on release. Suppress conflicts with object drags/UI/camera gestures. Add short card/coin pickup/release cues and a sound toggle, silent in automation.

**Validation:** Rectangle direction/boundary/dedup tests; real pointer preview/release/clear tests; no reducer actions or money changes while selecting; exact cue lifecycle tests and silent-mode evidence.

**Completion criterion:** Counting is an intentional inspectable action, and ordinary pickup/drop gives restrained sound feedback without network-echo repeats.

## [x] E — Integrate, validate and release

**Completion notes:** File-control schema 10 adds contextual diagnostics, projected targets, actual menu button centres and silent sound counters. Final integration passed 119 desktop tests, three puppet tests, nine money tests, four native round-flow tests and strict changed-crate Clippy. Native module `--lib` tests cannot link SpacetimeDB host imports on Windows; use the native `round_flow` integration target plus the actual Wasm build/publication. Final Maincloud windowless report v13 passed all 24 checks, two full rounds, rejoin/sibling presence and new direct input paths (72.39 ms authority, 171.25 ms peer for its measured pose). These are one-run observations, not latency guarantees. Inspected the final contextual contact sheet; the hand popup no longer obscures card ranks and container totals are readable. No error/panic/warning entries matched the test client stderr logs. Implementation commit `f40194c` was pushed to `origin/spacetimedb`; the module was published to the existing Maincloud database without a reset. This documentation follow-up records release completion.

Validation commands run from the repository root:

```powershell
cargo test --locked -p poche-spacetimedb-desktop --lib --bins
cargo test --locked -p poche-money
cargo test --locked -p poche-spacetimedb-module --test round_flow
cargo clippy --locked -p poche-spacetimedb-desktop -p poche-money -p poche-spacetimedb-module --all-targets --no-deps -- -D warnings
cargo build --locked -p poche-spacetimedb-desktop --bins
target\debug\poche-puppet.exe acceptance --server maincloud --output target\poche-puppet\contextual-table-final.png
```

The final receipt is `target/poche-puppet/contextual-table-final.json`; captures, endpoint credentials and logs remain ignored. The 24 checks include live preview/release/clear, zero authoritative changes from inspection, actual RMB/slider gesture capture, exact coin hover priority, ordinary chair/door/Help/Options input and one local pickup/drop cue without remote echoes. These checks do not prove speaker perception, every GPU/window configuration or formal UI conformance.

**Work:** Update file-control observations and acceptance for new direct world actions, run focused tests/lint/build, preserve existing 2-round acceptance, inspect rendered captures, publish only if authoritative layout changes need it, update guide, commit and push.

**Validation:** `cargo test --locked -p poche-spacetimedb-desktop --lib --bins`; affected money/module tests; strict changed-crate Clippy with `--no-deps`; `poche-puppet acceptance --server maincloud` using the rebuilt binary. Use a focused contextual-table scenario for new interactions in addition to existing round flow.

**Completion criterion:** All C1–C10 have evidence and any limitations are explicit. No network/schema reset or user identity loss. No claim of audible perception from a silent automation run.

## Risks

- Input ownership: a selection box or size popup must not simultaneously move a card or orbit the camera. Test initial press through release.
- Hover semantics: occluded coins count in an intentional marquee, but a coin under the cursor suppresses container text. Neither grants movement authority.
- Existing jars: presentation and authority must agree after canonical restacking; preserve active preview positions until committed/reconnected.
- Dense UI: labels should be contextual without removing the only discoverable next action. Use speech, hover and Help.
- Persistent identity and model agreement stay unchanged; no new formal proof is claimed for UI behavior.
