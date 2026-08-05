# Session, network, rendering, and RL architecture

- Status: Accepted; browser transport and final renderer remain evidence gates
- Date: 2026-08-04 (America/Toronto)
- Scope: `poche-phase-2`, G15-G32
- Supersedes: Nothing; `0001` and `0002` remain in force

This decision fixes the semantic and trust boundaries before session, network,
UI, or learning code can make them accidental. G26 and G32 deliberately remain
evidence-selected: their authorized choices and closing tests are specified
below, but no result is claimed before the executable spikes run.

## Semantic composition

`GameEnvironment` remains the sole authority for deals, bids, card play,
settlement, round scores, and game outcomes. `SessionState<Game>` is a separate
pure state machine for room membership, seats, readiness, countdown, pause,
chat metadata, reconnect epochs, and viewer grants. It may contain an optional
game value, but session types do not become variants of `GameState`.

```text
untrusted command bytes
  -> decode and structural limits
  -> signature and replay validation
  -> immutable authorization attempt/decision
  -> pure SessionState reducer
       -> optional typed GameEnvironment transition
  -> authority-ordered events
  -> per-viewer projection
  -> transport / NDJSON / CLI / renderer
```

Reducers do not read clocks, randomness, sockets, files, key stores, UI input,
or neural networks. Runtime adapters turn a wall-clock deadline into a logical
countdown event, obtain explicit chance values, verify signatures, persist the
event log, and deliver projections. RL calls typed game semantics directly and
does not traverse the session protocol unless a test explicitly exercises that
boundary.

The game/session integration contract is intentionally small:

- a game may be created only by a valid countdown expiry in a ready lobby;
- player game commands are allowed only in `Running`, from the current seated
  actor, against the current session and game revisions;
- `Paused` rejects player game actions and deterministic/chance advancement;
- round-score and terminal outputs become ordered session events without being
  reinterpreted by the session reducer;
- the game may finish while the room remains available for results, chat, or a
  later game.

## Authority and consistency model

The first networked implementation is host-authoritative. Participants sign
commands; the host verifies and orders them, executes the session/game reducers,
and signs the resulting events and snapshots. There is no consensus, host
migration, or trustless deal in this phase.

The host is trusted to execute the published code and is necessarily able to
inspect every hand because it owns complete game state. Signatures and the event
log make commands attributable and inconsistencies inspectable; they do not
prevent a malicious host from censoring commands, equivocating to disconnected
clients, choosing unfair chance values, or reading hidden cards. Trustless
dealing, consensus, and cryptographic proof of host behavior remain deferred.

Events have one monotonically increasing authority revision. A command binds
the protocol version, room ID, session epoch, command ID, principal public key,
expected revision, payload, and signature-domain version. Duplicate command IDs
are idempotent. Wrong-room, stale-epoch, stale-revision, invalid-signature, and
revoked-principal attempts fail before mutation. Diagnostic JSON is never signed
directly: Task 2.1 defines versioned length-framed canonical bytes with stable
payload tags and test vectors, then NDJSON and optional Phon codecs must preserve
the same typed meaning and semantic hash.

## Identity, rooms, and authorization

An application player identity is a stable Ed25519 signing key. It is separate
from a Veilid node ID, DHT owner key, and private route, all of which may change
after restart or route rotation. `PrincipalId` is derived from the application
public key; membership and grants bind that principal, not a transport peer.
The crypto implementation is behind a protocol verifier/signer trait so pure
reducers and exhaustive models do not depend on Veilid.

The native first implementation stores the application secret through an
`IdentityStore` backed by Veilid protected/table storage. An in-memory or
plaintext development store is allowed only behind an explicit insecure flag
and conspicuous diagnostic. Browser persistence is part of G26's spike: a
browser-only topology must prove non-exporting storage behavior or state its
limitations. Logs, snapshots, transcripts, DHT values, room codes, and error
messages never contain the secret key.

A room code is a versioned, checksummed rendezvous locator plus an expiring or
one-time invite secret/verifier. Redeeming it binds the stable application
public key to durable membership. The original invite is not a permanent bearer
credential and is unnecessary for a normal reconnect. Revocation increments the
applicable membership/capability epoch so replaying old membership, route, or
invite material fails closed.

