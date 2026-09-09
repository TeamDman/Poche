# Desktop Veilid lobby and recovery

**Plan status:** Active; user authorized goal execution
**Primary implementation root:** `D:\Repos\Games\poche-3`, `model-checking`
**Last updated:** 2026-09-09
**Intent audit:** Three passes completed for the September restart; scope gates pending

## Authorized execution contract

User explicitly requested setting the goal and proceeding after the final scope summary. G1 uses automatic protected same-device identity recovery; recovery-code export is outside this goal. G3 uses strict hand privacy in this desktop slice; retain legacy grant experiments without exposing them here. G4 requires two real desktop processes, create/join/seats/deal, synchronized full position/rotation manipulation and cross-viewport dragging, successful legal play and denied out-of-turn play without logical mutation, reveal, or forced snap-back. A complete trick is a validation witness, not a promise to finish all remaining full-game UX. G2's lifetime decision is confirmed; implement and document failure-detection and replication details before claiming recovery. These resolutions supersede the alternative proposals in the historical gate rows below. No further user approval is required for reversible internal choices within this scope.

Execution audit: rechecked U1-U17 extraction against the available September exchange; mapped each to tasks/gates; checked inverse scope for web removal, recovery export, shared versus local motion, physical/logical independence, and privacy. User approval closes product-scope questions; technical gates remain evidence obligations, not requests to reopen the agreed goal.

## How to update this plan

- `[ ]` Not started; `[~]` in progress; `[x]` complete; `[!]` blocked with evidence and unblock condition.
- Update task headings and completion notes together. Keep one implementation focus.
- Record actual commands, outcomes, decisions, commits, and exceptions under their task, not in a detached log.
- A phase is complete only when all its tasks are complete. Do not infer completion from older plans.
- The plan is only ready once we have literally triple checked that no intent from the user has been omitted without explicit direction from the user.

## Intent audit evidence

1. Extraction: inspected the available September research/restart messages through the crash-rejoin suggestion. Captured desktop preference, corrected reference path, Bevy patch request, clipboard qualifier, spatial agency, spectators, recovery, and the tentative nature of name/secret authentication as U1-U12.
2. Traceability: every row below maps to a task or gate. Recommendations (one-trick slice, recovery-code design, Windows-first support) are explicitly not user-approved requirements.
3. Adversarial review: checked that leaving is not permanent, an invitation is not a player credential, multiple devices retain agency, mock nodes do not connect separate processes, and a camera layer does not provide secrecy. Preserved prior work rather than declaring all historic features deleted.
Source boundary: the September user messages are available. Prior requirements remain in the existing plans, especially PLAN-5-LIVE-CONTROL-PUPPETS.md; its ledger was inspected. This is not a new certification of every historic implementation or an exhaustive re-audit of all five plans.

Amendment audit (September 9): extraction added U13 from the explicit everyone-leaves clarification; traceability maps it to G2/T5; adversarial review preserves crash recovery while peers remain, distinguishes intentional departure from temporary loss, and does not infer that cached DHT data resurrects a disbanded room. Rechecked U1-U12 mappings; other gates remain open.

Movement amendment audit: extraction added U14-U17 from the user's correction of the proposed legal-play-only/local-rearrangement scope. Traceability maps these to T4a/T4b and G4. Adversarial pass rechecked all ledger mappings and preserved cross-client (not merely local) motion, rotation as well as position, one shared world across viewports, and the tentative 'perhaps' concerning denied-play physical pose. No implementation/proof is claimed by this audit.

## Purpose and guidance ledger

Create a coherent desktop experience: independently launched games create/join a lobby, occupy seats around a table, and recover player identity after a crash. Preserve the independently executable rules and formal evidence.

