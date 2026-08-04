# Poche phase 2: multiplayer sessions, text protocol, rendering, and reinforcement learning

**Plan ID:** `poche-phase-2`

**Plan status:** Ready for execution

**Primary implementation root:** `D:\Repos\Games\poche-3` on branch `model-checking`

**Predecessor:** `PLAN.md`, whose formal-modeling milestone is complete and must remain a truthful historical record

**Last updated:** 2026-08-04

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

## Intent audit evidence

- **Pass 1 — extraction:** Reread the complete current request and the relevant
  confirmed predecessor guidance. U30-U46 preserve the three tracks, text-first
  ordering, Veilid room-code web goal, TPBAC/key authorization, every named room
  state, fast loopback, CLI template, chat, durable reconnect identity,
  spectator hand grants/revocation, Burn, PufferLib-as-reference, round scores,
  MPL-2.0, and the request to extend the existing principles.
- **Pass 2 — traceability:** Verified every active U30-U46 row maps to at least
  one concrete task and completion criterion. Checked protocol, session,
  authorization, formal-model, CLI, Veilid, rendering, RL, training, evidence,
  and documentation sections for weakened or missing requirements.
- **Pass 3 — adversarial omission:** Specifically searched for likely losses:
  renderer work accidentally blocking RL; text serialization contaminating the
  rollout hot path; Veilid being treated as authorization; room codes becoming
  permanent bearer credentials; DHT schemas being assumed mutable; browser
  Veilid being promised on HTTPS without evidence; chat or wall-clock values
  exploding formal state; pause invalidating an unconditional termination
  claim; spectators receiving globally broadcast hidden state; revocation being
  described as erasure; PufferLib becoming a dependency; Burn owning game
  semantics; score being replaced with win/loss shaping; or the completed plan
  being rewritten. Each has an explicit disposition below.
- **Known source limitation:** None. The user messages, predecessor plan, local
  repositories, and current primary documentation were available during plan
  creation.

## Outcome

Deliver one coherent Poche application substrate with four interchangeable
consumers:

1. a replayable text CLI;
2. a loopback or Veilid-backed multiplayer room;
3. a minimal viewer-correct web renderer;
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
  therefore a measured gate. A locally served web renderer backed by a native
  Veilid process is the reliable first web deployment.
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
| G18 | Provisional | First multiplayer authority model? | Host-authoritative room reducer and event log; signed participant commands; signed host events; no host migration. The host is trusted with full hidden game state for this phase. | Task 1.2 threat-model ADR, including explicit alternatives and user-visible trust statement. |
| G19 | Provisional | What does a room code authorize? | A versioned/checksummed rendezvous locator plus expiring or one-time invite secret. Successful redemption binds a stable player public key to membership; the code is not a permanent bearer authority. | Native two-node create/join/replay/revoke spike in Task 5.2. |
| G20 | Provisional | What identity persists? | An application-level player signing key stored through protected storage. Veilid node IDs and private routes are replaceable transport identities. Room membership refers to the stable application key. | Restart/reconnect test and secret-storage review in Tasks 5.1 and 5.3. |
| G21 | Decided | How are authorization decisions modeled? | TPBAC-shaped immutable attempt and decision records, default deny, explicit allow, deny override, stable policy IDs/reasons, and audit-only policy support. | Cross-model authorization fixtures and controlled defects. |
| G22 | Decided | First countdown behavior? | Host may arm only when minimum seats exist and every seated player is ready. Any seated player may unready or abort, cancelling the countdown. An authority clock emits a logical expiry event; expiry starts once if preconditions still hold. | Session rules and NuSMV/Rust liveness checks under named clock fairness. |
| G23 | Provisional | Who may pause/resume? | Recommended: any active player may request pause; host may also pause for disconnect; resume requires all connected active players to acknowledge, with an explicit host override policy only if the user approves it. | Confirm in Task 1.2 before pause reducer/model work; record abuse/disconnect tradeoffs. |
| G24 | Decided | Is chat game state? | No. Chat is an authorized, rate/size-bounded session event stream with ephemeral first-phase retention. Formal models track send permission/count abstractly, not text content. | Protocol and policy tests; persistence remains deferred. |
| G25 | Decided | What does spectator revocation mean? | Stop future hand projections/delivery at the next capability epoch. Never claim already delivered information can be forgotten. Hidden observations are produced per recipient and never room-broadcast. | Projection/noninterference tests and an Alloy bounded information-flow model. |
| G26 | Open | Can the public web app run Veilid directly? | First ship a locally served web client over the typed protocol to a native Veilid runtime. Separately test direct `veilid-wasm` on HTTP and HTTPS/WSS. Do not advertise direct Pages multiplayer until the HTTPS test passes. | Task 6.1 browser matrix. If direct HTTPS fails, any relay/native-companion production choice requires explicit user approval. |
| G27 | Provisional | First RL tensor shape? | `poche-2p-v1`: fixed two-player full-rule game, seat-relative viewer encoding, fixed bid/card action vocabulary, legal mask, and explicit public-history strategy. Add other player counts as new specs. | Task 7.1 schema audit, random-policy parity, and user-visible manifest. |
| G28 | Provisional | First reward projection? | `round-score-v1`: zero except at a round boundary, then the seat's raw rulebook points; terminal outcome and money are separately logged. No undocumented shaping or reward clipping. | Task 7.1 exact examples and baseline-return tests. |
| G29 | Provisional | First learning algorithm? | Implement a small actor-critic PPO/GAE loop in Rust over Burn, with legal-logit masking and self-play against frozen checkpoints. Use Burn DQN only as an API reference/smoke comparison. | Task 8.1 controlled micro-environment test and recorded algorithm ADR. |
| G30 | Decided | What liveness can be claimed once pause/network exist? | Preserve unconditional game termination only for the existing semantic game under its named scope. Session liveness is conditional on clock, delivery, player-action, and eventual-resume fairness. Paused/partitioned sessions may legitimately persist. | NuSMV/Rust properties must state assumptions and include counterexamples when each fairness assumption is removed. |
| G31 | Decided | How is state-space explosion controlled? | Independently model game, session/authorization, and abstract transport; compose contracts and a small integration scope. Bound principals/messages/ticks and omit chat content/cryptographic bitstrings. | Coverage matrix and exact scope statements for every formal result. |
| G32 | Open | Which Rust web UI stack is used? | Select only after the transport spike; require Rust/WASM compatibility, accessible semantic controls, deterministic projection rendering, modest dependency weight, and local/static build support. | Task 6.1 comparison ADR; visual fashion alone is not a deciding criterion. |

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

## Execution order

1. Generalize plan auditing; close authority/pause/web-test/RL-spec gates; write
   session rules and the threat model.
2. Implement the pure protocol, authorization, session reducer, projections,
   and deterministic transcripts.
3. Build and compare independent session models before networking can obscure
   semantic mistakes.
4. Deliver a complete loopback CLI vertical slice.
5. Add native Veilid rooms, identity, invites, reconnect, chat, and private
   projections.
6. Add the minimal web renderer and determine direct-browser deployment from
   evidence.
7. Implement and benchmark typed vectorized RL rollouts and baselines.
8. Add Burn inference/training, PPO self-play, checkpoints, and evaluation.
9. Run aggregate acceptance, publish honest evidence, commit, and push.

Phases 5-6 and 7-8 become independent after Phase 4. They may be worked in
parallel only when separate agents/worktrees are explicitly authorized and each
track updates this file without overlapping edits.

## Phase 1 - Preserve intent and lock the semantic boundary

### [ ] 1.1 Generalize resumable-plan auditing without weakening `PLAN.md`

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

**Completion notes:** Not started.

### [ ] 1.2 Record composition, threat model, dependency, and open-gate decisions

Create `docs/decisions/0003-session-network-rl-architecture.md` (using the next
available ADR number if 0003 is occupied). It must:

- confirm G15-G22 and G24-G31 or record a superseding decision;
- close G23's pause/resume policy with the user if the recommended policy is not
  accepted;
- define what the host, player, spectator, DHT cache, network peer, and web host
  are trusted to learn or do;
- state plainly that first-phase host authority can inspect all hands and that
  trustless dealing is out of scope;
- define app identity versus Veilid node/private-route identity;
- pin released MPL-compatible dependency versions, avoiding sibling path
  dependencies and dirty local worktrees;