Authorization uses TPBAC-shaped immutable records:

```text
Attempt { principal, room, epoch, revision, action, resource, context }
Decision { allow_or_deny, policy_id, reason, audit_only }
```

Unknown principals, roles, commands, capabilities, and protocol versions are
denied. Explicit deny overrides allow. Audit-only policies record the decision
they would make but cannot turn a denial into an allow. Transport authentication
is evidence about a connection, not application authorization.

## Lobby, countdown, pause, chat, and visibility

- The host may arm a countdown only with the minimum seats occupied and every
  seated player ready. Any seated player may unready or abort it.
- The authority runtime orders player commands ahead of a timer event observed
  at the same logical deadline. Abort/unready therefore wins the expiry race;
  stale expiry cannot start a game. Start is emitted at most once.
- Any active seated player may pause a running game. While paused, any active
  seated player may unpause it. There is no vote, acknowledgement quorum, or
  host override. Authority ordering resolves concurrent commands; duplicate IDs
  are idempotent and the losing expected revision becomes stale.
- This deliberately permits pause/unpause griefing. Rate limits may control
  command floods but cannot change the user-selected semantic rule.
- Chat is an authorized, attributed, bounded, ephemeral side stream. Text
  content is not game state and formal models retain only permission/count
  abstractions. Parsing cannot let chat inject protocol frames.
- Spectator hand access is recipient-, player-, capability-, and epoch-scoped.
  A player may grant or revoke a spectator's future projection. Hidden hands are
  never room-broadcast and filtered only in the UI. Revocation stops later
  delivery but cannot erase information already observed.

Unconditional game termination remains the existing game-only claim under its
named scope. Session termination is conditional on eventual countdown ticks,
delivery, player action, and eventual unpause/reconnect. A paused or partitioned
session may legitimately persist forever when those fairness assumptions are
removed.

## Threat boundaries

| Principal or component | Trusted for | Not trusted for / may learn |
| --- | --- | --- |
| Host application | Event order, reducer execution, full state, chance input, signed snapshots | Can inspect all hands; can censor, equivocate, choose unfair chance, or stop. This phase does not prevent those behaviors. |
| Player application/key | Authoring that player's signed commands and protecting that player's secret | Input is untrusted; receives only public state, its own hand, and grants addressed to it. |
| Spectator | Authoring its own requests/chat | Receives public state only unless a current exact grant exists; prior disclosure cannot be revoked retroactively. |
| DHT cache/record peer | Storing public rendezvous data and encrypted/opaque values | Untrusted for authorization and order; may observe record keys, timing, size, and network metadata. It receives no room-wide private hand. |
| Network peer/relay | Delivering bytes opportunistically | May drop, duplicate, reorder, delay, and observe routing metadata. Application signatures, epochs, revisions, and recipient encryption are still required. |
| GitHub Pages/static web host | Serving immutable client assets and rulebook/replay artifacts | Sees ordinary HTTP metadata and could serve modified future assets; it receives no live room state in a direct Veilid mode. Published digests and HTTPS reduce, but do not eliminate, supply-chain trust. |
| Datastar server/operator, if selected | HTTP/SSE delivery and possibly the host-authoritative runtime | Less anonymous: observes client IP/timing and, if authoritative, all game state. Prefer self-hosting by the room host so it does not add a second hidden-state principal. A third-party relay must not receive plaintext private projections without a new threat decision. |
| Renderer | Presenting an already authorized viewer projection and emitting typed commands | Never authorizes, computes legal semantics independently, or retains another viewer's private projection. |
| RL learner/policy | Selecting among a supplied legal mask and learning from versioned rewards | Never owns game rules, chance, hidden opponent state, session authorization, or network behavior. |

## Browser, server, and renderer evidence gate

G26 asks whether a public browser can run Veilid directly with no native Poche
or Veilid companion on the client device. G32 prefers egui but does not force a
renderer before transport evidence. Task 6.1 must execute this registered matrix:

| Spike | Required evidence | Selection consequence |
| --- | --- | --- |
| Native egui/eframe client | Build/run, projection fixture parity, accessibility tree, binary size, interaction latency | Establishes the portable native baseline. |
| Static egui/WASM replay | Pages-equivalent HTTPS build, fixture replay, browser matrix, bundle size | Must remain available even when live networking is not. |
| Browser-only Veilid over local HTTP/WS | No native process on the client; create/join/message evidence and exact configuration | Separates basic WASM viability from production TLS/relay constraints. |
| Browser-only Veilid over production-equivalent HTTPS/WSS | No native process on the client; bootstrap, rendezvous, reconnect, private projection, browser console/network capture | Passing permits direct live Pages multiplayer. Failure forbids advertising it. |
| Hostable Datastar/Axum server | Reproducible server binary, ordinary browser, SSE reconnect, typed protocol parity, operator metadata/state inventory | Candidate browser-portable but less-anonymous topology when direct Veilid fails. |
| Bounded native Vulkan comparison | Use cursor-latency/ash as evidence for latency/control, driver/distribution burden, code size, and web loss | May justify a later native renderer, but is not permission to rewrite the first UI in raw Vulkan. |

The decision rule is:

1. Prefer `egui`/`eframe` shared native and web projection code.
2. If production browser-only Veilid passes, select direct live browser Veilid.
3. If it fails, compare a native Veilid client against a self-hostable Datastar
   server using privacy, portability, operations, accessibility, and measured
   latency. Do not silently install or require an on-device companion.
4. External hosted-service deployment remains a separate user-authorized act;
   building and documenting a self-hostable binary is in scope.

## RL preregistration

The first immutable environment contract is `poche-2p-v1`: full 52-card,
two-player Poche, seat-relative observations, an explicit public-history
strategy, a fixed bid/card action vocabulary, and a legal-action mask. Task 7.1
must freeze exact dimensions, normalization, categorical encodings, and semantic
hashes before training. Own-hand cards are encoded; an opponent hand is never
present. Public phase/turn/dealer/trump/bids/table/history, remaining counts,
scores, and pot are encoded only to the extent the human viewer is entitled to
them.

The first reward contract is `round-score-v1`:

- zero on ordinary transitions;
- at a round score boundary, each seat receives exactly its raw rulebook points
  from `RoundScoreEvent`;
- no clipping, score differential, money, terminal win bonus, or undocumented
  shaping;
- money, outcome, win/tie, and score differential remain separate evaluation
  metrics.

Baselines precede learning: uniform random (expected to attempt illegal actions
and therefore used only as a negative control), legal-uniform random, and a
deterministic rule heuristic. The learned experiment is an in-house Rust
actor-critic PPO/GAE implementation over Burn with legal-logit masking, frozen
checkpoint self-play, `gamma = 1.0`, initial `lambda = 0.95`, clip `0.2`, value
coefficient `0.5`, entropy coefficient `0.01`, and gradient norm cap `0.5`.
Hyperparameter changes create a new run manifest rather than rewriting this
baseline. Before full Poche, the same implementation must solve a deterministic
micro environment with an exhaustive optimum.

Evaluation uses fixed disclosed seed sets and reports, per seat and opponent:
mean/median Poche score, score differential, round points, win/tie rate, pot
separately, illegal masked selections, episode length, inference/rollout
throughput, allocation counts, and confidence intervals across seeds. Selected
episodes must replay through typed semantics and the text/client renderer.
Training evidence is empirical and never upgrades a bounded formal claim.

## Exact dependency candidates and provenance policy

Every introduced dependency is an exact crates.io version in workspace
dependencies and `Cargo.lock`; sibling paths and dirty worktrees are forbidden.
The feature using a dependency may stay optional until its phase begins.