| ID | Guidance and qualifier | Coverage |
| --- | --- | --- |
| U1 | Prioritize desktop; dropping web support may be best. | T1/T6: desktop is this slice; preserve existing web sources, do not silently delete or promise maintenance. |
| U2 | Bevy stays; update to 0.19.1. | T1 |
| U3 | Understand transport abstraction and exploit new Veilid local testing. | T2; one real adapter exercised by mock and live core; retain device semantics. |
| U4 | Open two games to a main menu; Create lobby produces a code with Copy. | T3/T6; distinct profiles for two local players. |
| U5 | Join lobby reads clipboard and prefills a text area iff expected format matches; user clicks Join. | T3; no automatic joining or logging arbitrary clipboard content. |
| U6 | Both windows show a table, seats, and seat-taking. | T4/T6; seat conflicts explicitly rejected. |
| U7 | Tabletop-style object actions have permissions; Poche forbids spawning after initial deck and revealing hand cards except by playing. | T4/G3; no unrestricted spawn endpoint; independent validation, not only disabled controls. |
| U8 | Cards grounded in 3D; knowing faces and positioning cards are engine permissions. A second camera may show the hand at the bottom without background clutter. | T4; camera is an optional rendering technique, not secrecy; shared identity, no duplicate canonical cards. |
| U9 | Spectators are spatial participants, e.g. capsules around the room until seated. | T4 |
| U10 | Crash must allow rejoining; perhaps use the same name and secret. | G1/G2/T5; required recovery outcome, tentative credential mechanism. |
| U11 | Research paths: D:\Repos\rust\veilid, D:\Repos\rust\makepad, D:\Repos\Games\bevy, corrected D:\Repos\Games\bevy_veilid, G:\Programming\Repos\ash. bevy_veilid is reference-only, not a dependency. | T1/T2; Bevy selected, alternatives not an implementation obligation. |
| U12 | Use G:\Programming\Repos\skills\.github\skills\resumable-implementation-plans\SKILL.md to stay on track. | This living contract and T6 audit. |
| U13 | If everyone leaves the lobby it gets disbanded. | G2/T5: no requirement to restore an empty room later; credentials do not resurrect a disbanded room. |
| U14 | Physical location is position plus rotation; logical location is hand, deck at index n, or in play. Rules use logical location. | T4a; supersedes agent suggestion to restrict movement to legal play/local-only arrangement. |
| U15 | Wiggling a hand card in one client must be visible in the other client. | T4a/T6; shared authorized poses even when logical state does not change; hidden faces remain hidden. |
| U16 | Dragging hand-to-table crosses camera viewports whose projections map to the same 3D space. | T4b; continuous same-card drag, no canonical duplication or unrelated local coordinate state. |
| U17 | Entering the play area attempts a rules action; if not the player's turn, perhaps logical position stays unchanged even though physical position changed. | T4a/T4b; separate pose acceptance and play acceptance. Proposed MVP preserves allowed pose on denial with explicit status; no implicit reveal. |

Inherited constraints: MPL-2.0; one poche executable with CLI and GUI; Figue/Teamy CLI conventions; no Vox/local-instance control foundation; distinct authorized device keys per player, no extra votes from extra devices; typed observations/actions; same-player capture protocol; windowless puppets; preserve model scopes and independent oracles. Previous leave clarification remains: voluntary leavers may rejoin as spectators and take a seat at an appropriate phase; crash reconnect is distinct from voluntary leave, kick, or revoked credentials.

## Verified foundation, not fresh test evidence

- Workspace pins Bevy `=0.19.0`; lockfile agrees. crates.io API reports stable 0.19.1 on September 9.
- `crates/poche-native-ui/src/native_live.rs`: worker-thread bridge generic over DeviceTransport. CLI `cli/live_device.rs` concretely selects HTTP today.
- `crates/poche-runtime/src/oracle_session.rs`: pure rules adapter; renderer does not own legality or scoring.
- `crates/poche-player-client/src/protected_store.rs`: public profiles reference protected key storage; existing API deliberately has no secret export. Name/password recovery does not already exist merely because profiles do.
- `crates/poche-veilid`: optional native probes, not the default GUI transport.
- Veilid upstream 99c9616295f9195c97bf2f5f6b43f0d220ac5d8b has mock-api and faults; published core is still 0.5.7. Mock shared state is process-local; separate game processes require actual networking.
- PLAN-5 is marked complete historically. Previously reported external full-game PUPPET-NO-ACTION needs fresh reproduction; do not claim it fixed or assign a cause without evidence.
- No build or test was run while creating this document.