- record the Veilid upstream AI-contribution constraint and that no upstream
  code changes are planned;
- pre-register the browser test matrix that closes G26;
- pre-register the first `RlSpec`, reward, baselines, algorithm experiment, and
  evaluation metrics before training.

**Completion criteria:** No dependent task relies on an unrecorded authority,
pause, persistence, browser, reward, or dependency assumption. License files and
source provenance are compatible with MPL-2.0.

**Completion notes:** Not started.

### [ ] 1.3 Extract stable session and authorization rules

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

**Completion notes:** Not started.

## Phase 2 - Typed text protocol and pure session engine

### [ ] 2.1 Add protocol and session crates with versioned envelopes

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

**Completion notes:** Not started.

### [ ] 2.2 Implement room lifecycle, policy decisions, and game gating

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

**Completion notes:** Not started.

### [ ] 2.3 Implement viewer projections and spectator capabilities

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

**Completion notes:** Not started.

### [ ] 2.4 Add deterministic transcripts, snapshots, and replay

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

**Completion notes:** Not started.

### [ ] 2.5 Prove typed/direct, NDJSON, and optional Phon semantic parity

Run each fixture directly through typed commands and through NDJSON decode. If a
Phon codec is added, run it as a third path. Compare decisions, events,
snapshots, projections, errors, and semantic hashes, not merely successful
deserialization.

**Completion criteria:** All enabled codecs agree exactly; malformed/unknown
input fails closed; NDJSON remains the normative inspectable form even if Phon
is smaller/faster.

**Completion notes:** Not started.

## Phase 3 - Independent formal session models

### [ ] 3.1 Build the Alloy room/authorization oracle

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

**Completion notes:** Not started.

### [ ] 3.2 Build the NuSMV lifecycle/liveness oracle

Add `models/nusmv/session.smv` independently. Model logical ticks, readiness,
abort/expiry races, start, running, selected pause protocol, resume,
disconnect/reconnect abstraction, post-game, and close. Check safety plus
conditional CTL/LTL properties. Explicitly demonstrate that unconditional
session termination is false with arbitrary pause/partition, then prove the
intended liveness under named fairness assumptions.

**Completion criteria:** Native NuSMV results distinguish holds from expected
counterexamples; removing readiness, eventual-expiry, eventual-action, or
eventual-resume assumptions demonstrates why each liveness claim needs it.

**Completion notes:** Not started.

### [ ] 3.3 Build the Scryer Prolog policy/predecessor oracle

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

**Completion notes:** Not started.

### [ ] 3.4 Add exhaustive Rust session checking

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

**Completion notes:** Not started.

### [ ] 3.5 Audit full session coverage and cross-model agreement

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

**Completion notes:** Not started.

## Phase 4 - Loopback multiplayer CLI vertical slice

### [ ] 4.1 Add a Poche CLI by selectively porting the clean template

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

**Completion notes:** Not started.

### [ ] 4.2 Implement in-process multi-client transport and authoritative runtime

Define transport/client ports in `poche-runtime` and implement an
`InProcessTransport` that can host multiple independently authenticated clients,
inject duplicates/reordering/disconnects, and drive a manual logical clock.
Typed direct delivery is the default; an NDJSON loopback mode tests framing.

**Completion criteria:** Multiple CLI client instances (or scripted client
objects) create/join/play one full game without sockets; fault injection is
deterministic; RL crates do not depend on this transport.

**Completion notes:** Not started.

### [ ] 4.3 Deliver inspectable text play and replay

Create concise text projections for lobby, countdown, game observation, legal
actions, pause state, chat, spectator grants, and results. Also support raw
NDJSON input/output so a complete room can be driven by a checked-in script.
Text rendering must be a pure presentation of typed projections, never a second
semantic implementation.

**Completion criteria:** A golden script hosts two players and a spectator,
aborts one countdown, starts another, pauses/resumes, completes a game, and
replays to the same hashes. A new agent can inspect the transcript and identify
every action/decision/event without a graphical client.

**Completion notes:** Not started.

### [ ] 4.4 Add ephemeral chat and transcript-safe redaction

