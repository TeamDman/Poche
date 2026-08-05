# Poche phase 2: multiplayer sessions, text protocol, rendering, and reinforcement learning

**Plan ID:** `poche-phase-2`

**Plan status:** Execution in progress

**Primary implementation root:** `D:\Repos\Games\poche-3` on branch `model-checking`

**Predecessor:** `PLAN.md`, whose formal-modeling milestone is complete and must remain a truthful historical record

**Last updated:** 2026-08-05

## How to update this plan

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

Put status in each task heading and update its adjacent completion notes at the
same time. Record decisions, commit IDs, exact commands, output summaries, and
follow-ups under the task they affect; do not append a detached chronological
log. Keep no more than one task in progress unless the tasks are explicitly
independent.

Before changing scope, architecture, ordering, or acceptance criteria, reread
the guidance ledger and design gates. Add new material guidance with a stable
`U` ID. Never remove or silently generalize an active entry. If guidance
changes, mark the old row `Superseded by Ux`, add the replacement row, and
update traceability and affected tasks.

The plan is only ready once we have literally triple checked that no intent from the user has been omitted without explicit direction from the user.

## Authoritative user guidance ledger

This ledger continues the completed predecessor's U1-U29 sequence so IDs remain
unambiguous across the project. The predecessor requirements remain historical
authority for what was built; the rows below govern this phase.

| ID | Active guidance | Required plan consequence |
| --- | --- | --- |
| U30 | Start an actionable next phase in a new plan file because the old plan is complete and a separate file will improve context management. | This file is self-sufficient, preserves `PLAN.md`, and has its own tasks, gates, traceability, audits, and completion criteria. |
| U31 | Continue in three areas: reinforcement learning, multiplayer, and rendering. Rendering is useful but should not block a Codex-implemented learning environment. | All three tracks are in scope; the protocol, session core, and RL rollout path precede decorative UI work. |
| U32 | A text protocol is more inspectable by Codex. Multiplayer should be backed by that protocol and made pretty later. | Define versioned typed commands/events plus canonical NDJSON transcripts before network or graphical clients; every renderer consumes the same viewer projection/event stream. |
| U33 | Explore `D:\Repos\rust\veilid` and use Veilid to make a web app in which a host creates a room code that other people use to join. | Build a transport-independent room runtime, a native Veilid adapter, a compact invite flow, and a web-client viability spike before promising direct static-hosted Veilid connectivity. |
| U34 | Use the lobby/RBAC thinking in `G:\Programming\Repos\TPBAC`; Veilid's cryptographic model suggests authorizing actions according to what keys may do. | Model principals, signed attempts, default-deny policy decisions, scoped capabilities, grants, revocations, and audit reasons independently of transport identity. |
| U35 | Rooms add states: a host creates the room; players join; players are ready or not; a Halo-like start countdown can be aborted by players; and a running game can be paused. | Add a pure room/session state machine with explicit commands, events, actors, countdown deadlines/ticks, pause/resume policy, reconnect, and terminal/closed states. |
| U36 | The added room state space should be supported by the formal mechanism already built. | Give session rules stable IDs and independent Rust, Alloy, NuSMV, and Scryer Prolog models; use bounded composition and conditional liveness rather than taking the full game/session/network Cartesian product. |
| U37 | RL rollouts need a fast loopback or in-process path that does not spam an external network. | Use typed direct calls and preallocated batches for training; retain an in-process protocol transport for parity tests, but never make Veilid or text parsing part of the rollout hot path. |
| U38 | A CLI would help people run the program and create or join rooms; use `G:\Programming\Repos\teamy-rust-cli` as the template. | Add a Poche CLI with Facet/Figue-style subcommands, human and machine-readable output, structured diagnostics, cancellation, and transcript/replay commands; port patterns selectively without overwriting the workspace. |
| U39 | Rooms could support chat. | Include bounded ephemeral room chat as an authorized session side stream; keep message content outside formal game state and make persistence/moderation limits explicit. |
| U40 | Explore long-lived keys so a participant can rejoin the same room after the initial room-code exchange. | Separate stable application identity from Veilid node/route identity; persist secrets safely, issue durable room membership after invite redemption, and test reconnect without reusing the invite code. |
| U41 | Support spectators who can request permission to see a player's hand; the player can grant and revoke that access. | Add explicit request/grant/revoke commands, per-viewer projections, scoped capability epochs, encrypted recipient delivery, and the caveat that revocation prevents future disclosure but cannot erase information already observed. |
| U42 | Explore Burn in `G:\Programming\Repos\burn` for GPU tensor math. | Keep the environment framework-independent, then implement a Burn-backed learner/inference adapter with a CPU reference backend and at least one GPU backend smoke/performance run. |
| U43 | Use PufferLib in `G:\Programming\Repos\pufferlib` as a good reinforcement-learning reference while implementing our own system. | Borrow vectorized contiguous buffers, action masks, rollout horizons, policy/evaluation separation, and frozen-policy self-play concepts; do not add Python or PufferLib as a runtime dependency. |
| U44 | The prior round-end score decision remains important: score is a meaningful intermediary reward in Poche because score is the objective. | Version the RL reward projection; the default candidate emits the acting seat's raw round points at `RoundScoreEvent` and zero between score boundaries, while keeping outcomes and money separate. |
| U45 | MPL-2.0 remains the preferred project license. | New first-party crates, protocol specifications, models, and source files use MPL-2.0; third-party dependencies and copied ideas receive a license/provenance audit. |
| U46 | Research how the principles of the existing formal project can continue evolving, not merely bolt features onto it. | Preserve strong types, explicit chance/action ownership, viewer-scoped observations, semantic hashes, independent oracles, controlled defects, exact scopes, and honest evidence labels throughout this phase. |
| U47 | If Veilid cannot support a browser-only client with no on-device companion, that result should inform the broader technology direction, including whether a Vulkan renderer is worth its loss of web portability. Use `G:\Programming\Repos\cursor-latency` and `G:\Programming\Repos\ash` as local rendering references. | Treat browser-only Veilid as an architectural fork, not a pass/fail deployment footnote. Compare a portable client against a native Vulkan path using measured portability, latency, complexity, distribution, and privacy consequences before committing the renderer architecture. |
| U48 | Any player may pause the game, and while it is paused any player may unpause it. | Make pause and unpause single authorized player commands with no voting, acknowledgement quorum, or host override. Verify the exact policy in Rust and every applicable oracle, including repeated/concurrent/idempotent commands. |
| U49 | Keep the first UI simple; egui is an acceptable preferred choice. | Use egui as the default client-renderer candidate and require a concrete incompatibility or materially better evidence to replace it. Keep rendering a projection consumer so native and web targets can share UI logic when the selected egui backend permits. |
| U50 | If Veilid does not work well for a browser-only client, investigate Datastar and a hostable server binary as a less-anonymous alternative, using `G:\Programming\Repos\datastar` and `G:\Programming\Repos\datastar-rust`. | Spike a Datastar/Rust server topology alongside the browser-Veilid test. Compare self-hosting, server trust and metadata visibility, identity, room routing, reconnect, operations, and protocol parity; publish the anonymity/trust tradeoff rather than presenting it as equivalent to Veilid. |

## Intent audit evidence

- **Pass 1 — extraction:** Reread the complete current request and the relevant
  confirmed predecessor guidance. U30-U50 preserve the three tracks, text-first
  ordering, Veilid room-code web goal, TPBAC/key authorization, every named room
  state, fast loopback, CLI template, chat, durable reconnect identity,
  spectator hand grants/revocation, Burn, PufferLib-as-reference, round scores,
  MPL-2.0, the request to extend the existing principles, the exact any-player
  pause/unpause policy, the egui preference, the native Vulkan portability fork,
  and a potentially less-anonymous Datastar/server fallback.
- **Pass 2 — traceability:** Verified every active U30-U50 row maps to at least
  one concrete task and completion criterion. Checked protocol, session,
  authorization, formal-model, CLI, Veilid, rendering, RL, training, evidence,
  and documentation sections for weakened or missing requirements.
- **Pass 3 — adversarial omission:** Specifically searched for likely losses:
  renderer work accidentally blocking RL; text serialization contaminating the
  rollout hot path; Veilid being treated as authorization; room codes becoming
  permanent bearer credentials; DHT schemas being assumed mutable; browser
  Veilid being promised on HTTPS without evidence; treating a native companion
  as an automatic fallback; choosing Vulkan without accounting for the browser
  cost; describing a Datastar host as equally anonymous; letting egui own game
  semantics; chat or wall-clock values
  exploding formal state; pause invalidating an unconditional termination
  claim; spectators receiving globally broadcast hidden state; revocation being
  described as erasure; PufferLib becoming a dependency; Burn owning game
  semantics; score being replaced with win/loss shaping; or the completed plan
  being rewritten. Each has an explicit disposition below.
- **Known source limitation:** None. The user messages, predecessor plan, local
  repositories, and current primary documentation were available during plan
  creation.

### Final execution intent audit (2026-08-05)

- **Pass 1 — extraction rerun:** Reread U30-U50 literally against the completed
  implementation and handoff documentation. All 21 active requirements retain
  their original qualifiers: three parallel tracks; text-first inspection;
  Veilid browser feasibility as a topology decision; TPBAC-shaped key
  authorization; complete room/countdown/pause lifecycle; independent formal
  session models; no-network RL rollouts; CLI-template patterns; ephemeral chat;
  stable-key reconnect; future-only spectator revocation; Burn CPU/GPU;
  Puffer-inspired but Rust-native batching; raw round score; MPL-2.0; preservation
  of the prior formal principles; any-player pause/unpause; egui-first rendering;
  and the less-anonymous Datastar fallback.
- **Pass 2 — traceability rerun:** Checked every U30-U50 row one-to-one against
  the Guidance traceability table, all 42 completed task records through 9.3,
  the 88-row session coverage audit, the four decided topology/RL closure gates,
  and the README/contributor handoff. Every row has executable or documentary
  evidence in its mapped task; no row is credited only by another requirement.
- **Pass 3 — adversarial omission rerun:** Searched the final tree and evidence
  for stale `planned`/`not implemented` claims, browser-Veilid overclaims,
  companion-app assumptions, room-code authority, transport-ID authority,
  unconditional session termination, revocation-as-erasure, room-broadcast
  hands, host-trust anonymity claims, text/network in the RL hot path,
  reward-shaping drift, proof/empirical conflation, untracked generated
  artifacts, secret-bearing fixtures, and changes to `PLAN.md`. The remaining
  limitations are explicit deferred production work; no user intent was omitted
  or weakened without the user's direction.

## Outcome

Deliver one coherent Poche application substrate with four interchangeable
consumers:

1. a replayable text CLI;
2. a loopback or Veilid-backed multiplayer room;
3. a minimal viewer-correct egui-first client, with browser-only Veilid or a
   hostable Datastar topology selected from evidence;
4. a vectorized RL rollout/training system using Burn for tensors.

The common center is not a socket or UI. It is a deterministic typed protocol
and two composed pure state machines:

```text
signed command
    -> authorization decision with reason
    -> SessionState reducer
       -> optional GameEnvironment transition
    -> ordered events
    -> viewer-specific projection
       -> NDJSON / CLI / web / network recipient

GameEnvironment + RlSpec
    -> typed in-process batched rollout
    -> observation tensors + legal-action masks
    -> Burn policy/value inference
    -> actions
    -> raw round-score reward projection
```

The session layer controls membership, readiness, countdown, pause, chat, and
visibility grants. The existing game layer remains authoritative for deals,
bids, card plays, scoring, and termination. Transport state, wall clocks,
rendering state, and neural-network state belong to adapters, not either
semantic reducer.

## Scope

### In scope

- A stable session-rule catalog and room/capability coverage matrix.
- Versioned command, event, snapshot, projection, decision, error, and
  transcript schemas reflected with Facet.
- Canonical line-delimited JSON (NDJSON) for inspectable transcripts and CLI
  automation; Phon remains an optional canonical binary/checkpoint codec rather
  than being mislabeled as text.
- Pure deterministic session authorization and event reduction.
- Host, player, and spectator principals; room-scoped capabilities; explicit
  hand-view requests, grants, and revocations.
- Lobby join/leave/reconnect, ready/unready, countdown arm/abort/expiry,
  start-game, pause/resume, post-game, close, and bounded chat semantics.
- Independent Rust, Alloy, NuSMV, and Scryer Prolog session models with a named
  finite lobby scope and cross-model evidence.
- An in-process multi-client transport and a scriptable CLI.
- Veilid 0.5.x integration for rendezvous, private-route messaging, durable
  identities, invite redemption, reconnect, and network fault handling.
- A minimal web UI for room and game interactions, with its deployment mode
  selected only after browser/HTTPS Veilid evidence is recorded.
- Versioned fixed-shape RL observation/action/reward specifications, legal
  action masks, batched CPU rollouts, deterministic replay, and random/heuristic
  baselines.
- A first Burn actor-critic learner, self-play against frozen checkpoints,
  reproducible evaluation, and CPU/GPU backend comparison.
- Text and web replay of selected human games and RL episodes.
- Small committed evaluation summaries and semantic manifests; large generated
  traces, weights, binaries, and training logs remain ignored or CI artifacts.

### Out of scope for this phase

- Rewriting or contributing AI-generated changes upstream to Veilid. Its local
  `AGENTS.md` permits architectural research but not AI-authored implementation
  contributions; Poche consumes Veilid's public API only.
- Trustless dealing, mental poker, multiparty computation, consensus, host
  migration, or proof that a malicious authoritative host did not inspect or
  manipulate hidden state.
- Global matchmaking, parties across rooms, clans, friends, public discovery,
  ranking services, accounts hosted by Poche, voice, file transfer, or durable
  chat history.
- Treating possession of a room code, Veilid node ID, or transport route as
  sufficient authorization after membership establishment.
- Formally verifying cryptographic primitives or the live Veilid network. The
  formal model abstracts signature unforgeability and delivery faults and tests
  application policy around those assumptions.
- Full Cartesian exploration of 52-card game state x room state x network
  queues x clock time. Formal claims use decomposition, finite abstractions,
  and named fairness assumptions.
- Decorative card art, animation, sound, polished responsive layout, or making
  a renderer a prerequisite for training.
- One policy tensor shape that supports every player count from 2 through 51.
  Each versioned `RlSpec` has a fixed player count/shape; the first pipeline is
  two-player and later specs may add more counts.
- A claim that the first learned policy is optimal, formally verified, or
  generally strong. Training is empirical evidence and remains distinct from
  rule/model evidence.
- Distributed multi-GPU training, CUDA kernel authoring, a generic RL library,
  or a generic TPBAC library extraction before Poche supplies measured reuse
  evidence.

## Verified planning baseline

### Existing Poche foundation

- `D:\Repos\Games\poche-3` is clean and synchronized at commit `a6ee40c` on
  `model-checking` as of 2026-08-04.
- `PLAN.md` is execution-complete. It established MPL-2.0, 61 game-rule IDs,
  independent Rust/Alloy/NuSMV/Prolog models, explicit finite checking,
  cross-model conformance, and GitHub Pages rulebook publication.
- `poche-environment::GameEnvironment` already separates complete state,
  per-viewer observation, legal player actions, explicit chance, deterministic
  environment settlement, raw `RoundScoreEvent`, and final outcome.
- Existing observations hide opponents' hands. The RL/session work must audit
  whether reconnecting clients and memoryless policies also receive sufficient
  public history; it must not solve that by exposing hidden state.
- `poche-interchange` already carries model/rules/observation/scoring semantic
  hashes, scopes, confidence kinds, states, observations, actions, transitions,
  and traces. Phase 2 extends those identities instead of inventing an
  unrelated evidence format.
