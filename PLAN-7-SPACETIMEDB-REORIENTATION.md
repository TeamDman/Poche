# SpacetimeDB desktop reorientation

**Plan status:** Active; the create/join/seat/private-hand/shared-pose MVP and first oracle-backed trick are accepted, while multi-round play, recovery, and complete formal conformance remain
**Primary implementation root:** `D:\Repos\Games\poche-4` on branch `spacetimedb`
**Base revision:** `f0b371727301730f9db88ad53defa9d66c684269` from `model-checking`
**Last updated:** 2026-09-16 (diegetic table in PLAN-8; tactile interaction and physical money in PLAN-9)
**Intent audit:** Passed 2026-09-12 against the available original Poche conversation through the request to create `poche-4` and reorient around SpacetimeDB

## How to update this plan

The completed interaction/presentation slices are defined in [PLAN-8-DIEGETIC-TABLE.md](PLAN-8-DIEGETIC-TABLE.md) and [PLAN-9-TABLE-TACTILITY.md](PLAN-9-TABLE-TACTILITY.md). Their ledgers extend, rather than replace, the constraints below. New rooms require physical quarter antes before dealing; this supersedes the earlier automatic deal-on-seat shortcut.

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked
- Update a work item's heading and completion notes together.
- Record decisions, commands, relevant results, commit IDs, and intentional
  exceptions below the item they affect.
- A phase is complete only when every work item in it is `[x]`.
- Keep at most one current implementation focus unless the plan explicitly
  names independent tracks.
- The plan is only ready once we have literally triple checked that no intent
  from the user has been omitted without explicit direction from the user.

## Intent audit evidence

- **Pass 1 — extraction:** reread the available original messages from the
  Alloy/NuSMV/Prolog exploration through the current SpacetimeDB pivot. The
  ledger below separates confirmed product direction, tentative examples,
  prior directions that were superseded, local paths, testing constraints,
  and future-facing concerns. It includes the desktop-first correction, Bevy
  0.19.1 decision, physical/logical card distinction, two-window MVP,
  reconnect and empty-room lifetime, multiple-device agency, windowless
  capture, formal boundaries, compile-time concern, and the explicit
  prohibition on depending on `bevy_spacetimedb`.
- **Pass 2 — traceability:** checked every active U-row against the gates,
  phases, acceptance matrix, or explicit non-goal. Checked the inverse as
  well: the proposed central authority, crate boundaries, generated-code
  isolation, local prediction, bounded pose publication, token persistence,
  and licensing gate are either user-directed, verified from source, or
  labeled reversible working assumptions.
- **Pass 3 — adversarial omission:** reread the instructions from the plan
  backwards to the conversation. Preserved “reference material, not a
  library,” “move cards around” as synchronized position *and rotation* rather
  than only legal play, name-and-secret as tentative rather than decided,
  “everyone leaves” as explicit departure rather than transient disconnect,
  Bevy as presentation rather than rules authority, browser support as
  currently dropped rather than permanently impossible, and RL as a future
  shape constraint rather than part of this implementation goal. The review
  also distinguishes Codex sandbox elevation from Windows UAC and a trusted
  SpacetimeDB host from the earlier zero-trust aspiration.
- **Known source limitation:** no user-message limitation for this audit. Some
  historical command output is compacted, so repository plans and current
  source were used for implementation evidence; no missing tool output was
  treated as a successful test.

## 2026-09-12 implementation checkpoint

The transport rewrite's first player-observable milestone is complete:

- Poche-owned `poche-spacetimedb-module`, `poche-spacetimedb-client`,
  `poche-bevy-spacetimedb`, and `poche-spacetimedb-desktop` crates compile
  against the pinned 2.10.0 SDK/CLI and Bevy 0.19.1.
- Sender-scoped public views expose room/member state, each caller's own private
  hand, and face-free shared card poses. The trusted server owns the private
  rows and validates room membership, seat exclusivity, card ownership, pose
  sequence, and coordinate bounds.
- The workspace default binary is the SpacetimeDB `poche.exe`; its dependency
  path does not include Veilid. The previous transport remains in explicit
  packages and history.
- Two ordinary game-binary processes are controllable through fresh,
  per-instance file queues. Requests use typed player intents, correlated JSON
  responses contain semantic observation, join codes require an explicit
  private observation, and screenshot requests target the real GPU surface.
- `poche-puppet acceptance` launches two windowless copies, creates and joins
  one room, takes distinct seats, verifies two private five-card hands and ten
  face-free poses, moves Alice's owned card, waits for Bob's exact peer
  observation, and composes all four captures into one PNG. The latest run
  measured 18.09 ms to Alice's observed authority result and 101.96 ms until
  the puppet observed the exact pose from Bob.
- Visual review of the contact sheet confirmed own-seat-at-bottom projection,
  correct suit glyphs, opaque peer backs, immediate moved owner pose, matching
  moved peer pose, and context-sensitive seat controls.
- Final validation also passed the SpacetimeDB WASM module build, all targeted
  Rust tests for the preserved pure/session layers and new integration crates,
  protocol replay, the Rust/Alloy/NuSMV/Prolog lobby-micro comparison, strict
  Clippy with warnings denied for handwritten integration code, package-scoped
  formatting, and a default-desktop dependency audit excluding both Veilid and
  `bevy_spacetimedb`.

This checkpoint does **not** close the comprehensive plan. Physical movement
still leaves logical location as `hand`; no SpacetimeDB reducer yet calls the
pure `GameEnvironment` for bid/play/drop; restart/multiple-device presence
semantics and formal lifecycle adapters remain open. The existing Rust,
Alloy, NuSMV, and Prolog rule sources were retained rather than replaced by
the database schema. See `docs/spacetimedb-desktop.md` for the exact run,
acceptance, trust, privacy, and license boundaries.

## 2026-09-13 hosted/local authority checkpoint

- Published the existing Rust Poche module—not a React template—to the owned
  Maincloud development database `poche-6quz6`. The migration created the
  private room/member/hand/pose tables and caller-scoped views without deleting
  data.
- The ordinary binary now accepts `--server maincloud`, `--server local`, or an
  explicit HTTP(S) URL plus an optional `--database`. The selected profile and
  database are visible on the main menu. Local remains the no-argument default
  so offline automation cannot contact Maincloud accidentally; environment
  overrides remain compatible.
- Persisted player credentials are scoped by authority URI and database. The
  existing default-local credential namespace is preserved, while a same-name
  Maincloud player cannot accidentally reuse a local server token.
- `poche-puppet acceptance` accepts the same authority options, forwards the
  exact resolved URI/database to both ordinary windowless clients, and records
  them in its versioned JSON receipt.
- Maincloud acceptance passed with two identities, 5+5 private cards, ten
  face-free shared poses, 54.25 ms authority acknowledgement, and 108.34 ms
  exact peer observation. A fresh isolated `--server local` run passed the same
  checks at 18.51 ms and 68.43 ms respectively.
- Binaryen 131 passed normal `spacetime build` and module-path publication on
  both local SpacetimeDB 2.10 and Maincloud. Binaryen 132 remains excluded by
  upstream issue #5828 because its compact-import output is rejected by the
  current server parser.

This proves deployment selection and the current physical-pose slice on both
authorities. It does not yet prove durable logical card movement, reconnect,
multiple devices for one identity, full Poche play, or production operations.

## 2026-09-13 renderer/input correction and 3D restoration checkpoint

- Confirmed and then removed the SpacetimeDB client's top-down Bevy UI
  tabletop regression. Subscribed poses now drive `Transform`s on dimensional
  `Mesh3d` cards in a perspective, lit table scene with rail/floor geometry,
  shadows, seats, and spatial player avatars. The UI camera is a transparent
  overlay for status and commands rather than a substitute for the world.
- Millimetres become metres and millidegrees become Bevy quaternions only at
  the renderer boundary. Pointer dragging uses a camera ray intersecting the
  card-height world plane; selection projects actual world positions back to
  the viewport. The camera interpolates between spectator and seat-relative
  viewpoints as subscribed membership changes.
- Extracted the proven filled Slug raster path into renderer-neutral
  `poche-slug` code. Each client renders authorized card faces on raised card
  surfaces while peer cards remain opaque backs, without returning to the old
  malformed outline/stroke text.
- Removed horizontal pointer-delta rotation. Drag now changes position only;
  Q/E changes the visible table-normal angle in bounded steps, with held-key
  repeat. A top control cycles off/15°/30°/45°/60°/90° rotation snap.
- Kept the replicated schema in exact millidegrees for now. One full turn is
  360,000 units. A `Turn16`-style domain newtype is worth evaluating alongside
  the 3D orientation/Euler contract, but a schema migration is not justified
  merely to avoid radians because storage and reducers already use integers.
- The reported Vulkan present/acquire validation sequence matches minimal
  upstream Bevy/wgpu reports. Windows now defaults to DX12 while
  `--graphics-backend auto|dx12|vulkan` keeps diagnosis and future retesting
  explicit. Windowless evidence cannot validate a swapchain-present fix.
- Package unit tests and strict Clippy pass after the correction. An ordinary
  visible-window check remains required because only a real Winit swapchain can
  confirm that the user's startup diagnostics are gone.
- After rebuilding both sibling binaries, hosted two-device acceptance passed
  twice. The runs measured 56.74/147.34 ms and 112.17/222.47 ms for authority
  response/exact peer observation respectively. The final reviewed four-frame
  contact sheet visibly contains the real perspective scene from opposite
  seats and the same moved card at 45°, face-up for its owner and face-down for
  its peer. The run also exposed that `cargo run --bin
  poche-puppet` does not relink the sibling `poche.exe`; acceptance instructions
  continue to require `cargo build --bins` first so stale renderers cannot be
  mistaken for current evidence.
- T4 remains open for its larger contract: the dedicated private-hand inset
  camera, diegetic seat/action targets, durable logical drop/play reducers,
  denial presentation, and recovery semantics are not claimed by this visual
  restoration.

## 2026-09-13 oracle-backed first-trick checkpoint

- Replaced the five-card sample deal with the real deterministic first round
  from `OracleEnvironment<2>`. The module persists only its seed, an ordered
  typed bid/play log, a public latest projection, private card identities, and
  revealed faces. Every durable player action reconstructs and validates the
  full pure Rust game before its transaction commits.
- Added caller-scoped game/revealed-card subscriptions and carried typed Bid
  and PlayCard intents through the renderer-neutral client, Poche-owned Bevy
  bridge, visible action bar, and file-control puppet. Physical pose remains a
  separate latest-value channel; accepted play alone changes `hand` to
  `play:{seat}` and then `won:{winner}`.
- Replaced provisional oversized scene geometry with registered
  `poche-spatial` revision-one geometry. The shared table camera and dedicated
  private-hand camera render the same card entities on explicit render layers;
  pointer movement crosses viewports back into one world. Dropping in PLAY
  proposes typed play, while an out-of-turn drop restores the physical card to
  its hand and leaves logical state unchanged with visible status.
- Published the additive schema/reducers to Maincloud database `poche-6quz6`.
  The v3 windowless acceptance then created/joined/seated two independent
  clients, verified one distinct private card apiece and two face-free public
  poses, propagated a 45° card wiggle, submitted two legal bids and two legal
  plays, and converged on two revealed cards in `won:1` with phase `scoring`.
  Five reviewed runs measured 78.92–129.29 ms for caller observation and
  198.81–281.90 ms for peer observation of the sampled wiggle; this is
  acceptance evidence, not a p95 result, and keeps T6.4's latency investigation
  open.
- Reviewed the four-view PNG. It visibly shows opposite seat projections,
  private enlarged hand views, opaque peer backs before play, and common won
  cards after play. That review caught and fixed seat-label occlusion plus the
  seat-one hand camera's upside-down orientation. Remaining polish includes
  diegetic seat/bid controls and a less crowded HUD.
- Targeted tests (12 total), module WASM build, package-scoped formatting, and
  strict Clippy for all changed handwritten packages pass. The dependency
  `poche-spatial` still has a pre-existing `missing_panics_doc` warning under a
  workspace-wide `-D warnings`, so changed-package lint used `--no-deps`.

This checkpoint advances T4.3 substantially and T4.4/T6.2 partially. It does
not claim the complete multi-round game, denied-drop puppet coverage, restart,
multiple devices per player, explicit final-leave UX, formal adapter parity,
or the latency distributions required by T6.4.

## 2026-09-13 active-room and table-control checkpoint