## Gates: close before affected implementation

| Gate | Required choice / proposed default | Acceptance consequence |
| --- | --- | --- |
| G1 Recovery UX | Recommend automatic same-device recovery from protected persistent profile; optional generated high-entropy recovery code for lost profile/new device, not a user-chosen password. Name is a label. User's name+secret proposal remains tentative. | Restore same principal without name impersonation; lost-secret behavior, credential scope, export consent, revocation and device enrollment tests. Never silently add export to existing non-exportable keys. |
| G2 Failure contract — lifetime scope settled, protocol details open | U13: everyone leaving disbands the lobby; durable all-peer shutdown/resume is not required. Participant crash recovery remains required while peers retain the room, including creator failure. Proposed policy: explicit final departure closes immediately; unexpected loss uses a bounded reconnect grace period, not instantaneous disbanding on a missed message. Choose timeout, partition-safe membership/expiry rules, seat retention, absence progression, snapshot verification and replay deduplication before implementation. | Killed process rejoins with correct entitled state while room survives; final departure and timeout cases cannot revive a closed room through stale invitations/DHT records. Revoked devices denied; creator failure covered without claiming consensus/failover already solved. |
| G3 Reveal policy | Latest Poche example disallows hand reveal except play; earlier plans supported spectator hand grants. Confirm strict default Poche mode versus removing grants entirely. | No newly enabled reveal bypass; do not silently remove previous policy features or equate render visibility with authorization. |
| G4 Slice endpoint — movement clarified | User requires two instances create/join and move cards, specifically synchronized physical poses distinct from logical rules state (U14-U17). Include ready/deal to exercise hand-to-play valid and out-of-turn attempts; one complete trick remains an agent-proposed acceptance witness, not a full-game promise. | Local-only wiggle or legal-action-only dragging does not pass. End-to-end acceptance covers accepted pose with rejected play as well as a successful play. |

Working support assumption: Windows desktop first (two local independent processes); no web acceptance in this slice, no Makepad/ash migration, no generalized tabletop editor, new RL learner, or new mental-poker protocol. Network transport is not itself decentralized game authority. Preserve existing consensus/security work without claiming it production-ready.

## Tasks and validation

All commands run from the primary root. Commands below are existing package surfaces, not claimed successful results. Add exact new test/launch commands alongside implementation before marking done; never invent a passing future harness invocation.

### [~] T1 Update Bevy and establish baseline

Completion notes: goal activated September 9 after explicit user approval. Starting tree contains only this newly authored plan as untracked work. Root Bevy pin updated to 0.19.1; lock resolution and tests pending. User permits updating local Bevy reference checkout, but preserve its untracked assets and do not use a local path dependency.

Dependency resolution: sandbox network attempt failed; approved network retry `cargo update -p bevy --precise 0.19.1` succeeded, updating the Bevy family and adding its clipboard transitive packages. `cargo test --locked -p poche-native-ui -p poche-puppet -p poche-capture` is building (terminal session 80091); not yet a passing result.

Baseline result: session 80091 completed successfully: capture 8, native UI 14, headless puppet 4 passed; six browser/external/GPU tests explicitly ignored. This proves baseline source compatibility, not real Veilid or GPU rendering. Session 19407 `cargo test --locked -p poche-spatial --offline` completed: 31 passed including all three new manipulation tests. Windowless GPU acceptance launched separately; T1 remains in progress pending capture inspection and remaining baseline checks.