- The current `poche-xtask guidance audit` is hard-coded to predecessor U1-U29,
  G1-G14, and its task list. It must be generalized without making the completed
  audit weaker or silently editing `PLAN.md`.

### Local reference audit

| Reference | Verified revision/state | Applicable evidence |
| --- | --- | --- |
| `D:\Repos\rust\veilid` | `76b2176`, main, Veilid core 0.5.7, clean | MPL-2.0; native and WASM APIs; DHT records/watches; private routes; AppCall/AppMessage; protected/table stores; browser constraints. |
| `G:\Programming\Repos\TPBAC` | `f7c55c1`, main, clean | Lobby/party vocabulary; principal/action/attempt/result split; default deny; deny override; policy reasons; enforcement versus audit. |
| `G:\Programming\Repos\teamy-rust-cli` | committed baseline `7e62d72`; local worktree has unrelated uncommitted edits | Facet/Figue parsing, text/JSON/CSV output, structured logs, cancellation, build metadata, CLI fuzzing. Use the clean commit as reference and never copy the dirty worktree wholesale. |
| `G:\Programming\Repos\burn` | `546cacb`, tag v0.21.0, detached, clean | Backend-generic tensors/autodiff, Flex/WGPU/CUDA/ROCm, records/checkpoints, `burn-rl` traits, async policy batching, off-policy trainer, DQN example. |
| `G:\Programming\Repos\pufferlib` | `c5d3c63`, branch 4.0, clean | Contiguous preallocated rollout buffers, action masking, PPO-style rollout/train split, multi-agent fixed layouts, frozen-policy self-play, evaluation and performance measurement. |
| `G:\Programming\Repos\facet\phon` | local main at `adac882`; parent worktree dirty outside Phon | Phon is a typed **binary** format/execution engine, so it is suitable for a binary codec/checkpoint but not the requested human-readable text transcript. |
| `G:\Programming\Repos\cursor-latency` | `6c07705`, main, clean | MPL-2.0 native Windows/winit/ash reference with an explicit Vulkan swapchain and latency work. Use as native-rendering and latency evidence, not as a web-capable renderer assumption. |
| `G:\Programming\Repos\ash` | `a9a1fb1`, master, clean | MIT/Apache-2.0 low-level Vulkan bindings and window interop. Its explicit loader, device, surface, swapchain, synchronization, and driver requirements make the native complexity and portability tradeoff concrete. |
| `G:\Programming\Repos\datastar` | `85aa51ed`, develop, clean | MIT hypermedia framework reference for a browser UI driven by a hostable server rather than an on-device networking companion. |
| `G:\Programming\Repos\datastar-rust` | clean commit `b88ad8a`; local worktree has an unrelated modified `Cargo.lock` | MIT Rust SDK with Axum, Rocket, and Warp integrations plus SSE event/reconnect primitives. Use the clean commit as reference and preserve the dirty worktree. |

### Current primary-source findings

- Veilid's DHT is an asynchronous discoverability/data layer. Record schemas are
  immutable; DFLT gives the owner writable subkeys and SMPL allocates fixed
  writer subkey ranges. Dynamic lobby membership therefore cannot be implemented
  by mutating a SMPL writer list. The first room rendezvous record should be
  host-owned, with dynamic authorization in the Poche session layer.
- Veilid signs DHT writes, but it explicitly does not interpret or authorize
  AppCall/AppMessage payloads. Poche must authenticate and authorize every
  command, reject replay/room/revision mismatches, and return a policy reason.
- Veilid AppCall/AppMessage payloads are bounded (currently 32,768 bytes), and
  current APIs expose private routes, DHT record create/open/get/set/watch, and
  protected/table stores. High-frequency room commands belong on private-route
  messaging; DHT is rendezvous/recovery metadata, not the game event bus.
- Veilid 0.5.7's WASM package runs browser nodes over WebSockets, but its README
  documents browser socket/DNS restrictions and says HTTPS/WSS outbound relay
  support is not yet implemented. A GitHub Pages-hosted direct Veilid client is
  therefore a measured architectural gate; success preserves a browser-only,
  decentralized client, while failure changes the client/server and renderer
  choice rather than silently requiring an on-device companion.
- The local native rendering references expose the cost of a direct Vulkan
  direction: ash intentionally mirrors Vulkan and cursor-latency explicitly owns
  instance/device/surface/swapchain/synchronization work. That may buy control
  and measurable latency, but it gives up the straightforward browser target.
- Datastar's Rust SDK can stream server-generated UI changes over SSE through
  conventional Rust HTTP servers. It is a credible browser-portable fallback
  topology if browser-only Veilid fails, but introduces an operator-visible,
  less-anonymous server trust and metadata boundary that must be documented.
- Burn 0.21 provides an RL crate and DQN example, but the shipped trainer is
  principally off-policy/single-environment shaped. Poche needs turn-based
  multi-agent reward attribution, legal masks, partial observations, and
  self-play. Burn should provide tensors, autodiff, devices, optimizers, and
  records while Poche owns the rollout/PPO semantics.
- PufferLib 4.0 reinforces a performance direction rather than an API
  dependency: allocate fixed buffers once, batch active agents, avoid
  observation copies/parse work, keep evaluation separate, and train against
  frozen historical opponents.

## Architecture decisions and gates

`Decided` rows are binding for this plan. `Provisional` rows are the recommended
default and must be confirmed by the named task before dependent implementation.
`Open` rows stop dependent work if evidence does not select an option.

| ID | Status | Question | Disposition / default | Evidence required to change or close it |
| --- | --- | --- | --- | --- |
| G15 | Decided | Does multiplayer state become part of `GameState`? | No. `SessionState` composes an optional game instance and controls whether game transitions may occur. Network/render/RL state stays outside both reducers. | A concrete rule that cannot be expressed by composition and a state-space/conformance analysis. |
| G16 | Decided | What is the inspectable protocol? | Facet-reflected typed envelopes with canonical versioned NDJSON, one complete envelope per line. Phon may add a binary codec only after semantic parity tests. | Codec fixtures must round-trip with identical semantic hashes and unknown-version rejection. |
| G17 | Decided | Does RL traverse NDJSON or a socket? | No. RL calls the same typed game/action semantics in process and uses preallocated batches. In-process protocol transport exists for parity and debugging only. | Benchmark and parity evidence; never external network traffic during ordinary rollouts. |
| G18 | Decided | First multiplayer authority model? | Host-authoritative room reducer and event log; signed participant commands; signed host events; no host migration. The host is trusted with full hidden game state for this phase. | Closed by ADR 0003, reducer/policy tests, loopback/native lifecycle acceptance, and the user-visible trust disclosures. |
| G19 | Decided | What does a room code authorize? | A versioned/checksummed rendezvous locator plus expiring or one-time invite secret. Successful redemption binds a stable player public key to membership; the code is not a permanent bearer authority. | Closed by Task 5.2 codec/replay/cross-room/revocation tests and the public two-node redemption run. |
| G20 | Decided | What identity persists? | An application-level player signing key stored through protected storage. Veilid node IDs and private routes are replaceable transport identities. Room membership refers to the stable application key. | Closed by protected-store restart, signed membership recovery, route-rotation, reconnect, and public native refresh evidence. |
| G21 | Decided | How are authorization decisions modeled? | TPBAC-shaped immutable attempt and decision records, default deny, explicit allow, deny override, stable policy IDs/reasons, and audit-only policy support. | Cross-model authorization fixtures and controlled defects. |
| G22 | Decided | First countdown behavior? | Host may arm only when minimum seats exist and every seated player is ready. Any seated player may unready or abort, cancelling the countdown. An authority clock emits a logical expiry event; expiry starts once if preconditions still hold. | Session rules and NuSMV/Rust liveness checks under named clock fairness. |
| G23 | Decided | Who may pause/resume? | Any active player may pause a running game. While paused, any active player may unpause it. There is no vote, acknowledgement quorum, or special host override. Duplicate/same-revision commands remain idempotent under normal command ordering rules. | Rust/oracle coverage for authorization, pause blocking game advancement, any-player unpause, and concurrent/repeated commands. |
| G24 | Decided | Is chat game state? | No. Chat is an authorized, rate/size-bounded session event stream with ephemeral first-phase retention. Formal models track send permission/count abstractly, not text content. | Protocol and policy tests; persistence remains deferred. |
| G25 | Decided | What does spectator revocation mean? | Stop future hand projections/delivery at the next capability epoch. Never claim already delivered information can be forgotten. Hidden observations are produced per recipient and never room-broadcast. | Projection/noninterference tests and an Alloy bounded information-flow model. |
| G26 | Decided | Can a browser-only public web app run Veilid with no on-device companion? | HTTP/WS passes without a companion, but the documented public bootstrap resets WSS before TLS and upstream 0.5.7 has no outbound-relay HTTPS topology. Direct Pages multiplayer is not advertised. | ADR 0004 selects a self-hostable, host-colocated Datastar authority for live browsers; native Veilid remains available without imposing a companion. |
| G27 | Decided | First RL tensor shape? | `poche-2p-v1`: fixed two-player full-rule game, seat-relative viewer encoding, fixed bid/card action vocabulary, legal mask, and explicit public-history strategy. Add other player counts as new specs. | Closed by the immutable manifest/hash, hidden-state noninterference, scalar/batch parity, transcript replay, and Burn shape checks. |
| G28 | Decided | First reward projection? | `round-score-v1`: zero except at a round boundary, then the seat's raw rulebook points; terminal outcome and money are separately logged. No undocumented shaping or reward clipping. | Closed by exact settlement fixtures, same-seat transition assembly, GAE tests, score-first baselines, training, and held-out evaluation. |
| G29 | Decided | First learning algorithm? | Implemented a small actor-critic PPO/GAE loop in Rust over Burn, with legal-logit masking and self-play against frozen checkpoints. Burn DQN informed API usage only. | Task 8.1 controlled optimum passes on Flex; the identical update/checkpoint path passes WGPU; ADR 0003 and `docs/burn-learning.md` record the result. |
| G30 | Decided | What liveness can be claimed once pause/network exist? | Preserve unconditional game termination only for the existing semantic game under its named scope. Session liveness is conditional on clock, delivery, player-action, and eventual-resume fairness. Paused/partitioned sessions may legitimately persist. | NuSMV/Rust properties must state assumptions and include counterexamples when each fairness assumption is removed. |
| G31 | Decided | How is state-space explosion controlled? | Independently model game, session/authorization, and abstract transport; compose contracts and a small integration scope. Bound principals/messages/ticks and omit chat content/cryptographic bitstrings. | Coverage matrix and exact scope statements for every formal result. |
| G32 | Decided | Which first client UI stack is used? | Keep the renderer-neutral `PresentationModel`; use shared egui for native/static-WASM replay and semantic HTML for the accessible live browser. Raw Vulkan remains a bounded future latency experiment. | ADR 0004 and `docs/rendering-topology-spike.md` record the executable portability, latency, distribution, accessibility, operations, privacy, and complexity comparison. |

## Security and protocol invariants

The implementation and all applicable oracle tracks must give each item a
stable `S-*` rule/property ID and evidence:

- A transport peer, room-code holder, spectator, or former member has no
  authority unless an active policy/capability explicitly grants the attempted
  action.
- Every accepted command binds protocol version, room ID, session epoch,
  principal key, command ID, expected revision, payload, and signature.
- Duplicate command IDs are idempotent; stale epochs/revisions and cross-room
  replays are rejected without applying partial events.
- Events form one authority-ordered revision sequence; snapshots commit to the
  latest revision and semantic schema hash.
- Lobby membership and seat occupancy are distinct. Spectators are members but
  do not count as ready players or receive a hand by default.
- Countdown can be armed only under readiness preconditions; abort/unready wins
  over a same-revision expiry; a game starts at most once.
- Paused sessions reject game actions and chance/settlement advancement. Chat,
  reconnect, visibility grants, and the selected pause protocol remain explicit
  session actions.
- Ordinary player projections contain public state plus that player's hand.
  Spectator projections contain no private hand unless a current player-scoped
  grant authorizes that exact recipient and epoch.
- Granting one spectator does not grant other spectators. Revocation prevents
  every later projection/encrypted delivery for that capability ID.
- Public event history required to reconstruct legal knowledge survives
  snapshot/reconnect without exposing cards that were never public.
- Secret keys, invite secrets, unredacted states, and private hands never appear
  in human logs, error strings, chat, public DHT values, or room-wide events.
- Chat is size/rate bounded, attributed to an authorized member, and cannot
  contain protocol control frames by parsing ambiguity.

## Source references

### Local