Implement bounded chat through the same authorization/event service, but retain
only the configured in-memory tail. Escape/render user text as data. Add rate,
size, membership, and closed-room rejection tests and verify transcript export
can include chat while never including private protocol material.

**Completion criteria:** Chat works in loopback CLI; control-frame injection and
oversize/rate tests fail closed; formal coverage tracks permission and count,
not unbounded content.

**Completion notes:** Not started.

### [ ] 4.5 Run the loopback acceptance scenario

Add `cargo run -p poche-xtask -- multiplayer smoke --transport in-process` and
commit its small normalized evidence summary.

**Completion criteria:** The scenario covers every lifecycle and spectator
feature in U35/U39/U41, completes a real Poche game, reproduces from its seed and
transcript, and passes protocol/session/formal coverage gates.

**Completion notes:** Not started.

## Phase 5 - Native Veilid rooms and durable identity

### [ ] 5.1 Implement application identity and secret storage

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

**Completion notes:** Not started.

### [ ] 5.2 Implement DHT rendezvous, private routes, and invite codes

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

**Completion notes:** Not started.

### [ ] 5.3 Implement membership reconnect without the original code

Persist the room locator and membership credential/capability bound to the
stable player key. On restart or private-route change, resolve current
rendezvous data, prove the stable identity, refresh recipient routes, receive a
snapshot + event tail, and resume the same membership/seat when policy permits.

**Completion criteria:** Host and client restart/reconnect cases are tested;
route replacement does not change principal identity; reconnect needs no invite
code; removed/banned/revoked membership cannot reconnect as active.

**Completion notes:** Not started.

### [ ] 5.4 Carry commands, events, countdown, pause, and chat over Veilid

Implement retry categories for `TryAgain`, timeout, no connection, stale route,
watch renewal, duplicate delivery, and shutdown. Treat DHT watch notifications
as hints that trigger validated refresh; never as an authoritative event order.
Use the session revision log to deduplicate/recover.

**Completion criteria:** A local multi-node harness executes the Phase 4
scenario over Veilid; forced disconnect/reorder/retry does not duplicate state;
countdown uses authority time and clients display estimates only; chat and game
commands obey the same app authorization.

**Completion notes:** Not started.

### [ ] 5.5 Encrypt and deliver viewer-specific private projections

Encrypt private projection payloads to each stable recipient key (or document
and test an equivalent Veilid-supported end-to-end construction). Public room
events may be shared, but hands are never shared and UI-filtered afterward.
Rotate projection/capability epochs on grant/revoke and relevant membership
changes.

**Completion criteria:** Packet/event capture from an ungranted spectator lacks
decryptable hand data; granted spectator receives only the selected player's
current/future authorized projection; revoke stops subsequent delivery; other
spectators and players gain nothing.

**Completion notes:** Not started.

### [ ] 5.6 Run native Veilid security and lifecycle acceptance

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

**Completion notes:** Not started.

## Phase 6 - Minimal rendering and web delivery

### [ ] 6.1 Close browser transport and Rust UI gates with executable spikes

Build the smallest possible viewer/client against protocol fixtures, then test:

1. native runtime + locally served web UI;
2. `veilid-wasm` on local HTTP/`ws://`;
3. `veilid-wasm` on HTTPS/`wss://` with production-equivalent bootstrap/relay;
4. Pages-hosted replay/demo mode independent of live networking.

Compare candidate Rust UI stacks for G32. Record browser versions, Veilid
version/config, console/network evidence, bundle size, accessibility, and exact
failure mode. Do not modify Veilid upstream.

**Completion criteria:** G26 and G32 are closed. Native/local web is proven. If
direct HTTPS Veilid fails, the plan stops before choosing a relay or companion
deployment beyond the already local native runtime and requests explicit user
direction.

**Completion notes:** Not started.

### [ ] 6.2 Implement deterministic room and game rendering

Add the selected `poche-web` client as a projection renderer. Minimum surfaces:
identity, create/join code, member/seat/spectator list, ready state, countdown
and abort, own hand, public table, legal actions, score/pot, pause/resume, chat,
hand-view request/grant/revoke, reconnect status, errors with policy reasons,
and transcript export/replay.