- Diagnosed the apparent player in the middle of the table and failed
  leave-to-menu transition as one authority projection bug. A persisted
  SpacetimeDB identity could retain durable memberships in older rooms, while
  all sender-scoped views unioned every such room and `room_id()` selected an
  arbitrary first row.
- Added a private identity-keyed `active_room` table. Create and join move that
  focus atomically; room, roster, hand, game, reveal, and pose views expose only
  the focused room. Disconnect preserves focus for reconnect, explicit leave
  removes it, and switching focus marks the older membership disconnected.
  The additive migration was published to Maincloud `poche-6quz6` without
  deleting data.
- Unseated members no longer become tabletop avatars. A compact authoritative
  roster below rotation snap still shows seated and standing members,
  connection state, seat, and which identity is the viewer.
- Moved destructive leave into an Escape table menu. Its first activation
  changes the button from **Leave lobby** to **Confirm leave lobby** and
  explains the effect; Resume or Escape cancels pending confirmation. A
  successful leave now tears down cards/avatars/camera state, deactivates the
  world camera, and reconstructs the actual main menu with explicit status.
- Replaced the hard seat-camera lerp with a smoothed focal-point rig. RMB
  orbits, MMB pans, WASD moves the focus camera-relatively, and Space targets a
  smooth seat-relative reset. The focus is clamped to the canonical table-top
  rectangle expanded to twice its extents.
- Extended the file-control protocol with table-menu and leave-button actions.
  The v4 windowless acceptance captures the Escape menu, verifies that the
  first leave-button activation changes it to **Confirm leave lobby**, then
  verifies the second activation leaves Alice, her semantic surface and GPU
  capture become the main menu, and Bob's roster falls to one current-room
  member. The final Maincloud run measured 79.98 ms for authority response and
  199.60 ms for exact peer pose observation.
- Thirteen desktop tests pass, including camera bounds, non-teleporting reset,
  and leave-confirmation cancellation. The SpacetimeDB WASM build and the
  published additive migration pass. This does not yet prove visible physical
  mouse feel, restart/multiple-device identity behavior, the final-member room
  deletion path, or a complete multi-round Poche game.

## 2026-09-13 task-status and camera-control reconciliation

- Reconciled every task heading below against its full completion criteria.
  Tasks with an implemented positive slice but an untested negative, recovery,
  formal, load, or reproducibility criterion are `[~]`, not prematurely `[x]`.
- At the time of this reconciliation, T1.3, T3.4, the original T4.1
  create/join scope, T4.3, and T7.2 had checked implementation evidence. T0
  remains complete. The later multi-account correction below deliberately
  reopens T4.1. T2.3 stays partial until the same authorized hand is proved
  across multiple connections for one identity.
- Compile profiling, pinned-tool orchestration, generated-binding drift checks,
  full DTO/action conformance, multi-connection presence, callback
  backpressure, GUI/CLI parity, denied-drop automation, formal adapter parity,
  disposable service startup, visible restart acceptance, latency
  distributions, recovery faults, documentation, and CI/release receipts stay
  partial or open exactly where their task criteria say so.
- Development dependency optimization now makes ordinary debug rendering
  usable without changing the workspace release profile. Q/E direction and
  visual rotation are corrected and smoothed; mouse-wheel zoom and the focal
  camera are smoothed; F3 toggles Bevy's FPS/frame-time overlay; and O toggles
  a remembered orthographic tactical view. The desktop package has 19 passing
  tests and strict package-local Clippy through `8e40ca9`.
- The immediate implementation focus remains T3.1/T4.5. The server already
  retains membership and seat state across a disconnect, and final explicit
  leave already deletes an empty room. The missing proof is end-to-end resume,
  connection-counted presence for multiple devices, and clear visible states
  around reconnect, missing authority, and disbandment.

## 2026-09-13 multi-account identity-flow correction

- The installation is expected to retain several authenticated Poche
  identities, like the Azure CLI retains several accounts. There is no single
  installation-global active player: each running game window selects its own
  identity so two local windows can deliberately be Alice and Bob, or both be
  Alice for multi-device testing.
- A launch begins at an identity gate. Existing identities can be selected and
  a new local identity can be created. Only after selection does that window
  connect and discover whether the chosen identity has resumable membership;
  an unrelated identity must never see another identity's rejoin prompt.
- The title screen keeps the selected identity visible at its top. Left/right
  arrows cycle stored identities, and activating the identity name opens the
  full identity screen for selection and creation. Switching identities is a
  disconnect for the old identity, never an implicit room leave.
- A local account record has an immutable account ID, user-facing label, and
  authority association. Display names remain mutable presentation and may not
  be used as credential keys. The SpacetimeDB token remains in its protected
  credential store under the immutable account ID.
- “Several accounts logged in” means their credentials are retained and ready
  for selection. The MVP keeps one live selected identity per process; it does
  not maintain background network connections for every stored account.
- This supersedes the prior plan assumption that Poche should automatically
  connect the last-used profile before identity selection. T4.1 returns to
  `[~]` until the identity gate/title selector replaces the current conflated
  player-name/profile-name field. No implementation is claimed by this design
  correction.

## 2026-09-13 multi-account identity implementation checkpoint

- Added a versioned, installation-local identity catalogue. Each record binds
  an opaque immutable account ID to a user-facing label, display name, authority,
  database, and non-secret observed principal. SpacetimeDB tokens remain in
  the SDK credential store under authority plus immutable account ID and are
  absent from the JSON catalogue. Atomic writes, a bounded cross-process lock,
  and merge-on-write allow ordinary sibling windows to create different
  accounts without clobbering one another.
- The desktop now starts at `IdentityGate`, connects only after an account is
  created or selected, and waits for its initial sender-scoped subscription.
  An active room produces `ResumeOffer`; the table is reconstructed only after
  **Rejoin lobby**. The title shows `‹ account ›`, account arrows perform the
  same authenticated switch, activating the account opens the identity screen,
  and **Refresh identities** discovers accounts created by another live
  process. Switching accounts disconnects but never invokes leave.
- Create/join no longer accepts a display-name-derived credential profile.
  The Poche-owned Bevy bridge owns explicit Connect/Disconnect separately from
  room reducers, and principal binding fails closed if a protected credential
  unexpectedly resolves to a different SpacetimeDB identity.
- Added private `connection_presence` rows keyed by SDK `ConnectionId`.
  Lifecycle reducers derive member presence from whether any connection row
  remains for that identity. A second process can therefore authenticate as
  Alice without creating another member/player, and closing it cannot mark the
  original Alice window offline.
- Leave now yields a deliberate `LobbyEnded` screen with **Return to title**
  instead of jumping directly to the menu. The file-control protocol v2 names
  identity selection and resume explicitly and reports all six front-end
  surfaces.
- Generated 2.10.0 bindings, passed 25 focused client/bridge/desktop tests and
  the server-module WASM build, and cleared strict Clippy. A fresh isolated
  local database passed the v6 windowless acceptance:
  Alice/Bob created, joined, seated, dealt, wiggled, bid, and played; a second
  Alice process selected the same account, received the explicit resume offer,
  recovered the same principal/seat/private hand, entered the table, and
  disconnected while Alice remained connected. Explicit leave produced the
  terminal screen. The final evidence run measured 56.08 ms to the authority
  and 148.06 ms until Bob's exact pose observation. That run also caught and
  fixed a creator-capability readiness race: file control now completes room
  creation only after both the room projection and bearer join code arrive.
- Reviewed `identity-flow.png`: the identity gate, account-labelled title, and
  resume offer are legible at the real windowless Bevy target. Reviewed the
  final capture independently to confirm the lobby-ended interstitial.
- After explicit approval, published this additive module to Maincloud database
  `poche-6quz6` using the owning authenticated identity. The migration created
  only the private `connection_presence` table, its connection-ID uniqueness
  constraint, and connection/identity indexes; no deletion flag was used. An
  initial anonymous attempt was correctly rejected as a non-collaborator before
  the authenticated publication succeeded.
- The first real Maincloud launch exposed a latency-sensitive capability race:
  the render lifecycle cleared a newly generated code while waiting for the
  room projection. Capability removal is now tied to identity/leave/failure
  boundaries instead of table visibility. Added sender-scoped public view
  `my_room_capability`, backed by the still-private room-secret table, so any
  authenticated active member—including a restarted sibling device—recovers
  only that room's exact bearer code. Published the view to `poche-6quz6` and
  passed Maincloud acceptance with the second Alice process explicitly proving
  exact code recovery; the run measured 70.90 ms to authority acknowledgement
  and 137.85 ms to Bob's exact pose observation.

This advances T2.2, T2.3, T3.1, T4.1, and T4.5 but does not close their broader
criteria. Automatic retry/backoff, actual process crash/relaunch, unavailable-
authority UX, final-member stale-code rejection, visible two-window identity
switching, cross-machine enrollment, and formal lifecycle parity remain open.

## 2026-09-14 nested camera-options checkpoint

- Added **Options** to the in-table Escape menu as a distinct nested page.
  **Back** or Escape returns to the table menu; Escape from the parent resumes
  play. Opening either menu continues to pause only local camera input while
  the shared table remains live.
- Added **Invert camera Y: On/Off** for RMB vertical orbit. `On` is the new
  default and produces the exact opposite vertical response from the previous
  implementation; `Off` restores the previous sign. This presentation option
  remains local to the running device and does not enter authoritative state.
- Extended file control with `options`, `invert-camera-y`, and `back`, and the
  v7 acceptance receipt with default/toggled Options captures. Maincloud
  acceptance passed at 48.44 ms authority response and 157.72 ms peer pose
  observation. Visual review confirmed the nested page and both On/Off labels.
  The desktop package has 23 passing tests and strict Clippy is clean.

## 2026-09-14 activity and rejoin-coherence checkpoint

- Added a private append-only `activity_event` authority table and a
  sender-scoped `visible_activity` view. Reducers write only facts which are
  public when accepted: lobby create/join/leave, seat changes, deal start,
  bids, and revealed played cards. Physical wiggles and every unplayed card
  face remain absent. The desktop renders the newest entries immediately below
  the authoritative player list.
- Fixed the blank-table resume defect at its presentation boundary. A model
  snapshot can arrive while the identity is still on `ResumeOffer`; entering
  the table now invalidates card/avatar reconciliation even if no later network
  row changes. Client rows are canonically sorted so two devices compare and
  render the same projection independent of cache iteration order.
- Added render-grounded acceptance evidence. File-control schema v3 reports
  actual Bevy `CardVisual` and `SpatialPlayer` counts, and the puppet refuses a
  resumed table until those counts equal the subscribed card poses and seated
  roster. This closes the exact blind spot where semantic observations passed
  while the 3D scene remained empty until the next bid.
- Fixed the contradictory active-game leave state. Transient disconnect still
  preserves membership, seat, and hand. Explicitly vacating a seat now
  abandons the incomplete deal, clears its private/public cards and typed game
  action log, and keeps the room in a coherent unseated lobby. A later room-code
  join is a spectator until seats are selected again. Connection, join, and
  seat reducers also repair legacy incomplete deals left by the previous code.
- Added a client-side consistency guard: if the public projection reports a
  positive hand count while this sender's private view is empty, the HUD says
  the hand is synchronizing and offers no bid rather than claiming the round
  finished. The focused desktop test count is now 24.
- Published the additive table/view and reducer changes to Maincloud
  `poche-6quz6`. Windowless v8 acceptance passed with nine converged public
  events, immediate resumed-scene counts, coherent post-leave cleanup, 53.69 ms
  authority response, and 153.42 ms peer pose observation. The reviewed contact
  sheet shows the activity log in both seat-relative 3D projections without
  exposing the peer's unplayed card.

This advances T2.2, T2.3, T4.5, and T6.2/T6.5. A true process stop/relaunch,
final-member stale-code rejection, unavailable-authority recovery, multi-round
play, and formal lifecycle parity remain open.

## 2026-09-15 lobby-lifecycle and crash-evidence checkpoint

- Moved **Stand up** from the frequent table action bar into the Escape menu.
  Leaving remains a two-step Escape-menu action and still presents the
  deliberate lobby-ended interstitial before returning to the title.
- Extended each local identity account with a bounded, newest-first list of
  recent room capabilities. Create and authoritative capability hydration
  atomically persist the entry; the title exposes up to four direct rejoin
  actions after an explicit leave. Old version-one catalogues deserialize with
  an empty history, and concurrent catalogue writes retain the existing lock
  and atomic-replace discipline.