Checkpoint commit: 5451022 contains Bevy pin/lock upgrade, this plan and initial pose reducer. GPU result (session 88270): `$env:WGPU_BACKEND='dx12'; cargo test --locked -p poche-puppet --offline --test native_full_game -- --ignored --nocapture` built successfully but failed at `tests/native_full_game.rs:27`: DeviceProtocol, 'certified native capture device rejected the puppet operation'. No screenshot verified. Next T1 action: trace capture-provider rejection and recover precise underlying error rather than misclassify this as a GPU failure. Previous turn changed source and established test evidence (progress), not a blocked state.

Follow-up evidence: native puppet now preserves DeviceClientError class in redacted messages and logs enum-only readback denial stages. Diagnostic regression passed via `cargo test --locked -p poche-puppet --offline --lib capture_diagnostics`. Two reruns of native_full_game passed (69.73s and 79.14s) without a behavioral fix; original rejection remains intermittent/unexplained, not resolved. Added optional `POCHE_NATIVE_EVIDENCE_ROOT` to ignored GPU test for retained captures; second rerun used `D:\Repos\Games\poche-3\target\phase6-native-evidence`. Six captures published under `two-player-full-round-seed-29`. Inspected `captures/native-1d76fdaaeca77442/native-1d76fdaaeca77442-png.png` with view_image: nonblank 3D card-selection table, filled small labels; near avatar occludes hand and score sheet is tiny. T4 must improve these. This remains loopback device evidence, not two-process Veilid acceptance. Next work: real Veilid adapter integration, retain improved diagnostics for recurrence rather than claim this flake fixed.

T2 reconnaissance while baseline compiles: `replicated.rs` requires strict player-majority and joint membership quorums; in a two-player room one survivor cannot finalize new logical actions alone. Preserve safety: recovery may retain the room while waiting for the other player; do not silently lower quorum to claim availability. Creator restart must not depend on the creator retaining the only copy of accepted state. `bin/native_smoke.rs` currently has a HostRuntime with one InProcessAuthority and is not failover evidence. Resolve and test the replica/recovery path before T2 completion.

Work: inspect current dirty state, workflow `.github/workflows/pages.yml`, dependency pins, and native captures. Update root Cargo.toml/lockfile to 0.19.1 without sibling path dependencies. Reproduce historical puppet failure separately.

Validation: `cargo test --locked -p poche-native-ui -p poche-puppet -p poche-capture`; `cargo run --locked -p poche-cli -- --help`. Inspect fresh windowless captures via view_image. Network dependency download may be required; do not force offline before cache exists.

Complete when patch is locked, source builds, baseline results and any failures are recorded with exact commands. No claim of upgrade verification from manifest edit alone.

### [ ] T2 Establish Veilid device transport

Service lifetime checkpoint: `VeilidDeviceService::serve` consumes a bounded callback receiver and processes at most four concurrent async handlers on the node runtime, so a cooperating capture need not serially block observation RPCs. `RunningDeviceService` retains the node and aborts async acceptance on drop; it does not provide replica persistence. Callback overflow uses nonblocking try_send and is a lost request, not success. The integration now uses owned nodes on both sides and this service task instead of a test-only serial loop. `cargo test --locked -p poche-veilid --features device-service,veilid-mock-test --offline --test device_service` passed (0.65s). A non-mock service check passed before adapting the callback receiver's Box type. This test proves registration on the owned runtime, not a load/fault bound under saturation, actual network traffic, menu Create or failover. Next assemble room publication and initial creator action with this task retained beside the room.

Join orchestration checkpoint: `device_join.rs::join_device` is an ordinary-worker operation returning a lifetime-owning PlayerDeviceClient and RoomId after code validation, route resolution, certified observation, admission/reconnect and confirmed connected membership. It never executes CreateRoom for an invitation. Existing members omit bearer proofs; new members supply the code-derived proof. A missing route can trigger signed rebind, then the advertised Reconnect action. Added explicit redacted Unavailable RPC reply because collapsing route loss into Denied prevented recovery. Device-service mock test now calls this function rather than manually scripting admission, then disconnects/drops/rejoins the same device and checks retained seat 1. Command `cargo test --locked -p poche-veilid --features device-service,veilid-mock-test --offline --test device_service` passed (0.64s). This is clean client reconstruction against a surviving in-process authority, not creator failover, process-kill persistence or GUI registration. Non-mock device-transport check passed before the final error-mapping addition; repeat during startup integration. Menu background owner must call this function with protected profile/node rather than reproduce its admission protocol.