**Completion criteria:** UI controls produce typed commands only; replaying the
same projection/event stream yields the same DOM-relevant model; inaccessible
or unauthorized controls are not the only enforcement layer.

**Completion notes:** Not started.

### [ ] 6.3 Verify multi-view privacy and interaction behavior

Run browser tests with host, two players, an ungranted spectator, and a granted
then revoked spectator. Inspect rendered text/DOM, client state, logs, and
network payload access according to the threat model.

**Completion criteria:** Each viewer sees exactly its projection; countdown,
pause, chat, and reconnect are usable; revocation changes future spectator view;
no hidden hand is present in unauthorized client state.

**Completion notes:** Not started.

### [ ] 6.4 Publish only deployment modes proven by Task 6.1

Extend existing Pages automation to publish the web replay/demo and, only if
G26 passes, direct live multiplayer. Generated WASM/JS/assets remain untracked.
If live multiplayer requires the local native runtime, document/download that
mode rather than representing Pages as independently live.

**Completion criteria:** README links distinguish rulebook, replay/demo, local
native web, and any proven direct live mode; generated assets do not inflate Git
history; deployment limitations are conspicuous.

**Completion notes:** Not started.

## Phase 7 - Vectorized RL environment and baselines

### [ ] 7.1 Define versioned observation, action, history, and reward specs

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

**Completion notes:** Not started.

### [ ] 7.2 Implement deterministic batched CPU rollouts

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

**Completion notes:** Not started.

### [ ] 7.3 Add random, legal-random, and heuristic baselines

Implement framework-independent policies through a small policy trait. Include
uniform legal random and at least one explainable Poche heuristic. Log raw
per-round points, final cumulative points, score differential, exact bids,
illegal-action count, game length, and seeds. Win/loss may be secondary but must
not replace score as the primary objective.

**Completion criteria:** Baselines are deterministic by seed, never choose
masked actions, produce replayable transcripts on demand, and establish a fixed
evaluation corpus before learning.

**Completion notes:** Not started.

### [ ] 7.4 Apply Puffer-inspired performance work only after measurement

Profile direct typed scalar, naive batch, preallocated batch, and parallel batch
paths. Evaluate structure-of-arrays, fixed buffers, thread partitioning, and
double buffering based on evidence. Do not duplicate game rules in an unsafe
fast environment unless a separately tested/generated representation proves
semantic parity.

**Completion criteria:** A committed benchmark summary identifies environment,
encoding, inference, and synchronization costs; optimized paths retain scalar
parity; no external network traffic occurs.

**Completion notes:** Not started.

### [ ] 7.5 Connect text/web replay to selected RL episodes

Allow evaluation to retain a small chosen episode as the same protocol-style
public/viewer transcript used by CLI/web without putting transcript generation
in every rollout. Record selection criteria so only interesting/failing episodes
pay serialization cost.

**Completion criteria:** A seed from an evaluation summary replays in CLI and
web with identical game/observation/reward hashes; training throughput with
recording disabled is unaffected within the recorded benchmark tolerance.

**Completion notes:** Not started.

## Phase 8 - Burn learner, self-play, and empirical evaluation

### [ ] 8.1 Prove Burn backend and algorithm primitives on controlled scopes

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

**Completion notes:** Not started.

### [ ] 8.2 Implement PPO/GAE over turn-based rollout buffers

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

**Completion notes:** Not started.

### [ ] 8.3 Add self-play with frozen historical policies

Implement seat-randomized policy assignment, current-policy mirrors, and a
bounded pool of immutable historical checkpoints inspired by PufferLib. Record
which policy controls each seat in every evaluation episode. Separate rollout
opponents from evaluation opponents to avoid reporting a moving target as
progress.

**Completion criteria:** Frozen checkpoints never mutate; deterministic matchup
schedules replay; catastrophic forgetting/regression can be detected against a
fixed baseline suite; memory/storage bounds are explicit.

**Completion notes:** Not started.

### [ ] 8.4 Train the first full-rule policy and preserve artifacts responsibly

Run the preregistered `poche-2p-v1` training experiment. Store large weights,
optimizer state, raw logs, and traces outside Git or as named CI/release
artifacts. Commit only configuration, semantic hashes, code revision, seeds,
hardware/backend, summarized curves, and artifact digest/location.