- `D:\Repos\Games\poche-3\PLAN.md`
- `D:\Repos\Games\poche-3\crates\poche-environment\src\lib.rs`
- `D:\Repos\Games\poche-3\crates\poche-interchange\src\wire.rs`
- `D:\Repos\rust\veilid\veilid-core\src\veilid_api\routing_context.rs`
- `D:\Repos\rust\veilid\veilid-wasm\README.md`
- `D:\Repos\rust\veilid\veilid-core\examples\private_route\src\main.rs`
- `G:\Programming\Repos\TPBAC\paper\main.typ`
- `G:\Programming\Repos\teamy-rust-cli\README.md`
- `G:\Programming\Repos\burn\crates\burn-rl\src\`
- `G:\Programming\Repos\burn\examples\dqn-agent\`
- `G:\Programming\Repos\pufferlib\pufferlib\torch_pufferl.py`
- `G:\Programming\Repos\pufferlib\pufferlib\selfplay.py`
- `G:\Programming\Repos\skills\.github\skills\resumable-implementation-plans\SKILL.md`

### Authoritative external documentation

- [Veilid developer book: applications](https://veilid.gitlab.io/developer-book/apps/index.html)
- [Veilid developer book: DHT](https://veilid.gitlab.io/developer-book/concepts/dht.html)
- [Veilid developer book: AppCall and AppMessage](https://veilid.gitlab.io/developer-book/apps/api/appmessaging.html)
- [Veilid 0.5.7 `RoutingContext`](https://docs.rs/veilid-core/0.5.7/veilid_core/struct.RoutingContext.html)
- [Burn 0.21 API and backends](https://burn.dev/docs/burn/index.html)
- [Burn book](https://burn.dev/books/burn/)
- [PufferLib 4.0 repository](https://github.com/PufferAI/PufferLib/tree/4.0)
- [PufferLib documentation](https://puffer.ai/docs.html)

## Guidance traceability

| Guidance | Tasks / gates / evidence |
| --- | --- |
| U30 | This file, Task 1.1, Task 9.4 |
| U31 | Outcome, execution order, Phases 4, 6, 7, and 8 |
| U32 | G16-G17, Tasks 2.1-2.5, 4.3, 6.2 |
| U33 | G18-G20, G26, Tasks 5.1-5.6, 6.1-6.4 |
| U34 | G21, security invariants, Tasks 1.3, 2.2, 3.1-3.5, 5.1 |
| U35 | G15, G22-G24, G30, Tasks 1.2-1.3, 2.2, 3.1-3.5, 4.2 |
| U36 | G30-G31, Phase 3, Task 9.2 |
| U37 | G17, Tasks 4.2, 7.2-7.5 |
| U38 | Task 4.1-4.5 |
| U39 | G24, Tasks 1.3, 2.2, 4.4, 5.4, 6.3 |
| U40 | G19-G20, Tasks 5.1-5.3, 5.6 |
| U41 | G25, security invariants, Tasks 2.3, 3.1-3.5, 5.5, 6.3 |
| U42 | Tasks 8.1-8.5, Task 9.3 |
| U43 | Research baseline, G29, Tasks 7.2-7.4, 8.3-8.4 |
| U44 | G28, Tasks 7.1, 7.3, 8.5 |
| U45 | Scope, Task 1.2, every new source task, Task 9.1 |
| U46 | G15-G32, Phases 1-3, Tasks 7.1, 9.1-9.4 |
| U47 | Current findings, G26, G32, Tasks 6.1-6.4, risk register |
| U48 | G23, Tasks 1.3, 2.2, 2.4, 3.1-3.5, 4.2-4.3, 6.2-6.3 |
| U49 | G32, Tasks 6.1-6.4 |
| U50 | Current findings, G26, G32, Tasks 1.2, 6.1-6.4, 9.2, risk register |

## Execution order

1. Generalize plan auditing; record authority/pause/web-test/RL-spec gates; write
   session rules and the threat model.
2. Implement the pure protocol, authorization, session reducer, projections,
   and deterministic transcripts.
3. Build and compare independent session models before networking can obscure
   semantic mistakes.
4. Deliver a complete loopback CLI vertical slice.
5. Add native Veilid rooms, identity, invites, reconnect, chat, and private
   projections.
6. Add the minimal egui-first renderer and select browser-only Veilid,
   native-client, or hostable Datastar delivery from evidence.
7. Implement and benchmark typed vectorized RL rollouts and baselines.
8. Add Burn inference/training, PPO self-play, checkpoints, and evaluation.
9. Run aggregate acceptance, publish honest evidence, commit, and push.

Phases 5-6 and 7-8 become independent after Phase 4. They may be worked in
parallel only when separate agents/worktrees are explicitly authorized and each
track updates this file without overlapping edits.

## Phase 1 - Preserve intent and lock the semantic boundary

### [x] 1.1 Generalize resumable-plan auditing without weakening `PLAN.md`

Implement a plan-profile or data-driven audit in `poche-xtask` so the command
can validate both the completed predecessor and this plan. It must derive or
declaratively load required guidance/gate/task IDs, audit markers, readiness
sentence, and completion counts rather than replacing the old constants with a
new set of hard-coded phase-2 constants.

Add regression fixtures/tests proving that omitted guidance mappings,
noncontiguous IDs, unfinished tasks in a claimed-complete plan, missing audit
passes, duplicate task IDs, and unknown plan IDs fail. Preserve:

```powershell
cargo run -p poche-xtask -- guidance audit PLAN.md
cargo run -p poche-xtask -- guidance audit PLAN-2-MULTIPLAYER-RL-RENDERING.md
```

**Completion criteria:** Both plans audit with their correct status; predecessor
negative tests still fail; the new mechanism cannot silently accept a plan by
identifying it as the other profile.

**Completion notes (2026-08-04):** Complete. Added checked-in declarative audit
contracts under `tools/plan-audit/` and selected them by explicit Plan ID plus an
exact-title legacy bridge for unchanged `PLAN.md`. The auditor now applies each
profile's status lifecycle, exact guidance/gate/task IDs, table shape, audit and
adversarial markers, task criteria/notes, completion count, and deferred
sections. Twelve `poche-xtask` tests pass, including negative cases for omitted
mappings, noncontiguous IDs, unfinished tasks under a completed status, missing
audit passes, duplicate task IDs, unknown plan IDs, and cross-profile identity
spoofing. `cargo fmt --all -- --check`, `cargo clippy -p poche-xtask
--all-targets -- -D warnings`, `cargo test -p poche-xtask`, and both required
`guidance audit` commands pass. `git diff --exit-code -- PLAN.md` confirms the
predecessor remained byte-for-byte untouched.

### [x] 1.2 Record composition, threat model, dependency, and open-gate decisions

Create `docs/decisions/0003-session-network-rl-architecture.md` (using the next
available ADR number if 0003 is occupied). It must:

- confirm decided/provisional G15-G25 and G27-G31 or record a superseding
  decision, including G23's decided any-player pause/unpause semantics;
- preserve G26 and G32 as evidence-selected gates until Task 6.1 while recording
  their authorized options and exact selection criteria;
- define what the host, player, spectator, DHT cache, network peer, and web host
  are trusted to learn or do;
- state plainly that first-phase host authority can inspect all hands and that
  trustless dealing is out of scope;
- define app identity versus Veilid node/private-route identity;
- pin released MPL-compatible dependency versions, avoiding sibling path
  dependencies and dirty local worktrees;
- record the Veilid upstream AI-contribution constraint and that no upstream
  code changes are planned;
- pre-register the browser/topology/renderer test matrix that closes G26 and
  G32, including direct browser-only Veilid, egui, native Vulkan evidence, and a
  Datastar server binary without treating their privacy models as equivalent;
- pre-register the first `RlSpec`, reward, baselines, algorithm experiment, and
  evaluation metrics before training.

**Completion criteria:** No dependent task relies on an unrecorded authority,
pause, persistence, browser, reward, or dependency assumption. License files and
source provenance are compatible with MPL-2.0.

**Completion notes (2026-08-04):** Complete. Added accepted ADR
`docs/decisions/0003-session-network-rl-architecture.md`. It fixes the separate
game/session composition, signed host-authoritative threat model, host access to
all hands, app-key versus Veilid identity, one-time invite/durable membership,
TPBAC decision shape, exact any-player pause/unpause semantics, countdown race,
chat and spectator boundaries, and conditional session liveness. It registers
the no-companion Veilid/egui/Datastar/Vulkan matrix for G26/G32 and preregisters
`poche-2p-v1`, `round-score-v1`, PPO/GAE baselines, hyperparameters, and score-led
evaluation. Published exact dependency candidates and licenses were verified;
notably `figue = 5.0.0-rc.5` targets the existing Facet 0.50 release line while
stable Figue 4 targets Facet 0.46. The Veilid `AGENTS.md` restriction is recorded
and no reference repository was modified. `git diff --check` and the phase-2
guidance audit pass.

### [x] 1.3 Extract stable session and authorization rules

Create:

- `docs/session-rules.md` with `S-ROOM-*`, `S-AUTH-*`, `S-VIEW-*`,
  `S-CHAT-*`, `S-TIME-*`, and `S-FAULT-*` IDs;
- `docs/session-coverage.md` with Rust/Alloy/NuSMV/Prolog/protocol/network/UI
  dispositions;
- `docs/capability-matrix.md` mapping principal kinds and capability scopes to
  commands, projections, and deny reasons.

Every rule must identify its human guidance/ADR source, preconditions, actor,
state transition or observation effect, failure result, and whether it is
game-semantic, session-semantic, transport, security assumption, or UI only.

**Completion criteria:** Every named lifecycle/chat/spectator/reconnect behavior
and every security invariant has an immutable ID and full-track disposition;
unknown commands and roles are default-deny.

**Completion notes (2026-08-04):** Completed. `docs/session-rules.md` assigns 88
immutable IDs across room, authorization, visibility, chat, logical-time, and
fault behavior; every row records source, actor, preconditions, accepted effect,
failure, and semantic class. `docs/session-coverage.md` gives every ID an
explicit Rust/Alloy/NuSMV/Prolog/protocol/network/UI disposition, including
reasoned non-applicability rather than silent omissions.
`docs/capability-matrix.md` defines derived principal kinds, room/epoch/resource-
scoped capabilities, command policy, stable deny reasons, normative evaluation
order, and viewer projection boundaries. Unknown command, role, and principal
paths are explicitly default-deny. A mechanical comparison found 88 rule rows,
88 coverage rows, zero duplicates, zero missing/extra IDs, and zero malformed
table rows. `git diff --check` and the phase-2 guidance audit pass.

## Phase 2 - Typed text protocol and pure session engine

### [x] 2.1 Add protocol and session crates with versioned envelopes

Add workspace crates:

- `crates/poche-protocol` for semantic command/event/snapshot/projection/error
  types, Facet reflection, NDJSON codec, version negotiation, signatures' signed
  bytes, semantic hashes, and size limits;
- `crates/poche-session` for the pure reducer and authorization policy;
- `crates/poche-runtime` for composing a session with one concrete
  `GameEnvironment`, a clock port, and a transport-facing command service.

Initial protocol envelopes must include `protocol_version`, `room_id`,
`session_epoch`, `command_id` or `event_id`, `principal_id`, expected/current
revision, correlation/causation IDs, payload, and signature metadata. Define
canonical signed bytes separately from diagnostic rendering. Reject unknown
versions, extra control frames, oversize messages, invalid UTF-8/JSON, and
noncanonical signed forms.

**Completion criteria:** Facet schemas are reproducible; NDJSON is one envelope
per line; canonical fixtures round-trip byte-for-byte where required; fuzzing
never panics or accepts ambiguous control input; no secret type implements a
diagnostic formatter that reveals material.

**Completion notes (2026-08-04):** Completed. Added `poche-protocol`,
`poche-session`, and `poche-runtime`. All command/event/snapshot/projection/error
roots are Facet-reflected, versioned, bounded typed frames with the required
identity, epoch, revision, correlation/causation, payload, and public signature
metadata. Commands and events have distinct unsigned shapes and domain-
separated length-framed canonical signing bytes; diagnostic JSON is never the
unframed signing input. Protocol v1 pins schema hash
`1489b2887acc117dd1a9e98d2891b8640fa4d901621b734d1f7654940c4e11ad`, an
exact signed-byte vector, and `fixtures/protocol/command-chat-v1.ndjson`.
Strict decode rejects unknown versions/tags/fields, invalid UTF-8/JSON/IDs/
signatures/payloads, CRLF/interior control delimiters, multiple/missing frames,
oversize input, and every noncanonical spelling. A valid seed, all single-byte
mutations, and 10,000 deterministic arbitrary inputs prove no decoder panic and
that every accepted input re-encodes byte-for-byte. Compile-fail tests prove
secret key material is not `Debug`, `Display`, or `Clone`. The pure API requires
an immutable allow decision before `decide`, and runtime clock/transport ports
remain outside reduction. Workspace clippy passed; all workspace tests passed
when the 75-second native conformance suite was given its own process window.

### [x] 2.2 Implement room lifecycle, policy decisions, and game gating

Use strong enums/refinements rather than boolean bags. The room phase must make
invalid combinations unrepresentable where practical, for example:

```text
Lobby -> Countdown -> Running <-> Paused -> PostGame -> Closed
```

Membership/connection status remains orthogonal but finite. Implement pure:

```text
authorize(state, attempt) -> PolicyDecision
decide(state, authorized_command) -> [SessionEvent]
apply(state, event) -> state
```

`decide` may invoke `GameEnvironment` only while `Running`, for the correct
seated actor, and with the exact current game revision. External clocks produce
logical countdown events; reducers never read wall time. Include host/create,
join, accept/reject, reconnect/disconnect, ready/unready, arm/abort/expire,
start, game action, pause/resume, post-game, close, and bounded chat events.

**Completion criteria:** Unit/property tests cover every rule and deny reason;
controlled defects make readiness, duplicate-start, pause, and default-deny
tests fail; duplicate commands are idempotent and stale revisions cannot mutate
state.

**Completion notes (2026-08-04):** Completed. `SessionState<G>` uses distinct
uninitialized/lobby/countdown/running/paused/post-game/closed variants and
validates bounded unique memberships/seats, readiness, host, and command-record
invariants after every event. Pure authorization checks duplicate identity,
room/epoch/revision, authority-derived principal/capability, default deny,
enforce-deny override, and audit-only evidence. Pure decide/apply covers create,
invite join/replay/expiry/revocation, seat/ready/countdown, same-deadline player-
first cancellation, any-player pause/unpause, game/chance/settle gating,
terminal/reset/close, release/leave/remove, transport loss, stable-key reconnect,
and attributed logical-window chat. Exact event replay is idempotent; conflicts,
gaps, and stale revisions cannot mutate state. `OracleSessionGame` composes this
gate with the existing full-rule `OracleEnvironment`, including dense seats,
typed actions, complete deck validation, seeded chance provenance, raw scores,
and terminal status. All 31 deny codes have exact wire and immutable-policy
tests. Controlled readiness, duplicate-start, paused-advance, event-order,
duplicate-seat, and unknown-principal-allow defects fail closed. Invite proof
and stored verifier diagnostics are redacted. `docs/session-engine.md` and Rust
coverage cells record the evidence. Workspace formatting/clippy, phase-2 plan
audit, all non-native-conformance workspace tests, and the task-specific suites
pass; the unchanged native conformance suite passed in the preceding Task 2.1
slice.

### [x] 2.3 Implement viewer projections and spectator capabilities

Centralize projection as a pure function of session state, recipient principal,
active capability set, and projection epoch. Add request, grant, deny, revoke,
expiry/round-boundary (as selected by ADR), and audit events. Projection must
support:

- each player's normal observation;
- spectators' public observation;
- a spectator's explicitly granted view of exactly one player's hand;
- host/admin diagnostics only through a separately named local capability, never
  an ordinary network projection.

Audit existing game observations for public-history sufficiency. Choose and
version one of: include all public played-card history in the snapshot, or make
the ordered public event prefix a required part of an observation/reconnect
state. RL and web clients must receive the same legal public knowledge.

**Completion criteria:** Pairwise noninterference tests compare every viewer;
grant/revoke tests prove future projection changes; no room-wide event contains
a private hand; a reconnecting authorized player can reconstruct all public
knowledge and only their own current private state.

**Completion notes (2026-08-04):** Completed. Added the pure centralized
`project_viewer` boundary, typed public Poche state, and the versioned ordered
public action/score prefix needed beyond the oracle's current-trick-only
observation. Players receive only their own hand; an unseated spectator may
hold at most one exact player/recipient/epoch grant; ordinary host projection
has no extra privilege. Request, owner grant, explicit deny, owner revoke, and
audited round/seat-role/membership expiry are reducer events. Grant/revoke/
effective-expiry advance a stale-checked projection epoch; disconnect blocks
delivery without becoming membership loss. A separate non-serializable
`LocalHostDiagnosticCapability` is the only API returning all hands.
`EventPayload::GameTransitioned` remains hash-only and the public prefix never
contains a private hand. Four-viewer controlled-hidden-state tests establish
pairwise noninterference; grant/revoke proves future-only changes; round expiry
ordering and reconnect public-prefix/own-hand reconstruction pass. Protocol
validation pins schema hash
`1489b2887acc117dd1a9e98d2891b8640fa4d901621b734d1f7654940c4e11ad`.
ADR 0003, `docs/viewer-projections.md`, session-engine/protocol docs, and all
Rust/protocol coverage cells record the boundary and remaining crypto/network
work. Focused suites, decoder fuzz/compile-fail tests, full workspace tests
(including native Alloy/NuSMV/Prolog conformance), workspace clippy with denied
warnings, formatting, the phase-2 guidance audit, and the unchanged `PLAN.md`
check all pass.

### [x] 2.4 Add deterministic transcripts, snapshots, and replay

Add golden fixtures under `tests/fixtures/protocol/` for room creation, join,
ready/countdown abort, start, game actions, pause/resume, chat, spectator grant
and revoke, disconnect/reconnect, duplicate/reordered messages, post-game, and
close. A transcript stores input commands, decisions, emitted events, public or
explicitly scoped projections, semantic identities, and hashes. Secret keys and
invite secrets never enter fixtures.

Implement snapshot + tail replay and full genesis replay. Verify both produce
the same semantic state hash and viewer projections.

**Completion criteria:** `cargo run -p poche-xtask -- protocol replay --all`
replays every fixture deterministically; single-line deletions/reorders or a
controlled reducer defect produce a precise first-divergence report.

**Completion notes (2026-08-04):** Completed after locally committed Task 2.3
(`0822dc9`; remote push retained as an open requirement after the environment
usage-limit gate rejected network escalation). Added a 32-step secret-free
golden fixture at `tests/fixtures/protocol/session-micro-v1.json` covering
create, referenced one-time joins, seats/readiness, countdown abort/start,
chance, pause/denied advance/unpause, chat, hand request/deny/grant/duplicate/
revoke, disconnect/reconnect, actions, round expiry/settlement, stale input,
post-game/reset, and close. Every step commits the resolved command identity,
authorization/semantic outcome, event-kind sequence, complete semantic state,
and all viewer-projection hashes; privacy checkpoints retain complete typed
recipient projections. Snapshot v1 stores canonical secret-reference input
prefix bytes in `SnapshotPayload`, restores through the same pure reducer, and
then replays the tail. Its final state and every final viewer projection equal
full genesis replay. Deleted/reordered steps and a controlled dropped-revoke
apply defect report the exact first divergent step/field. Runtime tests prove
fixture/rendered text excludes both invite verifiers. Added
`cargo run -p poche-xtask -- protocol replay --all`, which scans all JSON
fixtures and reports the pinned final state
`678455b9937db722bd8e25c3585aa2c678455ba31522b3a9e07f36daffa3b662`.
`docs/protocol-replay.md`, session-engine docs, and Rust/protocol coverage cells
record scope and the deliberately inspectable (not fast-hydration) snapshot
choice. Formatting, workspace denied-warning clippy, replay command, focused
tests, the 137-second full workspace/native conformance suite, plan diff checks,
and the unchanged `PLAN.md` check pass.

### [x] 2.5 Prove typed/direct, NDJSON, and optional Phon semantic parity

Run each fixture directly through typed commands and through NDJSON decode. If a
Phon codec is added, run it as a third path. Compare decisions, events,
snapshots, projections, errors, and semantic hashes, not merely successful
deserialization.

**Completion criteria:** All enabled codecs agree exactly; malformed/unknown
input fails closed; NDJSON remains the normative inspectable form even if Phon
is smaller/faster.

**Completion notes (2026-08-04):** Completed after locally committed Task 2.4
(`d09abfb`). Every normal command in every checked JSON transcript now runs
independently as an already-typed envelope and through canonical NDJSON encode
plus strict bounded decode. Both paths perform the full authorization, semantic
decision/error, event apply, prefix-snapshot restore/tail replay, and scoped
projection pipeline. Exact `GoldenTranscript` equality compares command
identities, allows/denials, event sequences, semantic state hashes, complete
privacy-checkpoint projections, per-viewer hashes, and snapshot evidence.
`protocol replay --all` reports `codecs=typed,canonical-ndjson` and the same
pinned final state for both paths. No Phon protocol codec was added, so no third
path is claimed; canonical NDJSON remains normative. Existing strict decoder
tests cover unknown version/tag/field, ambiguous controls, noncanonical input,
malformed bytes, and 10,000 deterministic arbitrary inputs without semantic
entry. Protocol/runtime/xtask tests, workspace denied-warning clippy,
formatting, replay, diff checks, the immediately preceding full workspace/native
suite, and unchanged `PLAN.md` check pass. Codec parity and limitations are
recorded in protocol/replay docs and coverage.

## Phase 3 - Independent formal session models

### [x] 3.1 Build the Alloy room/authorization oracle

Add `models/alloy/session.als` independently from Rust. Model principals,
membership, seats, roles/capabilities, grants/revocations, room phases,
readiness, countdown, pause, and viewer knowledge in a named bounded scope.
Check structural consistency and bounded assertions for default deny,
single-seat ownership, no start without readiness, at-most-once start, scoped
spectator grants, revocation of future access, and no unauthorized knowledge
edge. Include controlled defective predicates with expected witnesses.

**Completion criteria:** Native Alloy executes every command; all applicable
session rule IDs have coverage; scopes and information-flow abstraction are
stated; controlled defects yield expected instances/counterexamples.

**Completion notes (2026-08-04):** Completed in the local Task 3.1 slice.
`models/alloy/session.als` is independent of Rust and models five principals,
two distinct seats, membership/readiness, six room phases, start cardinality,
player-gated pause/resume, command authorization, exact spectator grants, and
derived viewer knowledge. The native Alloy 6.2 receipt recognizes every one of
13 registered commands: two valid witnesses SAT, eight safe assertions UNSAT,
and three controlled defective predicates SAT. Commands use exact 2-seat,
2-or-4-snapshot, 5-bit-Int scopes. `docs/session-formal-models.md` states the
current/future knowledge abstraction, omissions, exact covered rule IDs, and
bounded nature of the results; applicable cells in `docs/session-coverage.md`
carry Task 3.1 evidence. Formatting and the native Alloy command pass.

### [x] 3.2 Build the NuSMV lifecycle/liveness oracle

Add `models/nusmv/session.smv` independently. Model logical ticks, readiness,
abort/expiry races, start, running, selected pause protocol, resume,
disconnect/reconnect abstraction, post-game, and close. Check safety plus
conditional CTL/LTL properties. Explicitly demonstrate that unconditional
session termination is false with arbitrary pause/partition, then prove the
intended liveness under named fairness assumptions.

**Completion criteria:** Native NuSMV results distinguish holds from expected
counterexamples; removing readiness, eventual-expiry, eventual-action, or
eventual-resume assumptions demonstrates why each liveness claim needs it.

**Completion notes (2026-08-04):** Completed in the local Task 3.2 slice.
`models/nusmv/session.smv` independently models a two-member lifecycle,
readiness, abort/expiry ordering, one-start safety, a two-action abstract game,
pause/resume, durable membership across disconnect/reconnect, post-game, and
absorbing close. NuSMV 2.7.1 recognizes all 16 properties: nine lifecycle/
safety holds, conditional CTL and LTL termination holds, unconditional
termination is false, and four single-assumption omissions are false. All five
counterexamples are nonterminal lassos; the CLI checks the four omission traces
use their exact modes. The FSM is total and deadlock-free. The assumptions are
explicit finite scheduler modes rather than global fairness clauses, and exact
scope/limitations/rule coverage are in `docs/session-formal-models.md` and
`docs/session-coverage.md`. The native runner now correlates catalog/results by
stable kind/expression rather than native output order and accepts NuSMV 2.7.1's
combined total/deadlock-free diagnostic; focused parser tests pass.

### [x] 3.3 Build the Scryer Prolog policy/predecessor oracle

Add `models/prolog/session.pl` independently. Provide relational queries for:

- allowed/denied actions and policy explanations;
- possible successors from a state and command;
- possible predecessor commands/states for an observed lobby/session state;
- who can see a given information item;
- what grant/revoke chain explains a projection;
- what ready/countdown/pause events could have led to a running or paused room.

Keep variables useful in multiple positions; do not replace reverse queries with
callbacks into Rust.

**Completion criteria:** Native Scryer returns complete normalized answer sets
for the bounded query corpus and controlled policy defects; explanations cite
stable session/policy IDs.

**Completion notes (2026-08-04):** Completed in the local Task 3.3 slice.
`models/prolog/session.pl` independently exposes relational step/predecessor,
policy decision/explanation, visibility, projection/revocation chain, replay,
and causal-history queries without Rust callbacks. Native Scryer executes seven
sorted fixtures totaling 63 rows: policy 11, successors 13, predecessors 12,
visibility 16, grant/revoke chains 4, histories 3, and controlled defects 4.
The CLI pins each complete answer set by count and BLAKE3 digest. Explanations
carry stable session IDs; the defect corpus witnesses permissive outsider pause
and room-wide hand disclosure alongside correct default-deny/exact-grant rows.
`docs/session-formal-models.md` records every digest, productive modes, scope,
and omissions. The generic native runner now safely selects repository-local
Prolog models/modules; formatting, denied-warning clippy, runner tests, and the
native corpus pass.

### [x] 3.4 Add exhaustive Rust session checking

Extend `poche-check` or add a session checker for a named scope such as one
host, two seated players, one spectator, bounded connection states, one
countdown, one pause cycle, one hand-view grant epoch, bounded chat metadata,
and an abstract single-step game. Explore all session commands/events and check
the security/progress catalog.

Do not embed the full card game graph. Compose an abstract `GamePort` contract,
then run separate integration fixtures against the real `GameEnvironment`.

**Completion criteria:** Exploration reaches a deterministic fixed point;
counts and semantic hashes are recorded; safety holds; liveness qualifications
match NuSMV; known false invariants produce minimal traces.

**Completion notes (2026-08-04):** Completed in the local Task 3.4 slice.
`poche-check::explore_session` reaches a deterministic fixed point for one
host/player, one other player, one spectator, bounded connection/readiness,
one countdown/pause/grant/chat slot, and a one-action `SingleStepGamePort`.
The pinned graph has 800 states, 38,400 action attempts (5,872 accepted and
32,528 denied), 272 terminal states, maximum depth 14, ten safety properties,
and semantic hash
`89c626a11bcef07d93007ba5a7bf097fa9dd88c94244775dec5c138a17e023b1`.
Unconditional termination is false; every state has a terminal path under the
five named readiness/expiry/action/resume/reconnect enabling assumptions,
matching NuSMV's qualification. Minimal witnesses have depths 0 (not every
state terminal), 5 (pause reachable), and 2 (authorized spectator hand grant).
`docs/session-formal-models.md` records the scope and separation from the full
card graph. Focused exhaustive checks, denied-warning clippy, and the real
`OracleSessionGame` integration fixture pass.

### [x] 3.5 Audit full session coverage and cross-model agreement

Normalize Alloy/NuSMV/Prolog/Rust results through `poche-interchange`. Compare
shared fixtures/projections/policy decisions without declaring Rust correct by
default. Update `docs/session-coverage.md` with modeled, checked/queried,
abstracted, inapplicable, or explicitly deferred dispositions.

Provide:

```powershell
cargo run -p poche-xtask -- session coverage audit --all
cargo run -p poche-xtask -- session oracle check all
cargo run -p poche-xtask -- session compare all --scope lobby-micro
```

**Completion criteria:** Every stable session rule has evidence or a reasoned
disposition in every applicable track; every disagreement is classified and
resolved or made an explicit gate; exact scope/confidence labels accompany all
claims.

**Completion notes (2026-08-04):** Completed in the local Task 3.5 slice.
`poche-interchange` now carries neutral session claim/track/agreement records
and compares every claim shared by two or more applicable tracks without a
privileged backend. The aggregate gate runs all source evidence first, then
compares four tracks, ten shared claims, and 31 observations with zero
disagreements in `lobby-micro`. A controlled unit proves Alloy/Prolog conflict
is retained rather than resolved in Rust's favor. `docs/session-agreement.md`
records the claim matrix and exact exhaustive/bounded/symbolic/queried
confidence labels. `session coverage audit --all` matches all 88 stable rule
IDs exactly once and rejects unclassified or planned formal cells; remaining
formal gaps are reasoned abstractions or explicit post-Phase-3 deferrals, while
later protocol/network/UI tasks remain named gates. All three required Task 3.5
commands pass, and `PLAN.md` remains unchanged.

## Phase 4 - Loopback multiplayer CLI vertical slice

### [x] 4.1 Add a Poche CLI by selectively porting the clean template

Create `crates/poche-cli` as a workspace binary. Port useful patterns from
`teamy-rust-cli` commit `7e62d72` rather than running a whole-repository
initializer over Poche. Follow the template's subcommand-per-directory/file
convention. Add structured stderr logs, optional NDJSON logs, cancellation,
build/revision metadata, and text/JSON output.

Initial command groups:

```text
poche room host|join|show|ready|unready|countdown|abort|pause|resume|leave|close
poche game observe|actions|act
poche chat send|tail
poche spectator request-hand|grant-hand|revoke-hand
poche transcript record|replay|inspect
poche identity show|create
```

**Completion criteria:** Help/version/output round trips and arbitrary CLI
inputs are tested; stdout is protocol/machine output and diagnostics stay on
stderr; no secret is printed by default or under debug logging.

**Completion notes (2026-08-04):** Completed in the local Task 4.1 slice.
Added the `poche-cli` workspace binary and selectively ported the pinned
`teamy-rust-cli` patterns for directory-shaped command dispatch, embedded
repository/build metadata, Ctrl+C/deadline cancellation, stderr tracing,
optional NDJSON file logs, and top-level text/JSON rendering. All 24 declared
commands parse into typed variants; until Task 4.2 supplies a runtime they emit
an explicit `parsed` receipt rather than pretending to execute semantics.
Parsing is strict, value-free errors cannot reflect an invite secret, and the
parsed CLI is never debug-logged. Four unit tests cover the full command table,
output selection, redaction, and 10,000 deterministic arbitrary token streams;
five process tests cover root/nested help, version metadata, text/JSON stdout,
stderr-only diagnostics, NDJSON validity/redaction, invalid input, and
deterministic cancellation. `cargo test -p poche-cli` and denied-warning clippy
pass.

### [x] 4.2 Implement in-process multi-client transport and authoritative runtime

Define transport/client ports in `poche-runtime` and implement an
`InProcessTransport` that can host multiple independently authenticated clients,
inject duplicates/reordering/disconnects, and drive a manual logical clock.
Typed direct delivery is the default; an NDJSON loopback mode tests framing.

**Completion criteria:** Multiple CLI client instances (or scripted client
objects) create/join/play one full game without sockets; fault injection is
deterministic; RL crates do not depend on this transport.

**Completion notes (2026-08-04):** Completed in the local Task 4.2 slice.
`poche-runtime` now defines separate client and authority transport ports plus a
no-socket `InProcessTransport`, `ScriptedClient`, `ManualClock`, and
`InProcessAuthority`. Connections bind one stable principal and reject spoofed
envelopes before reduction. The centralized authority delegates all semantics
to the existing authorize/decide/apply reducer, commits event batches
atomically, schedules logical countdown events, and emits only viewer-scoped
projections. Typed delivery is the default; canonical NDJSON mode round-trips
both ingress and egress through the strict protocol codec. Duplicate, next-pair
reorder, and disconnect faults have deterministic queue semantics, including an
authority-level proof that duplicate create delivery leaves revision and
processed-command cardinality unchanged. A three-client acceptance test hosts,
joins, seats, readies, starts, and completes the real two-player Poche oracle's
13-round schedule while a spectator receives scoped progress. The transport
crate has no socket dependency, and no RL crate depends on it. Thirteen unit
tests, the full-game integration test, and denied-warning clippy pass.

### [x] 4.3 Deliver inspectable text play and replay

Create concise text projections for lobby, countdown, game observation, legal
actions, pause state, chat, spectator grants, and results. Also support raw
NDJSON input/output so a complete room can be driven by a checked-in script.
Text rendering must be a pure presentation of typed projections, never a second
semantic implementation.

**Completion criteria:** A golden script hosts two players and a spectator,
aborts one countdown, starts another, pauses/resumes, completes a game, and
replays to the same hashes. A new agent can inspect the transcript and identify
every action/decision/event without a graphical client.

**Completion notes (2026-08-04):** Completed in the local Task 4.3 slice.
Added pure text renderers for lobby/countdown/running/paused/post-game viewer
projections, supplied legal actions, attributed escaped chat, spectator grants,
and final scores. The renderers consume typed scoped data and cannot access
authority state or calculate semantics. Added the canonical 32-line
`session-micro-v1.script.ndjson` driver beside its golden transcript. It covers
two players and a spectator, abort/re-arm, pause/denied action/resume, chat,
grant/revoke, disconnect/reconnect, completion, stale denial, reset, and close.
Script replay compares typed ingress, strict canonical NDJSON ingress, and the
checked state/projection hashes; its final hash remains
`678455b9937db722bd8e25c3585aa2c678455ba31522b3a9e07f36daffa3b662`.
`poche transcript replay` emits complete inspectable text, a JSON summary, or
33 raw NDJSON output records; `transcript inspect` presents the checked full
transcript without executing it. Runtime/CLI tests assert every required text
shape and action/decision/event markers. `protocol replay --all`, focused
tests, and denied-warning clippy pass.

### [x] 4.4 Add ephemeral chat and transcript-safe redaction

Implement bounded chat through the same authorization/event service, but retain
only the configured in-memory tail. Escape/render user text as data. Add rate,
size, membership, and closed-room rejection tests and verify transcript export
can include chat while never including private protocol material.

**Completion criteria:** Chat works in loopback CLI; control-frame injection and
oversize/rate tests fail closed; formal coverage tracks permission and count,
not unbounded content.

**Completion notes (2026-08-04):** Completed in the local Task 4.4 slice.
Added a configurable fixed-capacity `ChatTail` whose entries contain only the
accepted event revision, stable principal ID, and message text. The in-process
authority records a `ChatPosted` event only on its first application, so a
deterministically duplicated transport delivery cannot duplicate the side
stream. The tail is explicitly memory-only, truncates oldest-first, supports a
zero capacity, and exports strict NDJSON without serializing command envelopes,
invites, signatures, capabilities, or viewer-private projections. An end-to-end
canonical-NDJSON test covers member attribution, outsider default denial,
closed-room denial, the logical rate limit, protocol-boundary oversize
rejection, bounded truncation, duplicate delivery, newline/carriage-return and
JSON-looking control injection, and secret/private-field scans. The canonical
CLI replay also asserts that its accepted chat action crosses the loopback path;
live native-room attachment remains in the transport/client phases. Focused
runtime/CLI tests and denied-warning clippy pass, and the coverage matrix now
tracks bounded permission/count metadata rather than unbounded formal content.

### [x] 4.5 Run the loopback acceptance scenario

Add `cargo run -p poche-xtask -- multiplayer smoke --transport in-process` and
commit its small normalized evidence summary.

**Completion criteria:** The scenario covers every lifecycle and spectator
feature in U35/U39/U41, completes a real Poche game, reproduces from its seed and
transcript, and passes protocol/session/formal coverage gates.

**Completion notes (2026-08-04):** Completed in the local Task 4.5 slice.
Added the exact `multiplayer smoke --transport in-process` xtask command and a
checked normalized evidence summary. Its secret-free symbolic transcript drives
the canonical-NDJSON transport through create/join/seat, ready/unready, aborted
and completed countdowns, chat in all five open phases, player pause and a
different player's unpause, paused-action denial, spectator request/grant/exact
card projection/revoke, disconnect/new-route reconnect, post-game reset, and
close. Between those lifecycle operations it completes the real two-player
Poche oracle's 13-deal schedule from seed `0x5eed`, recording scores `[40, 20]`.
A fresh authority replays all 178 inputs and must match every disposition,
revision, event count, phase, and viewer-projection hash; invite secrets never
enter the serialized transcript. The command then passes the canonical protocol
replay, 88-rule session coverage audit, and zero-disagreement Rust, Alloy,
NuSMV, and Scryer Prolog comparison. The pinned transcript hash is
`ab90ff79acb755d72defaef22a37369a71a9125df64c91a8645260254934a075`.

## Phase 5 - Native Veilid rooms and durable identity

### [x] 5.1 Implement application identity and secret storage

Add `crates/poche-veilid` behind a non-default `veilid` feature. Pin a released
Veilid version. Define stable player signing/encryption keys separately from
Veilid node/route identities. Use protected storage suitable for the platform;
define explicit backup/loss semantics and a development-only insecure mode that
cannot be selected silently.

Map TPBAC principals to stable public keys and room capabilities. Sign protocol
commands at the application layer even though Veilid authenticates its own
network/DHT operations.

**Completion criteria:** Restart preserves the application identity; wrong or
missing storage credentials fail safely; logs/state snapshots contain no secret
material; signature/replay tests include controlled wrong-key attacks.

**Completion notes (2026-08-05):** Completed in the local Task 5.1 slice.
Added `poche-veilid` with empty default features, an explicit
`insecure-development` mode, and a non-default `veilid` feature pinned to the
released MPL-2.0 `veilid-core = 0.5.7`. Each stable application identity owns
independent Ed25519 signing and X25519 recipient secrets; its 64-character
signing-public-key hex is the protocol `PrincipalId`, while Veilid node IDs,
DHT owner keys, and routes remain absent from the type. Only the public identity
is serializable/debuggable. Compile-fail tests keep the secret identity from
`Debug`/`Clone`; its versioned integrity-checked blob has no diagnostic or
serialization traits and zeroes on drop. The production adapter uses Veilid's
platform protected store, with config validation that requires credentials and
rejects insecure fallback, forced insecure storage, and delete-on-start. The
memory-only development adapter requires conspicuous opt-in at construction and
use. Restart, missing/wrong credential, corrupt/error nonreplacement, public
snapshot secret scan, strict command signature, wrong-key, and signed-revision
replay tests pass. Canonical command/event signer/verifiers remain outside pure
reducers. Default and `--features veilid` tests plus denied-warning clippy pass
offline. Implementation also recorded that released 0.5.7 exposes synchronous
protected-store methods while the newer local checkout retains the same version
label but adds async `VeilidAPI` conveniences; Poche targets the release.

### [x] 5.2 Implement DHT rendezvous, private routes, and invite codes

Create a host-owned DFLT rendezvous record containing only bounded public room
metadata, current private-route rendezvous material (encrypted as required),
protocol/schema versions, epoch, and expiry. Do not attempt dynamic SMPL schema
membership. Define a compact room code with version, network, record key,
one-time/expiring invite secret or verifier, and checksum.

Use `app_call` for join and command requests requiring a response and
`app_message` or an explicitly justified alternative for event notification.
Keep messages below Veilid limits and reassemble nothing unless the protocol
specifies authenticated chunks.

**Completion criteria:** Two native nodes create and redeem a code; invalid,
expired, replayed, cross-room, and revoked codes fail; successful redemption
creates membership for the joining stable key; no invite secret is written in
plaintext to DHT or logs.

**Completion notes (complete, 2026-08-05):** Added the
compact redacted/zeroing `p3-` room-code codec, a strict bounded one-subkey DFLT
rendezvous schema, application-host/network/version/expiry binding, hashed
authority-side invite verifiers, stable-key signed redemption tests, and the
released Veilid 0.5.7 create/open/set/get/flush/private-route/`app_call`
adapter. Codec/session acceptance rejects invalid, expired, replayed,
cross-room, and revoked codes and scans the serialized DHT shape for structural
absence of invite material.

The required real execution passed with two distinct native Veilid 0.5.7 nodes
under the explicit public-network guard. The host created/flushed the encrypted
DFLT record and private route; the client resolved and validated the record,
imported the route, redeemed the one-time code by `AppCall`, and became a
durable stable-key member. The separate transport probe reached 64 host/52
client peers with DHT, private route, and reply all true, and the full lifecycle
reused that construction without exposing invite material in the DHT value or
diagnostics.

The local criterion was not weakened or mislabeled: the reproducible isolated
probe proves released 0.5.7 direct bootstrap skips `LocalNetwork` peers while
private routes require `PublicInternet`, and the released virtual-network path
is incomplete. It therefore records zero peers/no route as an expected upstream
topology limit and performs no application send. Actual byte-delivery evidence
is the manual public gate; normal tests, CI, and RL remain network-free. Exact
commands, measurements, limitations, and machine-readable evidence are in
`docs/veilid-native-acceptance.md` and
`evidence/veilid-native-acceptance.json`.

### [x] 5.3 Implement membership reconnect without the original code

Persist the room locator and membership credential/capability bound to the
stable player key. On restart or private-route change, resolve current
rendezvous data, prove the stable identity, refresh recipient routes, receive a
snapshot + event tail, and resume the same membership/seat when policy permits.

**Completion criteria:** Host and client restart/reconnect cases are tested;
route replacement does not change principal identity; reconnect needs no invite
code; removed/banned/revoked membership cannot reconnect as active.

**Completion notes (complete, 2026-08-05):** Added a host-signed, stable-key
`MembershipCredential`; a strict zeroing `MembershipLocator` whose persisted
shape contains the encrypted DHT locator and credential but structurally omits
the invite secret; explicit protected/insecure-development membership stores;
and released Veilid protected-store integration. A persisted member can now
open and validate refreshed DHT rendezvous data without the original code,
then prove a replacement recipient route and route epoch with both the signed
protocol `Reconnect` command and a stable-application-key signature. Room,
network, host, session epoch, principal, credential, route, and expiry
mutations fail closed.

Host publication now persists the encrypted DHT record key and DHT owner
keypair only in Veilid protected storage under a hashed room key.
`resume_host_room` requires the same stable host application identity, reopens
that owner record, allocates a new private route, increments `route_epoch`, and
republishes without retaining or recreating the old invite. Client and host
application identities are reloaded in restart tests; strict/checksummed
client and host secret blobs, wrong-key/forgery, route replacement, corruption,
expiry, and secret/debug scans are covered.

Recovery now uses a host-signed viewer snapshot plus a bounded, gap-free,
host-signed authority event tail. Tests reject wrong hosts, mutation,
oversized state, deletion, and reordering and ensure diagnostics redact the
viewer payload. The session regression
`removed_member_cannot_reconnect_with_the_former_stable_principal` proves that
a cryptographically valid former identity does not override current
authoritative membership or restore its released seat. The two gates and
reproduction commands are documented in `docs/membership-reconnect.md`, with
coverage updates for `S-ROOM-020`, `S-AUTH-013`, `S-AUTH-018`, `S-AUTH-019`,
`S-FAULT-004`, and `S-FAULT-005`.
`RemoveMember` is the v1 membership-revocation operation; v1 deliberately has
no second banned-but-active membership state that could bypass that gate.

Evidence: `cargo test --workspace --offline` passed the complete workspace and
native Alloy/NuSMV/Scryer-backed suite; `cargo test -p poche-veilid --features
veilid --offline` passed 18 units plus 2 compile-fail docs; both `cargo clippy
--workspace --all-targets --offline -- -D warnings` and the feature-specific
Veilid clippy gate passed. The released 0.5.7 DHT/protected-store/private-route
paths compile against their exact APIs. Actual separate-process private-route
execution is not misreported here: it remains the explicit public/topology
acceptance gate in Task 5.6.

### [x] 5.4 Carry commands, events, countdown, pause, and chat over Veilid

Implement retry categories for `TryAgain`, timeout, no connection, stale route,
watch renewal, duplicate delivery, and shutdown. Treat DHT watch notifications
as hints that trigger validated refresh; never as an authoritative event order.
Use the session revision log to deduplicate/recover.

**Completion criteria:** A local multi-node harness executes the Phase 4
scenario over Veilid; forced disconnect/reorder/retry does not duplicate state;
countdown uses authority time and clients display estimates only; chat and game
commands obey the same app authorization.

**Completion notes (complete, 2026-08-05):** Added strict
bounded/canonical `TransportCommandCall` and `TransportCommandReply` schemas,
with transport schema v2 carrying and verifying the stable application's full
public identity rather than trusting a node or route. Replies enforce
contiguous authority-event revision validation and reject invalid
disposition/frame mixtures. Added `CommandRetryState`, bound to the command ID
and full canonical signed command bytes, with explicit same-route,
validated-rendezvous refresh, shutdown, permanent-failure, and exhaustion
outcomes for every required failure category. Added non-authoritative watch/
route hint classifiers and a countdown display estimate that can only wait for
the authority transition. `InProcessAuthority` now retains a committed event
journal for exact post-revision delivery; duplicates never append twice.

The released Veilid 0.5.7 adapter now keeps resolved DHT records open for
watches; classifies `ValueChange`, dead watch, dead route, and shutdown updates
only as refresh/lifecycle hints; maps `TryAgain`, timeout, no connection,
invalid target/stale route, shutdown, malformed, and oversize failures without
diagnostic leakage; and provides strict client call plus host decode/reply
methods over the exact `app_call`, `app_call_reply`, and `watch_dht_values`
APIs. `docs/veilid-command-transport.md` records retry, authority, clock, and
recovery behavior and the coverage rows for `S-AUTH-016`, `S-FAULT-002`,
`S-FAULT-003`, and `S-FAULT-006` are updated.

Native acceptance passed the complete Phase 4 scenario between two distinct
Veilid nodes using validated DHT rendezvous, private routes, and `AppCall` for
every remote command. The run made 174 calls and delivered 172 contiguous
signed event frames through final revision 173. It covered room creation,
one-time join, seats, ready/countdown/abort, 13 game rounds ending 40-20,
any-player pause/unpause plus denial while paused, three chat messages, exact
duplicate handling, disconnect/DHT-route refresh/reconnect, and encrypted
spectator grant/revoke, player leave, and host close. `TryAgain`/timeout reuse exact bytes; no-connection,
stale-route, and watch renewal release the stale route, reread/validate DHT,
import the replacement route, and resend those same bytes.

The exact local commands also pass 26 Veilid units, 2 compile-fail docs, the
88-row coverage audit, zero Rust/Alloy/NuSMV/Scryer disagreements, and the
network-free lifecycle. Released 0.5.7 cannot form private routes in the
isolated LocalNetwork topology, so its diagnostic is explicitly separate from
the guarded public byte-delivery acceptance. Evidence and limitations are in
`docs/veilid-command-transport.md`, `docs/veilid-native-acceptance.md`, and
`evidence/veilid-native-acceptance.json`.

### [x] 5.5 Encrypt and deliver viewer-specific private projections

Encrypt private projection payloads to each stable recipient key (or document
and test an equivalent Veilid-supported end-to-end construction). Public room
events may be shared, but hands are never shared and UI-filtered afterward.
Rotate projection/capability epochs on grant/revoke and relevant membership
changes.

**Completion criteria:** Packet/event capture from an ungranted spectator lacks
decryptable hand data; granted spectator receives only the selected player's
current/future authorized projection; revoke stops subsequent delivery; other
spectators and players gain nothing.

**Completion notes (2026-08-05):** Complete. Added strict/canonical/bounded
`EncryptedProjectionPacket` delivery and the released Veilid 0.5.7 VLD0 HPKE
base-mode adapter over the stable application's separate X25519 recipient key.
All visible room/session/host/recipient/projection-epoch/revision metadata is
authenticated as HPKE associated data. Because base mode does not authenticate
the sender, the stable host additionally signs the complete metadata and
ciphertext. Opening verifies that signature first, requires the exact recipient
and caller-supplied current projection epoch, then revalidates one canonical
`ProjectionEnvelope` against every duplicated field.

The transport reply schema now rejects plaintext projection frames and carries
at most one opaque exact-recipient encrypted projection. Existing session
events rotate projection/capability epochs on grant, revoke, and relevant
membership/seat/round expiry; old packets cannot overwrite a newer-epoch view,
while the honest limitation that past knowledge cannot be erased remains.

The released-crypto capture acceptance starts a temporary Veilid API instance
and proves ungranted empty output; opaque serialized packet/reply capture;
current and future selected-hand delivery; absence of another seated player's
hand; other-spectator/player rejection; cryptographic wrong-key failure;
post-revoke omission; old-epoch replay rejection; and host-signature/metadata
tamper rejection. `docs/veilid-projection-privacy.md` documents the construction
and threat boundary. `cargo test -p poche-veilid --features veilid --offline`
passes 26 units plus 2 compile-fail docs, both default-workspace and Veilid-
feature clippy pass with warnings denied, and the updated 88-rule session
coverage audit passes. Separate-node byte delivery remains honestly assigned
to Task 5.6 rather than weakening this cryptographic acceptance.

### [x] 5.6 Run native Veilid security and lifecycle acceptance

Provide a reproducible local test-network command, documented prerequisites,
and a separate opt-in public-network smoke test that does not run in normal RL
or unit-test workflows.

```powershell
cargo run -p poche-xtask -- transport test veilid-local
cargo run -p poche-xtask -- multiplayer smoke --transport veilid-local
```

**Completion criteria:** Clean native clients create/join/reconnect/play/chat/
spectate across separate nodes; fault and unauthorized-command suites pass;
public-network testing is rate-limited and never required for ordinary CI.

**Completion notes (complete, 2026-08-05):** `poche-xtask` now exposes both
documented local commands plus environment-guarded `veilid-public` transport
and full-lifecycle commands. The local transport gate passes 26 units and 2
compile-fail docs; the local multiplayer gate passes the complete 88-row
coverage/formal/in-process suite with zero oracle disagreements and then
reproduces the honest released-0.5.7 isolated-topology limitation. It never
contacts the public network or claims local application bytes crossed Veilid.

With
`POCHE_ALLOW_VEILID_PUBLIC_TEST=I_ACCEPT_PUBLIC_NETWORK_TRAFFIC`, two distinct
clean native nodes proved DHT/private-route/`AppCall` delivery and the complete
authorized lifecycle. The final measured full run made 174 calls, emitted 172 signed
event frames, handled one exact duplicate and one denial, refreshed/reconnected
once, completed all 13 rounds, made the player leave, closed through the host at
final revision 173 while preserving score 40-20, exchanged
three chats, and cryptographically verified spectator grant and revoke.
Application signatures are verified independently of transport identity and
viewer projections are encrypted to exact stable recipients.

The public commands use ephemeral nodes, are explicit/manual, and are absent
from unit tests, CI, and RL. The diagnostic-only local feature and temporary
insecure-development protected stores are conspicuously documented; neither is
a production configuration. This is native protocol acceptance, not a packaged
end-user client. Commands, prerequisites, trust boundary, exact results, and
limitations are recorded in `docs/veilid-native-acceptance.md` and
`evidence/veilid-native-acceptance.json`.

## Phase 6 - Minimal rendering and web delivery

### [x] 6.1 Close browser transport and Rust UI gates with executable spikes

Build the smallest possible viewer/client against protocol fixtures, then test:

1. an egui projection renderer on its supported native backend;
2. an egui/WASM Pages-hostable replay/demo with no live transport dependency;
3. browser-only `veilid-wasm` on local HTTP/`ws://`, with no native Poche or
   Veilid companion process on the client device;
