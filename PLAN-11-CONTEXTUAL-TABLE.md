# Contextual table information and direct interaction

**Plan status:** Complete — title-screen follow-up G included
**Primary implementation root:** `spacetimedb`, following completed PLAN-10
**Last updated:** 2026-09-17
**Intent audit:** Latest title/identity overlap report mapped to C16 and checked against the screenshot, layout code and regression scope. A–F retain their completed release evidence.

## How to update this plan

Use `[ ]` not started, `[~]` active, `[x]` complete and `[!]` blocked. Keep evidence beside each task. Parallel owners: world affordances/speech, money inspection/stacking, hand settings/menus, and root selection/audio/integration. Preserve user intent and existing data. Windowless file-control validation must not use the OS clipboard or visible automation windows.

## Authoritative guidance and traceability

| ID | User direction | Coverage |
| --- | --- | --- |
| C1 | Reduce always-present visual information. Original container-hover totals were implemented in A; the latest request explicitly replaces those totals with selection-only counting (C13). Spatial placement identifies the owner. | A, superseded by F |
| C2 | Primary-button drag on empty screen creates a 2D selection rectangle. Show live coin count/value before release; release selects encompassed pieces. User considers centroid or full enclosure preferable to mere intersection, without insisting on one. | D |
| C3 | Put larger quarters below dimes in jars. | A |
| C4 | Unpaid 25¢ ante should prompt first-person speech above each owing player instead of a bowl label. Preserve clear actionable progression. | B |
| C5 | Deck card quantity should be contextual on hover, not permanent. | B |
| C6 | Add pickup and put-down sound effects. | D |
| C7 | Bottom-screen cards are too small on this display. Right-clicking an inset card should open a slider popup to change its display size. Preserve the physical/shared card dimensions and the hand/world mapping. | C |
| C8 | Move instructions out of Options into a separate Help entry. | C |
| C9 | Replace Esc-menu Stand up with clicking one's own physical seat and a Stand up hover tag. Add a physical door with Leave hover tag to leave, instead of an Esc-menu Leave lobby button. | B, C |
| C10 | Preserve working camera reset, bidding, physical payment, game rules and peer synchronization while changing presentation. | E |
| C11 | Enlarge the room/base plate, moving the exit farther away and to the actual perimeter, without a margin of floor behind it. | F |
| C12 | Overlapping bottom-screen hand cards must not let the left card hide the rank and suit of cards to its right. | F |
| C13 | Remove bowl, lid and jar hover counts; players count money through selection instead. This does not remove deck hover information or coin hover outlines. | F |
| C14 | Put the selection count/value near an edge or corner of the selection, styled as floating world text rather than a fixed bottom-left HUD label. | F |
| C15 | Remove the centre-containment and click-empty-space hints from the selection readout. Keep its existing local inspection behaviour. | F |
| C16 | Fix the title-screen POCHE heading overlapping the identity selector. Preserve identity switching, create/join and recent-lobby access. | G |

## Intent audit evidence

1. Extraction: reread the latest original message and separated context labels, coin precedence, selection preview/release, containment alternatives, coin size ordering, ante speech, deck hover, sound, display scaling and diegetic navigation.
2. Traceability: every C1–C10 entry has a task and validation boundary. Existing plans remain foundation. C9 supersedes the earlier Esc-menu placement, not its authority checks or leave confirmation/terminal screen.
3. Adversarial omission: retained pre-release money counting, the distinction between coins and container hover, selection alternatives as a choice rather than a mandate, screen-size-only scaling and right-click gesture ownership. No tool mode or bulk money-transfer authority is inferred.

Source limitation: none for the latest message. Earlier behavior is documented in PLAN-7 through PLAN-10.

## Decisions and scope

- Use projected centres inside the rectangle, with a live preview. Select coins and card pieces; count public coin denominations without exposing hidden card faces. A piece shown both in the world and the inset is counted once. Counting through the glass jar includes its projected coin centres, not just the topmost visible coins.
- Selection is local inspection, not a new authoritative object move or multi-coin payment. It starts on empty space, not on an existing draggable object or UI control. A click on empty space clears it. C15 removes the on-screen explanation; Help may describe the gesture.
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
- Hover semantics: occluded coins count in an intentional marquee; C13 removes container totals altogether. Hovering a coin still highlights it without granting movement authority.
- Existing jars: presentation and authority must agree after canonical restacking; preserve active preview positions until committed/reconnected.
- Dense UI: labels should be contextual without removing the only discoverable next action. Use speech, hover and Help.
- Persistent identity and model agreement stay unchanged; no new formal proof is claimed for UI behavior.

## [x] F — Room perimeter, readable hands and selection-only counting

**Intent checks:** (1) Extracted all five changes from the original message and its screenshot. (2) Mapped each to C11–C15 and specific implementation/validation work. (3) Checked for omissions: the door belongs on the floor boundary, not merely farther away; overlap must work from both seats; only money container hover counts are removed; the live and released selection both retain useful counts without hints. No new authority, rule or private-card disclosure is intended.

**Work:** Expand the floor while preserving table/rule geometry. Place the door frame flush with the floor edge. Replace opaque-ID card depth ordering with owner-facing spatial ordering; picking and rendering must agree. Remove container tooltips. Add an unlit, camera-facing world-mesh readout at the selection corner, retaining its world anchor on release. Preserve centroid membership and deduplication.