**Completion criteria:** The run is reproducible from the recorded manifest;
checkpoint loading rejects incompatible game/observation/action/reward hashes;
failure to beat a baseline is reported as a result rather than hidden or
reframed as formal evidence.

**Completion notes:** Not started.

### [ ] 8.5 Evaluate by Poche score and render representative behavior

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

**Completion notes:** Not started.

## Phase 9 - Aggregate evidence, documentation, and handoff

### [ ] 9.1 Run workspace, license, protocol, and secret-safety gates

Run formatting, linting, docs, workspace tests, dependency/license audit,
protocol fuzz/property tests, redaction tests, and ignored-artifact checks.
Ensure new first-party files have MPL-2.0 notices where repository policy
requires them.

**Completion criteria:** A clean clone passes; no local sibling dependency or
dirty reference is required; generated models/web/training artifacts are not
tracked; secret scanners/fixtures contain no actual key/invite material.

**Completion notes:** Not started.

### [ ] 9.2 Run formal, loopback, Veilid, and renderer acceptance

Run session coverage/oracles/conformance, loopback scenario, local Veilid
scenario, and the deployment/browser matrix selected by G26. Record native tool
and Veilid versions, exact state/result counts, fixture hashes, and limitations.

**Completion criteria:** All advertised lifecycle/security/rendering behaviors
have evidence; formal results name exact scopes/fairness; external-network
unavailability cannot be misreported as a semantic pass.

**Completion notes:** Not started.

### [ ] 9.3 Run RL correctness, performance, GPU, and evaluation acceptance

Run scalar/batch parity, baseline corpus, allocation/performance benchmark,
Burn CPU/GPU smoke, controlled learning task, full training manifest replay,
and held-out evaluation. Separate deterministic semantic equality from expected
floating-point/training variability.

**Completion criteria:** The environment is fast without network use; selected
GPU tensor execution is proven; training/evaluation artifacts are digest-bound;
results emphasize score and carry empirical confidence labels.

**Completion notes:** Not started.

### [ ] 9.4 Complete guidance audit, documentation, commits, and push

Update README and contributor docs with architecture, threat model, CLI, room
codes, reconnect, chat, spectator grants, local/direct web modes, RL manifests,
proof-versus-training evidence, and exact reproduction commands. Mark tasks
complete only with adjacent evidence. Rerun all three intent-audit passes and:

```powershell
cargo run -p poche-xtask -- guidance audit PLAN.md
cargo run -p poche-xtask -- guidance audit PLAN-2-MULTIPLAYER-RL-RENDERING.md
git status --short --branch
```

Commit coherent slices with meaningful messages, push `model-checking`, verify
the remote branch SHA, and update the plan status to `Execution complete` only
after no required work remains.

**Completion criteria:** Every U30-U46 row has completion evidence; all gates
are decided/deferred/blocked with exact conditions; all overall criteria below
are checked; the local branch is clean and synchronized with the verified
remote commit.

**Completion notes:** Not started.

## Overall completion criteria

- [ ] The predecessor plan remains unchanged as truthful completed history, and
  both plans pass generalized guidance audits.
- [ ] Every U30-U46 requirement maps to completed evidence; the final triple
  intent audit finds no silent omission or weakened qualifier.
- [ ] `SessionState` and `GameEnvironment` remain separate, deterministic,
  strongly typed reducers with an explicitly tested composition boundary.
- [ ] Versioned typed commands/events and canonical NDJSON drive CLI, network,
  renderer, fixtures, and replay; RL uses the same semantics without parsing or
  networking in its hot path.
- [ ] Host/create, code/join, membership, ready, abortable countdown, start,
  pause/resume, post-game, reconnect, leave/close, and bounded chat work in
  loopback and native Veilid scenarios.
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
- [ ] A minimal web client renders viewer-correct rooms and games. README and
  Pages advertise only the live-network deployment modes that Task 6.1 proved.
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
| Veilid browser HTTPS/WSS support is insufficient | A Pages-hosted direct live client cannot connect | G26/Task 6.1; ship local native-backed web first; request user direction before adding relay trust/infrastructure. |
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