4. browser-only `veilid-wasm` on HTTPS/`wss://` with production-equivalent
   bootstrap/relay and again no on-device companion;
5. a minimal hostable Rust server binary using the clean Datastar/Datastar Rust
   reference APIs to connect an ordinary browser to the same typed protocol;
6. a bounded native-rendering comparison using cursor-latency/ash evidence to
   quantify what direct Vulkan control offers and what web/distribution support
   it costs. This is a spike, not authorization to rewrite the UI in raw Vulkan.

Record browser versions, Veilid version/config, console/network evidence,
round-trip interaction latency, bundle and binary size, accessibility,
deployment steps, operator/client metadata exposure, and exact failure modes.
The Datastar spike must identify whether the server is authoritative, a relay,
or colocated with the room host and must not expose private projections to an
additional operator without an explicit threat-model change. Do not modify
Veilid or any reference repository upstream.

**Completion criteria:** G26 and G32 are closed with executable evidence and an
ADR-selected live topology plus renderer. Direct live Pages is selected only if
browser-only HTTPS Veilid passes. If it fails, select between a self-hostable
Datastar server topology and a native Veilid client based on the recorded
portability/privacy/operations evidence; do not silently require an on-device
companion. The static replay/demo remains available in either case.

**Completion notes (implementation in progress, 2026-08-05):** Added
`poche-ui`, whose renderer-neutral `PresentationModel` consumes only an exact
viewer `ProjectionPayload` plus explicitly public client supplements; it has no
authority-state dependency. The checked golden transcript becomes a static
host/alice/bob replay deck covering running, spectator grant, later no-grant,
disconnect/reconnect, post-game, and closed checkpoints. The same egui widget
tree is exposed through a native `poche-replay` binary and a WASM `WebHandle`;
generated web output is directed to ignored `site/replay`.