| Purpose | Exact candidate | License / disposition |
| --- | --- | --- |
| Existing reflection/binary/IR | `facet = 0.50.0-rc.5`, `phon = 0.2.0-rc.5`, `weavy = 0.2.2` | MIT OR Apache-2.0; already pinned by ADR 0001. |
| Facet JSON and CLI | `facet-json = 0.50.0-rc.5`, `figue = 5.0.0-rc.5` | MIT OR Apache-2.0. Figue 5 RC is selected because it exactly targets Facet 0.50; stable Figue 4.0.5 targets Facet 0.46. |
| Canonical hashes and JSON interop | `blake3 = 1.8.5`, `serde = 1.0.229`, `serde_json = 1.0.151` | CC0/Apache-2.0 and MIT OR Apache-2.0 families; already resolved in the lock and audited again when made direct. |
| Application signatures | `ed25519-dalek = 3.0.0` | BSD-3-Clause; strict verification, no `legacy_compatibility`; signer/verifier kept outside reducers. |
| Async runtime | `tokio = 1.53.1` | MIT; enable only the features required by runtime/network crates. |
| Veilid transport | `veilid-core = 0.5.7` | MPL-2.0; optional `poche-veilid` dependency with native/wasm features selected per target. |
| Client UI | `egui = 0.35.0`, `eframe = 0.35.0` | MIT OR Apache-2.0; optional until G32 closes. |
| Hostable web fallback | `datastar = 0.3.1`, `axum = 0.8.9` | MIT; optional and selected only by G26/G32 evidence. The local Datastar checkout is newer than its latest published release and is reference-only. |
| Learning/tensors | `burn = 0.21.0` | MIT OR Apache-2.0; optional features start with `std`, `autodiff`, `flex`, `wgpu`, and `store`; Poche owns PPO/rollout semantics. |

All Poche-authored source remains MPL-2.0. BSD-3-Clause, MIT, Apache-2.0, and
MPL-2.0 dependencies are compatible with distribution of this MPL-2.0
application when their notices and file-level terms are preserved. Task 9.1
must audit the resolved graph, not merely this intended direct list.

## Veilid contribution constraint

`D:\Repos\rust\veilid\AGENTS.md` was read before this decision. It forbids AI
agents from writing contributor code or generating upstream merge requests.
Poche will consume the released public API and may read local source/docs for
architecture. No Veilid source, issue, merge request, or upstream branch will be
modified by this work.

## Sources checked

- [Veilid 0.5.7 API](https://docs.rs/veilid-core/0.5.7/veilid_core/)
- [Veilid developer book: applications](https://veilid.gitlab.io/developer-book/apps/index.html)
- [Veilid developer book: DHT](https://veilid.gitlab.io/developer-book/concepts/dht.html)
- [Veilid AppCall/AppMessage](https://veilid.gitlab.io/developer-book/apps/api/appmessaging.html)
- [egui 0.35.0](https://docs.rs/crate/egui/0.35.0)
- [eframe 0.35.0](https://docs.rs/crate/eframe/0.35.0)
- [Datastar Rust 0.3.1](https://docs.rs/datastar/0.3.1/datastar/)
- [Burn 0.21.0](https://docs.rs/crate/burn/0.21.0)
- [ed25519-dalek 3.0.0](https://docs.rs/crate/ed25519-dalek/3.0.0)
- [Figue 5.0.0-rc.5 package metadata](https://crates.io/crates/figue/5.0.0-rc.5)
- Local reference revisions and dirty-worktree warnings recorded in
  `PLAN-2-MULTIPLAYER-RL-RENDERING.md`

## Gate dispositions

| Gate | Disposition after this ADR |
| --- | --- |
| G15-G17 | Confirmed: separate reducers, typed canonical NDJSON, and direct in-process RL. |
| G18 | Confirmed: signed host-authoritative event log; host sees all hidden state; no migration. |
| G19 | Confirmed: locator plus expiring/one-time invite, then key-bound membership. |
| G20 | Confirmed: stable application key separate from Veilid node/routes. |
| G21-G25 | Confirmed, including exact any-player pause/unpause and future-only spectator revocation. |
| G26 | Open until Task 6.1 executes the no-companion HTTPS/WSS browser matrix. Authorized fallbacks are native Veilid or self-hostable Datastar, not an implicit companion. |
| G27 | Confirmed as `poche-2p-v1`; exact tensor manifest freezes in Task 7.1 before training. |
| G28 | Confirmed as `round-score-v1`. |
| G29 | Confirmed as Burn-backed PPO/GAE with the preregistered controlled experiment. |
| G30-G31 | Confirmed: conditional session liveness and decomposed bounded models. |
| G32 | Provisional egui/eframe default; Task 6.1 evidence closes renderer/topology selection. |