- Added a typed `LoadingRoom` surface for create, join, and resume. It remains
  until at least two Bevy frames have elapsed and the authoritative room,
  self-member row, private hand, and expected public poses agree. The table HUD
  is not shown on a black or partially populated 3D scene.
- Added default durable windowed logs under the platform-local Poche app-data
  directory, `--log-file FILE_OR_EXISTING_DIRECTORY`, and a panic hook which
  records a forced backtrace. Interactive terminals wait for Enter after a
  fatal error; redirected/windowless automation never waits. The behavior was
  informed by the local `teamy-rust-cli` reference checkout without persisting
  its machine-specific absolute path or importing it as a dependency.
- Reduced the reported maximize crash across three real DX12 runs. Fresh-title
  maximize passed; live-table maximize passed; table -> explicit leave ->
  lobby-ended -> title -> maximize failed with `ResizeBuffers` and `window is
  in use`. The table and private-hand cameras remained bound to the primary
  swapchain even when inactive. They now exist only while `UiScreen::Table`
  exists. The exact flow then passed and the puppet observed the maximized
  title process still running.
- File-control schema v4 adds return-to-title and maximize/restore actions so
  the OS-window lifecycle is reproducible without Computer Use or the user's
  clipboard. The focused package now has 30 passing unit tests after the
  camera lifecycle replacement; the preceding teardown-helper version had 31
  before its helper-only test was removed.
- A follow-up visible run exposed a narrower readiness gap: the semantic model
  could select `UiScreen::Table` before the newly spawned spatial cameras and
  dynamic entities reached a rendered frame. Loading now creates cameras,
  cards, and seated avatars behind its opaque frontend; it reveals the table
  only when the `TabletopCamera` exists, rendered counts match the subscribed
  projection, and two complete scene-update cycles have elapsed. The exact
  unseated Alice/Bob state is preserved as
  `loading_does_not_reveal_an_unseated_table_before_the_3d_scene_frame`.
  Thirty-one focused tests, strict Clippy, and full Maincloud acceptance pass;
  the acceptance measured 61.57 ms authority response and 128.01 ms peer
  observation, and all four reviewed captures contained the 3D table.
- A second visible two-window run proved that main-world update cycles were
  still the wrong evidence boundary: both unseated clients could reveal the
  complete table HUD over a black scene, then draw the 3D table later without
  input. The loading gate now consumes a generation-scoped signal emitted by
  Bevy's render world only after the table camera has a non-empty opaque 3D
  phase, every referenced render pipeline is compiled, and the render graph
  has run. An older scene generation cannot release a newer loading screen.
  The corrected regression is
  `loading_does_not_reveal_an_unseated_table_before_the_render_world_confirms_it`.
  The two-device Maincloud acceptance remained green at 85.37 ms authority
  response and 142.85 ms peer observation, with all reviewed final captures
  containing the rendered table.
- The durable log reduced an untouched-title minimize crash to a distinct
  Windows/DX12 surface transition. A minimize event changed Bevy's main-world
  window extent from `1180x760` to `0x0`; the extracted renderer attempted to
  reconfigure the live swapchain and exited after `ResizeBuffers` returned
  `window is in use`. File-control schema v5 adds `minimize`/`unminimize` and
  reproduced that exact failure in a fresh process. Poche now retains the last
  non-zero physical render extent during the transient zero-sized interval;
  ordinary non-zero resize events still replace it. The same controlled
  minimize -> wait -> restore -> observe sequence then kept the original
  process alive and reported zero DX12 surface errors. Three regression
  variants preserve both the failing zero-axis boundary and the nearest
  passing ordinary resize. Thirty-four focused tests and strict Clippy pass;
  the full two-device Maincloud acceptance also remained green at 58.75 ms
  authority response and 178.31 ms peer observation.
- **Intent audit pass 1 — extraction:** reread the full current request and
  extracted six independent requirements: Escape-menu stand, per-identity
  post-leave history, coherent join/rejoin loading, exact maximize crash,
  persistent app-data logs, and readable terminal failure behavior.
- **Intent audit pass 2 — traceability:** mapped all six requirements to U41,
  this checkpoint, T4.5/T6.3/T7.1, code, focused tests, or the visible puppet
  transcript; the local template reference is recorded generically under the
  path-safety policy.
- **Intent audit pass 3 — adversarial omission:** checked that history means
  rejoin after explicit leave rather than only resume membership, loading
  covers both join and rejoin, stand is absent from the frequent bar, crash
  evidence uses the reported table-to-title resize order, and a clean run does
  not claim windowless rendering proves swapchain behavior.
- **Follow-up extraction pass:** recorded the exact distinction in U42 between
  semantic room readiness and visible 3D readiness; the attached image showed
  standing Alice/Bob, table HUD, and an otherwise black scene.
- **Follow-up traceability pass:** mapped U42 to T4.5, T6.2, the staged-camera
  implementation, rendered-count guard, warmup guard, regression test, and
  two-client capture review.
- **Follow-up adversarial omission pass:** verified that the fix keeps the
  loading screen visible rather than replacing blackness with a second blank
  state, covers the zero-card/zero-seated-player lobby shown by the user, and
  still requires private card/player entities before revealing a resumed deal.
- **Renderer follow-up extraction pass:** preserved the user's exact second
  failure: two standing identities, zero cards, correct HUD, no loading curtain,
  a black 3D region, and eventual recovery without user input.
- **Renderer follow-up traceability pass:** replaced the main-world warmup
  premise in U42 with U43's render-world generation signal and mapped it to the
  focused regression, strict build gates, and two-device acceptance.
- **Renderer follow-up adversarial omission pass:** checked that the gate does
  not use an arbitrary longer delay, cannot accept stale evidence from a prior
  room, supports a legitimate zero-avatar/zero-card lobby, and retains the
  table-scoped camera teardown which fixed leave-to-title resize crashes.
- **Minimize extraction pass:** retained the exact context—untouched title,
  minimize only, `0x0` resize, DX12 `ResizeBuffers` failure, application exit—
  as U44 rather than conflating it with the earlier post-lobby maximize bug.
- **Minimize traceability pass:** mapped U44 to the window extent guard, the v5
  minimize/unminimize puppet actions, three focused tests, and a live
  before-fails/after-survives process receipt.
- **Minimize adversarial omission pass:** verified that the fix neither forces
  the OS window visible nor freezes legitimate resizes, applies before any
  lobby exists, preserves the table-camera teardown, and treats either zero
  axis as non-renderable.

This advances T3.3, T4.1, T4.5, T6.2, T6.3, T6.5, and T7.1. It does not yet
close unavailable-authority retry/backoff, real process restart, final-member
stale-code rejection, or complete multi-round play.

## Authoritative user guidance ledger

| ID | Active guidance | Required plan consequence | Superseded by |
| --- | --- | --- | --- |
| U1 | Create a new `poche-4` worktree for the SpacetimeDB direction. | Use `D:\Repos\Games\poche-4` and a dedicated branch without disturbing `poche-3`. | — |
| U2 | Reorient multiplayer around SpacetimeDB because the current multi-second experience is unacceptable and the centralized trade can buy a more streamlined game. | Make SpacetimeDB the authoritative multiplayer backend for this branch and require latency evidence before a broader migration. | — |
| U3 | `G:\Programming\Repos\bevy_spacetimedb` is reference material only; do not add a hard dependency on it. | Maintain a provenance review and implement the required bridge behavior in Poche-owned code. | — |
| U4 | Any Bevy/SpacetimeDB integration should be a new crate in the Poche workspace that Poche owns. | Add a leaf integration crate with a Poche API and no upstream plugin dependency. | — |
| U5 | Plan workspace structure deliberately to reduce Rust compile times, using Cloud Terrastodon and Cursor Hero as references. | Measure first, centralize versions, isolate heavy targets, keep fast core commands, and adopt only evidence-backed linker/profile settings. | — |
| U6 | Desktop development is the priority; dropping active web support is acceptable. | Windows native is the first supported player target. Do not make web a completion gate or pull the old web spike into the new backend. | — |
| U7 | Bevy stays and should be 0.19.1; it is the renderer/input layer rather than the game authority. | Preserve the existing 0.19.1 pin and keep Bevy/ECS types out of canonical rules and server state. | — |
| U8 | Rust remains the typed source of truth, informed and checked by Alloy, NuSMV, and Scryer Prolog. | SpacetimeDB reducers must call the pure Poche transition boundary; tables and Bevy systems may not become competing rule engines. | — |
| U9 | The MVP should run the game twice: one window creates a lobby and another joins it. | Two real processes and two rendered windows are an end-to-end acceptance surface. | — |
| U10 | Create should produce a copyable, opaque room code; Join should recognize a valid clipboard value without blindly consuming arbitrary clipboard data. | Preserve the main-menu Create/Join flow and valid-shape-gated clipboard convenience; tests must not use the user's clipboard. | — |
| U11 | The lobby/table should visibly contain seats; players and spectators are grounded around the table and can take seats. | Replicate room membership and seating, and render it in both clients before implementing a full game. | — |
| U12 | A card's physical location is position plus rotation; its logical location is `hand`, an indexed deck position, `in play`, and so on. | Store and validate these as separate domains. Formal rules consume logical location; rendering consumes both. | — |
| U13 | Wiggling a card in one hand in one client must be visible in the other client; hand/table camera viewports still map to one 3D world. | Add immediate local prediction, synchronized position and rotation, remote interpolation, and a cross-viewport acceptance test. | — |
| U14 | Moving into the play area is an attempt to play. An out-of-turn or otherwise illegal attempt must not change logical location; forced physical snap-back was only a tentative option. | Make drop intent authoritative and test rejection without logical mutation. Decide rejection animation separately at G8. | — |
| U15 | Hidden card faces must be visible only to authorized player devices, although the centralized authority is trusted and can technically see all hands. | Keep deck/hands private in the module and expose recipient-scoped views; test both positive and negative visibility. | — |
| U16 | A crashed player must be able to rejoin. A name plus secret was suggested as one possible UX, not fixed protocol. | Persist an identity credential per local profile and test process restart. Close the recovery UX at G5 before polishing it. | — |
| U17 | If everyone explicitly leaves a lobby, the lobby is disbanded. | Distinguish explicit leave from connection loss; delete/close room state only when the final membership leaves. | — |
| U18 | A player can have several devices; actions on one should be perceivable on another without same-device Vox IPC. | Model player identity separately from connection/device identity and avoid adding a local-instance control protocol to the product foundation. | — |
| U19 | Automated control should not disturb the OS clipboard. File-driven live control, windowless Bevy targets, captures, and a multi-player puppet are desired. | Reuse the existing Poche control/capture concepts with the new client adapter and require headless two-client evidence. | — |
| U20 | Instrument actual latency. Prior isolated Veilid latency was reasonable, while the integrated card path remained hundreds of milliseconds or seconds. | Timestamp input, reducer send/commit/callback, subscription application, Bevy ingestion, and render observation. Add a measured go/no-go gate. | — |
| U21 | Poche prefers MPL-2.0. | Keep new Poche-authored crates MPL-2.0 and document third-party license boundaries. | — |
| U22 | Do not silently copy or vendor reference code with incompatible or unclear terms. | Record provenance for every borrowed behavior; either reimplement from behavior or preserve Apache attribution if code is adapted. | — |
| U23 | Poche should not require Windows Administrator/UAC. Codex sandbox elevation is a development execution detail, not an application feature. | Bind local services and run both clients as ordinary users; document firewall/network prompts separately from UAC. | — |
| U24 | Reinforcement learning is not part of this goal, but future `State -> Observation -> Action -> Reward` use should not require an architectural rewrite; round score is the meaningful intermediate reward. | Keep pure state/action/observation boundaries and do not couple rollouts to SpacetimeDB, Bevy, sockets, or rendering. | — |
| U25 | The prior formal and oracle work has merit and should inform the rewrite rather than be discarded. | Base from the current branch and preserve formal fixtures, pure reducers, exact projections, diagnostics, and headless evidence where they remain valid. | — |
| U26 | The earlier preferred zero-trust/decentralized design may be conceded for a trusted-host hybrid/centralized MVP. | State explicitly that the SpacetimeDB host is trusted and can see/alter hidden state; do not claim consensus, anonymity, or mental-poker guarantees. | — |
| U27 | The same `poche.exe` should remain a useful graphical application and CLI/automation entry point. | Keep the executable-level UX, but route ordinary player commands through the same SpacetimeDB client contract as the GUI. | — |
| U28 | Player actions should be discoverable in an action surface and, where useful, diegetically grounded in the table. | Preserve one typed action vocabulary with multiple input projections; MVP table manipulation and action buttons invoke the same intent. | — |
| U29 | Build output should stay out of OneDrive and generated artifacts should not inflate Git history. | Keep `target/` ignored and local; commit source/schema/bindings only when reproducibility policy says to, not binaries or runtime databases. | — |
| U30 | The Veilid work remains valuable evidence even if it is no longer the default transport. | Do not delete its branch/history. Keep it out of the default SpacetimeDB dependency graph and document comparative results. | — |
| U31 | Direct browser support is being dropped as a priority, not proven impossible forever. | Mark browser as unsupported for this phase, not architecturally forbidden; keep pure/client boundaries portable where inexpensive. | — |
| U32 | Device-to-device graphical capture may later use the same player/device agency model. | Keep capture commands above the transport adapter. Do not make capture delivery part of the first SpacetimeDB gameplay slice. | — |
| U33 | Codex should be able to manipulate live game instances ad hoc through SFM-style files, request screenshots, and combine many views into one image rather than relying only on fixed puppets or repeated image previews. | Provide fresh named file endpoints, typed actions plus semantic observation, GPU capture, a general control CLI, and one two-device contact sheet. Keep the endpoint on the ordinary player-intent path. | — |
| U34 | Prefer programmer-facing angle units that avoid gratuitous radians/π conversions; turns or half-turns may be clearer and preserve common fractions exactly. | Keep radians at renderer math boundaries. Evaluate an exact bounded turn newtype before the next pose-schema version, together with the 3D orientation representation. | — |
| U35 | Vulkan startup emits repeated wgpu presentation-layout and acquire-semaphore validation errors on the user's Windows/NVIDIA machine. | Default Windows to DX12, retain explicit auto/Vulkan overrides, link the upstream reproductions, and require visible-window evidence rather than treating an offscreen test as proof. | — |
| U36 | Pointer position must not implicitly rotate cards; Q/E should rotate, and a prominent control should cycle common rotation-lock increments such as 45° and 90°. | Separate translation from rotation input, make the table-normal axis visible, add a local snap-mode control, and test exact wrap/cycle behavior. | — |
| U37 | The desktop environment should remain the established 3D table rather than regress to a 2D replacement. | Keep the restored perspective 3D table as the SpacetimeDB presentation boundary; complete the dedicated hand camera and diegetic picking/capture refinements in T4. | — |
| U38 | One installation should retain multiple authenticated identities. Each launched window chooses its identity before any resume prompt; the title shows an identity carousel and its name opens an identity selection/creation screen. | Separate immutable local account ID, mutable display name, protected token, and per-process active selection. Add an identity gate and title selector; scope resume discovery to the chosen identity. | Supersedes T3.1's 2026-09-13 automatic-last-profile assumption. |
| U39 | The Escape menu should contain a nested Options menu with a camera-Y inversion toggle, and the default vertical response should be the opposite of the current behavior. | Model parent/options navigation explicitly, apply the toggle only to local RMB vertical orbit, default it to inverted, and verify navigation plus both response signs. | — |
| U40 | The table needs a public activity history below its player list, and a resumed client must render subscribed players/cards immediately without waiting for another action. Leaving/rejoining may not strand bidding controls against an absent private hand. | Store and subscribe only hidden-information-safe activity; invalidate Bevy reconciliation on table entry; expose rendered counts to the puppet; distinguish disconnect from explicit seat-vacating leave; suppress actions during private-view synchronization. | — |
| U41 | Stand belongs in the Escape menu; an identity needs recent-lobby rejoin after explicit leave; join/rejoin must hide partial 3D hydration behind a loading screen; leaving then maximizing the title must not crash; ordinary crashes need durable app-data logs and a readable terminal. | Move infrequent stand, persist bounded per-identity bearer history, gate table entry on a coherent projection, scope spatial cameras to the table lifecycle, and add durable logging plus a terminal-aware panic receipt. Extend file control so the exact visible resize flow is reproducible. | — |
| U42 | The loading screen must remain visible while the 3D scene is absent; semantic room readiness alone must not reveal a black table with only HUD elements. | Build spatial cameras and subscribed entities behind the opaque loading frontend. Require a present camera, matching rendered counts, and completed scene-update cycles before switching to the table UI. | U43 |
| U43 | Both clients still revealed the HUD over black 3D regions after the U42 update-cycle gate, then recovered without input; loading must follow actual renderer progress. | Replace main-world frame counting with generation-scoped Bevy render-world evidence: a non-empty table-camera opaque phase, compiled referenced pipelines, and a completed render-graph pass. | — |
| U44 | Minimizing an untouched title-screen window must not crash the game. | Preserve the last non-zero render extent across Windows' transient `0x0` minimize event, accept ordinary restore/resize extents, and keep the exact minimize/restore flow available through file control. | — |