Admission checkpoint: RoomCode now derives a domain-separated session bearer proof from its canonical secret code, binding admission to the complete invitation rather than a fixed player label. The RPC integration installs its hashed InviteRecord and action-source proof, admits an independently signed second player, denies a wrong secret, excludes occupied seat 0 and commits seat 1. `cargo test --locked -p poche-veilid --features device-service,veilid-mock-test --offline --test device_service` passed (0.43s); pure `--offline --lib room_code` passed five tests including proof round-trip and differing secrets. Test identities are fixtures, both device clients use one mock client node, and InviteRecord revision expiry is deliberately unbounded in this test. Production startup must set/enforce invitation lifetime and revocation; this does not prove two-process admission, protected profiles or GUI wiring. Next assemble startup/admission around these verified boundaries rather than introducing predetermined guest identities.

Node ownership checkpoint: `device_node.rs` starts Veilid on a dedicated ordinary thread that owns Tokio's runtime. Cloned owners retain the node; final drop signals asynchronous shutdown without blocking Bevy, while explicit worker-only `shutdown` waits for cleanup. Configuration/storage isolation and nonblocking callbacks remain caller responsibilities; startup does not implicitly attach to the public network. `VeilidDeviceTransport::from_node` retains the node after menu lifetime ends. `cargo test --locked -p poche-veilid --features device-transport,veilid-mock-test --offline --lib device_node` passed clone-survival and same-namespace clean restart. `cargo check --locked -p poche-veilid --features device-service --offline` passed without mocks. The device_service integration now uses this node owner and owning transport; its create/seat/disconnect/reconnect test passed with `--features device-service,veilid-mock-test --offline --test device_service` (0.30s). Still process-local simulation, not real sockets, crash recovery, admission persistence or launcher wiring. Next connect the menu's requests to startup/profile/admission, with explicit error and connecting feedback; do not confuse clean node restart with surviving game state.

Full client-path checkpoint: expanded device_service test to instantiate PlayerDeviceClient<VeilidDeviceTransport> on an ordinary worker with a resolved room. Through mock private-route callbacks it observes pending room, commits CreateRoom, takes seat 0, disconnects/rebinds the device route, observes reconnect-only actions, commits Reconnect, and regains ordinary actions. Fixed missing epoch 0 -> 1 transition after committed CreateRoom in VeilidDeviceTransport. `cargo test --locked -p poche-veilid --features device-service,veilid-mock-test --offline --test device_service` passed (0.31s runtime). This uses test-generated credentials and an in-process surviving authority: not process-kill recovery, creator failover, real sockets, GUI menus, or protected-profile persistence. Production startup/menu remains the next integration task.

Server RPC checkpoint: `VeilidDeviceService` dispatches through existing CertifiedDeviceRoom verification and replay boundaries, executes prepared cooperation outside the room mutex, and answers AppCalls using spawn_blocking for reducer work. `cargo test --locked -p poche-veilid --features device-service,veilid-mock-test --offline --test device_service` passed: valid signed observation returns, signed-room tampering denies, malformed envelope rejects, and a two-node private-route AppCall receives the observation via actual callback/service/reply APIs under mock simulation. This test does not use the synchronous VeilidDeviceTransport client yet or prove real sockets, menu, per-recipient reply cryptographic binding, pagination, epoch/route refresh, or failover. Next wire the real desktop startup/menu plus bounded runtime service and extend the full client test, without presenting this central CertifiedDeviceRoom adapter as completed decentralized recovery.