**Validation:** Reproduce the overlap before fixing it; test both seats and seven-card overlap chains, mesh bounds at the door/floor boundary, terse selection summaries and label placement. Run the windowless two-client acceptance through ordinary input, including absent money hover totals, selection preview/release/clear, world readout diagnostics, enlarged hands and the farther exit. Inspect rendered captures, run desktop tests/lint and build the client. No module republish is needed unless authoritative code changes.

**Evidence:** Before the overlap fix, the filtered test run passed the near-seat two-card test and failed the far-seat two-card and seven-card chain tests. Afterward all six focused overlap tests passed. The floor is 4.2 m square, with mesh-bounds tests proving jamb/floor alignment. Obsolete container tooltip code was removed, not merely hidden. File-control schema 11 records actual mesh readouts and own-hand corner pick targets. The final rebuilt Maincloud windowless run passed all 27 v14 checks, including two full rounds, ordinary pointer picks of both two-card hands at normal and double size, absent container counts, world readout placement/retention/clear, and an ordinary camera orbit plus two-click exit at the perimeter. Inspected the selection, two-seat hand and perimeter captures. Rank/suit corners, including a ten, are visible in both hand scales. The measured authority/peer times were 74.48/192.71 ms in this run, not a guarantee. No stderr output was produced by the test clients. No module, database schema or existing player identity was changed.

**Evidence boundary:** Seven-card and common-rotation coverage checks depth ordering and picking, not every glyph extent in every viewport. Graphical acceptance covers two-card hands from both seats at the tested landscape viewport. The readout is a local world-space annotation, not an authoritative game object. Existing A–E hover screenshots describe the superseded release.

**Release contents:** Rebuilt desktop client only; no server publication needed. The final recheck passed 125 desktop tests, three puppet tests and strict desktop Clippy after correcting the unit fixture to the module's actual ±520 mm hand centres. Runtime build and graphical acceptance succeeded. Receipts remain ignored at `target/poche-puppet/table-refinement-final.json`; selection and perimeter contact images live beside it and hand comparisons inside the run's Bob captures directory. This plan and the desktop guide accompany the refinement commit on `spacetimedb`.

**Completion criterion:** C11–C15 are implemented and verified, existing two-round play remains intact, and the rebuilt client plus a concise restart guide is available. Preserve prior release evidence above as history, not as a claim that hover totals remain current.

## [x] G — Separate title branding from identity controls

**Completion notes:** Identity controls occupy their own header above the branding. The remaining menu scrolls with the wheel or Page Up/Page Down in short windows; long names wrap without hiding the arrows. The desktop executable and puppet were rebuilt with `cargo build --locked -p poche-spacetimedb-desktop --bins`. The guide documents scrolling. This is a client-only release; no module publication or database change is required. Validation evidence follows.

**Intent checks:** (1) The original message and screenshot identify title/identity overlap with four recent lobbies. (2) C16 maps to the production frontend layout and actual computed-bounds tests. (3) The fix must retain the selector and history, not remove either to make room. The screenshot is evidence, not an instruction source; its room codes need not be persisted or contacted.

**Verified cause:** `spawn_identity_selector` uses absolute top positioning while `spawn_frontend` centres a growing content stack. The selector takes no layout space. Tall history can place the title beneath it.

**Reproduction and visual evidence:** Real Bevy font/layout tests reproduced the original overlap at 1770×1140 with 150% scaling and at 1180×760 with 100% scaling, both with four recent lobbies. A 640×600 long-name case clipped the title above the window. The nearby empty-history case passed. The replacement reserves a non-shrinking identity header and scrolls only the remaining menu. Windowless GPU captures exposed zero-width text boxes in an intermediate revision; the final selector retains intrinsic text sizing. Regression checks now require nonzero shaped text bounds contained within all three identity buttons, not merely non-overlapping button backgrounds. Inspected final captures at the two original extents and 640×600 before/after scrolling: names and arrows are visible, and history remains reachable. Synthetic lobby codes and disposable identity files avoid copying the user's live capabilities into fixtures.

**Validation results:** Seven focused regressions pass, including a 480×600 window with a wrapping maximum-length 32-character name, Page Up/Page Down access to all history entries, and equivalent line/pixel wheel scrolling with end clamping at 150% scaling. `cargo test --locked -p poche-spacetimedb-desktop --lib --bins` passes 132 desktop tests and three puppet tests. Strict desktop Clippy passes. The normally ignored GPU capture test was run explicitly with `cargo test --locked -p poche-spacetimedb-desktop --lib capture_title_identity_layout -- --ignored --nocapture`; all four images were inspected. Captures remain ignored under `target/poche-puppet/title-layout`. This proves production menu layout and rendered labels at the tested sizes, not every window configuration or network identity workflow. No authority connection, user clipboard or visible test window was used.

**Work:** Put identity controls in a non-shrinking header. Give the remaining menu a separate bounded content area; allow scrolling when short windows cannot fit the full menu. Preserve actions, identities and history. Root owns layout; the validation agent owns real Bevy UI layout regression cases.

**Validation:** First reproduce using production UI with a four-entry history and a nearby empty-history case. Check actual title/selector bounds at the screenshot's logical extent, narrow/short windows and long names. Inspect a windowless render. Run desktop tests, strict lint, rebuild, commit and push. No database or authority change is needed.

**Completion criterion:** Branding and identity controls do not overlap; all controls remain accessible without changing identity semantics. Record the exact tested sizes and release evidence here.