## Guidance traceability

| Guidance | Plan coverage | Evidence when complete |
| --- | --- | --- |
| U1 | T0.1, T0.2 | Worktree/branch listing and plan commit |
| U2, U20, U26 | G2, G7, T1.1, T2.5, T3.3, T6.4 | Latency report and documented trust boundary |
| U3, U4, U21, U22 | G3, G9, T1.3, T3.4, T7.1 | Dependency audit, provenance note, license review |
| U5, U29 | G6, T1.1–T1.4, T7.3 | Compile matrix, timings, ignored artifact audit |
| U6, U7, U31 | Scope, T1.3, T3.4, T4, acceptance matrix | Windows native build and explicit unsupported web row |
| U8, U24, U25 | G4, T2.1–T2.4, T5 | Pure tests plus formal/conformance receipts |
| U9, U10, U11 | T2.2, T4.1, T4.2, T6.2, T6.3 | Headless and two-window create/join/seat evidence |
| U12–U14, U28 | G8, T2.5, T4.4, T5.1, T6.4 | Pose/logical tests, rendered manipulation, rejection proof |
| U15 | G10, T2.3, T4.3, T5.3, T6.2 | Sender-scoped subscription and negative privacy tests |
| U16–U18 | G5, G11, T2.2, T3.1, T4.5, T6.5 | Restart, multi-device, explicit-leave, disband receipts |
| U19, U32, U33 | T3.3, T6.1–T6.3 | Ad-hoc file-control transcript, windowless captures, and one composed contact sheet |
| U23 | Constraints, T6.3, T7.1 | Ordinary-user run guide and acceptance record |
| U27 | T3.1–T3.3, T4.1, T7.1 | GUI and CLI invoke the same client adapter |
| U30 | Scope, T1.3, T7.2 | Default dependency graph excludes Veilid; history retained |
| U34–U37 | T4.3, T4.4, T6.2–T6.4 | Exact angle tests, visible Q/E/snap behavior, real 3D two-window captures, and a clean Windows DX12 startup log |
| U38 | G5, T3.1, T4.1, T4.5, T6.2, T6.3 | Multi-account identity-gate captures plus Alice/Bob and same-Alice two-process receipts |
| U39, U40 | T2.2, T2.3, T4.5, T6.2, T6.5 | Nested-options captures, converged public activity, pre-action resume-scene counts, and coherent post-leave projection |
| U41 | T3.3, T4.1, T4.5, T6.2, T6.3, T6.5, T7.1 | Recent-lobby title view, loading readiness tests, durable panic log, and a live table -> leave -> title -> maximize puppet receipt |
| U42 | T4.5, T6.2 | Exact unseated-scene regression plus reviewed two-client captures in which every revealed table surface contains the 3D scene |
| U43 | T4.5, T6.2 | Render-world generation regression, strict desktop tests/Clippy, and two-device acceptance that cannot complete unless the GPU image target reports readiness |
| U44 | T4.5, T6.3, T7.1 | Zero/non-zero extent unit variants plus a real DX12 title minimize -> restore -> observe receipt with the same process still alive and no surface error |

## Purpose

Produce a desktop-first Poche vertical slice in which a local SpacetimeDB
instance is the trusted multiplayer authority and two ordinary `poche.exe`
processes can create/join a room, take seats, receive private hands, and move a
card smoothly in a shared physical world. The server validates durable room and
logical game transitions through the existing pure Rust rules. The client
predicts high-frequency physical manipulation locally and reconciles it from
subscribed server state.

This phase is successful only if it is easier to understand, test, and run than
the current transport path and if measured same-host latency supports visible
card wiggling. “Uses SpacetimeDB” alone is not success.

## Scope

### In scope

- A `poche-4` worktree and `spacetimedb` branch based on the current completed
  formal/model-checking work.
- A pinned local SpacetimeDB development toolchain and repeatable doctor/start/
  publish/generate workflow.
- Poche-owned workspace crates that separate the server WASM module, native
  client adapter, generated bindings, and Bevy integration.
- Trusted-server room, identity, membership, presence, seats, private hands,
  logical transitions, command receipts, and latest physical pose state.
- Two native Windows clients using the official Rust SDK directly.
- Local prediction, bounded pose publication, remote interpolation, and
  authoritative drop intent.
- Crash/reconnect, multiple devices per identity, explicit leave, and final
  member room disbanding.
- Pure-core, adapter, module, privacy, headless puppet, real-process, formal
  conformance, and latency evidence.
- Documentation of licensing, trust, deployment, compile-time, and migration
  boundaries.

### Out of scope

- Browser or WASM player support, Datastar/Axum gateway work, mobile packaging,
  Makepad, Vulkan/ash replacement, or polished production art.
- Peer-to-peer consensus, anonymity, untrusted-host card secrecy, mental poker,
  threshold shuffle, or Byzantine recovery.
- Hosted matchmaking, accounts, payments, anti-abuse, production operations,
  or a public SpacetimeDB deployment.
- Full tabletop-simulator freedom, physics, collision-based rule inference,
  voice chat/TTS, shared cursors, arbitrary object spawning, or 3D score-sheet
  skeuomorphism. The data boundaries must permit later expansion.
- RL training. Direct pure rollouts remain supported and transport-free.
- Deleting the Veilid, web, formal, governance, capture, or RL research.
- Depending on or publishing a fork of `bevy_spacetimedb`.
- Sending a durable transaction for every rendered mouse frame.

## Established foundation

| Foundation | Verified evidence on 2026-09-12 | Consequence |
| --- | --- | --- |
| New worktree | `git worktree add -b spacetimedb D:\Repos\Games\poche-4 model-checking` created `poche-4` at `f0b3717`. | Implementation happens only in `poche-4`. |
| Existing workspace | Root `Cargo.toml` uses resolver 3, Rust 1.96, MPL-2.0, 26 packages, and centrally pinned dependencies. | Improve rather than replace the workspace. |
| Bevy | Workspace pins Bevy `=0.19.1` with a reduced feature set. | No Bevy upgrade is part of this phase. |
| Pure session rule boundary | `poche-session/src/machine.rs` exposes authorization, event decision, and pure apply behavior around `SessionState`. | SpacetimeDB reducers adapt to this boundary. |
| Logical game authority | README and current source identify `GameEnvironment`/session reducers as rules authority. | Neither tables nor ECS systems decide legal play independently. |
| Physical/logical separation | `poche-player-client/src/physical.rs` and `poche-runtime/src/device_client.rs` carry signed pose state without incrementing logical game state. | Reuse the concept, not the current request/response carrier. |
| Formal evidence | Alloy, NuSMV, Prolog, finite Rust, conformance, and spatial tracks are present and documented. | Update affected lifecycle scopes; do not restart formal modeling from zero. |
| Bevy evidence | Native UI, hand/table cameras, windowless capture, rendered input probes, and live-control files exist. | Adapt the connection edge while preserving renderer/input tests where possible. |
| Current transport evidence | `PLAN-6-DESKTOP-VEILID.md` records integrated latency and idle-write findings. | Use as comparative baseline, not as proof for SpacetimeDB. |
| SpacetimeDB source | `G:\Programming\Repos\SpacetimeDB` is at product version 2.10.1 and uses Rust 1.93 in the inspected checkout. | Pin the app-facing CLI/SDK/module versions together. |
| SpacetimeDB client model | Current docs state that `DbConnection` is a persistent WebSocket, subscriptions maintain a local cache, and updates are ordered atomically per committed transaction. | Use subscriptions rather than follow-up snapshots/polling. |
| Recipient views | Current views support caller-aware `ViewContext` keyed by sender identity. | Spike private hand delivery through a sender-scoped view. |
| Identity/reconnect | Current docs return identity/token on connect, recommend saving the token, distinguish `Identity` from `ConnectionId`, and require application reconnection. | Persist a profile token; model multiple connections separately. |
| Reference Bevy adapter | `G:\Programming\Repos\bevy_spacetimedb` is Apache-2.0, targets older Bevy/SDK versions, and bridges SDK callbacks to Bevy messages through channels. It also leaks a connection and contains unsafe delayed-connect casts. | Reimplement only the small callback/channel pattern with owned lifetimes and no unsafe code. |
| Compile references | Cloud Terrastodon uses workspace crates, optional heavy entrypoint features, and stable `rust-lld`. Cursor Hero uses workspace dependencies, many leaf crates, dev opt-level 1/dependency opt-level 3, and historical nightly rustflags. | Prefer Cloud's stable pattern; benchmark before copying profiles and do not adopt Cursor Hero's nightly flags. |
| Local tool availability | The user installed `C:\Users\Teamy\AppData\Local\SpacetimeDB\spacetime.exe`. Direct execution reports CLI/library 2.10.0 at commit `baca5cdf77577ed4e3f30da48a5158189c4ea43f`; `spacetime version list` marks 2.10.0 current. The source checkout inspected above identifies 2.10.1, so they are not assumed interchangeable. | T1.2 must choose one coherent CLI/module/SDK version, record the mismatch resolution, and document invocation without relying on sandbox PATH inheritance. |