Client RPC checkpoint: added optional `device-transport` feature and `VeilidDeviceTransport<S>` implementing the existing DeviceTransport on a dedicated native worker with a live Tokio handle/resolved private route. It signs existing observe/invoke/route requests, carries signed cooperation, binds operations to the resolved room, and rejects wrong response variants and oversize/unknown-version envelopes. `cargo check -p poche-veilid --features device-transport --offline` passed (non-mock); `cargo test --locked -p poche-veilid --features device-transport,veilid-mock-test --offline --lib device_transport` passed envelope-negative tests. No server dispatch or GUI wiring yet. Next use the existing CertifiedDeviceRoom signature-verifying boundary (`device_client.rs`) for dispatch, with explicit recipient reply protection, AppCall integration tests, route refresh and epoch updates. Current 30KB bound fails oversized observations explicitly; chunked/trimmed-history protocol needed before full-history acceptance. These partial surfaces do NOT establish production identity/failover safety. Keep protocol decoding distinct from authorization.

Integration progress: root dependency now pins upstream 99c9616295f9195c97bf2f5f6b43f0d220ac5d8b (still versioned 0.5.7 upstream). Added non-default `veilid-mock-test`; migrated identity, membership and host-capability storage to async VeilidAPI secret methods. `cargo check --locked -p poche-veilid --features veilid-mock-test --offline` and separate non-mock `--features veilid` both passed. `cargo test --locked -p poche-veilid --features veilid-mock-test --offline --test mock_nodes` passed: two nodes share DHT, isolate device secrets, publish and resolve Poche rendezvous via real adapter in process-local simulation. This is not cross-process/live networking, durable OS keychain/restart evidence, or a completed DeviceTransport/GUI integration. Next: implement the device command/observation service and real-node lifecycle; keep mock out of desktop feature closure. Existing check-then-save identity race predates this migration and must be addressed before claiming concurrent profile creation safety.

Work: close G2 architecture portion; adapt poche-player-client/poche-veilid boundary without Bevy dependencies in protocol/core. Pin reviewed upstream mock-capable revision; isolate mock feature builds from real builds. Background service owns networking; typed proposals/observations cross the UI boundary. Read bevy_veilid only for task/event bridging inspiration.

Validation: `cargo test --locked -p poche-player-client -p poche-veilid`; add adapter tests for distinct devices, exact-recipient views, stale/duplicate actions, injected failures and recovery. Record actual mock-feature command once implemented. Separate opted-in two-process real-network acceptance from offline mock harness; respect existing public-network opt-in guards.

Complete when same adapter passes mock tests and actual two-process exchange, with topology documented. HTTP tests alone do not satisfy it.

### [ ] T3 Implement desktop menu and invitation contract

Menu preparation checkpoint: `crates/poche-native-ui/src/desktop_menu.rs` adds a Bevy-native name/Create/Join surface, bounded editable invitation input, shape-validator callback, explicit Join and busy/status feedback. Clipboard bytes are not logged; unrelated clipboard data leaves existing invitation text untouched; valid clipboard data does not initiate a connection. Native crate now explicitly enables Bevy's `ui` feature alongside workspace `3d`; `cargo check -p poche-native-ui --offline` passed. The plugin is not registered in the desktop launcher yet: the current launcher still uses a fixture or HTTP live-device path. It emits typed requests but has no production connection owner, invitation parser binding or room-transition wiring. Do not present this as a working menu or two-process acceptance. `cargo test --locked -p poche-native-ui --offline --lib` passed all 16 tests, including clipboard refusal/prefill and explicit Join/duplicate-press suppression. These inject ECS interactions/clipboard results; GPU menu capture/input and actual OS clipboard acceptance remain pending. Next integration must keep credential loading/network startup on a background owner and retain that owner after transition to the live table.

Work: name/profile selection, Create, Join, Copy, shape-gated clipboard prefill, explicit Join, connecting/error states. Invitation distinct from player recovery secret. Validate length/version/encoding before network use. Two instances can select independent profiles without accidentally sharing identity.

Validation: `cargo test --locked -p poche-native-ui -p poche-player-client`; parser tests for malformed/oversized/unsupported codes; UI-input puppet tests for create/copy/prefill/join and clipboard refusal without secret logging.