Native execution is proven: focused tests and denied-warning clippy pass, the
5,590,016-byte release binary opened a live eframe/Glow top-level window named
`Poche projection replay` with a nonzero Win32 handle, and the probe closed
only that process. A timed run reached a window handle in 499.831 ms with a
20,885,504-byte peak working set. The unchanged cursor-latency reference also
ran for three seconds with direct ash/Vulkan `IMMEDIATE` presentation, one
frame in flight, and latest Win32 cursor sampling; its release binary is
3,819,008 bytes. The bounded comparison records the source/unsafe/control/
accessibility/web costs without treating raw Vulkan as authorization for a UI
rewrite.

The real Rust 1.96 WASM target and pinned `wasm-bindgen` 0.2.126 tool now build a
3,545,279-byte WASM plus 73,769-byte generated JS. A local browser loaded it in
121 ms to ready, reported no console errors, and rendered ungranted, granted,
then revoked spectator checkpoints. Its canvas lacks ordinary DOM semantics,
so it remains the static Pages replay rather than the accessible live web UI.

The portable `poche-web-spike` uses published Datastar Rust 0.3.1/Axum rather
than an absolute local dependency. Its 3,013,632-byte release server is the
room-host-colocated authority: a browser-triggered typed `CreateRoom` traversed
canonical NDJSON ingress and the pure reducer, returned revision 1/event 1 and
the exact host projection, then reset and recreated successfully. Semantic
viewer patches proved Bob ungranted, granted only host cards `2C 3C`, and later
revoked. G32 is therefore evidence-closed around the renderer-neutral
`PresentationModel`, shared egui native/static-WASM rendering, and semantic
HTML for an accessible live browser surface.