## Confirmed constraints

1. New Poche-authored code remains MPL-2.0 and inherits `unsafe_code =
   "forbid"` unless a separately reviewed platform boundary proves unavoidable.
2. `poche-domain`, `poche-model`, `poche-environment`, `poche-session`, and
   their fast tests must not depend on Bevy, the SpacetimeDB SDK, generated
   client bindings, sockets, or a running database.
3. The server module may depend on the official SpacetimeDB module bindings;
   the native client adapter may depend on the official Rust client SDK. The
   Bevy bridge depends on the client adapter, not vice versa.
4. `G:\Programming\Repos\bevy_spacetimedb` must never appear in `Cargo.lock`,
   `cargo metadata`, a path dependency, a git dependency, or vendored source.
5. The SpacetimeDB host is trusted for this phase. It owns authoritative
   ordering and can inspect hidden rows. User-facing documentation must say so.
6. Physical pose does not confer logical ownership and does not play a card by
   itself. A durable typed drop/action is required for logical transition.
7. The initiating client renders its predicted pose immediately. Server
   rejection cannot retroactively make the local interaction feel blocked.
8. Explicit leave is distinct from disconnect. A room survives temporary loss
   while memberships remain and disbands when the final membership explicitly
   leaves.
9. Tests do not read or write the human's OS clipboard. Production copy/paste
   remains a UI convenience.
10. Running the game, local database, tests, and puppets must not require
    Windows Administrator/UAC.
11. Build artifacts, database directories, captures, timing HTML, and generated
    runtime logs remain ignored. Source schema and any deliberately committed
    generated bindings are reviewed text.

## Architecture contract

```text
                         pure / fast / transport-free
  poche-domain -> poche-model -> poche-environment -> poche-session/runtime
                                        |
                                        | DTO conversion + pure transition
                                        v
                  poche-spacetimedb-module  (wasm target)
                            |
                   generated client API
                            v
                  poche-spacetimedb-client  (native, no Bevy)
                            |
                    typed channel/events
                            v
                  poche-bevy-spacetimedb    (Bevy 0.19.1 leaf)
                            |
                            v
                     poche-native-ui / poche.exe
```

The target split is structural, not aesthetic:

- server module and native SDK require different target/runtime dependencies;
- generated bindings change with schema and should not recompile pure rules;
- Bevy is heavy and should remain at a dependency leaf;
- headless client/module tests should not link a renderer;
- another renderer can later consume `poche-spacetimedb-client`.

Initial owned crate names:

| Crate | Owns | Must not own |
| --- | --- | --- |
| `poche-spacetimedb-module` | Tables, views, reducers, lifecycle hooks, server DTO conversion | Bevy UI, client token storage, duplicated game legality |
| `poche-spacetimedb-bindings` or an isolated generated module | Reproducible generated Rust client API and schema fingerprint | Handwritten business logic |
| `poche-spacetimedb-client` | Connection lifecycle, profile token, subscriptions/cache, reducer calls, typed client events, latency stamps | Bevy ECS/rendering |
| `poche-bevy-spacetimedb` | Bevy Plugin, Resources, Messages/Events, frame-safe ingestion and command egress | Database schema, pure rules, unsafe global connection lifetime |

G4 decides whether generated bindings justify their own crate after the first
codegen measurement. The other three boundaries are required.

### State durability classes

| Class | Examples | SpacetimeDB representation | Client behavior |
| --- | --- | --- | --- |
| Durable logical | room, membership, seats, round/game state, command receipt, explicit leave | Private canonical rows plus public/recipient projection rows, atomically changed by reducers | Subscribe, never speculate durable completion |
| Durable secret | deck order, card identity, hand ownership | Private tables inaccessible to ordinary subscriptions | Receive only through sender-scoped view |
| Latest physical | card position/rotation, owner, generation, sequence | One latest-value row per card; bounded reducer rate; exact design closed by G7 | Predict locally; interpolate remote; discard stale sequences |
| Presence | active `ConnectionId` values, last activity, device labels | Lifecycle-maintained or replaceable rows, not player identity | May flicker/recover without deleting membership |
| Local-only presentation | camera, hover, drag affordance, speculative samples, animation progress | Not stored | Render every frame |

## Design gates

| Gate | Status | Required decision | Acceptance consequence |
| --- | --- | --- | --- |
| G1 Base strategy | Closed | Base `poche-4` on `model-checking` at `f0b3717` so formal/core work is retained. | Worktree and branch point are recorded. |
| G2 Authority | Closed | SpacetimeDB is a trusted centralized authority for this branch; no consensus claim. | Threat model and docs name host visibility/control. |
| G3 Bevy integration ownership | Closed | Build `poche-bevy-spacetimedb` in-tree; no `bevy_spacetimedb` dependency. | Metadata/license audit fails if the reference package enters the graph. |
| G4 Generated bindings | Open in T1.4 | Decide dedicated crate vs isolated module based on codegen workflow and rebuild fan-out. Generated code must be reproducible and schema-fingerprinted. | A clean generation check produces no diff and pure crates do not rebuild. |
| G5 Identity/recovery UX | Open in T2.2/T3.1 | Working assumption: protected persistent Spacetime token per named local profile; display name is mutable metadata, not authority. Decide whether a user-entered recovery secret is needed later. | Same profile reconnects after process restart; a new profile cannot impersonate it by reusing the name. |
| G6 Compile strategy | Open in T1.1/T1.2 | Measure current warm/clean package builds, then choose `default-members`, stable `rust-lld`, profile overrides, and CI cache keys. | Fast-core commands avoid Bevy/SDK; changes improve measured iteration or are not adopted. |
| G7 Physical update carrier/rate | Open in T2.5/T6.4 | Compare bounded latest-row reducer updates (starting at 15 Hz) with any supported transient/event mechanism. Do not add a second networking stack before measuring. | Same-host remote pose p95 target is below 100 ms; otherwise stop broader migration and diagnose. |
| G8 Rejected drop presentation | Open in T4.4 | Preserve physical/logical separation. Choose retain, snap, or tween-back UX without treating it as rule state. | Illegal attempt leaves logical location unchanged and gives visible feedback. |
| G9 Licensing/distribution | Open in T1.2/T7.1 | Confirm the exact CLI/server, client SDK, and module-binding licenses for the pinned version and acceptable deployment. Current 2.10.1 root/client is BSL 1.1 with a one-production-instance additional grant and 2031-09-08 change date; module bindings are Apache-2.0. | No release claim or distribution step proceeds with an inaccurate license statement. |
| G10 Private hand delivery | Open in T2.3 | Prove sender-scoped views do not leak private rows through generated bindings, broad subscriptions, callbacks, logs, diagnostics, or captures. | Two identities see their own faces and never the other's; server trust remains disclosed. |
| G11 Room lifetime | Closed at product level; implementation proof pending | Explicit final-member leave disbands. Disconnect only removes presence and permits reconnect. | Restart and final-leave tests cover both sides. |
| G12 Canonical storage mapping | Open in T2.1 | Prefer a private canonical room snapshot/event boundary plus normalized public/recipient rows, with lossless DTO conversions. Avoid making normalized projections the only rule state until atomic reconstruction is proven. | Round-trip and reducer conformance tests show one logical result. |
| G13 Local orchestration | Open in T1.2/T6.1 | Add `poche-xtask spacetimedb` commands after inspecting the pinned CLI help; do not rely on undocumented shell state. | Doctor/start/publish/generate/test work from a fresh checkout with explicit prerequisites. |

## Source and implementation references

### Poche

- `D:\Repos\Games\poche-4\Cargo.toml` — current workspace/version/feature root.
- `D:\Repos\Games\poche-4\crates\poche-session\src\machine.rs` — pure
  authorization/decide/apply boundary.
- `D:\Repos\Games\poche-4\crates\poche-runtime\src\device_client.rs` —
  projection/action and current physical-pose authority behavior.
- `D:\Repos\Games\poche-4\crates\poche-player-client\src\physical.rs` —
  position/rotation, device generation, and sequence concepts.
- `D:\Repos\Games\poche-4\crates\poche-native-ui\src\native_live.rs` and
  `desktop_menu.rs` — rendered table and menu connection edge.
- `D:\Repos\Games\poche-4\crates\poche-native-ui\src\desktop_menu\live_control.rs`
  — bounded file control and observations.
- `D:\Repos\Games\poche-4\PLAN-6-DESKTOP-VEILID.md` — completed baseline,
  recovery, latency, and acceptance evidence.
- `D:\Repos\Games\poche-4\PLAN-5-LIVE-CONTROL-PUPPETS.md` — capture/device/
  automation boundaries.

### SpacetimeDB

- `G:\Programming\Repos\SpacetimeDB\LICENSE.txt` — inspected 2.10.1 BSL
  parameters and subdirectory-specific-license rule.
- `G:\Programming\Repos\SpacetimeDB\crates\bindings\LICENSE` — Apache-2.0
  module binding license pointer.
- `G:\Programming\Repos\SpacetimeDB\sdks\rust\LICENSE` — root BSL license
  pointer for the client SDK.
- `G:\Programming\Repos\SpacetimeDB\docs\docs\00200-core-concepts\00600-clients\00300-connection.md`
  — persistent WebSocket, token saving, identity/connection distinction, and
  application-managed reconnect.
- `G:\Programming\Repos\SpacetimeDB\docs\docs\00200-core-concepts\00400-subscriptions.md`
  and `00200-subscription-semantics.md` — local cache and atomic ordered update
  semantics.
- `G:\Programming\Repos\SpacetimeDB\docs\docs\00200-core-concepts\00200-functions\00500-views.md`
  — sender-scoped and anonymous views.
- `G:\Programming\Repos\SpacetimeDB\docs\docs\00200-core-concepts\00200-functions\00300-reducers\00400-reducer-context.md`
  — sender identity, connection ID, deterministic RNG, and reducer context.

### Reference-only integration and workspace examples

- `G:\Programming\Repos\bevy_spacetimedb` — Apache-2.0 behavioral reference,
  not a dependency. Useful concepts: background `run_threaded` connection,
  callback-to-channel bridge, lifecycle messages, table insert/update/delete
  messages. Do not inherit its leaked connection, unsafe delayed connect, API
  shape, or version pins automatically.
- `D:\Repos\Azure\Cloud-Terrastodon\Cargo.toml` and `.cargo\config.toml` —
  workspace-wide dependencies, optional heavy entrypoint, stable `rust-lld`.
- `D:\Repos\Games\Cursor-Hero\Cargo.toml` and `.cargo\config.toml` — historic
  leaf/plugin separation and dev dependency optimization. Its Bevy 0.12 fork,
  extreme microcrate count, and nightly `-Z` flags are not suitable defaults.

## Execution order

```text
T0 branch/plan
  -> T1 toolchain + compile boundaries + codegen
      -> T2 module schema/reducers/privacy/pose
          -> T3 native client + Bevy bridge
              -> T4 rendered two-window gameplay
                  -> T5 formal and adapter conformance
                      -> T6 headless/real-process/recovery/latency acceptance
                          -> T7 docs, dependency retirement, CI, release receipt
```