Complete when two game instances join through the presented menu, not a privileged fixture route.

### [ ] T4 Ground lobby and private-hand interactions

Work: close G3/G4. Table, seat claims, spectator capsules, local camera, ready/deal/actions as agreed. Card views share logical identity; render-layer proxies allowed. Authority checks movement/reveal/spawn separately. Render back-only opponent observations; never send faces and hide them with camera layers. Preserve action palette/CLI access beside spatial affordances.

Validation: `cargo test --locked -p poche-session -p poche-runtime -p poche-spatial -p poche-native-ui`; `cargo run --locked -p poche-xtask -- coverage audit --all`. Add seat-race, illegal spawn/reveal, privacy and UI action tests. Update formal models/coverage for changed semantics with explicit scopes; screenshots do not prove secrecy.

Complete when two windows agree on seating/public state while private hands and actions obey policy; actual input and captured output meet G4 endpoint.

### [ ] T4a Separate shared poses from logical rules transitions

Work: audit current poche-spatial refinement assumptions before changing them: geometry must no longer require logical location to follow every pose. Keep stable opaque card IDs, logical location (including deck index), and separately authorized world position/rotation. A pose update is not a bid/play/ownership change. Check manipulation permission independently from turn legality; disallow moving opponents' cards absent a grant. Pose traffic needs per-object/device ordering, bounded rate/coalescing, finite bounded coordinates and a convergent latest accepted pose; do not route every mouse sample through the scoring log. Record final accepted poses for reconnect. Resolve competing devices through explicit manipulation ownership/ordering, not packet-arrival accidents.

Source inspection note: current `PoseMm` in `units.rs` supports yaw only. Introduce an explicit full-orientation manipulation representation rather than silently interpreting the user's rotation requirement as yaw alone; preserve v1 fixtures through an adapter/version boundary. `abstract_viewer_scene` already reads typed card locations but invokes scene validation: audit those validation assumptions before overlaying arbitrary permitted poses.

Initial implementation (while T1 build runs): added `manipulation.rs` physical overlay with full yaw/pitch/roll, bounded position, monotonic per-lease sample sequence, explicit lease changes and play-region entry edges. It stores no card face/logical location and does not authenticate network senders: authorization must remain in the eventual transport/session integration. Added three unit tests for rejection atomicity, stale writers/samples, and no delayed automatic play. Not yet integrated into GUI/network; T4a remains incomplete. Spatial test command is waiting on the baseline build lock (session 19407); native baseline session 80091 has reached native UI compilation. Reference Bevy checkout now detached at b56fc29d3016e641754765244b5ba3f9cc504671 with untracked assets preserved.

Validation: `cargo test --locked -p poche-spatial -p poche-session -p poche-runtime -p poche-player-client`; add invariants that pose-only actions leave logical rules state unchanged; stale/out-of-order packets cannot revert the settled pose; unauthorized poses are rejected. Two-client puppet wiggles position AND rotation during another player's turn and observes motion before drag release, with the remote side receiving only a back/opaque identity. Test convergence after reconnect.

Complete when physical motion is shared without turn advancement, card reveal, ownership change or mutation of the rules-engine state. Update formal abstractions to represent pose-only stuttering of logical state; bounded geometry models and input/render tests provide separate evidence, not a proof of arbitrary continuous geometry.

### [ ] T4b Bridge viewports and attempt play separately