The pinned, unchanged Veilid 0.5.7 source was built for browser WASM with WSS
enabled. With no native Poche or Veilid process, HTTP/direct WS reached
`AttachedFull`, 31 live peers, and public readiness in 20.264 seconds. The same
artifact using WSS stayed `Attaching` with zero peers for 20.112 seconds, and a
separate TLS probe showed the documented bootstrap resetting the handshake.
Because an HTTPS Pages origin cannot fall back to insecure WS and upstream has
no outbound-relay deployment, G26 closes against direct Pages multiplayer.

ADR 0004 closes G26/G32 and selects the self-hostable, host-colocated Datastar
authority plus semantic HTML for live browsers, shared egui for native/static
WASM, and no implicit companion. The raw WSS-enabled Veilid package is
8,879,727 bytes of WASM plus 407,153 bytes of JavaScript before optional
`wasm-opt`. Exact commands, timings, sizes, exposure, and failure evidence are
tracked in `docs/rendering-topology-spike.md`.

### [x] 6.2 Implement deterministic room and game rendering

Add the selected egui-first client renderer and, when Task 6.1 selects the
Datastar topology, its browser/server adapter. Minimum surfaces:
identity, create/join code, member/seat/spectator list, ready state, countdown
and abort, own hand, public table, legal actions, score/pot, pause/resume, chat,
hand-view request/grant/revoke, reconnect status, errors with policy reasons,
and transcript export/replay.

**Completion criteria:** UI controls produce typed commands only; replaying the
same projection/event stream yields the same presentation model; native egui,
egui/WASM, or Datastar rendering differences never change semantics; inaccessible
or unauthorized controls are not the only enforcement layer.

**Completion notes:** Completed 2026-08-05. `poche-ui` now has a pure
`LiveClientPresentation` over the existing exact-recipient
`PresentationModel`; its opaque IDs retain typed `CommandPayload`s server-side
and resolve against a fresh presentation. Shared egui widgets and semantic HTML
cover identity, codes, members/seats/readiness, logical countdown, own/granted
hands, table/legal actions/scores/pot, pause/resume, chat, grants, reconnect,
policy notices, transcript export, and replay without reading authority state.
`poche-web-spike::LiveDemo` drives a real
`InProcessAuthority<OracleSessionGame<2>>` through canonical NDJSON rather than
mutating fixture state. Invented/stale IDs fail closed, and a direct typed
spectator `Pause` bypass test is still denied `D-NOT-SEATED` by the authority.
`cargo test -p poche-web-spike -p poche-ui -p poche-runtime` and strict focused
Clippy passed; browser interaction submitted a real legal `bid 0` and observed
the projected actor/bid transition. Exact commands and evidence are in
`docs/live-client.md`.

### [x] 6.3 Verify multi-view privacy and interaction behavior

Run client/browser tests with host, two players, an ungranted spectator, and a
granted then revoked spectator. Inspect rendered text/widgets/DOM, client state,
logs, server state where applicable, and network payload access according to the
selected topology's threat model.

**Completion criteria:** Each viewer sees exactly its projection; countdown,
pause, chat, and reconnect are usable; revocation changes future spectator view;
no hidden hand is present in unauthorized client state.

**Completion notes:** Completed 2026-08-05. Focused tests decode every
spectator network projection before a grant and after revoke and require
`own_hand = None` plus empty `granted_hands`; the authorized interval contains
exactly Alice's cards, and the recipient transcript contains no invitation
value. The release server was exercised in the in-app browser with host, Alice,
Bob, and spectator. DOM snapshots proved ungranted -> Alice `3C` granted ->
future view revoked, a visible/abortable logical countdown, Alice pause -> Bob
resume, attributed spectator chat, transport loss -> typed reconnect, legal
game action, and exact-recipient replay. Historical authorized projections are
retained honestly—revocation is future-only, not erasure. The downloadable
13-line test transcript used `application/x-ndjson` and exposed neither a
`POCHE-LAB` code nor command/secret material. See `docs/live-client.md`.

### [x] 6.4 Publish only deployment modes proven by Task 6.1

Extend existing Pages automation to publish the web replay/demo and, only if
G26's browser-only test passes, direct Veilid live multiplayer. Generated
WASM/JS/assets remain untracked. If Task 6.1 instead selects the hostable
Datastar server, publish reproducible server build/run/container guidance and a
clear connection configuration without claiming Pages itself hosts the live
backend. If native Veilid is selected, document/download that client without
representing Pages as independently live.

**Completion criteria:** README links distinguish rulebook, replay/demo, native
client, browser-only Veilid, and Datastar-hosted modes according to what was
actually proven; the trust/anonymity difference is conspicuous; generated
assets do not inflate Git history.