T5 pure/model work may begin beside late T4 rendering only after T2's canonical
mapping is closed. T6 latency is measured narrowly during T2/T3 as well as at
end to prevent a visually complete but unresponsive architecture.

## Phase 0 — Reorient safely

### [x] T0.1 Create the isolated worktree and branch

**Completion notes:**

- Verified `D:\Repos\Games\poche-4` did not exist and branch `spacetimedb` was
  free.
- Created it from clean `model-checking` revision
  `f0b371727301730f9db88ad53defa9d66c684269`.
- Existing `Poche`, `poche-2`, and `poche-3` worktrees were unchanged.

**Validation:**

```pwsh
git -C D:\Repos\Games\poche-3 worktree list --porcelain
git -C D:\Repos\Games\poche-4 status --short --branch
git -C D:\Repos\Games\poche-4 log -1 --format="%H %s"
```

**Completion criteria:** `poche-4` is an independent clean worktree on the
named branch at the recorded base.

### [x] T0.2 Record the reorientation contract and three-pass intent audit

**Completion notes:**

- Inspected the current Poche workspace, completed plans, relevant rules,
  physical-pose and native UI boundaries.
- Inspected current SpacetimeDB docs/licenses, the reference Bevy adapter, and
  both compile-time workspace examples.
- Recorded active, tentative, superseded, and deferred guidance in this file.

**Validation:**

```pwsh
git -C D:\Repos\Games\poche-4 diff --check
rg -n "^\| U[0-9]+" D:\Repos\Games\poche-4\PLAN-7-SPACETIMEDB-REORIENTATION.md
rg -n "bevy_spacetimedb|Cloud-Terrastodon|Cursor-Hero|SpacetimeDB|Intent audit" D:\Repos\Games\poche-4\PLAN-7-SPACETIMEDB-REORIENTATION.md
```

**Completion criteria:** A fresh agent can identify the next task, design
boundaries, unresolved gates, exact references, and acceptance evidence
without the conversation.

## Phase 1 — Establish a reproducible, fast workspace boundary

### [~] T1.1 Measure current and proposed compile surfaces

**Work:**

- Record package/dependency graphs and warm build times for pure session,
  native UI, and full workspace without deleting the shared `target` tree.
- Use an isolated ignored target directory for a clean-build comparison instead
  of `cargo clean`.
- Identify which current dependencies make pure edits compile Bevy, Burn,
  Veilid, or the future SpacetimeDB SDK.
- Define an intentional `default-members` set or explicit fast aliases only if
  it measurably improves the common command without hiding release coverage.

**Validation:**

```pwsh
cargo metadata --no-deps --format-version 1
cargo build --locked -p poche-session --timings
cargo build --locked -p poche-native-ui --timings
$env:CARGO_TARGET_DIR = "target\compile-baseline"
cargo build --locked -p poche-session --timings
Remove-Item Env:CARGO_TARGET_DIR
```

**Completion criteria:** A checked-in compile matrix identifies fast and heavy
surfaces, includes measured evidence, and chooses `default-members`/aliases
without weakening `--workspace` validation.

### [~] T1.2 Pin and document the SpacetimeDB toolchain and licenses

**Work:**

- Select one reviewed SpacetimeDB version as the project prerequisite. The
  installed launcher currently provides 2.10.0 while the local source checkout
  identifies 2.10.1; resolve that mismatch deliberately rather than mixing
  generated bindings or protocols across them.
- Teach the doctor command to discover the installed launcher at
  `C:\Users\Teamy\AppData\Local\SpacetimeDB\spacetime.exe` when ordinary PATH
  discovery is unavailable, without committing a user-specific path as the
  only supported configuration.
- Pin matching module and Rust SDK versions in the lockfile.
- Inspect `spacetime help` for exact local start, publish, generate, logs, and
  database-reset commands before wrapping them.
- Add `poche-xtask spacetimedb doctor` and local orchestration that stores
  runtime data under ignored `target/spacetimedb/`.
- Close G9 for development and document what remains before production.
- Test stable `rust-lld` on Windows. Do not adopt nightly `-Z` flags.

**Validation:**

```pwsh
spacetime --version
cargo run -p poche-xtask -- spacetimedb doctor
cargo metadata --format-version 1 --locked
cargo test -p poche-session --locked
```

**Completion criteria:** A fresh ordinary-user shell can diagnose exact
versions/licenses and start the local prerequisite without global hidden state;
fast pure tests remain independent.

### [x] T1.3 Add the three required Poche-owned integration crates

**Work:**

- Add `poche-spacetimedb-module`, `poche-spacetimedb-client`, and
  `poche-bevy-spacetimedb` with minimal compiling APIs.
- Keep module WASM dependencies, native SDK dependencies, and Bevy dependencies
  in their respective leaves.
- Make `poche-native-ui` opt into the new bridge; do not make the workspace's
  pure default surface compile it.
- Remove Veilid from the new desktop default feature only after the new empty
  connector compiles; retain its crate and explicit research features.
- Add an automated dependency/provenance assertion excluding the reference
  `bevy_spacetimedb` package/path.

**Validation:**

```pwsh
cargo check -p poche-spacetimedb-module --locked
cargo check -p poche-spacetimedb-client --locked
cargo check -p poche-bevy-spacetimedb --locked
cargo test -p poche-session --locked
cargo metadata --format-version 1 --locked | rg "bevy_spacetimedb"
```

**Completion criteria:** Each layer compiles independently, the pure core has no
new heavy dependency, and metadata contains no reference-plugin dependency.

### [~] T1.4 Make schema generation reproducible and isolated

**Work:**

- Create the smallest schema and run pinned Rust client codegen.
- Measure whether a dedicated `poche-spacetimedb-bindings` crate reduces rebuild
  fan-out versus a private module in `poche-spacetimedb-client`.
- Close G4 and commit either the generated Rust or an exact generation rule
  with schema fingerprint; CI must detect drift.
- Never hand-edit generated files.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb generate --check
git diff --exit-code -- crates/poche-spacetimedb-client crates/poche-spacetimedb-bindings
cargo check -p poche-spacetimedb-client --locked
```

**Completion criteria:** Running generation twice is idempotent, schema/client
drift fails clearly, and editing pure rules does not regenerate bindings.

## Phase 2 — Put Poche rules behind a SpacetimeDB module

### [~] T2.1 Define canonical server DTOs and lossless pure-core conversion

**Work:**

- Close G12 with explicit DTOs for room IDs, player principals, devices,
  membership, seats, game/logical state, command IDs, receipts, cards, and
  projections.
- Keep SpacetimeDB derives/macros out of pure crates.
- Convert at the module boundary and test round trips, bounds, stable ordering,
  stale revision behavior, and error mapping.
- Reuse stable Poche identifiers where valid instead of inventing database IDs
  as a second identity system.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module dto --locked
cargo test -p poche-session --locked
cargo test -p poche-environment --locked
```

**Completion criteria:** Every module DTO used by a reducer has a tested mapping
to/from the pure contract, and no database row bypasses rule validation.

### [~] T2.2 Implement room, membership, identity, seating, and lifetime

**Work:**

- Add opaque room creation/join lookup, profile registration, membership,
  multiple connections/devices, take/release seat, ready/countdown minimum,
  reconnect, explicit leave, and final-member disband reducers.
- Authorize from reducer sender identity, not display name or caller-provided
  principal.
- Distinguish connection lifecycle from durable membership.
- Close the working identity/recovery assumptions in G5 and G11.
- Return correlated typed receipts for accepted, denied, duplicate, stale, and
  missing-room calls.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module room --locked
cargo test -p poche-spacetimedb-client reconnect --locked
cargo run -p poche-xtask -- spacetimedb scenario room-lifecycle --clients 2
```

**Completion criteria:** Two identities create/join/seat; restart preserves the
same member; a same-name new identity cannot impersonate; disconnect preserves
membership; final explicit leave removes the room and rejects stale join codes.

### [~] T2.3 Prove private hands and recipient-scoped projections

**Work:**

- Keep deck order, card face, and hand ownership in private tables.
- Expose common public room/table data through anonymous/shared subscriptions
  where possible and own-hand data through a sender-scoped view.
- Avoid `subscribe_to_all_tables` and broad diagnostic serialization.
- Deal deterministically from reducer RNG with a recorded seed/receipt for
  reproducible tests, while documenting that the trusted host controls it.
- Verify multiple devices with one identity receive the same authorized hand.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module privacy --locked
cargo test -p poche-spacetimedb-client privacy --locked
cargo run -p poche-xtask -- spacetimedb scenario private-deal --clients 2 --seed 1
```

**Completion criteria:** Alice sees Alice's faces, Bob sees Bob's, neither
receives the other's in cache/events/logs/captures, and the module alone sees
the complete deck.

### [~] T2.4 Route durable game actions through the pure transition engine

**Work:**

- Adapt bid/play and the minimum room commands from authenticated reducer input
  to the existing typed action, `SessionState`, and `GameEnvironment`.
- Apply resulting rows and receipt atomically; no follow-up full snapshot is
  required after a successful reducer callback/subscription delta.
- Make idempotency keys and expected logical revisions explicit.
- Compare reducer outcomes with direct pure execution for accepted and denied
  actions.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module reducer_conformance --locked
cargo test -p poche-conformance --locked
cargo run -p poche-xtask -- spacetimedb scenario one-trick --clients 2 --seed 1
```

**Completion criteria:** The database adapter and direct pure reducer produce
the same logical state/events/denial for the scoped lifecycle and one trick.

### [~] T2.5 Add bounded latest-value physical pose updates

**Work:**

- Add card pose rows containing room/card, controlling identity/device,
  generation, monotonic sequence, position, rotation, logical anchor, and
  server commit time.
- Validate bounds and current control authority without changing logical game
  revision.
- Coalesce client samples and begin at 15 Hz while rendering locally every
  frame. Treat remote samples as a piecewise curve for interpolation.
- Ignore stale/reordered samples and keep a reliable final pose/drop boundary.
- Instrument every latency stage and close G7 with a narrow two-client spike
  before continuing UI work.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module pose --locked
cargo test -p poche-spacetimedb-client pose --locked
cargo run -p poche-xtask -- spacetimedb latency --clients 2 --samples 500 --json target/spacetimedb/pose-latency.json
```

**Completion criteria:** The initiating model updates immediately, the peer
receives monotonic latest poses with same-host p95 below 100 ms after warm-up,
idle clients write nothing, and pose traffic does not mutate logical state.

## Phase 3 — Build the native client and Bevy bridge

### [~] T3.1 Own connection, token persistence, reconnect, and shutdown

**Work:**

- Implement `poche-spacetimedb-client` around generated `DbConnection` with
  explicit owned lifetime and background execution.
- Persist tokens through the existing protected-profile abstraction where
  possible; never print them in ordinary logs or room codes.
- Recreate `DbConnection` with bounded backoff after interruption.
- Expose identity, connection ID, module/schema version, connection state, and
  actionable error diagnostics.
- Support two local profiles and multiple connections for one profile without
  conflating display names.
- Add a device-local identity vault/index containing only immutable account ID,
  user-facing account label, and authority metadata. Store each actual token in
  the protected SDK credential store under authority plus immutable account ID;
  renaming an account or room display name must not change identity.
- Make active identity a per-process selection. Multiple windows read the same
  account catalogue but may choose different accounts without overwriting a
  shared global “current account.” Retained credentials do not require every
  account to keep a background connection open.
- Treat each SDK connection as presence for one device, keyed by connection ID.
  The member is connected while at least one such row is live; disconnecting
  one of two devices must not mark their shared player offline.
- Limit the current multi-device acceptance claim to two processes that can
  access the same protected local profile. Enrolling another physical machine
  requires an explicit credential-transfer or device-enrollment design and is
  not silently included in this milestone.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-client connection --locked
cargo test -p poche-spacetimedb-client reconnect --locked
cargo run -p poche-xtask -- spacetimedb scenario reconnect --clients 2
```

**Completion criteria:** Start/stop/restart is leak-free, the credential is
protected and redacted, reconnection restores the correct identity, and clean
shutdown joins its worker.

### [~] T3.2 Convert SDK subscriptions into a typed local client model

**Work:**

- Register only required tables/views.
- Translate insert/update/delete, reducer outcome, subscription-applied/error,
  connect, and disconnect callbacks into bounded typed events.
- Maintain one exact local model with atomic transaction application; Bevy must
  not query the SDK from arbitrary systems.
- Apply backpressure/coalescing for pose rows without dropping durable room or
  game changes.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-client subscription --locked
cargo test -p poche-spacetimedb-client backpressure --locked
```