Work: use the hand and table cameras to project/unproject onto the same world with stable drag ownership and offset across the viewport boundary. Proxies may render the same card but must not create another logical object. Entering the play region creates a uniquely identified play attempt tied to the relevant rules revision; define edge-trigger/rearm behavior so holding a card there does not spam commands or automatically play it when the turn later changes. Proposed MVP default (from user's tentative example): an authorized pose may stay in the play area after rejection while the card logically stays in hand; visibly label 'not played' and offer return-to-hand. Face disclosure occurs only with accepted authorized reveal, even if a private card is rotated or moved to the center.

Validation: `cargo test --locked -p poche-native-ui -p poche-spatial -p poche-runtime`; puppet actual viewport crossing, rotation, legal play, out-of-turn denial and repeated region entry. Compare both clients' poses and logical states; inspect screenshots of denied versus accepted play. Verify no automatic delayed play when actor changes and no secret face in remote payload/capture.

Complete when both views show the same authorized world motion; only accepted rules commands change logical location. The hand camera may keep showing a logically owned but physically displaced card through a clearly identified proxy; document chosen representation rather than silently snapping it back.

### [ ] T5 Implement crash/rejoin identity and state recovery

Work: close G1/G2 fully. Persist credential references before admission succeeds; protect secrets, atomic profile writes, explicit recovery errors. Reconnect proves identity cryptographically, not by matching display name. New device enrollment must not clone a device identity or increase player rights/votes. Persist or reconstruct canonical accepted history and entitled private state according to agreed fault model. Distinguish disconnected, voluntarily left, kicked, and revoked.

Validation: `cargo test --locked -p poche-player-client --features protected-store`; `cargo test --locked -p poche-session -p poche-runtime`. Add process-kill tests after join/deal/action submission, replay without double application, wrong-secret/name-collision/expired-invite/revoked-device cases, secret-redacted logs/captures, and agreed creator/all-peer failure cases. Use temporary test profiles, never overwrite user keyring entries.

Complete when a killed/relaunched player resumes the same authorized identity/state within the agreed availability model; ordinary leave still permits spectator rejoin where allowed. Game state recovery must be proven independently of credential persistence.

U13 acceptance: final intentional departure disbands the lobby; reuse of its invitation reports closed/unavailable rather than recreating it. A new lobby uses a fresh identity/invitation. No all-peer offline restoration promise. Add unexpected-all-peer-loss tests under the chosen grace/expiry policy, temporary partition tests that cannot independently finalize conflicting histories, creator crash with a surviving peer, and multiple-device presence tests (one device closing is not automatically its player's final departure). Grace duration and protocol remain design decisions, not user-confirmed timing requirements.

### [ ] T6 Acceptance, docs, and handoff

Work: one documented desktop launch path and windowless puppet invocation; run menu-to-G4 endpoint plus T5 failures. CLI is another authorized device, not window IPC. Capture screenshots and semantic revision/history evidence, inspect images, redact credentials. Update README/coverage/release notes; preserve prior web artifacts with explicit support scope. Review diff, commit scoped work and follow repository push workflow when implementing the agreed goal.

Validation: `cargo test --workspace --locked`; `cargo run --locked -p poche-xtask -- coverage audit --all`; actual new end-to-end commands recorded here. Existing native test surface: `$env:WGPU_BACKEND='dx12'; cargo test --locked -p poche-puppet --test native_full_game -- --ignored --nocapture` (verify applicability before using it as new acceptance).

Complete when all tasks/gates have evidence, failures are resolved or honestly scoped, fresh captures inspected, docs reproduce two-window play and recovery, and final three-pass audit is recorded.

## Acceptance matrix and risks

| Surface | Required evidence | Current evidence |
| --- | --- | --- |
| Single-process mock nodes | Real adapter, fault injection, identities and privacy | Pending |
| Two Windows game processes | Real Veilid, UI menu/seats/agreed gameplay, crash recovery | Pending |
| Windowless puppets | Same actions, no visible windows, actual GPU capture inspected | Pending |
| CLI sibling device | Same authorization and observations, no extra vote weight | Pending |
| Browser/other OS | Outside this slice; retain prior artifacts without claiming new validation | Not targeted |

Critical risks: credential recovery without surviving room state (G2/T5); stolen recovery capability (G1/T5); same-process mock falsely proving interprocess networking (T2/T6); speculative animation mistaken for accepted play (T4); all peers receiving private faces (T4); name collision granting identity (T5); feature unification accidentally shipping mocks (T2); scope drifting into general engine/UI rewrite (G4). Each must have a concrete negative test or documented gate disposition before release.