**Completion notes:** Completed 2026-08-05. Pages run
[`31026114728`](https://github.com/TeamDman/Poche/actions/runs/31026114728)
built and deployed commit `16f213252e3c447216582b462a498fbf27800379`
with pinned Typst 0.15.1, Rust 1.96.0, and `wasm-bindgen` 0.2.126. HTTPS probes
returned 200 for the landing page, `replay/`, and its `application/wasm` asset;
the in-app browser followed the public replay link and reached the active
`Poche projection replay` WASM application. Generated 3,544,207-byte WASM and
73,769-byte JS remained under ignored `site/` and out of Git history. README,
landing page, `docs/pages-publication.md`, and `docs/deployment-modes.md` now
distinguish the rulebook, static browser/native replay, unsupported direct
HTTPS/WSS browser Veilid, not-yet-released native live Veilid client, and the
host-colocated Datastar development authority. Trust/anonymity and the current
demo's unauthenticated named-viewer/static-code/in-memory limitations are
conspicuous. A multi-stage container recipe and loopback-first bind/run commands
are checked in; Docker was unavailable on the evidence host, so no image-run
claim is made.

## Phase 7 - Vectorized RL environment and baselines

### [x] 7.1 Define versioned observation, action, history, and reward specs

Add `crates/poche-rl` with an `RlSpec` manifest. For `poche-2p-v1`, define exact
feature order, normalization, categorical encoding, seat-relative symmetry,
public-history/memory treatment, fixed action vocabulary, legal mask, chance/
settlement auto-advance, reset semantics, round score reward, terminal metrics,
and semantic hashes.

Recommended fixed action vocabulary is explicit bid slots plus card-identity
slots; policy actions never include chance or settlement. Do not silently map
an illegal masked action to a legal one: fail in tests and count/reject under an
explicit production policy.

**Completion criteria:** Handwritten examples and exhaustive micro fixtures
match expected tensors/masks/rewards; hidden-state mutations invisible to a
viewer cannot change that viewer's encoding; every schema/reward change forces a
new ID/hash.

**Completion notes (2026-08-05):** Complete. Added the network-free
`poche-rl` crate and checked `poche-2p-v1`/`round-score-v1` manifest. Its 307
seat-relative features have exact contiguous spans for phase, dealer/actor,
private hand, relative public fields, ordered trick, and an episode-owned
52-card public history; its 60 actions are bids `0..=7` plus dense card
identities. Chance and settlement never enter the policy vocabulary. Masked
actions fail rather than remap, with an explicit optional rejection counter.
Raw rulebook points appear only at settlement and terminal score/money metrics
remain separate. The canonical spec hash is
`8852f8568ead1e40aad7bb4ca5b7725340cc01422e077ffddcd5d4f5665bf0bf`.
Handwritten tensor/mask/reward fixtures, exhaustive legal-slot execution,
manifest-drift tests, and an opponent/stock-only hidden mutation prove the
viewer encoding boundary. See `rl/specs/poche-2p-v1.json` and
`docs/rl-environment.md`.

### [x] 7.2 Implement deterministic batched CPU rollouts

Implement a batch that owns many independent typed game environments and
preallocates observation, action-mask, action, reward, terminal, seat, episode,
and seed buffers. Auto-step chance/environment turns deterministically. Batch
all currently acting agents for inference, then scatter actions without NDJSON,
sockets, or dynamic protocol allocation.

Correctly assemble turn-based per-seat transitions: a seat's action is paired
with its next observation/terminal, accumulating any intervening round reward
and recording decision-time distance. Make reset and seed derivation replayable.

**Completion criteria:** Scalar and batched rollouts agree step-for-step across
fixed seeds; buffer bounds/masks are checked; one million-step benchmark (or a
recorded smaller diagnostic when game length makes that impractical) reports
steps/s and allocation profile.

**Completion notes (2026-08-05):** Complete. `PocheBatch` owns independent
strongly typed games with stable per-slot/per-episode seed derivation and
preallocated observation, legal-mask, action, reward, terminal, seat, episode,
and seed buffers. It auto-advances only deterministic chance/settlement and
resets terminal slots without NDJSON, sockets, protocol allocation, or Veilid.
`TurnBasedBatchRollout` adds fixed pending slots per environment/seat and a
fixed-capacity structure-of-arrays transition buffer: each action is paired
with that same seat's next observation or terminal, while intervening round
reward and decision-time distance accumulate. A checked first-legal full game
yields 124 completed transitions and two terminal rows. Scalar and batch state,
mask, observation, reward, terminal, and per-seed parallel signatures agree.
The release one-million-decision gate completed 1,000,192 preallocated batch
decisions at 69,594.485 decisions/s with zero hot-buffer reallocations.

### [x] 7.3 Add random, legal-random, and heuristic baselines

Implement framework-independent policies through a small policy trait. Include
uniform legal random and at least one explainable Poche heuristic. Log raw
per-round points, final cumulative points, score differential, exact bids,
illegal-action count, game length, and seeds. Win/loss may be secondary but must
not replace score as the primary objective.

**Completion criteria:** Baselines are deterministic by seed, never choose
masked actions, produce replayable transcripts on demand, and establish a fixed
evaluation corpus before learning.

**Completion notes (2026-08-05):** Complete. Added framework-independent
`Policy` implementations for full-vocabulary rejection-sampled random,
compact legal-random, and the explainable viewer-only high-card/low-play Poche
heuristic. All are deterministic by seed and return only enabled mask slots.
The preregistered `poche-baselines-v1` corpus fixes 64 seeds, both
legal-random/heuristic seat assignments, and two random controls before
learning. Its hash is
`1ab1c865758b33fda0ab90870b8f2fb74bd76c554ac577db4e3de85770d3aea4`.
All 256 games replayed at exactly 124 decisions with zero illegal actions;
score means/differentials and empirical 95% intervals are recorded in
`evidence/rl-baselines-v1.json`, summary hash
`1cc6637dbeae0066eac0b27bff758cf4fc7a519ea674c41246c3afb78006ea3d`.
Raw round and final scores remain primary; the observed seat effect is retained
rather than averaged away or reframed as proof.

### [x] 7.4 Apply Puffer-inspired performance work only after measurement

Profile direct typed scalar, naive batch, preallocated batch, and parallel batch
paths. Evaluate structure-of-arrays, fixed buffers, thread partitioning, and
double buffering based on evidence. Do not duplicate game rules in an unsafe
fast environment unless a separately tested/generated representation proves
semantic parity.

**Completion criteria:** A committed benchmark summary identifies environment,
encoding, inference, and synchronization costs; optimized paths retain scalar
parity; no external network traffic occurs.

**Completion notes (2026-08-05):** Complete. A Rust 1.96.0 Windows release
profile measured at least one million full-rule decisions per path: direct
typed scalar 57,904.667/s; naive batch 64,423.005/s with 3,907 deliberate hot
allocations; preallocated batch 69,594.485/s with zero; four independent
thread partitions 201,006.447/s with zero. Measurement therefore justified
preallocated structure-of-arrays buffers and bounded thread partitioning, but
not an unsafe duplicate environment or speculative double buffering. Exact
sequential/parallel signatures retain semantic parity and every path reports
`network=none`. Environment, encoding/buffer, allocation, and synchronization
limits and the honest non-process-wide allocation caveat are recorded in
`docs/rl-performance.md`; inference/device costs remain assigned to Burn.

### [x] 7.5 Connect text/web replay to selected RL episodes

Allow evaluation to retain a small chosen episode as the same protocol-style
public/viewer transcript used by CLI/web without putting transcript generation
in every rollout. Record selection criteria so only interesting/failing episodes
pay serialization cost.

**Completion criteria:** A seed from an evaluation summary replays in CLI and
web with identical game/observation/reward hashes; training throughput with
recording disabled is unaffected within the recorded benchmark tolerance.

**Completion notes (2026-08-05):** Complete. Evaluation serializes only the
preregistered worst/lower-median/best selections; normal rollout buffers never
construct transcript or protocol strings. A selected best seat-zero
legal-random-versus-heuristic episode at seed `3025338370` is reproducible as
inspectable NDJSON through `rl replay`, in the native/static-WASM egui replay,
and in the semantic hostable web route. All three pin episode hash
`7ab24388ab33c34e4763e065e493b2529c6562c8d70b59e909f70920442fc5a9`,
scores `[99, 12]`, differential 87, 13 round rewards, 124 decisions, and zero
illegal actions. Browser DOM evidence displayed that exact hash and a terminal
transition while containing neither `private_hand` nor `deck`; the NDJSON
contains only viewer observation/action/reward hashes and public score metrics.
The WASM build passes, and benchmark recording remains disabled in every
throughput path by construction.

## Phase 8 - Burn learner, self-play, and empirical evaluation

### [x] 8.1 Prove Burn backend and algorithm primitives on controlled scopes

Add `crates/poche-burn` depending on `poche-rl`, never the reverse. Pin Burn
0.21.x or the exact approved release. Implement observation tensors, masked
categorical sampling, policy/value modules, optimizer, checkpoint records, and
CPU Flex plus WGPU (and optional CUDA) devices.

Before full Poche, train on a tiny deterministic/micro environment with a known
optimal or exhaustive reference. Compare selected behavior with Burn's DQN
example/API where useful, but close G29 with the actor-critic ADR.

**Completion criteria:** Forward/backward/checkpoint round trips work on CPU and
one available GPU backend; masks assign zero selection probability to illegal
actions; the controlled task learns the preregistered behavior.

**Completion notes (complete, 2026-08-05):** Added `poche-burn` with exact
Burn 0.21.0 and the required one-way dependency on `poche-rl`. A backend-
generic 307→hidden→hidden shared-trunk actor-critic emits 60 policy logits and
one value. The same strict path implements one batched host/device transfer,
legal-logit masking, clipped PPO loss, value loss, entropy, backward, Adam with
norm clipping, and separate model/optimizer records. Illegal host probabilities
are exactly zero and masked selections fail before device work.

The controlled two-context deterministic task has an exhaustive optimal action
for each context. Starting from a fixed seed, the real PPO update changes both
greedy decisions to those optima; the 0.2 trust-region clip is respected rather
than demanding unbounded logits. CPU Flex model+optimizer checkpoints restore
exact outputs. The explicit backend smoke then passed the identical full-rule
observation/mask forward/backward/checkpoint transaction on Flex and the
machine's default WGPU backend: both reported 51,994 model bytes, 103,766
optimizer bytes, exact-zero illegal probability, and exact restored logits.
Backend-specific output digests are
`b65573ccf4324b6abd58e10c51ec40ecc03999259c1e1d4948adc26d1f5bf23b`
(Flex) and
`f6507295bf4781cd8b91a344c2466ecd6109284592c19c7568908447bf83baf4`
(WGPU); cross-device bit identity is not claimed.

### [x] 8.2 Implement PPO/GAE over turn-based rollout buffers

Implement advantage/return calculation with explicit terminal and per-decision
time handling, clipped policy objective, value loss, entropy term, gradient
clipping, minibatches, epochs, deterministic seed controls, and metrics. Keep
reward clipping/shaping off unless added as a new reward version.

Add numerical unit tests against small hand-calculated tensors and gradient/
update sanity tests. Avoid frequent device synchronization and small tensor
allocations; batch host/device transfers.

**Completion criteria:** Loss/advantage fixtures pass; a saved run resumes
without semantic-schema mismatch; CPU/GPU runs use the same algorithm and
produce appropriately qualified reproducibility evidence.

**Completion notes (complete, 2026-08-05):** Implemented reverse-time GAE with
explicit terminal reset and `gamma^decision_time_distance`, plus a hand-
calculated test that distinguishes two-step discounting and terminal bootstrap.
Full-rule collection uses `PocheRlEnv` only: it retains exact viewer
observations/masks and pairs an action with that same seat's next observation,
accumulating intervening raw `round-score-v1` reward and decision distance.
There is no duplicated game rule or hidden-state input.

The learner validates flat shapes/finiteness/masks, computes gathered old/new
log probabilities, clipped and unclipped policy objectives, value MSE, entropy,
and gradient-norm clipping. Training uses deterministic seed-controlled
sampling, Fisher-Yates minibatch order, four epochs, and batches host/device
transfers. Reward clipping/shaping remains absent. Model and optimizer records
resume together only after Burn version, model shape, spec ID/hash, reward ID,
and artifact digests validate. Flex and WGPU execute the same generic update;
their floating-point outputs are qualified per backend, not asserted bitwise
equal. Exact numerical, masking, update, and checkpoint tests pass.

### [x] 8.3 Add self-play with frozen historical policies

Implement seat-randomized policy assignment, current-policy mirrors, and a
bounded pool of immutable historical checkpoints inspired by PufferLib. Record
which policy controls each seat in every evaluation episode. Separate rollout
opponents from evaluation opponents to avoid reporting a moving target as
progress.

**Completion criteria:** Frozen checkpoints never mutate; deterministic matchup
schedules replay; catastrophic forgetting/regression can be detected against a
fixed baseline suite; memory/storage bounds are explicit.

**Completion notes (complete, 2026-08-05):** Added a bounded append-only
`FrozenPolicyPool` whose ID/digest identities cannot be replaced or duplicated.
Its deterministic schedule randomizes the current policy's seat from the seed
and fails explicitly when full. The first run fixes capacity five and retains
initial plus updates 1–4. Each update runs one current-policy mirror game and
three seat-randomized games against the immutable prior checkpoint; every
actual seat assignment is recorded in the training summary. Frozen bytes are
loaded into a separate inference model and never mutated by the optimizer.

Evaluation opponents are not drawn from this moving pool: legal-random and the
heuristic remain fixed, held-out, seat-swapped baselines. The pool's memory/
storage bound is five in-memory model+optimizer records during this short run;
only the final large record persists beneath ignored `artifacts/`. Unit tests
prove duplicate/full rejection and deterministic schedule replay, while the
run recorded five distinct immutable artifact identities.

### [x] 8.4 Train the first full-rule policy and preserve artifacts responsibly

Run the preregistered `poche-2p-v1` training experiment. Store large weights,
optimizer state, raw logs, and traces outside Git or as named CI/release
artifacts. Commit only configuration, semantic hashes, code revision, seeds,
hardware/backend, summarized curves, and artifact digest/location.

**Completion criteria:** The run is reproducible from the recorded manifest;
checkpoint loading rejects incompatible game/observation/action/reward hashes;
failure to beat a baseline is reported as a result rather than hidden or
reframed as formal evidence.

**Completion notes (complete, 2026-08-05):** The committed manifest
`rl/manifests/poche-ppo-v1.json` fixes Burn 0.21.0/Flex, 307×64×64×60 model,
all preregistered PPO parameters, root seed 1347374915, four updates, four games
per update, five frozen policies, 16 held-out seeds, and ignored artifact
location. `cargo run -p poche-xtask --offline -- rl train --manifest
rl/manifests/poche-ppo-v1.json` completed 16 full games and 1,240 same-seat
transition rows. Mean total loss by update was 112.599, 80.789, 55.877, and
42.578; this is a diagnostic curve, not proof of policy quality.

`model.bin`, `optimizer.bin`, full logs/summaries, and replay payloads stay under
ignored `artifacts/rl/poche-ppo-v1-short`. The current concrete artifacts are
identified by model digest
`7e5438ee3a7c22fa41ff0d0a1409ffce8282ad2bd95f3075465ec13995b4d4f9`
and optimizer digest
`6a8f32c6fc660380918bd386ff3ceeda05360f9e515fb26467cf985234965776`.
Burn deliberately generates random internal parameter IDs, so regenerated
container digests differ; two clean repeat runs nevertheless reproduced every
loss value and the fixed viewer-policy output digest
`1aa3597e69074098bdeecc43b7ff3f52f0d25d9e5f0de1cb31d4a1cf21140a08`.
The manifest/artifact digest checks reject semantic or concrete-file mismatch
before loading, and evaluation rechecks the semantic output digest afterward.

### [x] 8.5 Evaluate by Poche score and render representative behavior

Evaluate current and frozen policies against legal-random and heuristic
baselines on a fixed held-out seed corpus with seat swaps. Primary statistics
are raw cumulative score and score differential, with confidence intervals;
round scores, exact-bid rate, game length, win rate, and illegal-action count are
secondary diagnostics.

Render representative best, median, worst, and failure episodes through the
text/web replay path. State clearly that policy quality is empirical.

**Completion criteria:** `cargo run -p poche-xtask -- rl evaluate --manifest
<path>` regenerates the summary and selected episodes; no claim exceeds the
measured corpus; evaluation never exposes hidden state to the policy.

**Completion notes (complete, 2026-08-05):** The exact required command now
dispatches learned manifests through the Burn evaluator while preserving the
existing baseline-manifest path. It ran 64 full games: 16 held-out seeds across
learned/legal-random and learned/heuristic with both seat assignments. Learned
seat mean differentials were +2.625 versus legal-random as seat 0 (95% CI
[-14.734, 19.984]), +24.125 as seat 1 (the reported seat-0 opponent interval
[-40.289, -7.961]), +18.563 versus heuristic as seat 0 ([-3.094, 40.219]),
and +15.438 as seat 1 (opponent interval [-33.696, 2.821]). Thus one interval
excludes zero and three do not; no general superiority claim is made. Mean raw
scores, all 13 mean round-score pairs, exact bids, 124-decision length, wins/
ties, and zero illegal actions are recorded.

Every matchup writes best/median/worst episodes as inspectable NDJSON and
strict `EpisodeTranscript` JSON; no illegal/execution failure occurred, so the
explicit failure selection is null rather than invented. The web app accepts a
selected JSON through `POCHE_RL_EPISODE_PATH`, revalidates spec/reward/zero-
illegal semantics, and renders it through `/rl/replay`; a test proves the same
semantic hash and absence of `private_hand`. `docs/burn-learning.md` and
`evidence/burn-poche-ppo-v1.json` contain commands, selected hashes, measured
results, artifact policy, and empirical-only limitations.

## Phase 9 - Aggregate evidence, documentation, and handoff

### [x] 9.1 Run workspace, license, protocol, and secret-safety gates

Run formatting, linting, docs, workspace tests, dependency/license audit,
protocol fuzz/property tests, redaction tests, and ignored-artifact checks.
Ensure new first-party files have MPL-2.0 notices where repository policy
requires them.

**Completion criteria:** A clean clone passes; no local sibling dependency or
dirty reference is required; generated models/web/training artifacts are not
tracked; secret scanners/fixtures contain no actual key/invite material.

**Completion notes (complete, 2026-08-05):** Phase 8 entered the release
gate from a clean synchronized commit at `279cb214322b3908bb7fca502177c606939133a4`.
The first fresh offline workspace test run passed every unit, integration,
native-formal, corpus, controlled-learning, renderer, and doc test in 169.5 s.
Formatting and strict all-target workspace Clippy also pass after extracting a
focused selected-replay writer from the evaluation summary. Workspace docs and
targeted protocol fuzz, larger-scope proptests, CLI arbitrary-input, identity,
redaction, and web projection tests also pass.

Windows-target Cargo metadata audited 646 resolved packages: no license field
is missing, every expression is permissive/data-font or MPL-2.0, no package is
an external local path, and the sole Git source is the revision-pinned MPL-2.0
`teamy-cancellation` 0.3.1 at `cc782906`. All new Burn Rust files carry an
explicit MPL-2.0 notice; the root license and workspace package metadata cover
older first-party files. Git tracks zero generated artifact candidates, while
`artifacts/`, `target/`, `site/`, `docs/main.pdf`, and representative weights/
WASM/PDF outputs resolve through `.gitignore`. Focused tracked-data scans found
zero PEM private keys, serialized secret fields, full room codes, or the lab
invite value in evidence/fixture/formal/page/RL data.

A guarded fresh clone of exact pushed commit
`279cb214322b3908bb7fca502177c606939133a4` passed `cargo check --workspace
--locked --offline`, protocol decoder fuzz, and all Burn tests while sharing
only Cargo's compiled target cache; source paths in Cargo output were solely
inside the clone and the clone was removed afterward. This proves the committed
source/lock do not require a sibling checkout or dirty reference worktree.

### [x] 9.2 Run formal, loopback, Veilid, and renderer acceptance

Run session coverage/oracles/conformance, loopback scenario, local Veilid
scenario, and the deployment/browser matrix selected by G26. Record native tool
and Veilid versions, exact state/result counts, fixture hashes, and limitations.

**Completion criteria:** All advertised lifecycle/security/rendering behaviors
have evidence; formal results name exact scopes/fairness; external-network
unavailability cannot be misreported as a semantic pass.

**Completion notes (complete, 2026-08-05):** Fresh tool discovery pinned Rust
1.96.0, Alloy 6.2.0, NuSMV 2.7.1, Scryer Prolog
0.10.0-17-ge4d96925, and Typst 0.15.1. The user-owned WinGet Typst location is
inaccessible to the sandbox identity, so its explicit path was verified under
the host identity rather than misreported as missing.

The aggregate session command explored the exact Rust lobby scope at 800
states and 38,400 attempted-command edges (5,872 accepted, 32,528 denied, 272
terminal, maximum depth 14) with semantic hash
`89c626a11bcef07d93007ba5a7bf097fa9dd88c94244775dec5c138a17e023b1`.
Alloy recognized 13 bounded results at its printed exact Seat/Snapshot/Int
scopes. NuSMV recognized 16 properties, a total/deadlock-free FSM, the intended
false unconditional-termination claim, true conditional CTL/LTL termination,
and the four named missing-fairness counterexamples. Seven Scryer query corpora
matched their pinned answer counts/digests. The 88-row catalog passed and four
tracks agreed on 10 claims/31 observations with zero disagreements.

The canonical-NDJSON loopback smoke passed all 178 inputs, 177 revisions, 13
deals, and complete lifecycle/security behavior with transcript hash
`ab90ff79acb755d72defaef22a37369a71a9125df64c91a8645260254934a075`
and final public hash
`be439108bb7ddb5695232faa074cb9180d1b34df6a1748598ad6e19c6f9759dd`.
Veilid 0.5.7 then passed 26 transport/security tests and two compile-fail secret
boundary docs. Its two-node isolated probe correctly had zero peers and no
public/local readiness or private routes; this is recorded as the released
topology limitation, not a semantic/network pass. The earlier explicit public
acceptance remains the real-network evidence: 174 AppCalls/172 event frames,
one duplicate, one denial, one reconnect, final revision 173, scores 40-20,
chat three, and verified spectator grant/revoke, player leave, and room close
over DHT/private routes.

Native egui replay, WASM replay, and the Datastar server passed tests and fresh
release builds; the ignored artifacts were 5,699,584-byte native replay,
3,397,120-byte server, 3,643,651-byte WASM, 73,769-byte JS, and 1,763-byte page
with zero publication sentinels. Pinned Typst generated a valid five-page,
192,974-byte tagged PDF; Poppler rendered every page with no overlap, clipping,
missing glyph, or placeholder defect. Local TeX Gyre fonts were unavailable,
so that copy uses fallbacks; Pages installs and verifies the named fonts.
A fresh in-app browser run exercised ordinary semantic HTML: Alice paused, Bob
received and applied `Resume game`, and the spectator saw no hand. This
reconfirms G26/G32: HTTPS Pages hosts rulebook/static replay only; live browsers
use the host-colocated Datastar authority, while direct browser Veilid remains
unsupported on HTTPS/WSS and no companion is implied.

### [x] 9.3 Run RL correctness, performance, GPU, and evaluation acceptance

Run scalar/batch parity, baseline corpus, allocation/performance benchmark,
Burn CPU/GPU smoke, controlled learning task, full training manifest replay,
and held-out evaluation. Separate deterministic semantic equality from expected
floating-point/training variability.

**Completion criteria:** The environment is fast without network use; selected
GPU tensor execution is proven; training/evaluation artifacts are digest-bound;
results emphasize score and carry empirical confidence labels.

**Completion notes (complete, 2026-08-05):** The exact `poche-2p-v1` contract
regenerated at 307 observation values, 60 actions, `round-score-v1`, and hash
`8852f8568ead1e40aad7bb4ca5b7725340cc01422e077ffddcd5d4f5665bf0bf`.
All 14 environment/batch/baseline tests passed, including hidden-state
noninterference, mask round trips, same-seat reward attribution, scalar/
preallocated/parallel equality, deterministic seeds, score-first transcripts,
and fixed seat-swapped evaluation.

Three one-million-decision release samples were network-free. Median throughput
was 61,052.586/s direct, 64,747.282/s naive batch, 68,285.275/s preallocated,
and 195,637.909/s across four partitions. Preallocation was slower than naive
in one sample but about 5.5% faster at the median; crucially it reported zero
hot reallocations in all samples versus 3,907 naive allocations. Exact ranges
and the non-promise qualification are in `docs/rl-performance.md`.

Fresh Flex CPU and default WGPU smoke executed the identical 307×60 forward,
masked PPO backward, and checkpoint round trip. Both reported exactly zero
illegal probability, 51,994 model bytes, 103,766 optimizer bytes, and their
committed backend-specific output digests. The controlled two-context optimum,
GAE fixture, clipping/update sanity, immutable pool, and record tests pass.

The exact manifest replay again completed 16 full games/1,240 rows and
reproduced every update loss (112.59850025177002, 80.78937244415283,
55.87652349472046, 42.578110694885254) plus semantic policy probe
`1aa3597e69074098bdeecc43b7ff3f52f0d25d9e5f0de1cb31d4a1cf21140a08`.
Fresh concrete model/optimizer digests were `a9441efff094567614664c814d0f33bde63e3b9a5b35d2952d6d61064ba3ffe6`
and `24b9fe07f507921247513fe43dd22fd60ef75800c545d1a00041daa158607717`;
their difference from the earlier recorded containers is the disclosed Burn
parameter-ID variability, not hidden semantic drift.

Held-out evaluation reproduced all 64 games, score means/differentials/95%
intervals, and 12 selected hashes exactly, with 124 decisions and zero illegal
actions per game, `hidden_state_exposed=false`, and empirical-only labels. Only
one of four intervals excludes zero. Final acceptance found and fixed a CLI
dispatch gap: `rl replay` now detects a learned manifest, validates current
checkpoint/evaluation/selection/episode hashes, and emits the same 126-line
canonical NDJSON framing as baseline replay. Learned hash
`adc2da13e127d65678a5c1cca3d04019a701ba1f1f30b4f965bc0c934586b3fa`
and baseline hash
`7ab24388ab33c34e4763e065e493b2529c6562c8d70b59e909f70920442fc5a9`
passed; an unselected seed failed closed.

### [~] 9.4 Complete guidance audit, documentation, commits, and push

Update README and contributor docs with architecture, threat model, CLI, room
codes, reconnect, chat, spectator grants, native/direct/hosted client modes and
their privacy differences, RL manifests, proof-versus-training evidence, and
exact reproduction commands. Mark tasks
complete only with adjacent evidence. Rerun all three intent-audit passes and:

```powershell
cargo run -p poche-xtask -- guidance audit PLAN.md
cargo run -p poche-xtask -- guidance audit PLAN-2-MULTIPLAYER-RL-RENDERING.md
git status --short --branch
```

Commit coherent slices with meaningful messages, push `model-checking`, verify
the remote branch SHA, and update the plan status to `Execution complete` only
after no required work remains.

**Completion criteria:** Every U30-U50 row has completion evidence; all gates
are decided/deferred/blocked with exact conditions; all overall criteria below
are checked; the local branch is clean and synchronized with the verified
remote commit.

**Completion notes (in progress, 2026-08-05):** All implementation/acceptance
tasks through 9.3 are complete. The final documentation coverage, gate/U-row
audit, triple intent audit, overall criteria, full workspace regression,
commit/push, Pages status, clean-branch, and remote-SHA checks remain in
progress; plan status stays `Execution in progress` until they all pass.

## Overall completion criteria

- [ ] The predecessor plan remains unchanged as truthful completed history, and
  both plans pass generalized guidance audits.
- [ ] Every U30-U50 requirement maps to completed evidence; the final triple
  intent audit finds no silent omission or weakened qualifier.
- [ ] `SessionState` and `GameEnvironment` remain separate, deterministic,
  strongly typed reducers with an explicitly tested composition boundary.
- [ ] Versioned typed commands/events and canonical NDJSON drive CLI, network,
  renderer, fixtures, and replay; RL uses the same semantics without parsing or
  networking in its hot path.
- [ ] Host/create, code/join, membership, ready, abortable countdown, start,
  any-player pause and any-player unpause, post-game, reconnect, leave/close,
  and bounded chat work in loopback and native Veilid scenarios.
- [ ] Application keys and capabilities, not room codes or transport IDs alone,
  authorize commands; duplicate/stale/cross-room/revoked attempts fail closed
  with auditable policy reasons.
- [ ] Spectator request/grant/revoke works per recipient; unauthorized clients
  never receive private hands; revocation is accurately described as preventing
  future disclosure.
- [ ] Independent Alloy, NuSMV, Prolog, and Rust session models cover every
  applicable session rule, agree or classify differences, and label bounded/
  conditional liveness precisely.
- [ ] The existing game's termination evidence remains valid and separate;
  session pause/network liveness claims state fairness assumptions and show the
  expected unconditional counterexamples.
- [ ] The CLI provides inspectable human text, NDJSON automation, structured
  diagnostics, replay, and safe identity/room operations based on the clean
  template patterns.
- [ ] A minimal egui-first client renders viewer-correct rooms and games. The
  browser-only Veilid/native-client/Datastar choice follows Task 6.1 evidence;
  README and Pages advertise only proven modes and their actual trust boundary.
- [ ] `poche-2p-v1` fixes observation/history/action/mask/reward semantics and
  semantic hashes; scalar and vectorized rollouts are deterministic and agree.
- [ ] Legal-random and heuristic baselines, fixed evaluation seeds, selected
  episode replay, and performance/allocation measurements exist before training
  conclusions.
- [ ] Burn performs policy/value tensor inference and learning on CPU and one
  available GPU backend; PPO/self-play/checkpoint code passes controlled tests.
- [ ] The first full-rule policy has a reproducible training/evaluation manifest
  and is reported primarily by Poche score without optimality/proof overclaim.
- [ ] Large generated PDFs, web bundles, traces, logs, and model weights remain
  out of Git; committed summaries identify artifact digests and semantic/code
  revisions.
- [ ] The full workspace, native formal tools, local Veilid harness, renderer
  tests, RL parity, and documentation acceptance pass from a clean clone.
- [ ] All first-party work is MPL-2.0, third-party provenance is recorded, and
  no Veilid upstream contribution was generated or implied.
- [ ] The completed work is committed, pushed to `model-checking`, and the
  remote commit is verified.

## Deferred follow-ups

### Trustless multiplayer and host migration

If host trust is unacceptable, start a separate research plan covering
verifiable shuffle/deal protocols, commitments, mental poker/MPC, replicated
event authority, conflict resolution, host migration, and recovery. Do not
incrementally imply these properties in the host-authoritative design.

### Persistent social system

Global accounts, friends, parties, matchmaking, discoverable public rooms,
moderation, bans, durable chat, notifications, and rankings require separate
privacy, abuse, data-retention, and availability decisions. The TPBAC vocabulary
is preserved so those can compose later.

### More RL populations and algorithms

Add new immutable specs for three/four-player Poche, recurrent policies,
population-based training, alternative rewards, search/planning agents,
opponent modeling, distributed training, and hyperparameter sweeps. Never reuse
`poche-2p-v1` identifiers for shape/semantic changes.

### Polished rendering

Card art, animations, sound, responsive/mobile design, accessibility refinement,
spectator broadcast presentation, and richer training dashboards follow the
functional renderer and privacy tests.

## Risk register

| Risk | Consequence | Mitigation / evidence gate |
| --- | --- | --- |
| Veilid browser HTTPS/WSS support is insufficient | A Pages-hosted direct live client cannot connect and an on-device companion would undermine the intended browser-only experience | G26/Task 6.1; use the failure to select between a native Veilid client and a self-hostable Datastar server, retaining a static replay/demo and documenting the topology. |
| A native Vulkan direction is chosen without accounting for portability | Low-level renderer work prevents the browser client and consumes effort unrelated to game semantics | G32/Task 6.1; egui first, bounded ash/cursor-latency evidence spike, and an ADR comparing latency/control against distribution and web reach. |
| A Datastar server is described as equivalent to Veilid anonymity | Operators or infrastructure can observe metadata or state users expected to remain decentralized/private | Threat-model the server role and private projections, make self-hosting reproducible, disclose metadata visibility, and never reuse Veilid privacy claims. |
| Veilid transport signatures are mistaken for app authorization | Unauthorized room commands or replay | G21; app-signed canonical commands, default deny, epochs/revisions/IDs, controlled attack tests. |
| Immutable DHT schemas conflict with dynamic membership | Join/revoke cannot be represented safely | Host-owned DFLT rendezvous plus Poche event/capability log; do not mutate SMPL membership. |
| Host authority weakens hidden-information fairness | Host can inspect/manipulate full state | Explicit threat model and UI disclosure; signed audit log; trustless dealing deferred rather than implied. |
| Room code becomes permanent bearer credential | Leaked code gives lasting access | One-time/expiring invite; bind durable membership to stable application key; code replay tests. |
| Long-lived key is lost or leaked | Identity loss or room impersonation | Protected storage, explicit recovery/non-recovery semantics, revocation, no log/export by default. |
| Revocation is interpreted as erasure | Spectator retains already learned cards | Capability epochs stop future delivery; documentation/tests state irreversibility of prior disclosure. |
| Pause, disconnect, or chat explodes/invalidates liveness | False claim that every session ends | G30-G31; separate game/session models, logical ticks, bounded chat metadata, named fairness. |
| Network/DHT order is treated as event order | Divergent room state | Host revision sequence, idempotent commands, snapshot + tail recovery, DHT watches as refresh hints only. |
| Existing observation forgets public history | Reconnecting humans or feed-forward policies lose legal information | Task 2.3 audit; versioned public history/event prefix; recurrent policy only as explicit choice. |
| Variable player/action shapes complicate tensors | Brittle padding and policy incompatibility | One immutable fixed-player `RlSpec` at a time, action masks, semantic hash checks. |
| Score rewards are delayed and non-zero-sum | Incorrect per-seat transition/advantage attribution | Turn-based transition assembler, round-boundary fixtures, separate score/differential/outcome metrics. |
| GPU overhead exceeds game/model compute | Slower training despite Burn GPU support | CPU Flex baseline, batched inference, synchronization profiling, WGPU/CUDA comparison. |
| RL framework code silently becomes the game oracle | Training-specific behavior diverges from rules | `poche-rl` depends on `GameEnvironment`; scalar/batch parity and transcript replay; no duplicated fast rules. |
| Dirty local templates/references leak into dependencies | Irreproducible build or overwritten user work | Pin released versions/clean commits; selective port; never copy dirty worktrees wholesale. |
| Generated artifacts bloat Git | History churn and large repository | Ignore weights/logs/web builds/PDFs; publish artifacts separately; commit only manifests/summaries/digests. |

## Resume checklist

On any resumed task:

1. Read this file's guidance ledger, current gate table, current in-progress task,
   and that task's completion notes.
2. Run `git status --short --branch` and preserve unrelated user changes.
3. Recheck dependency/version evidence if the relevant external repository or
   current date changed.
4. Work only the current task unless the plan explicitly authorizes an
   independent track.
5. Run the task's exact acceptance commands and record concise evidence next to
   the task before changing `[~]` to `[x]`.
6. If a material architecture question is not answered by a decided gate, stop
   dependent work, record the evidence and decision needed, and ask the user.
7. Before claiming the phase complete, rerun guidance audit and the three intent
   audit passes; do not rely on memory or compacted conversation context.