**Completion criteria:** A deterministic callback transcript reconstructs the
same local model, pose bursts remain bounded, and durable changes are ordered
and lossless.

### [~] T3.3 Expose one command/observation contract to GUI, CLI, and puppets

**Work:**

- Map the existing typed action catalogue to reducer calls without NDJSON or
  forced snapshot round trips in the hot path.
- Return correlated outcomes and timestamps suitable for UI, CLI JSON/NDJSON,
  diagnostics, and latency reports.
- Preserve local action derivation from the viewer-safe projection.
- Keep file live-control above this adapter so it exercises ordinary UI input
  or ordinary client commands rather than mutating database state.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-client action_contract --locked
cargo run -p poche-cli -- --output json room status
cargo run -p poche-xtask -- spacetimedb scenario cli-gui-parity --clients 2
```

**Completion criteria:** GUI, CLI, and automation see the same available
actions and receipts for the same projection; no privileged testing ingress
changes state.

### [x] T3.4 Implement the Poche-owned Bevy 0.19.1 bridge

**Work:**

- Create a small `Plugin` with Resources and typed Messages/Events for client
  state, lifecycle, transactions, action results, and outgoing intents.
- Drain a bounded channel on the Bevy schedule and publish immutable frame
  snapshots; send commands through a narrow handle.
- Own and shut down the worker without `Box::leak`, global statics, unsafe
  `World`/`App` casts, or reflection over generated types.
- Record a behavioral comparison to `bevy_spacetimedb` and provenance.

**Validation:**

```pwsh
cargo test -p poche-bevy-spacetimedb --locked
cargo test -p poche-bevy-spacetimedb --no-default-features --locked
cargo tree -p poche-bevy-spacetimedb | rg "bevy_spacetimedb"
```

**Completion criteria:** A minimal headless Bevy App connects, receives a
transaction, emits a reducer intent, and disconnects cleanly with no unsafe
code or reference dependency.

## Phase 4 — Deliver the two-window card-table MVP

### [~] T4.1 Reconnect the main menu to create/join flows

**Work:**

- Add a launch identity gate that lists locally retained identities and can
  create a new one. Do not connect or offer room resumption until this window
  explicitly chooses an identity.
- Keep the game-like full-window title menu with Create lobby, Join lobby,
  valid-shape-gated paste, and explicit status/errors. Show the chosen identity
  at the top with left/right account cycling; activating its name opens the
  identity selection/creation screen.
- Treat identity/account label, SpacetimeDB principal/token, and mutable room
  display name as separate fields. Remove the current behavior in which typed
  display name doubles as the credential-profile key.
- Generate an opaque code that identifies the authoritative module/room without
  embedding player secrets.
- Make Copy a human convenience; use direct observed text in automated tests.
- Run both windows as ordinary users with no UAC requirement.

**Validation:**

```pwsh
cargo test -p poche-native-ui desktop_menu --locked
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario create-join
```

**Completion criteria:** Two windows independently choose stored identities;
one creates and displays/copies a code and the other joins it. The title always
identifies the acting account, switching it cannot impersonate or implicitly
leave, and invalid/unknown/stale codes give useful errors without changing
identity.

### [~] T4.2 Render shared lobby membership, spectators, and seats

**Work:**

- Drive table avatars/capsules, spectators, seats, ready state, and room status
  from subscribed client state.
- Allow both clients to take/release distinct seats through diegetic table
  targets and the complete action surface.
- Show connection state separately from durable membership.

**Validation:**

```pwsh
cargo test -p poche-native-ui lobby_scene --locked
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario seats
```

**Completion criteria:** Both clients converge on membership and seats without
manual refresh, and denied seat races appear as explicit receipts.

### [x] T4.3 Deal and render recipient-private hands

**Work:**

- Start the smallest playable two-player deal using the authoritative reducer.
- Render each own hand through its dedicated viewport/camera while keeping the
  shared card object grounded in one world.
- Render peer card backs/counts but never peer faces.
- Preserve existing card glyph/Slug improvements and windowless capture.

**Validation:**

```pwsh
cargo test -p poche-native-ui private_hand --locked
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario private-deal --captures
```

**Completion criteria:** Captures and semantic observations prove distinct own
hands and negative peer-face visibility for both clients.

### [~] T4.4 Make card manipulation immediate, shared, and rule-aware

**Work:**

- Reuse the current physical/logical and hand/table viewport mapping.
- Update the grabbed card locally each frame, publish coalesced position and
  rotation, and interpolate the peer representation.
- On release into a logical zone, send one typed drop/play intent. Reconcile
  accepted logical movement; visibly explain rejected movement and close G8.
- Expose the equivalent card command in the action surface.
- Keep planned deal/snap animations as explicit curves; do not pretend a future
  arbitrary mouse path is known.

**Validation:**

```pwsh
cargo test -p poche-native-ui card_drag --locked
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario shared-card-wiggle --captures
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario legal-and-denied-drop --captures
```

**Completion criteria:** Local drag never waits for the network, the second
window sees smooth position/rotation changes, cross-viewport dragging
preserves one object, legal drop changes logical state, and denied drop does
not.

### [~] T4.5 Make restart and room departure understandable

**Work:**

- Show reconnecting/rejoined/disbanded states without silently jumping views.
- Closing a window, Alt-F4, a process crash, or a temporary network loss is a
  disconnect, never an implicit leave. Preserve durable membership, seat,
  active-room focus, private-hand authorization, and game state.
- On reopen, show the identity gate before any room-specific prompt. When this
  window selects an existing account, recover that account's protected
  SpacetimeDB token, connect, and wait for the initial sender-scoped
  subscription. Only if the chosen identity still has active-room membership
  show a rejoin-lobby prompt; accepting it rebuilds the table with the same seat
  and authorized private view without the join code or a replacement player.
- Identity arrows on the title screen perform the same explicit selection flow.
  Switching from Alice to Bob disconnects Alice in that process, connects Bob,
  and only then may show Bob's own resume prompt. It does not leave Alice's
  durable room membership.
- If the authority is unavailable, remain in an explicit reconnecting screen
  with retry/backoff and a deliberate return-to-profile-selection action. If
  the authority exists but the room was disbanded, clear the stale resume hint
  and show “This lobby has ended” before returning to the main menu.
- Permit explicit leave; final leave disbands and presents a terminal screen.
- Retain a bounded recent-lobby list per local identity so an explicit leaver
  can rejoin by capability from the title without being mistaken for a
  still-authorized resume. Gate create/join/resume table entry behind a loading
  state until private and spatial projections agree.
- Keep table/hand cameras scoped to the table screen so a later title-screen
  resize cannot retain or reconfigure an in-use swapchain.
- Exercise a second device for one identity and prove it does not create
  another player or vote.
- Replace the membership `connected: bool` write-on-every-connect/disconnect
  behavior with connection-counted presence. Two live connections for one
  identity share membership, seat, hand, and actions; closing either leaves the
  other online and fully authoritative for that same player.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario restart
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario multi-device
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario final-leave
```

**Completion criteria:** Crash/restart recovers while membership exists;
explicit final leave disbands; multiple device connections retain one player
identity.

## Phase 5 — Reconnect the formal evidence to the new authority edge

### [~] T5.1 Prove physical and logical refinement boundaries

**Work:**

- Extend pure/Rust properties for latest pose sequence, control authority,
  bounded coordinates/rotation, and pose independence from logical location.
- Check that a release into a zone maps to at most one typed logical intent.
- Reuse the existing bounded spatial Alloy/NuSMV/Prolog scopes where applicable;
  add only state variables that carry logical meaning.
- Do not ask SAT/SMV to prove smooth floating-point rendering.

**Validation:**

```pwsh
cargo test -p poche-spatial --locked
cargo run -p poche-xtask -- spatial compare all --scope micro
cargo run -p poche-xtask -- spatial coverage audit --all
```

**Completion criteria:** Formal and pure checks cover logical refinement and
bounded spatial invariants without conflating frames/poses with game truth.

### [ ] T5.2 Update lifecycle oracles for centralized membership semantics

**Work:**

- Model identity vs connection, multiple devices per player, disconnect/
  reconnect, explicit leave, final-member disband, create/join/seat/ready, and
  the scoped game lifecycle.
- Update Alloy, NuSMV, Prolog, and Rust oracle fixtures consistently.
- Check invariants: one seat per player, one player per seat, disconnected is
  not departed, no join after disband, no device multiplicity voting weight,
  and eventual room deletion after final explicit leave.

**Validation:**

```pwsh
cargo run -p poche-xtask -- session compare all --scope lobby-micro
cargo run -p poche-xtask -- protocol replay --all
cargo test -p poche-formal --locked
```

**Completion criteria:** All four models agree in their named bounded/symbolic/
query scopes and publish counterexamples for deliberately broken properties.

### [~] T5.3 Prove adapter authorization and projection privacy

**Work:**

- Property-test caller identity mapping, stale/idempotent reducer calls,
  per-recipient DTO conversion, and diagnostics redaction.
- Run two real SDK identities against the local module for positive and
  negative hand subscriptions.
- Keep claims precise: server privacy enforcement against clients, not secrecy
  from the server operator.

**Validation:**

```pwsh
cargo test -p poche-spacetimedb-module authorization --locked
cargo test -p poche-spacetimedb-client projection --locked
cargo run -p poche-xtask -- spacetimedb scenario privacy-adversarial --clients 2
```

**Completion criteria:** Forged IDs/names, stale calls, unauthorized card
motion, and broad subscription attempts fail without leaking secret rows.

## Phase 6 — Automate and measure the real system

### [~] T6.1 Build a disposable local SpacetimeDB test harness

**Work:**

- Start a pinned local server on an isolated port/data directory, publish a
  unique module/database name, wait for readiness, and always collect logs.
- Tear down only processes and directories created by the harness.
- Support fault points for client loss, server restart, delayed messages, and
  reducer denial without requiring Windows firewall or UAC changes.
- Make failure preserve artifacts under ignored `target/spacetimedb/runs/...`.

**Validation:**

```pwsh
cargo test -p poche-xtask spacetimedb_harness --locked
cargo run -p poche-xtask -- spacetimedb smoke --keep-artifacts
```

**Completion criteria:** Repeated runs allocate no shared state, leave no
process behind, and produce correlated server/client logs on failure.

### [~] T6.2 Port the windowless two-player puppet to the new client

**Work:**

- Fan out one harness into creator and joiner devices using file/live-control
  observations rather than the OS clipboard.
- Exercise Create, Join, seats, ready/deal, private hands, card wiggle, legal
  drop, denied drop, and explicit leave through ordinary surfaces.
- Capture each viewer with Bevy's windowless image target and record semantic
  state beside pixels.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario mvp --captures --seed 1
```

**Completion criteria:** One command produces two private captures, a
correlated action/state transcript, server log, latency trace, and an HTML
contact sheet with no visible windows or clipboard changes.

### [~] T6.3 Perform real two-window ordinary-user acceptance

**Work:**

- Build once and launch two `poche.exe` processes with separate profiles.
- Manually verify main-menu flow, code copy/paste, seats, private deal, card
  movement, legal/denied drop, and restart.
- Record screenshots/captures and machine-readable observations without
  claiming a headless test proves OS integration.
- Confirm neither process nor the local server requests UAC.
- Repeat the exact table -> confirmed leave -> lobby-ended -> title -> maximize
  path through the ordinary window and require a subsequent semantic
  observation. Preserve the render/crash log as evidence.

**Validation:**

```pwsh
cargo build --locked -p poche-cli
target\debug\poche.exe --profile alice
target\debug\poche.exe --profile bob
```

**Completion criteria:** A human can follow the documented guide and complete
the slice in two visible windows; the evidence records application, server,
ports, versions, and any firewall prompt separately from UAC.

### [~] T6.4 Publish a latency and load report with a go/no-go decision

**Work:**

- Measure input-to-local-frame, reducer enqueue, server commit, caller outcome,
  peer subscription callback, Bevy ingestion, and peer rendered-frame latency.
- Measure idle traffic, 15 Hz drag traffic, concurrent durable action traffic,
  reconnect, and server resource use.
- Compare with the retained Veilid isolated/integrated evidence using payload
  sizes and topology, not just headline averages.
- Tune coalescing/interpolation and confirmed-read settings only with evidence.
- Stop and revisit G7 if same-host remote pose p95 remains at or above 100 ms or
  visible movement stalls/teleports.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb latency --clients 2 --samples 1000 --json target/spacetimedb/final-latency.json
cargo run -p poche-xtask -- spacetimedb report --input target/spacetimedb/final-latency.json
```

**Completion criteria:** The report gives distributions and correlated traces,
local response is within one rendered frame at 60 Hz p95, same-host peer pose
is below 100 ms p95, durable action peer visibility is below 150 ms p95, and
idle state generates no writes.

### [~] T6.5 Prove recovery and failure boundaries

**Work:**

- Test player process crash/restart, second device, server process restart,
  stale token, network interruption, simultaneous seat attempt, duplicate
  reducer call, explicit one-of-two leave, and final leave.
- State which guarantees come from durable local SpacetimeDB storage versus
  client token persistence.
- Do not claim high availability, failover, consensus, or hosted durability.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario recovery-matrix
```

**Completion criteria:** Every named fault has an expected visible/client/
server outcome, deterministic test evidence, and an honest unsupported-case
entry.

## Phase 7 — Document, integrate, and release the branch

### [~] T7.1 Write the player, contributor, trust, and license guides

**Work:**

- Add an exact “play twice locally” guide covering prerequisite, server start,
  build, two profiles, create/copy/join, seat/deal/move, restart, and shutdown.
- Explain server trust, private-view scope, identity token handling, room-code
  non-authority, centralization, licenses, no-UAC expectation, and firewall
  behavior.
- Add an architecture page showing pure rules, module, SDK client, Bevy bridge,
  rendering, and formal evidence.
- Document the behavioral inspiration/provenance from `bevy_spacetimedb`
  without implying a dependency.

**Validation:**

```pwsh
cargo run -p poche-xtask -- spacetimedb doctor
cargo run -p poche-xtask -- pages build
git diff --check
```

**Completion criteria:** A fresh user can execute the visible MVP and a fresh
developer can explain security/license/authority boundaries from repository
docs alone.

### [x] T7.2 Make SpacetimeDB the desktop default without erasing prior work

**Work:**

- Point no-argument `poche.exe` and ordinary desktop commands at the new
  connector.
- Keep Veilid, web, replication, hidden-card, and old gateway experiments
  behind explicit non-default packages/features or historical branches.
- Update README status/matrix and plan links; label superseded instructions
  rather than deleting evidence.
- Ensure a normal desktop dependency graph excludes Veilid and the reference
  Bevy adapter.

**Validation:**

```pwsh
cargo tree -p poche-cli --edges normal
cargo build --locked -p poche-cli
cargo test -p poche-cli --locked
```

**Completion criteria:** The ordinary binary runs the SpacetimeDB flow, legacy
research remains available and accurately labeled, and no old transport is
silently linked into the default path.

### [~] T7.3 Enforce fast and complete CI surfaces

**Work:**

- Add separate jobs/cache keys for pure core, server WASM, native SDK/bridge,
  formal tools, and full workspace.
- Keep the fast core job independent of Bevy and SpacetimeDB service startup.
- Run generated-binding drift, dependency/provenance, formatting, lint, and
  ignored-artifact checks.
- Record measured compile impact of linker/profile/default-member changes.

**Validation:**

```pwsh
cargo fmt --all -- --check
cargo clippy -p poche-session -p poche-environment --all-targets -- -D warnings
cargo test --workspace --all-targets --locked
cargo run -p poche-xtask -- spacetimedb generate --check
git status --short
```

**Completion criteria:** Fast failures stay fast, all supported surfaces run
elsewhere in CI, and no cache or default-member choice conceals full coverage.

### [ ] T7.4 Produce the completion receipt and publish the branch

**Work:**

- Re-run the full support matrix from a clean-enough isolated target and fresh
  local database.
- Record exact versions, commands, pass/fail/skip results, latency report,
  capture manifest, formal scopes, trust limits, license decision, and commits.
- Re-run all three intent-audit passes and repair the plan if any active
  guidance lacks evidence or an explicit non-goal.
- Commit and push `spacetimedb` only after `git diff --check` and targeted
  validation pass.

**Validation:**

```pwsh
cargo test --workspace --all-targets --locked
cargo run -p poche-xtask -- protocol replay --all
cargo run -p poche-xtask -- session compare all --scope lobby-micro
cargo run -p poche-xtask -- spacetimedb acceptance --surface headless --scenario mvp --captures --seed 1
git diff --check
git status --short --branch
```

**Completion criteria:** Every task is `[x]` with local evidence, the two-window
MVP and latency gates pass, docs and code agree, limits are accurate, and the
remote branch contains all intended commits.

## Support and acceptance matrix

| Surface | Status for this plan | Required validation | Evidence |
| --- | --- | --- | --- |
| Pure Rust domain/session/environment | Supported, transport-free | Focused unit/property tests and full workspace | Pending |
| Alloy oracle | Supported in existing named scopes | Session/spatial comparison and counterexample witness | Pending |
| NuSMV oracle | Supported in existing named scopes | Session/spatial comparison and temporal witness | Pending |
| Scryer Prolog oracle | Supported for relational queries | Session/spatial comparison and query witness | Pending |
| SpacetimeDB module | Supported, pinned local 2.10.0 | WASM build, publish, reducer/privacy integration | Module build/publish and create/join/seat/private-view/pose flow pass; pure durable play and adversarial suite remain |
| Native Rust SDK client | Supported on Windows first | Connect/subscribe/reducer/reconnect tests | Generated bindings, canonically ordered subscription cache, sender-scoped activity, reducers, credential persistence, and live two-client flow pass; forced reconnect remains |
| Poche-owned Bevy bridge | Supported with Bevy 0.19.1 | Headless App and real rendered clients | Owned channel bridge and rendered windowless clients pass; resume acceptance compares actual card/avatar entity counts to the preloaded model; no `bevy_spacetimedb` dependency |
| Two visible `poche.exe` processes | Primary player acceptance | Create/join/seat/deal/wiggle/drop/restart | The identical ordinary binary passes create/join/seat/deal/wiggle through its windowless target; explicit leave returns peers to a coherent lobby; final human visible-window check and true restart remain |
| Windowless two-player puppet | Primary repeatable acceptance | Two private captures plus semantic/latency transcript | Latest Maincloud v8 passed with nine public events, immediate resumed-scene hydration, coherent post-leave cleanup, 53.69 ms authority, and 153.42 ms peer; fresh-local rerun remains for this schema |
| Web/browser client | Explicitly unsupported this phase | Build graph/doc audit; no accidental promise | Pending |
| Veilid transport | Retained research, non-default | Historical tests/docs remain; default tree excludes it | Default member/package is SpacetimeDB; dependency audit required at release |
| Untrusted-host/zero-trust play | Not supported | Threat-model statement | Trusted-server boundary documented in `docs/spacetimedb-desktop.md` |
| Hosted Maincloud development | Supported for the current physical-table slice; production operations not claimed | Owned publish plus explicit hosted acceptance | `poche-6quz6` published and two-client acceptance passed; local remains supported independently |

## Overall completion criteria

- [ ] `poche-4` remains isolated, committed, pushed, and based on the recorded
  formal/core foundation.
- [ ] The pinned SpacetimeDB development stack is reproducible and its BSL/
  Apache boundaries are accurately documented.
- [ ] Pure Poche rule tests do not compile Bevy or SpacetimeDB.
- [ ] Poche owns its module/client/Bevy bridge and has no dependency on
  `bevy_spacetimedb`.
- [ ] Two real desktop processes create/join one opaque-code room, take seats,
  deal recipient-private hands, and recover after a client restart.
- [ ] A local grabbed card moves immediately and the peer sees smooth,
  monotonic position and rotation within the latency gate.
- [ ] Physical movement and logical location remain separate; legal drop
  advances the pure game and denied drop does not.
- [ ] Explicit final-member leave disbands; mere disconnect does not.
- [ ] Multiple connections for one identity remain one player and receive only
  that player's authorized projection.
- [ ] Headless/windowless puppets prove the same slice without touching the OS
  clipboard or opening windows.
- [ ] Alloy, NuSMV, Prolog, Rust, and the SpacetimeDB adapter agree for the
  declared lifecycle and game scopes.
- [ ] Latency, privacy, recovery, compile-time, trust, license, and unsupported
  target reports are committed and linked from README.
- [ ] Full workspace validation and the final three-pass intent audit have
  recorded evidence.

## Risk register

| Risk | Weight | Guardrail / controlling work |
| --- | --- | --- |
| SpacetimeDB does not improve integrated latency or pose updates still teleport. | Critical | G7, early T2.5 spike, local prediction, bounded samples, T6.4 stop condition |
| BSL terms conflict with intended distribution/hosting. | Critical | G9 before broad implementation; T7.1 exact disclosure; no production claim |
| Installed CLI 2.10.0, source checkout 2.10.1, and selected crates drift. | High | T1.2 coherent pin, doctor version check, lockfile and generated-schema fingerprint |
| Private tables/views leak another player's cards through cache, bindings, logs, or diagnostics. | Critical | G10, T2.3 negative tests, T5.3 adversarial SDK identities |
| Database schema becomes a second rules engine and diverges from formal Rust. | Critical | G12, pure reducer call, DTO round trips, T2.4/T5 conformance |
| High-frequency pose transactions inflate commit logs or starve logical actions. | High | Latest-value schema, 15 Hz starting cap, coalescing, priority/backpressure tests, load report |
| Multiple `ConnectionId` values accidentally create players or voting weight. | High | Identity/device separation, T2.2 and T4.5 multi-device tests |
| Disconnect is mistaken for leave and destroys a recoverable room. | High | Separate presence/membership tables, G11, T6.5 lifecycle matrix |
| Generated bindings churn rebuilds and create merge noise. | Medium | G4 measured isolation, schema fingerprint, idempotent `generate --check` |
| Excessive crate splitting repeats Cursor Hero's compile/maintenance overhead. | Medium | Only target/codegen/heavy-leaf splits; T1.1 compile evidence before further crates |
| Copying the reference plugin imports stale assumptions or attribution debt. | Medium | No dependency, provenance ledger, owned small API, no unsafe/leaks, metadata audit |
| Optimized dependency profiles reduce runtime iteration but worsen clean builds or disk use. | Medium | Benchmark stable settings individually; do not adopt without measured benefit |
| Test harness gives false confidence by mutating state directly or using one in-process client. | High | Ordinary SDK/reducer paths, two processes/identities, visible acceptance distinct from headless |
| Centralization is later mistaken for anonymity, consensus, or secrecy from host. | High | Confirmed trust boundary, README/deployment matrix, explicit unsupported rows |
| Existing valuable formal/UI work is broken during transport replacement. | Medium | Branch isolation, focused existing tests after every phase, legacy code retained until replacement passes |
| Local services trigger firewall confusion or are assumed to need Administrator. | Medium | Loopback-first ordinary-user acceptance, explicit logs/docs, no UAC in harness |

## Immediate next slice

Continue from the accepted multi-account/activity/lifecycle checkpoint by
testing real process restart, unavailable-authority recovery, final-member
disband/stale-code rejection, and visible two-window account switching, then
expand the one-trick oracle adapter into multi-round Poche play. Keep
diegetic seat/bid/play affordances synchronized with the exhaustive action bar,
and collect a real visible-window camera/mouse feel check before treating the
new focal rig as polished.

Implement that lifecycle slice in this order:

1. Extend the windowless harness to stop and relaunch Alice from the same vault,
   not only attach a simultaneous sibling, and prove the resume interstitial
   remains authoritative after a real process boundary.
2. Add bounded reconnect/retry and an unavailable-authority screen; exercise a
   temporary local authority loss without converting disconnect into leave.
3. Explicitly leave both durable members and prove the final leave disbands the
   room and its stale join code is rejected without changing the joiner's
   identity.
4. Prove title-arrow switching Alice -> Bob -> Alice changes authenticated
   projections without invoking room leave, including a same-name distinct-
   account negative case.
5. Repeat the lifecycle once in two visible ordinary-user windows and record
   the account/title/resume screenshots plus DX12 startup evidence.
6. Begin the complete multi-round rules adapter and reconnect the bounded
   lifecycle behavior to the Alloy/NuSMV/Prolog models.
