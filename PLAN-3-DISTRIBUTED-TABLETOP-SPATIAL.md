# Poche phase 3: spatial tabletop refinement and distributed agency

**Plan status:** Active
**Primary implementation root:** `D:\Repos\Games\poche-3` on `model-checking`
**Last updated:** 2026-08-05 (America/Toronto)
**Intent audit:** Passed 2026-08-05 against the complete post-phase-two user discussion and the completed phase-one/phase-two plans
**Current implementation focus:** 5.3, formal and cross-implementation consensus evidence

## How to update this plan

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

Put status in each work-item heading and update the heading and its completion
notes together. A phase is complete only when every work item is `[x]`. Keep at
most one implementation item `[~]` unless this file names intentionally
independent tracks and owners. Record decisions, exact commands, relevant
results, source revisions, and commit IDs under the task they affect rather
than in a detached chronological log.

Do not weaken a claim to make a test pass. Record bounded, symbolic, exhaustive,
queried, sampled, empirical, cryptographic-assumption, and runtime evidence as
different kinds of evidence. A fresh agent must read this plan, repository
instructions, the referenced ADRs, and the affected source before changing a
task.

## Intent audit evidence

- **Pass 1 — extraction:** Reread the available original user messages from the
  browser/Veilid gateway discussion through the latest `/play_card`, Slug,
  formal-spatial, and ECS questions. Split browser facilitation, device agency,
  consensus, dropout, voting, retrospective cheating, command permissions,
  tabletop looseness, typed-state strength, geometry, Slug, Bevy, formal
  methods, and vertical-slice expectations into P3-U1 through P3-U38. Rechecked
  `PLAN.md` and `PLAN-2-MULTIPLAYER-RL-RENDERING.md` so established choices were
  preserved rather than silently reopened.
- **Pass 2 — traceability:** Mapped every active P3-U ID to one or more gates,
  tasks, support-matrix rows, completion criteria, or explicit non-goals. The
  inverse audit tied every material plan choice to current source evidence, a
  completed ADR, explicit user direction, or a named reversible assumption.
  The initial missing distinction between a Poche-owned browser gateway and an
  upstream general Veilid browser change was repaired in G9 and tasks 7.1-7.3.
- **Pass 3 — adversarial omission:** Reread the user messages in reverse order
  and specifically checked qualifiers that are easy to lose: typed state is the
  main representation; the 3D model needs no physics; browser HTML should act
  like HTML rather than imitate a native renderer; an accused cheater's vote is
  visible but can be non-counting; disconnected or kicked players cannot hold
  progress hostage; client-local automation may propose but not secretly
  execute shared changes; arbitrary score changes are governable while the
  finite card universe remains hard; and an upstream Veilid improvement is an
  option to research, not authority to modify that repository. No unrepresented
  active intent remained after the repair, so all three passes were rerun.
- **Known source limitation:** None for user intent. The linked YouTube and Math
  Stack Exchange pages were not transcribed; the links are retained as user
  inspiration, while security decisions require primary cryptographic sources
  and executable evidence. Local reference repositories with dirty worktrees
  are read-only evidence and never dependency sources.

## Purpose

Evolve the checked Poche model into a player-visible tabletop system without
discarding its strongest property: one explicit typed definition of game state
and transition semantics. A semantic command such as `/play-card jack-spades`
must resolve to a typed card action, produce the same checked transition as RL,
formal fixtures, and NDJSON replay, and deterministically realize that successor
as objects, zones, surfaces, and Slug text in a spatial scene. Native rendering
may faithfully display that scene in 3D while browsers project the same
authorized information into accessible HTML zones and ordinary drag controls.

The later half of this phase replaces the assumption of one trusted host with a
separately versioned experimental mode: player identities own sets of device
keys; devices exchange signed proposals and deterministic events; governance
can pause, accuse, vote, recover, kick, or amend scores without a disconnected
or malicious participant deadlocking the room; and a researched mental-poker
protocol attempts fair hidden-card dealing without a trusted dealer. The
existing host-authoritative mode remains honest, supported, and useful while
the stronger mode earns evidence. No cryptographic or decentralization claim is
inferred merely from using Veilid.

## Authoritative user guidance ledger

| ID | Active guidance | Required plan consequence | Superseded by |
| --- | --- | --- | --- |
| P3-U1 | A browser may use an Axum/Datastar gateway that performs Veilid transport on its behalf; the gateway should facilitate a peer experience rather than automatically become the semantic owner. | G9; 7.1-7.3 distinguish transport delegation, key custody, ordering, and semantic authority. | — |
| P3-U2 | A player has a set of device keys. Opening browser and desktop simultaneously adds agency instead of moving one singleton player authority between devices. | G6; 5.1-5.3 model player/device membership, concurrent devices, revocation, and recovery. | — |
| P3-U3 | Truly decentralized shared state is preferred when mechanisms can make it real; conceding a hybrid mode is acceptable because pure anonymity is not the only goal. | G5-G9; host mode remains fallback while replicated mode is evidence-gated and explicitly qualified. | — |
| P3-U4 | Datastar/SSE is preferred for simple server-to-browser updates; WSS/WebTransport should be selected only for concrete bidirectional or transport evidence, not fashion. | G9; 7.2 uses HTTP commands plus SSE first and measures fallback/reconnect/compression behavior. | — |
| P3-U5 | A room code may contain gateway or direct-dial information, but this is distinct from Veilid public bootstrap and should not be overclaimed. | 7.1 versions route hints separately from invite authority and documents bootstrap versus rendezvous. | — |
| P3-U6 | Hidden-card dealing resembles the cited no-trusted-party Secret Santa problem: exclusive choices from a pool, other choices hidden, and no trusted allocator. | G7; 6.1 retains the links, researches primary mental-poker sources, and freezes exact threat assumptions before code. | — |
| P3-U7 | A disconnected, AFK, kicked, or malicious player must not hold the game forever; vote-kick and progress recovery are parallel requirements. | G5, G8; 4.3, 5.2, 6.3 add out-of-turn governance and named dropout behavior. | — |
| P3-U8 | Poche actions may become provably illegal only after later plays or round-end hand disclosure; the history must support retrospective accusations. | G11; 4.2 stores evidence-linked findings against stable action/event IDs and re-audits when knowledge grows. | — |
| P3-U9 | Room policy may prevent illegal actions, allow attempted cheating, automatically flag detectable cheating, or require manual accusation. | G11; 4.1-4.2 separate hard structural invariants, game legality, audit mode, and client proposal preferences. | — |
| P3-U10 | After cheating is established, the state remains recoverable; players may vote to redeal, kick, end, amend scores, or choose another supported remedy. The cheater's vote may remain visible while explicitly not counted. | G8, G11; 4.3 defines proposal/vote/tally/effect records and recovery fixtures. | — |
| P3-U11 | Every client may run its own cheat detector and preferences, for example automatically proposing a rights-removal vote, but shared effects still require accepted authority/consensus. | 4.2-4.3 and 5.3 distinguish local detection/proposal automation from committed shared events. | — |
| P3-U12 | Commands and permissions are first-class. `/score add player1 100` may be wrapped by `/startvote`, while a gamemaster-like grant can permit unilateral execution. TPBAC is a reference. | G10; 4.1, 4.3 introduce typed command ASTs, policy attempts/decisions, vote wrapping, and explanation. | — |
| P3-U13 | Players may agree to arbitrary score adjustments, but governance cannot violate hard universe invariants such as exactly one instance of every card. | 4.1 and 4.3 classify command effects into structural, game, and governable layers. | — |
| P3-U14 | Devices agree on shared history and deterministic state. Any device may propose an automatic next transition; no hidden tick owner should be required. A scorekeeper may receive a role without becoming an unexplained global authority. | G5, G8; 5.1-5.3 define deterministic proposal derivation, duplicate suppression, conflict order, and scoped roles. | — |
| P3-U15 | A broader tabletop action space is appealing: objects with volumes can be repositioned, while Poche rules audit or deny the resulting actions. No physics engine is required. | G1, G11; 2.1-2.4 define semantic scene objects, optional loose locations, fixed poses, and classification without physics. | — |
| P3-U16 | The stronger typed Poche representation is the main one for communication. Geometry is a constrained realization/refinement, not the sole evidence that a card is in a hand. | Confirmed invariant C1; 1.1, 2.1-2.4, and 3.4 require `abstract(realize(s)) == s`. | — |
| P3-U17 | `/play_card jack_spades` means resolve the matching owned semantic card and move its realized object to the play zone. | 2.3 and 4.1 provide the exact parse/resolve/transition/realize contract and CLI acceptance. | — |
| P3-U18 | Player names and scores may appear as physical text on a score sheet using the Slug work in Teamy Studio/Terminal. | G3; 2.2, 8.1-8.2 use semantic text runs attached to surfaces and retain the typed ledger as truth. | — |
| P3-U19 | A card face can render as `A♠`; hidden faces must still obey viewer-specific projection and spectator-grant boundaries. | 2.2, 8.1-8.3 test exact-recipient face-text presence and absence. | — |
| P3-U20 | Use explicit units and concrete layouts for player counts. Regions determining Deck/Hand/Play must be exclusive and mechanically checkable. | G1-G2; 2.1, 2.4 and 3.1 specify fixed integer units, separated snap regions, and layout families. | — |
| P3-U21 | Alloy, NuSMV, Scryer Prolog, and Rust should model the spatial constraints and find counterexamples within honestly stated scopes. | G4; phase 3 builds separate spatial oracles plus neutral agreement evidence. | — |
| P3-U22 | Reassess whether Bevy ECS is weaker than a dedicated engine, using the earlier Bevy Poche implementation as evidence. | G3; 1.2 records the boundary. Bevy may store/render projections but arbitrary components cannot become game truth. | — |
| P3-U23 | `aevyrie/big_space` is a candidate for explicit origins and physical distances. | G12; 1.2 measures need. Table-local fixed units are the reversible default. | — |
| P3-U24 | A desktop application may faithfully render the spatial scene; browsers should do browser-native HTML/2D interactions that project back to the same model. The model stays separate from the view. | 8.1-8.3 and the support matrix require native 3D and semantic HTML parity without identical pixels. | — |
| P3-U25 | Tween actions such as moving a card over five seconds should be reconstructable without making every continuous animation frame a consensus state. | 2.3 and 8.1 store semantic endpoints plus optional deterministic animation metadata; only endpoints affect rules. | — |
| P3-U26 | The plan must name a concrete player-observable vertical slice, not remain architectural speculation. | Phase 2 and overall criteria require CLI plus drag parity, spatial overlay, Slug score/card text, formal counterexample, HTML projection, and replay. | — |
| P3-U27 | Poche remains the actual game being pursued; general tabletop capability should emerge from clean boundaries rather than replacing the project with a generic simulator. | Scope/non-goals and task ordering keep strict Poche acceptance mandatory. | — |
| P3-U28 | First-party work remains MPL-2.0 and verified increments are committed and pushed. | C12 and 9.1-9.3 include license, clean-tree, commit, push, and remote verification gates. | — |
| P3-U29 | Use the resumable-plan skill and literally triple-check that no user intent was omitted. | This ledger, traceability table, and intent-audit evidence are mandatory before execution. | — |
| P3-U30 | A general Veilid browser improvement is worth researching if feasible, but a Poche-specific facilitator is also valid. | G9 and 7.3 investigate/read only. Veilid's `AGENTS.md` forbids AI-authored upstream code. | — |
| P3-U31 | NDJSON stays inspectable, but players should use a normal CLI/GUI rather than type wire frames. Facet and Figue are relevant command-schema tools. | G10; 4.1 evaluates exact pinned Figue compatibility and keeps the typed AST independent of parser choice. | — |
| P3-U32 | Rooms, layouts, and formal scopes must account for multiple players instead of accidentally baking a two-player renderer into the architecture. | G2; phase 2 supports layout cardinalities independently while game/RL specs remain separately versioned. | — |
| P3-U33 | Existing chat, any-player pause/unpause, and spectator request/grant/revoke behavior must survive the new authority and rendering modes. | C4-C6; phases 5, 7, and 8 include preservation fixtures. | — |
| P3-U34 | The browser need not entrust every player secret to Axum. Device-local keys are preferred; server-managed keys are an explicit degraded mode, not the default. | G6, G9; 5.1 and 7.2 threat-model browser key custody and export/revocation. | — |
| P3-U35 | At round end, score is the useful RL reward; spatial rendering must not become an RL dependency or alter `round-score-v1`. | C7; 2.3 and 8.3 preserve direct reducer rollouts and existing reward hashes. | — |
| P3-U36 | The action log must reconstruct semantic state and spatial endpoints; text and graphical experiences consume the same history. | 2.3, 4.2, 5.3, and 8.3 add replay and semantic-hash parity. | — |
| P3-U37 | Geometry may validate attachment and placement, but card identity, glyph ownership, and score meaning should remain exact even when cards touch or glyph surfaces are infinitely thin. | 2.2 uses explicit attachment IDs and local poses; proximity is only an audit, never identity. | — |
| P3-U38 | The user authorized updating the plan, setting a durable goal, and proceeding immediately. | Plan becomes active after the three-pass audit; implementation begins at 1.1 without another approval gate. | — |

## Guidance traceability

| Guidance | Plan coverage | Evidence when complete |
| --- | --- | --- |
| P3-U1-P3-U5, P3-U30, P3-U34 | G6, G9; 5.1-5.3; 7.1-7.3 | Gateway/direct/hybrid fixtures, key-custody threat matrix, route-code vectors, topology receipts |
| P3-U6-P3-U7 | G7-G8; 4.3; 6.1-6.3 | Threat model, primary-source decision, shuffle/deal vectors, disconnect/dropout recovery |
| P3-U8-P3-U14 | G5, G8, G10-G11; phase 4; phase 5 | Stable accusation evidence, typed votes/commands, policy explanations, consensus convergence corpus |
| P3-U15-P3-U21, P3-U25-P3-U26, P3-U36-P3-U37 | G1-G4; phases 1-3; 8.3 | Refinement laws, formal counterexamples, replay parity, vertical-slice transcript |
| P3-U17-P3-U19, P3-U24 | 2.2-2.3; 8.1-8.3 | `/play-card`/drag parity, Slug text attachment, native/HTML viewer privacy |
| P3-U22-P3-U23 | G3, G12; 1.2; 8.1 | ADR, dependency/license evidence, measured table-scale precision |
| P3-U27-P3-U29, P3-U35, P3-U38 | Scope; C7, C12; 9.1-9.3 | Poche acceptance, unchanged RL hashes, plan audit, commit/push verification |
| P3-U31-P3-U33 | G2, G10; 4.1; phases 5, 8 | CLI schema tests, layout matrix, pause/chat/grant regression matrix |

## Scope

### In scope

- An engine-neutral spatial model with stable object, surface, zone, attachment,
  layout, unit, pose, and classification types.
- A strict realization from authorized typed Poche projections/state into a
  spatial scene, plus an inverse classifier used for refinement checks and
  input resolution.
- Fixed integer canonical units, deterministic layouts, exclusive snap regions,
  and endpoint-based animation records; no continuous physics simulation.
- Typed command parsing/resolution for Poche actions and governable commands,
  with normal CLI/GUI entry points over the existing strict NDJSON protocol.
- Poche legality in prevent, audit/allow-attempt, automatic-proposal, and manual
  accusation modes while structural invariants remain non-bypassable.
- Proposals, votes, counted/non-counted vote evidence, command permissions,
  recovery actions, disconnect handling, and retrospective audit findings.
- A separately versioned experimental replicated authority mode with
  player-owned device sets, deterministic event reduction, conflict resolution,
  snapshot/tail recovery, and convergence evidence.
- Primary-source research and a bounded prototype for fair hidden-card
  shuffle/deal/reveal, including explicit collusion and dropout assumptions.
- A Poche-owned browser gateway with semantic HTML, HTTP command submission,
  SSE updates, and Veilid/native routing where supported, without silently
  equating transport facilitation with semantic authority.
- A native 3D adapter and a browser-native 2D/HTML projection that consume the
  same authorized semantic/spatial inputs; Slug-backed card and score text on
  the native path.
- Alloy, NuSMV, Scryer Prolog, Rust explicit/property checks, cross-track
  evidence, documentation, reproducible acceptance, commit, and push.

### Out of scope unless a later explicit plan revision adds it

- A general-purpose physics engine, rigid-body solver, mesh-level formal proof,
  or agreement on every animation frame.
- Treating rendered pixels, glyph outlines, nearest-surface distance, or raw
  Bevy component combinations as the canonical game state.
- Replacing Poche with a generic Tabletop Simulator clone, arbitrary scripting
  language, marketplace, asset pipeline, or user-generated game format.
- Claiming unconditional Byzantine consensus, Sybil resistance, anonymous
  matchmaking, global availability, or cryptographic security beyond the exact
  registered threat model and quorum assumptions.
- AI-authored changes, issues, or merge requests in `D:\Repos\rust\veilid`.
  Read-only architecture research is permitted by that repository's
  instructions; upstream work requires the user's own contribution process.
- Production public hosting, TLS/domain procurement, a public Veilid relay, or
  third-party gateway operation without separate deployment authorization.
- Replacing `poche-2p-v1`, `round-score-v1`, or completed RL evidence merely to
  accommodate rendering. New player counts require new immutable RL specs.
- Committing generated PDFs, WASM bundles, GPU captures, model weights, or large
  transcripts already governed by the repository artifact policy.

## Established foundation

- `PLAN.md` is execution complete and defines the independent Poche Alloy,
  NuSMV, Scryer Prolog, conventional Rust, strict Rust/Weavy, explicit checking,
  conformance, Pages, MPL-2.0, and evidence boundaries.
- `PLAN-2-MULTIPLAYER-RL-RENDERING.md` is execution complete and deliberately
  defers trustless multiplayer/host migration to a separate research plan.
- `poche-model` owns phase-specific typed state, legal actions, pure transitions,
  score events, and viewer observations. `poche-environment` remains the direct
  RL boundary.
- `poche-protocol` owns versioned Facet-reflected commands/events/projections and
  strict canonical NDJSON. `poche-session` owns the pure default-deny session
  reducer. `poche-runtime` separates transport and clock ports.
- Application identities are distinct from Veilid node IDs/routes; commands and
  host events already have Ed25519 signing boundaries. Viewer projections use
  exact recipient scopes and Veilid VLD0 HPKE in native acceptance.
- Host-authoritative native Veilid, a host-colocated Datastar/Axum browser
  topology, native/static-WASM egui replay, chat, pause/unpause, hand grants,
  reconnect, replay, and RL have executable phase-two evidence.
- Production-equivalent direct browser Veilid failed because the public WSS
  bootstrap/relay topology was unavailable; local HTTP/WS browser Veilid worked
  without a companion. ADR 0004 remains accurate until new evidence replaces it.
- The earlier Bevy Poche stores mutually exclusive card states as separate
  `InHand`, `InDeck`, `Played`, and ownership components, then derives a
  positioning behavior. It is evidence that an unconstrained ECS world admits
  contradictory semantic combinations, not evidence that Bevy rendering is
  unsuitable.
- Teamy Studio contains transformed-plane Slug glyph instances; Teamy Terminal
  contains reusable Slug geometry and a retained Vulkan renderer. Both local
  worktrees are reference-only and not clean dependency sources.

## Confirmed constraints

1. **C1 — typed truth:** Typed Poche/session state and accepted events are
   canonical. Spatial state is a deterministic refinement and input surface.
2. **C2 — exact identity:** Stable semantic IDs and attachment relations—not
   proximity, pixels, font contours, or renderer entity IDs—identify cards,
   glyph owners, players, zones, and score fields.
3. **C3 — structural invariants are not governable:** Governance may change
   scores, room roles, recovery policy, or legal action disposition; it cannot
   create duplicate/missing cards, forge identities, or bypass codec bounds.
4. **C4 — pause:** Any active seated player may pause; any active seated player
   may unpause. New authority modes preserve this exact rule.
5. **C5 — visibility:** Viewer projections remain the only renderer input.
   Native geometry and HTML never receive unauthorized card faces.
6. **C6 — chat/grants:** Existing attributed bounded chat and future-only
   spectator grant/revoke semantics remain supported.
7. **C7 — RL separation:** RL continues to call typed game semantics directly;
   spatial layout, networking, Slug, Bevy, and browser code are not rollout
   dependencies. `round-score-v1` remains unchanged.
8. **C8 — deterministic endpoints:** Consensus/replay records semantic actions,
   committed endpoint poses, and optional animation descriptors. Wall-clock
   samples and rendered frames are local presentation.
9. **C9 — honest evidence:** Alloy is bounded, NuSMV is finite/symbolic, Prolog
   is queried/constraint-based, Rust checks the stated finite/property scopes,
   and cryptographic prototypes are qualified by their assumptions.
10. **C10 — additive authority:** Host-authoritative mode remains supported
    until the replicated mode independently passes its matrix. Protocol/schema
    versioning prevents accidental interchange of their claims.
11. **C11 — read-only references:** Dirty sibling repositories inform design but
    are neither modified nor used as absolute path dependencies.
12. **C12 — repository policy:** Poche-authored files use MPL-2.0 headers where
    applicable; verified increments are committed and pushed to
    `origin/model-checking`; generated artifacts remain out of Git.

## Design gates

| Gate | Question and required decision | Downstream acceptance consequence | State |
| --- | --- | --- | --- |
| G1 | What exact scene, pose, unit, zone, attachment, loose-object, snap/dead-band, realization, and abstraction types form `poche-spatial-v1`? | 1.1 must freeze a versioned contract and laws before scene code. | Closed by ADR 0005: typed state is canonical; fixed-mm viewer-scene refinement |
| G2 | Which player counts get concrete spatial layouts now, independently of current two-player game/RL specs? | Layout matrix and formal scopes name every supported/unsupported count. | Closed by ADR 0005: spatial layouts 2-8; game/RL/formal scopes remain independent |
| G3 | Does native 3D use Bevy, which exact published version/features, and how is Slug reused without a dirty path dependency? | ADR, license audit, compile probe, and renderer boundary precede 8.1. | Closed by ADR 0005: Bevy 0.19 `3d` leaf adapter; provenance-pinned MPL-2.0 `poche-slug` extraction |
| G4 | Which geometry is encoded directly in Alloy/NuSMV/Prolog versus precomputed by Rust? | Formal claims state finite grid/zone abstraction and never imply mesh/real proof. | Closed by tasks 3.1-3.4: finite cells, endpoints, and ground relations only; continuous renderer excluded |
| G5 | What replicated-log algorithm, membership epoch, fork rule, quorum, leaderlessness/temporary coordinator, and liveness assumptions define experimental consensus? | Model/check convergence, safety, partitions, stale devices, and recovery before network advertising. | Closed by ADR 0007: accountable crash-fault strict-player-majority prevote/precommit log; deterministic rotating proposer; joint epochs; fork halt/evidence; partial-synchrony liveness |
| G6 | How do player roots authorize multiple device keys, and how do add/revoke/loss/browser export work? | Exact signing vectors, projection scope, simultaneous-device and revocation tests. | Closed by ADR 0007: existing principal bytes are root identity; root-signed per-device keys; one vote/player; local default; gateway custody disclosed and export-and-rotate |
| G7 | Which published mental-poker construction and threat assumptions cover fair shuffle/deal/reveal, collusion, active cheating, and dropout? | No implementation or fairness claim before primary-source review and vectors. | Open in 6.1 |
| G8 | What proposal/vote quorum, eligibility snapshot, timeout, tie, accused-member tally, and kick/redeal/end semantics apply? | Rust/formal fixtures cover out-of-turn recovery and visible non-counting votes. | Closed by task 4.3: strict majority of a proposal-time active eligibility snapshot; visible excluded votes; logical deadline rejection; exact capability alternative |
| G9 | What exactly does a gateway know/do; which keys stay in-browser; and when are HTTP+SSE, WSS, WebTransport, or direct Veilid used? | Threat/topology matrix, reconnect evidence, and no false anonymity/directness claim. | Open in 7.1 |
| G10 | What versioned command AST backs CLI, GUI, votes, and protocol; is pinned Figue compatible with the workspace Facet version? | Parser/help/completion/codec tests prove one typed meaning; raw strings are never authorized directly. | Closed by ADR 0006: `poche-governance-command-v1`; first-party parser/catalog because exact Figue pair fails offline resolution |
| G11 | How do strict prevention, allow-attempt, auto-propose, manual accusation, retrospective findings, and recovery compose? | Policy matrix and history corpus demonstrate each mode without weakening C3. | Closed by ADR 0006: structural, authorization, legality, finding-publication, and governable-effect layers |
| G12 | Does table-scale precision or multi-table scope justify `big_space`? | Measured precision/complexity ADR; default is table-local fixed units and render-time `f32`. | Closed by ADR 0005: no `big_space`; table-local integer mm plus render-time `f32` |

Each remaining gate is closed in the named task before dependent
implementation. If research cannot support a safe cryptographic or consensus
choice, the task records an honest prototype/non-goal rather than inventing a
protocol.

## Source and implementation references

### Poche source

- `crates/poche-model/src/state.rs` — phase-specific game state, viewer
  observation, and legal action enumeration.
- `crates/poche-model/src/semantics.rs` — pure transitions, score events,
  provenance, and semantic diffs.
- `crates/poche-protocol/src/types.rs` and `codec.rs` — command/projection schema,
  signatures, validation, and canonical NDJSON.
- `crates/poche-session/src/state.rs`, `machine.rs`, and `projection.rs` — pure
  room reducer, TPBAC-shaped policy, pause/chat/grants, and exact viewers.
- `crates/poche-ui/src/presentation.rs`, `semantic_html.rs`, and `live.rs` —
  renderer-neutral and browser presentation boundaries.
- `models/alloy`, `models/nusmv`, `models/prolog`, `crates/poche-check`, and
  `crates/poche-conformance` — existing formal/checking patterns to extend.
- `docs/decisions/0003-session-network-rl-architecture.md` and
  `0004-live-client-topology-and-rendering.md` — preserved phase-two boundaries.

### Read-only local references and observed revisions

| Reference | Observed evidence / disposition |
| --- | --- |
| `D:\Repos\Games\poche-2` at `0a19283bf949` | Bevy 0.13 implementation with component-derived card positions; clean; architecture evidence only. |
| `G:\Programming\Repos\Teamy-Studio` at `c827d78c4b60` | Transformed 3D text plane/glyph instances; dirty/ahead; reference only. |
| `G:\Programming\Repos\teamy-terminal` at `8aede3a196d4` | Slug font geometry and retained GPU renderer; ahead; reference only. |
| `G:\Programming\Repos\TPBAC` at `f7c55c1788df` | Deny-by-default, explicit deny, audit/enforce, priority, impersonation vocabulary; clean; conceptual reference. |
| `G:\Programming\Repos\facet\figue` at `adac882811c4` | Facet-reflected subcommands/help/completions; dirty/diverged; published exact dependency only if G10 passes. |
| `G:\Programming\Repos\teamy-rust-cli` at `7e62d72bbbf3` | CLI organization/reference; dirty; do not copy unrelated template state. |
| `D:\Repos\rust\veilid` at `76b2176926dc` | Browser/native architecture source; clean; `AGENTS.md` forbids AI-authored upstream changes. |
| `aevyrie/big_space` | Official project uses nested integer grids while retaining Bevy `Transform`; candidate only after G12 evidence. |

### Cryptographic research starting set

- User inspiration: `https://www.youtube.com/watch?v=wqOb5n3BIn0` and
  `https://math.stackexchange.com/a/2896914`.
- Philippe Golle, *Dealing Cards in Poker Games*:
  `https://crypto.stanford.edu/~pgolle/papers/poker.pdf`.
- Wei and Wang, *A Fast Mental Poker Protocol*:
  `https://eprint.iacr.org/2009/439`.
- Dropout-tolerant protocols and modern libraries must be evaluated from primary
  papers/implementations in 6.1; a title or abstract is not implementation
  authority.

### Baseline validation commands

```pwsh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p poche-xtask --offline -- coverage audit --all
cargo run -p poche-xtask --offline -- compare all --scope micro
cargo run -p poche-xtask --offline -- session coverage audit --all
cargo run -p poche-xtask --offline -- session compare all --scope lobby-micro
```

## Support and acceptance matrix

| Target/mode | Intended phase-three status | Required validation | Evidence |
| --- | --- | --- | --- |
| Engine-neutral spatial core | Supported on workspace targets | Unit/property/refinement tests, no renderer dependency | Pending 2.1-2.4 |
| Existing host-authoritative native/loopback | Supported unchanged | Full phase-two regression and semantic hashes | Pending 9.1 |
| Experimental replicated authority | Explicitly experimental | Deterministic corpus, formal model, partition/dropout/convergence tests | Pending phase 5 |
| Native 3D tabletop | Windows first; portable Bevy support named by G3 | Real window, input/CLI parity, privacy, snapshot, timing | Pending 8.1 |
| Semantic HTML live browser | Supported through self-hosted gateway | Ordinary DOM/accessibility, HTTP+SSE reconnect, exact viewer state | Pending 7.2 and 8.2 |
| Static Pages replay | Supported and preserved | WASM/site build plus checked replay; no live authority claim | Pending 8.3/9.2 |
| Direct browser Veilid HTTPS/WSS | Unsupported until new public topology evidence | Re-run ADR-0004 production-equivalent matrix before changing status | Deferred behind 7.3 |
| Poche-owned hybrid gateway | Experimental then supported if matrix passes | Key custody, route hints, reconnect, simultaneous native/browser device, threat disclosure | Pending phase 7 |
| Trustless hidden-card protocol | Research/prototype until G7 security and dropout matrix passes | Published construction mapping, vectors, active-cheat/collusion/dropout evidence | Pending phase 6 |
| Veilid upstream general patch | Not implemented by this AI-driven plan | Read-only feasibility note; user-owned upstream process only | Pending 7.3 |

## Execution order

```text
plan/contract gates
  -> engine-neutral spatial refinement
  -> spatial formal models and conformance
  -> typed command/audit/governance semantics
  -> device identity + replicated history
  -> researched hidden-card protocol
  -> browser gateway/direct transport composition
  -> native 3D + semantic HTML interaction
  -> cross-mode acceptance, docs, commit, push
```

Spatial and governance types precede distributed transport so replicas agree on
typed meaning rather than bytes or transforms. The hidden-card protocol follows
device/consensus contracts because dropout and membership are part of its
security assumptions. Renderers follow the stable projection/refinement
contract and may be developed with fixtures before public network acceptance.

## Phase 1 — Freeze the refinement and renderer boundary

### [x] 1.1 Define `poche-spatial-v1` and scaffold the engine-neutral crate

**Completion notes:** Completed 2026-08-05. ADR 0005 closes G1/G2: typed
Poche/session state is canonical; `poche-spatial-v1` uses table-local integer
millimetres, milli-degree yaw, opaque projection-epoch card handles, explicit
surface/text attachments, typed endpoint locations, strict/audit-loose modes,
and endpoint-only animation records. Layout identities accept 2 through 8
players without widening any game/RL scope. Added dependency-free
`crates/poche-spatial` and a two-player scene containing table, seats, two hand
zones, deck, play zone, score sheet, cards, authorized `A♠`/played-card text,
hidden backs, names, and scores. `cargo tree -p poche-spatial --offline` lists
only the root crate. `cargo test -p poche-spatial --offline` passed 11 tests;
negative controls reject hidden/missing/misattached face text, projection-epoch
mismatch, invalid units/layouts, and prove touching card poses cannot reassign
semantic attachments. `cargo clippy -p poche-spatial --all-targets --offline --
-D warnings` and `cargo fmt --all --check` passed. G3/G12 remain for 1.2.

**Work:**

- Close G1 and G2 in ADR 0005 with exact units, IDs, shape bounds, poses,
  attachment, semantic zones, loose-state policy, snap/dead-band rules,
  supported layout counts, realization/abstraction laws, privacy inputs, and
  animation endpoint semantics.
- Add `crates/poche-spatial` with no Bevy, GPU, window, network, or RL
  dependency. Use Facet only where a stable reflected boundary is justified.
- Define a minimal two-player fixture that realizes deck, two hands, current
  trick, card faces, player names, and score-sheet text while keeping hidden
  faces absent from unauthorized viewer scenes.
- Record separate `GameState`, viewer `Projection`, `SpatialScene`, and
  `SpatialInteraction` responsibilities. Do not make a renderer entity ID a
  network or semantic ID.

**Validation:**

```pwsh
cargo test -p poche-spatial --offline
cargo clippy -p poche-spatial --all-targets --offline -- -D warnings
cargo fmt --all --check
```

**Completion criteria:** ADR 0005 closes G1/G2; the new crate compiles without
renderer/network/RL dependencies; fixture tests name and exercise the exact
round-trip and privacy contracts that later tasks implement.

### [x] 1.2 Decide Bevy/Slug packaging and the `big_space` gate

**Completion notes:** Completed 2026-08-05 after task 1.1 commit `4cf2132` was
pushed and verified. ADR 0005 closes G3 with exact Bevy 0.19.0,
`default-features = false`, feature `3d`, confined to the future
`poche-native-ui` leaf. Slug will be extracted into an MPL-2.0 `poche-slug`
crate from Teamy Terminal revision `8aede3a196d4`, with exact source/WGSL
SHA-256 provenance, explicit font bytes, and no sibling path dependency. Its
terminal-specific Ash lifecycle is not copied. G12 is closed without
`big_space`: the checked-in isolated probe compiled a Bevy component/transform
with Rust 1.96 and checked every integer endpoint in `-10_000..=10_000`; maximum
rounded error was `0 mm`, with a conservative 2 m epsilon of `0.000238419 mm`.
Its lock contains 501 dependencies (502 metadata packages including the probe),
reinforcing the leaf boundary. `cargo-deny` was unavailable and is not claimed;
resolved metadata reports zero packages missing a license field and only
permissive/MPL-compatible expressions. `cargo test -p poche-spatial --offline`
passed 11 tests and both workspace/probe dependency trees resolved offline.
Dirty or ahead sibling repositories remained read-only.

**Work:**

- Close G3 and G12 with a published-version/license/feature comparison, a tiny
  isolated compile probe, and measured table-scale `f32` error after conversion
  from canonical integer units.
- Compare disciplined Bevy projection components against a custom renderer
  shell using the earlier Poche ECS and current `poche-ui` boundaries. Select
  Bevy unless evidence shows it blocks deterministic projection, supported
  targets, Slug integration, or MPL-compatible distribution.
- Decide whether to extract/publish/copy MPL-compatible Slug pieces, implement a
  narrow Poche text backend, or temporarily use a renderer text adapter. Never
  introduce a dirty absolute path dependency.
- Use `TableId + local fixed pose` by default. Add `big_space` only if a named
  multi-table scale produces measurable precision or origin-management value.

**Validation:**

```pwsh
cargo test -p poche-spatial --offline
cargo tree --workspace --offline
cargo deny check licenses
```

If `cargo-deny` is unavailable, record that fact and use the repository's
existing resolved-license audit rather than claiming the command passed.

**Completion criteria:** ADR 0005 records exact renderer/text/origin choices,
their support consequences, and executable probe results. No renderer code is
allowed to become canonical game state.

## Phase 2 — Build the spatial Poche vertical slice

### [x] 2.1 Implement fixed units, objects, zones, layouts, and classification

**Completion notes:** Completed 2026-08-05. `poche-spatial` now has stable
`TableId`-scoped local frames, checked `+-10,000 mm` poses/bounds, exact AABB
operations, deterministic revision-one layouts for every 2-8-player
cardinality, seats/player anchors, table/score-sheet objects, and complete
deck/play/hand/won zone sets. Each zone has a closed inner snap volume and a
strictly separated outer dead band. Classification examines the complete card
bound and returns `Snapped`, `DeadBand`, `Free`, `Ambiguous`, or `OutOfBounds`;
it never selects the first of multiple matches. Tests cover all registered
zone centers, inner/outer boundaries, standard-card containment, deliberately
broad ambiguous input, invalid revisions, duplicate semantic identities, and
rejection of overlapping outer regions. `cargo test -p poche-spatial` passed 20
tests; the focused `units`, `layout`, and `classify` commands, strict Clippy,
formatting, and the complete workspace test suite all passed offline.

**Work:**

- Implement checked integer units and bounded poses, semantic card/player/table/
  hand/seat/deck/trick/score-sheet objects, surfaces, zone ownership, and stable
  layout identities.
- Implement concrete layouts for the G2 matrix with pairwise-separated hand,
  play, and deck snap regions and explicit ambiguous/free classifications.
- Use simple certified primitives such as AABBs/OBBs, planes, and local slots;
  do not formalize arbitrary render meshes.

**Validation:**

```pwsh
cargo test -p poche-spatial units
cargo test -p poche-spatial layout
cargo test -p poche-spatial classify
```

**Completion criteria:** Every supported layout classifies all generated zone
poses uniquely, rejects/marks boundary ambiguity, and demonstrates no forbidden
cross-player or central-zone overlap.

### [x] 2.2 Implement viewer-safe spatial realization and semantic text runs

**Completion notes:** Completed 2026-08-05. Added a dependency-direction-safe
`ViewerSpatialProjection` contract and pure realization that accepts only
public plus exact-recipient authorized data. Every realized active projection
partitions exactly 52 opaque card objects across deck, separate face-up trump,
hands, current play, and won piles; unknown faces are `None` and have no face
text. Known face duplication, bad seat/hand/trick/won cardinalities, and deck
over-allocation fail closed. Card faces use canonical Unicode labels such as
`A♠`; names and canonical decimal scores bind explicitly to score-sheet
surfaces, and `interpret_score_sheet(realize(...))` round-trips the typed
ledger. `poche-ui` now retains authorized/public numeric card codes beside
human labels and maps `PresentationModel` to a spatial scene without importing
session authority. Alice-own-hand, ungranted spectator, and granted-spectator
tests prove that only the exact additional grant faces/text appear. Focused
`realization`, `visibility`, `text_attachment`, and UI adapter tests pass, as do
strict Clippy and formatting with renderer/default UI features disabled.

**Work:**

- Realize `PresentationModel`/authorized projection data into spatial objects
  without importing authority state or opponent secrets.
- Represent `A♠`, names, and scores as semantic text runs explicitly attached
  to a card surface or score-sheet cell. Glyph curves and nearest-surface
  inference are presentation/audit only.
- Keep score truth in the typed ledger and viewer projection; verify that score
  text realization and interpretation agree for supported values.

**Validation:**

```pwsh
cargo test -p poche-spatial realization
cargo test -p poche-spatial visibility
cargo test -p poche-spatial text_attachment
```

**Completion criteria:** Exact viewer fixtures contain precisely the permitted
face/name/score text, and attachments remain unambiguous even with touching or
overlapping card bounds.

### [x] 2.3 Bridge `/play-card`, drag intent, transition, replay, and tween endpoints

**Completion notes:** Completed 2026-08-05. Added canonical exact-lowercase
card names in `poche-domain`; `jack-spades` maps to dense code 48 and variants
fail without echoing user data. The CLI now exposes `game play-card <room>
<rank-suit>` and returns the existing typed `GameActionWire::Play`. Engine-
neutral interaction resolution selects only one visible card in the issuing
seat's typed hand; hidden, missing, other-seat/granted, duplicate, unknown,
wrong-zone, dead-band, ambiguous, free, out-of-bounds, and reducer-illegal
paths have stable findings and never mutate state. Named and drag releases use
the same resolver. A `SpatialPlayRecord` stores only opaque object, typed
source/destination, source endpoint, duration, and easing; replay reconstructs
the exact destination from the registered layout without frame samples. The
runtime adapter checks the reducer-supplied legal action set and emits the
existing canonical protocol payload. Its executable fixture deliberately deals
`jack-spades` to the actor and proves typed and drag payload equality, canonical
NDJSON byte/hash round-trip, equal full oracle successors, and equal semantic
successor hashes over public plus both private hands. Focused spatial/runtime/
CLI tests and strict Clippy pass offline.

**Work:**

- Resolve a rank/suit command only against the issuing viewer's typed owned
  hand, producing the existing canonical `GameActionWire::Play`/model action.
- Translate a drag release through spatial classification into the same typed
  action. Ambiguous/free/unauthorized drops return stable findings without state
  mutation.
- Record semantic source/destination and optional deterministic tween metadata;
  reconstruct endpoints from the action log without storing render frames.
- Prove direct typed, CLI, drag, and canonical NDJSON paths have identical
  semantic successor hashes.

**Validation:**

```pwsh
cargo test -p poche-spatial interaction
cargo test -p poche-runtime spatial
cargo test -p poche-cli play_card
```

**Completion criteria:** `/play-card jack-spades` and dragging the corresponding
owned card to the play zone produce the same accepted transition and realized
successor; illegal, hidden, missing, duplicate, and ambiguous resolution paths
fail with stable evidence.

### [x] 2.4 Prove the Rust refinement laws over supported scopes

**Completion notes:** Completed 2026-08-05. Added a canonical scene
abstraction and enforced canonical seat/revealed-card ordering plus exact
typed-face/text agreement, making `abstract(realize(projection, layout)) ==
projection` executable rather than aspirational. The reproducible scope
`spatial-rust-v1-layouts-2-8-complete-2p-traces-seeds-0-15` checks all seven
registered layout cardinalities and 16 complete two-player oracle games. Decks
are generated by rotating the standard deck by seed plus deal index; each
player deterministically chooses the first advertised legal action. This is
bounded generated evidence, explicitly not an exhaustive claim over all deck
orders or policies. The receipt contains 2,400 semantic transitions, 2,416
reachable states, 4,832 exact-viewer projections, 4,839 total exact round
trips (including seven layout fixtures), 2,416 public-projection privacy
comparisons, 784 ordinary typed-play/spatial-endpoint commutations, 784 atomic
trick resolutions, and 16 finished traces. Re-realization equality checks
endpoint determinism; scene/layout validation checks 52-card conservation,
identity and text-attachment uniqueness, checked bounds, and pairwise zone
separation. Four negative controls retain their first deterministic witnesses:
Deck/Trump sharing the Deck inner AABB; a broad Deck/Trump bound that a faulty
first-match classifier would call Deck; a typed `4♣` card-face text changed to
`A♠`; and concealed card handle epoch 77/ordinal 2 gaining a face without its
authorized text attachment. `cargo test -p poche-spatial --offline` passed 26
tests, `cargo test -p poche-check spatial --offline` passed the reproducible
receipt twice, and strict Clippy passed for both crates.

**Work:**

- Check `abstract(realize(projection, layout)) == projection` for all registered
  fixtures and property-generated reachable states representable by the viewer
  model.
- Check uniqueness, conservation, attachment, zone separation, transition/
  realization commutation, endpoint determinism, and privacy noninterference.
- Add deliberately broken layouts/classifiers to prove the harness detects
  counterexamples rather than passing vacuously.

**Validation:**

```pwsh
cargo test -p poche-spatial --offline
cargo test -p poche-check spatial --offline
```

**Completion criteria:** Receipts state exact counts/seeds/scopes and retain
minimal counterexamples for each negative-control fault.

## Phase 3 — Add independent formal spatial oracles

### [x] 3.1 Model static layout and refinement relations in Alloy

**Completion notes:** Completed 2026-08-05. Added the independently
handwritten `models/alloy/spatial.als` and a fail-closed normalized runner at
`spatial alloy --scope layout-micro`. The exact scope ID is
`layout-micro-2p-3v-8c-7z-7cells-8slots-int5`: two players, one spectator,
two seats, seven typed zones, eight cards/faces/slots, seven discrete
integer-coordinate cells, one typed viewer projection, one spatial scene, and
5-bit Alloy integers (`-16..=15`). Eleven native commands pass with receipt-
derived scopes: one SAT canonical realization, seven UNSAT positive assertions
covering seat/owned-zone injection, zone separation, card location/slot
uniqueness, face/score attachment totality, visibility, and
realization/abstraction, plus three deliberately false assertions with SAT
counterexamples. Those controls retain an overlapping shape-complete layout,
a broad drop implicating multiple otherwise separated zones while selecting
one, and a scene satisfying every modeled condition except a face run attached
to the wrong object. Raw output, normalized polarity/source results, and full
Alloy instances are written under ignored
`target/spatial-alloy-layout-micro/`. `docs/spatial-alloy.md` states that cells
are a finite relational coordinate abstraction and that this is not proof of
continuous geometry, arbitrary meshes, floating-point rendering, physics, all
52-card arrangements, or unbounded player/layout scopes. The prescribed CLI,
focused conformance test, formatting, and strict Clippy all pass offline.

**Work:**

- Close G4 for Alloy: model objects, players, seats, zones, ownership,
  attachment, exclusive classification, and finite coordinate/slot relations.
- Check card/location uniqueness, seat injection, hand-zone separation,
  attachment totality, visibility relations, and realization/abstraction within
  named finite scopes.
- Export normalized positive and deliberately failing counterexample evidence.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- spatial alloy --scope layout-micro
```

**Completion criteria:** Alloy independently passes the registered bounded
claims, finds the seeded overlap/ambiguity defects, and documents integer
bitwidth/scope rather than implying continuous-mesh proof.

### [x] 3.2 Model temporal spatial transitions in NuSMV

**Completion notes:** Completed 2026-08-05. Added the independently
handwritten `models/nusmv/spatial.smv` and normalized command `spatial nusmv
--scope transition-micro`. Its exact scope ID is
`transition-micro-2cards-2viewers-endpoints-transit-pause-recovery`: two stable
card identities, two owner/viewers, typed hand/play/won endpoints, optional
presentation-only transit, and running/paused/recovery/finished phases. All 13
named properties have their required values; `check_fsm` reports the mixed
transition system total and deadlock-free. Four invariants prove safe-mode
owner-zone confinement, private-face knowledge, and transit-to-committed-play
refinement. Six true CTL/LTL properties check one-card movement, paused and
recovery immobility, deadlock freedom, and endpoint convergence. There is no
global `FAIRNESS`: the named `fair_animation` environment mode explicitly
schedules one legal play, one transit state, and presentation service; both
CTL and LTL convergence hold only under that inspectable assumption. Three
false controls retain and structurally validate a three-state stuck-transit
lasso, a two-state cross-owner move, and a two-state private-face leak. The
canonical `committed*` variables cannot contain transit; only `present*` can,
so renderer progress never becomes game authority. Native output, normalized
properties, carried-forward counterexample assignments/loop index, and FSM
diagnostics stay under ignored `target/spatial-nusmv-transition-micro/`.
`docs/spatial-nusmv.md` records that the model abstracts the 52-card universe,
coordinates, frames, clocks, network delivery, and complete round lifecycle.
The prescribed CLI and direct NuSMV run pass offline.

**Work:**

- Model discrete card zones, stable identities, committed transition endpoints,
  pause/recovery abstractions, and optional finite transit state.
- Check one-card movement, no cross-owner mutation, paused immobility, endpoint
  convergence under named fairness, and no unauthorized face reveal.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- spatial nusmv --scope transition-micro
```

**Completion criteria:** Symbolic receipts distinguish unconditional safety from
fairness-conditioned liveness and include negative scheduler/property fixtures.

### [x] 3.3 Add Scryer Prolog spatial queries and explanations

**Completion notes:** Completed 2026-08-05. Added the independently
handwritten finite relational model `models/prolog/spatial.pl` and normalized
command `spatial prolog --scope query-micro`. Its exact scope ID is
`query-micro-2p-3v-8cards-8drops-4layouts-7transitions`. Seven complete ground
fixture queries return 63 sorted duplicate-free answers: 8 card locations, 24
viewer-specific/public semantic text attachments, 5 named/drag command
resolutions, 8 legal/illegal drop explanations, 4 valid/invalid layout
findings, 7 successors, and the exact 7 reverse predecessors. Fifty-five rows
retain stable `because/2` explanations. Typed and drag input produce exactly
two rows with the same `allow(play(c0))` intent/rule; the one broad Deck/Trump
placement returns `deny(ambiguous([deck,trump]))` and never selects the first
zone. Essential answer checks include attachment privacy, overlap, and
pause/recovery reversal. Length-framed per-fixture hashes combine into corpus
digest
`blake3:5dbf46198a011fe7815f1ea92c7378b925d088d9a7a8d3c189d504290dc2aa54`;
raw/normalized evidence stays in ignored
`target/prolog-conformance/spatial-*/`. `docs/spatial-prolog.md` documents that
the productive modes are finite enumeration, unification, positive rules, and
reverse use of `spatial_step/5`. No Rust callback participates, and the model
does not claim arbitrary real/nonlinear constraints, collision/mesh solving,
continuous motion, or completeness outside the named corpus. The prescribed
CLI and direct seven-fixture Scryer run pass offline.

**Work:**

- Add complete bounded queries for card location, attached text, command
  resolution, legal/illegal drop explanations, layout violations, predecessor,
  and successor spatial relations.
- Use Scryer-supported integer/boolean constraint libraries only where verified;
  do not claim arbitrary real nonlinear solving.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- spatial prolog --scope query-micro
```

**Completion criteria:** Query answer sets are normalized with counts/digests
and include explanations for both valid resolution and an ambiguous placement.

### [x] 3.4 Compare spatial claims without treating Rust as oracle

**Completion notes:** Completed 2026-08-05. Added 15 stable `P3-S-*`
obligation IDs and Facet-reflected neutral evidence shapes in
`poche-interchange`; the agreement function rejects mixed comparison scopes,
duplicate backends/claims, missing native qualifications/exclusions, and claim
sets with no shared abstraction. It never assigns backend priority and retains
contradictory observations as successful report data. The conformance command
runs all four native source gates before comparing the common `spatial-micro`
projection. Its receipt records four sources/tracks, nine genuinely shared
claims, 24 participating observations, five single-track claims, and zero
current disagreements. Exact native scope IDs remain attached: Rust contributes
10 sampled claims, Alloy six bounded claims, NuSMV seven symbolic claims, and
Scryer Prolog six queried claims. Prolog's named pause/recovery/one-card facts
are explicitly excluded because they are explanatory rows rather than
structural theorems; no stronger backend silently fills that gap.

Added `docs/spatial-coverage.md` with all 15 obligations across all four tracks.
The machine audit checks every one of 60 cells, accepts only the backend's
native evidence prefix or a reasoned `N/A`, and cross-checks its 29 applicable
cells against the exact emitted claim inventory. The renderer/continuous
obligation is explicitly `N/A` for all four sources. The prescribed commands
pass offline: comparison reports `source_gates=4 tracks=4 shared_claims=9
observations=24 unshared_claims=5 disagreements=0`; coverage reports
`obligations=15 tracks=4 classified_cells=60 applicable_cells=29`. Five focused
spatial conformance tests, formatting, and strict Clippy also pass.

**Work:**

- Extend interchange/conformance evidence with stable spatial claim IDs and
  per-track qualifications.
- Compare only genuinely shared abstractions; retain disagreements and exclude
  renderer-only/continuous claims explicitly.
- Update coverage ledgers and docs with every rule/refinement obligation.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- spatial compare all --scope micro
cargo run -p poche-xtask --offline -- spatial coverage audit --all
```

**Completion criteria:** The report proves all independent sources ran, records
every applicable result and disagreement, and never widens a track's scope.

## Phase 4 — Generalize commands, auditing, and governance

### [x] 4.1 Freeze typed command and legality-policy contracts

**Completion notes:** Completed 2026-08-05. ADR 0006 closes G10/G11 and
freezes `poche-governance-command-v1`. The Facet-reflected AST contains only
typed execute, start-vote, and vote commands over Poche game actions, bounded
score amendments, exact rights changes, stable-event accusations, and named
redeal/kick/end-game recovery. Each action exposes its rule layer and exact
direct authority consequence (`CurrentGameActor`, `ActivePlayer`, or
`VoteOrCapability`). There is no raw execute, recursive vote string, arbitrary
state patch, or create/delete/change-card variant. The isolated canonical JSON
codec is versioned, unknown-field rejecting, limited to 4,096 bytes, and bounds
nonzero score changes to absolute 1,000,000 without treating that abuse bound
as a Poche scoring rule.

Added the normal `command parse <SLASH-COMMAND>` CLI surface and removed the
untyped `game act <string>` placeholder. `/play-card jack-spades`, `/score add
player1 100`, `/rights remove ...`, `/accuse event-2`, recovery, vote, and
`/startvote "/score add player1 100"` all discard source text and yield the same
typed protocol meanings used by GUI/protocol callers. Help and Bash/Zsh/Fish
completion output share the parser's fixed command catalog. The observable
JSON smoke prints `start_vote(adjust_score(player1,+100))` and executes nothing.

The exact Figue probe found API and version compatibility (`figue
5.0.0-rc.5`/Facet `0.50.0-rc.5`) but failed the required locked/offline
dependency gate because `figue-attrs ^5.0.0-rc.5` was absent from the local
registry index. No dirty/absolute sibling path was added; ADR 0006 permits a
later leaf-parser replacement only if the exact published pair resolves without
changing AST bytes or meanings. TPBAC-shaped session evidence now records
integer priority, evaluates higher priority then canonical policy ID, rejects
duplicate policy IDs, keeps audit-only decisions non-authoritative, default
denies unknowns, and lets any explicit enforce-deny override allow regardless
of priority. Full protocol tests pass (11 unit, two decoder-fuzz, three compile-
fail doctests), the prescribed protocol/session/CLI filtered suites pass (2, 2,
and 6 tests), and strict Clippy passes for all three crates.

**Work:**

- Close G10/G11 in ADR 0006. Define versioned typed ASTs for game actions,
  score amendments, rights changes, accusations, proposals, votes, and recovery
  commands. User-facing slash syntax and Figue/manual parsing map into this AST;
  signed raw strings never execute.
- Separate hard structural validation, Poche legality prevention, audit-only
  findings, client-local automation preferences, and governable effects.
- Preserve TPBAC deny-by-default, explicit-deny precedence, audit/enforce, stable
  policy IDs, reason codes, and immutable attempt/decision evidence.

**Validation:**

```pwsh
cargo test -p poche-protocol command
cargo test -p poche-session policy
cargo test -p poche-cli command
```

**Completion criteria:** `/score add player1 100`, `/startvote ...`,
`/play-card ...`, accusation, and rights commands have one stable typed meaning,
help/codec/parser fixtures, and explicit authorization consequences.

### [x] 4.2 Implement retrospective rule findings and accusation evidence

**Completion notes:** Completed 2026-08-05. Added the append-only
`RetrospectiveAudit` evidence subsystem without weakening or replacing strict
`Game`. Every recorded action has a stable event ID, global logical sequence,
round ID, actor, typed bid/play, led suit, exact action-time knowledge, and
either `Accepted` or `AttemptedStructurallyValid` disposition. The latter means
hard card identity/ownership/conservation already passed even though Poche
legality did not commit a game successor. Invalid/duplicate cards, event IDs,
identifiers, suits, and non-increasing history fail closed. Complete round-end
remaining-hand disclosures are separately identified evidence events.

The first declarative retrospective rule is `R-TRICK-005`. It confirms an
off-suit play only by exhibiting a held card of the led suit from immediate
action-time knowledge, a later same-player public play in the same round, or a
complete round-end disclosure. Findings link stable rule/finding IDs, offending
action and disposition, revealing action/disclosure, concrete card, detector,
and `ImmediateHeldCard`, `DelayedPublicPlay`, or `RoundEndDisclosure`
confidence. The finding ID derives from rule plus offending action (not
detector/publication mode), and repeated audits emit nothing twice. The delayed
inference is explicitly qualified by no intra-round card acquisition plus a
structurally verified ownership boundary.

Manual accusations contain only accusation/detector/offending-action IDs; the
caller cannot supply a rule, evidence, confidence, or confirmed bit. The same
engine returns confirmed or stable unknown-action/not-violation/insufficient-
evidence outcomes. Exact duplicate accusations replay immutably, conflicting
ID reuse fails, and an early unfounded accusation cannot be rewritten after
later evidence. `poche-runtime::process_accusation` is a thin delegation seam,
not a second rules engine. `docs/retrospective-audit.md` records the proof and
cryptographic/authority boundaries. The prescribed two session corpus tests
and one runtime test pass, covering immediate, delayed, round-end, accepted,
attempted, unfounded, duplicate, manual, and reconstructed deterministic replay;
full session/runtime regression and strict Clippy also pass.

**Work:**

- Attach every accepted/attempted game action to stable history IDs and the
  knowledge available at that point.
- Re-run declarative audit rules when new public evidence appears, including a
  later card proving an earlier failure to follow suit and round-end disclosure.
- Emit idempotent findings linking rule origin, offending action, revealing
  action/evidence, detector, and confidence. Manual accusations refer to the
  same evidence and cannot forge a confirmed finding.

**Validation:**

```pwsh
cargo test -p poche-session retrospective
cargo test -p poche-runtime accusation
```

**Completion criteria:** A corpus proves immediate, delayed, round-end,
unfounded, duplicate, and manually accused cases with deterministic replay.

### [x] 4.3 Implement proposals, votes, recovery, and arbitrary permitted amendments

**Completion notes:** Completed 2026-08-05. Closed G8 with a pure,
transactional `GovernanceState` whose proposal, eligibility, visible-vote,
tally, status, effect, authority, and idempotent receipt records are typed and
stable. Eligibility is snapshotted at proposal creation: disconnected and
kicked members are excluded; kick targets and confirmed accused subjects of
rights changes are excluded. Their votes may still be appended and displayed,
but carry `counted=false` plus an exact exclusion reason. Approval requires a
strict majority of eligible voters. A rejection majority, all eligible votes
without approval (including tie/abstention), or a logical deadline without
majority produces an immutable rejection. Proposal and effect IDs derive
deterministically from command/proposal identities, exact retries replay, ID
conflicts fail, and every error rolls back without partial state.

Approved proposals or exact unilateral capabilities can adjust scores, grant
or revoke rights, redeal, kick, or end a game. Capabilities are separated into
score, rights, and recovery grants rather than a single omnipotent role. Kick
disconnects the member and removes its grants; redeal advances a deterministic
epoch; none of these paths checks or waits for the current Poche actor. Accused
status can only be derived from an existing confirmed retrospective finding.
The command AST still cannot represent create/delete/change-card operations,
and a controlled JSON attempt is rejected without state mutation.

Added independently handwritten `models/alloy/governance.als`,
`models/nusmv/governance.smv`, and `models/prolog/governance.pl`, plus a neutral
four-source comparison gate and `docs/governance.md`. The registered
`governance-micro` receipt contains four source gates, seven shared claims, 27
observations, zero disagreements, nine Alloy commands, nine NuSMV properties,
and 42 exact Prolog rows. Alloy retains counted-excluded-vote and card-universe
mutation witnesses; NuSMV retains both defect counterexamples and reports a
total/deadlock-free finite transition system; Prolog includes reverse
effect-to-vote explanations. Focused Rust governance tests and strict Clippy
pass. The scopes explicitly exclude unbounded rosters, real transport timing,
network liveness, device certification, and replicated conflict ordering,
which remain phase-5 work.

**Work:**

- Close G8 with versioned proposal/vote/eligibility/tally/effect records,
  deadlines expressed as logical events, tie/abstention rules, and exact
  accused/kicked/disconnected eligibility behavior.
- Support vote-wrapped commands and explicit unilateral capability grants.
  Preserve visible but non-counting votes where policy requires it.
- Implement redeal, kick, end game, score amendment, and rights-change recovery
  without waiting for the current actor. Hard structural invariants remain
  non-amendable.

**Validation:**

```pwsh
cargo test -p poche-session governance
cargo run -p poche-xtask --offline -- session compare all --scope governance-micro
```

**Completion criteria:** Deterministic fixtures cover approval, rejection,
timeout, concurrent proposals, visible excluded vote, AFK actor kick/redeal,
score grant/removal, and attempts to amend the finite card universe.

## Phase 5 — Add player/device identity and replicated event authority

### [x] 5.1 Freeze player/device identity and replicated-log semantics

**Completion notes:** Completed 2026-08-05. ADR 0007 closes G5/G6 and
separates unchanged host-authoritative protocol v1 from experimental
`poche-replicated-v1`. The latter is explicitly an accountable crash-fault
protocol, not an unconditional Byzantine-consensus claim. Any device may
gossip a semantic command proposal, while a deterministic rotating proposer
packages one canonical batch per height/round. Players prevote/precommit and
lock values; voting power is one per active player regardless of device count.
A strict player majority (`floor(N/2)+1`) certifies an event. Membership changes
require old/new joint majority, and logical round changes—not reducer clocks—
drive liveness under eventual delivery, a connected majority, and an
eventually responsive proposer.

Two conflicting certificates for one parent/height are signed fork evidence;
replicas halt at the last common head instead of selecting an arrival-order or
smallest-hash winner after users were told an effect committed. An explicit
new-session fork may preserve player agency but does not rewrite old-room
finality. The ADR states the unavoidable two-player partition boundary: quorum
is two, so one survivor cannot safely distinguish a disconnect from a 1-1
partition. Three players tolerate one crash only under non-equivocation; no
less-than-one-third Byzantine guarantee is advertised. Tendermint/HotStuff and
RFC 8032 are retained as primary design references, not used to launder a
security claim for the smaller implementation.

Added versioned player-root, device-certificate/revocation, custody,
capability, candidate, vote, commit-certificate, membership-transition,
replicated-event, and certified-snapshot shapes. `PrincipalId` bytes remain the
player root; network identities migrate through a domain-separated legacy self
device certificate. Native/browser-local custody is the default.
Gateway-custodied keys are visibly degraded, grant no extra player vote, and
move to another device only through export-and-rotate plus revocation. Root
loss has no invented recovery scheme in v1. Veilid node IDs/routes remain
transport metadata.

The protocol suite pins actual Ed25519 certificate/candidate/vote signatures
and BLAKE3 candidate, commit, and snapshot vectors, verifies signatures with
fixed keys, rejects domain/key changes, enforces sorted bounded capabilities
and commands, and proves two devices cannot form two player votes. The pure
session boundary requires an explicit cryptographic verifier port, validates
root/device capabilities, deterministic proposer, strict/joint quorum,
next-epoch revocation, idempotent commit, snapshot/head binding, and retained
fork evidence. Four replicated session fixtures cover multi-device counting,
majority/weak certificates, joint kick/revocation, snapshot mismatch boundary,
conflict halt, and the two-player quorum limit. The prescribed three protocol
device tests and four session replicated tests pass offline; strict Clippy
passes. Phase 5.2 remains responsible for the full delivery/reorder/partition
runtime and persistent historical roster/lock machinery.

**Work:**

- Close G5/G6 in ADR 0007. Define player root identity, device certificates,
  add/revoke/loss, simultaneous devices, membership epochs, event IDs, proposal
  IDs, deterministic reduction, fork/conflict rule, quorum, snapshots, and
  liveness assumptions.
- Keep transport node IDs/routes distinct. Preserve current principal IDs via a
  versioned migration/adapter rather than silently reinterpreting old keys.
- Model browser-local keys as default and gateway-custodied keys as an explicit
  degraded capability with export/revoke UX and threat disclosure.

**Validation:**

```pwsh
cargo test -p poche-protocol device
cargo test -p poche-session replicated
```

**Completion criteria:** Exact signing/certificate/event vectors and migration
fixtures close G5/G6 before network or hidden-card code depends on them.

### [x] 5.2 Implement deterministic convergence, partition, reconnect, and kick recovery

**Completion notes:** Completed 2026-08-05. Added the transport-independent
`ReplicatedRuntimeLog` and registered
`replicated-micro-3players-5devices-4events-majority-partition-snapshot-tail`.
The deterministic scenario drives four replicas, three players, five devices,
and four committed events through a majority-side partition, opposite-order
concurrent proposals, height-two-before-height-one buffering/draining, exact
duplicate delivery, certified snapshot plus tail recovery, a joint-quorum kick
of the current actor, stale-epoch and revoked-device denials, a minority
no-quorum denial, and three device-local automatic proposals collapsing to one
semantic event. Nine attempts yield five unique transition keys. All replicas
finish at epoch two with exact state hash
`1b51070836855771b6e51e8a0aae8cffe4784af2c478674e13f579ab5713c168`.

The log verifies canonical proposal ordering, player-deduplicated strict
majority, parent/event/successor hashes, and old/new joint quorum before
mutation. Its snapshot constructor explicitly consumes already-certified
metadata; cryptographic verification remains the protocol/session boundary
from 5.1. That boundary now persists sorted historical epoch rosters and fully
validates a conflicting event's historical proposer, device authority,
candidate signature, vote signatures, and quorum before recording fork
evidence. A dedicated negative test proves an uncertified fork-shaped event
cannot halt the log. Commit certificates are now explicitly bound to the
candidate round as well as room, epoch, height, and value; unknown players in a
membership transition fail closed.

The retained expected-false witness removes non-equivocation: two distinct
two-of-three certificates intersect only at one player, so that player's
equivocation can produce conflicting commits. The result is therefore
conditional accountable crash-fault evidence under deterministic reduction,
eventual delivery among connected honest devices, a strict player majority,
and non-equivocation—not Byzantine consensus or one-survivor progress in a
two-player partition. `docs/replicated-runtime.md` records the causal transcript
and exact simulator/cryptographic/network boundaries. The prescribed runtime
test and CLI check pass offline; full runtime/session regression (22 and 26
unit tests plus integration/doc tests), formatting, and strict Clippy for the
affected crates also pass.

**Work:**

- Implement the experimental replicated reducer/log separately from current
  host-authoritative revision ordering.
- Exercise duplicate, reorder, concurrent valid proposals, stale/revoked
  devices, partitions, reconnect, snapshot+tail, current-actor disconnect,
  quorum changes, and kicked-member recovery.
- Ensure automatic deterministic transitions proposed by many devices collapse
  to one semantic event rather than multiplying effects.

**Validation:**

```pwsh
cargo test -p poche-runtime replicated
cargo run -p poche-xtask --offline -- consensus check --scope micro
```

**Completion criteria:** Every connected honest device converges under the named
delivery/quorum assumptions; safety holds under tested omissions/partitions;
counterexamples are retained when assumptions are removed.

### [~] 5.3 Add formal and cross-implementation consensus evidence

**Work:**

- Add finite Alloy/NuSMV/Rust models for membership/device authority, fork
  safety, proposal uniqueness, vote eligibility, out-of-turn recovery, and
  conditional liveness.
- Add Prolog explanations for why a proposal/vote/device event is accepted,
  ignored, superseded, or denied.
- Preserve client-local auto-proposal behavior as input, never as hidden shared
  mutation.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- consensus compare all --scope micro
cargo run -p poche-xtask --offline -- consensus coverage audit --all
```

**Completion criteria:** Shared claim evidence has zero unexplained
disagreements and states exact Byzantine/crash/quorum/fairness limits.

## Phase 6 — Research and prototype trustless hidden cards

### [ ] 6.1 Select a published protocol and freeze the security model

**Work:**

- Close G7 from primary papers and maintained cryptographic implementations.
  Define adversary, collusion threshold, authenticated-channel assumption,
  fairness, privacy, uniqueness, verifiable shuffle/deal/reveal, abort/dropout,
  transcript verification, performance, and post-round disclosure.
- Compare full shuffle, on-demand card generation, threshold decryption,
  commitments/verifiable mixnets, and a retained trusted-dealer fallback.
- Map the user's Secret Santa analogy precisely, including where it stops being
  sufficient for an actively malicious, dropout-prone card game.
- Obtain an explicit security review gate before any production-strength claim;
  do not invent cryptographic primitives.

**Validation:**

```pwsh
cargo test -p poche-crypto-prototype --offline
```

The crate/command is created only after protocol selection; before that, task
evidence is an ADR plus independently reproducible published vectors/prototypes.

**Completion criteria:** ADR 0008 selects or rejects a concrete construction,
names every assumption and unsupported case, and provides a bounded prototype
plan. If no construction satisfies dropout requirements, trustless mode remains
research-only and the plan says so plainly.

### [ ] 6.2 Implement shuffle/deal/hold/play/reveal verification

**Work:**

- Implement only the selected construction through audited libraries where
  possible, with domain separation, bounded messages, stable transcript types,
  zeroization, and deterministic public test vectors.
- Ensure only the holder learns a private card before reveal; all devices can
  verify exclusivity, deck conservation, revealed identity, and the final-round
  audit without relying on Slug/text/geometry.
- Re-randomize unlinkable public card handles at shuffle privacy boundaries so
  stable object trajectories cannot reveal future identities.

**Validation:**

```pwsh
cargo test -p poche-crypto-prototype --offline
cargo clippy -p poche-crypto-prototype --all-targets --offline -- -D warnings
```

**Completion criteria:** Honest, tampered, replayed, colluding-within-threshold,
wrong-card reveal, duplicate-card, and privacy-redaction vectors behave exactly
as ADR 0008 claims.

### [ ] 6.3 Implement dropout and governance recovery around cryptographic rounds

**Work:**

- Exercise disconnect before/after shuffle contributions, during deal, while
  holding cards, and before final disclosure. Use the selected protocol's
  threshold/recovery mechanism rather than assuming kicked secrets appear.
- Compose vote-kick/redeal/end remedies with cryptographic transcript state so
  no removed player is awaited by an ordinary turn or hidden cleanup tick.
- Preserve evidence needed to distinguish an abort, unavailable share, invalid
  proof, and retrospectively proven game-rule cheat.

**Validation:**

```pwsh
cargo test -p poche-runtime trustless_dropout
cargo run -p poche-xtask --offline -- trustless smoke --scenario dropout
```

**Completion criteria:** Every registered dropout point either recovers under a
named threshold or reaches an explicit governable abort/redeal state; none hang.

## Phase 7 — Compose Veilid, browser gateways, and device agency

### [ ] 7.1 Define hybrid/direct route codes and gateway trust boundaries

**Work:**

- Close G9 in ADR 0009. Version room rendezvous hints for direct Veilid,
  Poche gateway HTTPS origin, and local/explicit fallback without making a
  locator permanent authority.
- Inventory what the gateway sees and can censor/reorder in host and replicated
  modes, which keys remain in-browser, and when a gateway may hold an explicitly
  degraded device key.
- Separate Veilid bootstrap, Poche room rendezvous, application membership,
  device authorization, event consensus, and projection encryption.

**Validation:**

```pwsh
cargo test -p poche-veilid room_code
cargo test -p poche-protocol gateway
```

**Completion criteria:** Codec vectors, threat matrix, and recovery semantics
close G9 without changing current room-code meaning silently.

### [ ] 7.2 Implement HTTP commands plus SSE projections for browser devices

**Work:**

- Extend the self-hostable Axum/Datastar binary so browser devices submit signed
  typed commands over bounded HTTP and receive ordered exact-recipient updates
  over reconnectable SSE.
- Preserve browser-local device keys where platform APIs permit; make any
  gateway-custodied/exported key mode conspicuous and revocable.
- Demonstrate one player simultaneously connected through browser and native
  devices, independent device revocation, pause/chat/grants, and a dropped SSE
  reconnect without duplicating semantic effects.
- Measure payload compression, reconnect behavior, latency, accessibility, and
  operator-visible metadata. Add WSS/WebTransport only for a requirement that
  HTTP+SSE cannot satisfy and record the evidence.

**Validation:**

```pwsh
cargo test -p poche-web-spike --offline
cargo run --locked --release -p poche-web-spike
```

Browser automation must exercise ordinary DOM controls and network
disconnect/reconnect; a unit test alone does not close this task.

**Completion criteria:** The browser participates as a device with an inspectable
signed command path and exact-recipient stream; gateway knowledge/authority is
accurately displayed and documented.

### [ ] 7.3 Re-evaluate direct browser Veilid and document upstream feasibility

**Work:**

- Re-run ADR-0004 HTTPS/WSS evidence against the pinned/released Veilid version
  selected by the workspace and inspect upstream docs/issues for browser relay
  progress.
- Write a Poche-facing feasibility note describing whether a general upstream
  improvement is possible, which public infrastructure/API changes it would
  require, and how it differs from the Poche gateway.
- Do not edit Veilid source or generate an upstream issue/MR. Point the user to
  `CONTRIBUTING.md`, existing human discussions, and exact source locations if
  they elect to pursue it themselves.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- doctor
```

Network probes are opt-in, bounded, and documented; absence of public relay
infrastructure is a result, not a test failure to hide.

**Completion criteria:** The deployment matrix is updated from executable
evidence; direct browser multiplayer is advertised only if production-equivalent
HTTPS/WSS succeeds without a companion.

## Phase 8 — Deliver native 3D and browser-native interaction

### [ ] 8.1 Implement the native 3D spatial adapter and Slug text

**Work:**

- Implement the G3-selected adapter. Bevy entities/components mirror stable
  spatial object IDs and realized locations; gameplay systems cannot mutate
  canonical state through arbitrary transforms/components.
- Render table, seats, hand/deck/trick zones, cards, viewer-authorized faces,
  player names, and score sheet. Use the selected Slug path for semantic text
  runs, with explicit parent surfaces and local z offsets.
- Add picking/drag release, `/play-card` parity, deterministic tweens, camera,
  and a spatial-debug overlay for bounds, anchors, classifications, and findings.

**Validation:**

```pwsh
cargo test -p poche-native-ui --offline
cargo build -p poche-native-ui --release --offline
```

Run a bounded real-window acceptance and record startup, interaction-to-commit,
frame/presentation timing, screenshot, and privacy fixtures without calling
automation latency input-to-photon latency.

**Completion criteria:** A player can inspect a hand, play by CLI or drag, see
the same committed transition/tween, inspect score glyphs, and toggle spatial
audit overlays in a real desktop window.

### [ ] 8.2 Project the same scene semantics into accessible HTML

**Work:**

- Map hands, deck, trick, score sheet, findings, proposals, and votes to ordinary
  semantic HTML regions, lists, buttons, forms, and drag/drop or click actions.
- Let HTML lay itself out; do not transmit or reproduce native camera pixels.
  Every browser action resolves through the same typed command/spatial intent.
- Preserve exact viewer privacy, keyboard/screen-reader operation, reconnect,
  and recovery controls.

**Validation:**

```pwsh
cargo test -p poche-ui --offline
cargo test -p poche-web-spike --offline
```

Browser acceptance inspects DOM/accessibility and exercises play, pause, chat,
grant/revoke, accusation, vote, and reconnect.

**Completion criteria:** Native and HTML clients consume the same semantic
fixture/history and reach identical hashes while using target-appropriate
layouts and interactions.

### [ ] 8.3 Publish the complete inspectable vertical slice and replay

**Work:**

- Add a checked transcript covering room join, multiple devices, deal, typed and
  dragged play, delayed cheat finding, recovery vote, score text, pause/chat/
  grant preservation, and final replay.
- Add a Pages-safe static view of spatial endpoints/formal counterexamples and
  link it from the engineering status without shipping secrets or live claims.
- Demonstrate an Alloy layout counterexample rendered/highlighted by Rust and an
  HTML projection of the same abstract witness.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- spatial vertical-slice
cargo run -p poche-xtask --offline -- pages build
```

**Completion criteria:** A human can run one documented command and inspect
typed NDJSON, semantic text, native spatial behavior, HTML behavior, formal
evidence, and exact scope qualifications for the same scenario.

## Phase 9 — Acceptance, documentation, and release

### [ ] 9.1 Run full regression, security, license, and performance gates

**Work:**

- Run formatting, clippy, workspace tests, existing formal/session/RL gates,
  new spatial/governance/consensus/trustless gates, codec fuzzing, secret scans,
  dependency/license audit, renderer acceptance, and performance measurements.
- Verify existing semantic hashes or document/version every intentional change.
- Test from current source, not stale installed binaries or dirty reference
  repositories.

**Validation:**

```pwsh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p poche-xtask --offline -- coverage audit --all
cargo run -p poche-xtask --offline -- compare all --scope micro
cargo run -p poche-xtask --offline -- session compare all --scope lobby-micro
cargo run -p poche-xtask --offline -- spatial compare all --scope micro
cargo run -p poche-xtask --offline -- consensus compare all --scope micro
```

**Completion criteria:** Every advertised row has current evidence; unavailable
external infrastructure and research-only claims are explicitly scoped rather
than skipped silently.

### [ ] 9.2 Update public architecture, threat model, rules, and status

**Work:**

- Update README, status page, deployment matrix, protocol docs, ADR index,
  contributor evidence guide, and GitHub Pages links.
- Explain typed/spatial refinement, ECS/rendering boundaries, Slug attachment,
  prevent/audit/governance modes, device identity, gateway custody, consensus
  assumptions, dropout, and mental-poker limitations in user-readable language.
- State which modes are host-trusted, gateway-facilitated, replicated,
  cryptographically private, prototype-only, or unsupported.

**Validation:**

```pwsh
cargo run -p poche-xtask --offline -- pages build
git diff --check
```

**Completion criteria:** Public documentation matches executable evidence and
contains no stronger anonymity, fairness, liveness, decentralization, or spatial
proof claim than the registered receipts.

### [ ] 9.3 Complete the guidance audit, commit, push, and remote verification

**Work:**

- Rerun the three intent-audit passes against P3-U1-P3-U38 and repair all gaps.
- Ensure every task has adjacent completion evidence, decisions are in ADRs,
  generated artifacts remain ignored, and the worktree contains only intended
  Poche changes.
- Commit coherent verified increments throughout execution, then commit the
  final plan/docs state, push `model-checking`, and verify remote HEAD.

**Validation:**

```pwsh
git diff --check
git status --short --branch
git log -12 --oneline --decorate
git rev-parse HEAD
git rev-parse origin/model-checking
```

**Completion criteria:** The plan is execution complete, every active guidance
ID has evidence or an explicit approved non-goal, local and remote commits
match, and the worktree is clean.

## Overall completion criteria

- [ ] Every task is `[x]` with adjacent exact evidence; no open/blocked gate is
  hidden by a broad completion claim.
- [ ] Typed Poche/session state remains canonical, and engine-neutral spatial
  refinement proves the registered round-trip, uniqueness, conservation,
  attachment, endpoint, and viewer-privacy laws.
- [ ] `/play-card jack-spades`, native drag, typed calls, and canonical NDJSON
  resolve to the same checked transition and spatial successor.
- [ ] Native 3D and semantic HTML present the same authorized state/history
  while remaining target-appropriate; Slug text is attached semantically and
  never used as score/card identity authority.
- [ ] Alloy, NuSMV, Scryer Prolog, and Rust independently check their honest
  spatial scopes, detect seeded counterexamples, and emit neutral agreement
  evidence without treating Rust as expected answer.
- [ ] Prevent, allow-attempt, automatic-proposal, and manual-accusation modes
  preserve hard invariants and produce replayable retrospective findings.
- [ ] Typed permissions, proposals, votes, excluded-vote evidence, score
  amendments, redeal/kick/end recovery, and AFK/disconnect handling work without
  waiting for the blocked player.
- [ ] Player identities support multiple device keys with exact add/revoke/loss
  semantics; the replicated experimental mode has stated quorum/fork/fairness
  assumptions and convergence/counterexample evidence.
- [ ] Hidden-card fairness/privacy/dropout claims are backed by a selected
  published construction, explicit threat assumptions, vectors, and prototype
  evidence—or remain plainly research-only if no safe construction is selected.
- [ ] The Poche browser gateway preserves device agency as designed, uses
  ordinary accessible HTML plus the selected HTTP/SSE/direct transport, and
  accurately discloses key custody, operator knowledge, and censorship/order
  capabilities.
- [ ] Existing host mode, chat, any-player pause/unpause, spectator grants,
  replay, Pages, formal oracles, and `poche-2p-v1`/`round-score-v1` RL evidence
  remain passing or are explicitly versioned with cause.
- [ ] MPL-2.0, dependency/license, secret-safety, generated-artifact, and Veilid
  upstream-contribution boundaries are satisfied.
- [ ] The final three-pass guidance audit passes; intended changes are committed
  and pushed; `HEAD == origin/model-checking`; the worktree is clean.

## Risk register

| Risk | Consequence | Mitigation / evidence gate |
| --- | --- | --- |
| Geometry becomes a second game oracle | Native, HTML, RL, and formal behavior diverge | C1; ADR 0005; pure realization/abstraction; transition commutation and privacy tests |
| Raw ECS markers admit contradictory card state | A card is in deck/hand/play simultaneously or system order masks a bug | One typed location/projection ID; Bevy adapter has no semantic mutation authority; negative component-world tests |
| Glyph proximity becomes semantic attachment | Touching cards/font changes reassign identity or leak faces | C2; explicit surface/text attachment; nearest-distance audit only |
| Floating point or zone boundaries classify inconsistently | Replicas disagree on a drag or card location | Integer canonical units, separated snap regions/dead bands, endpoint consensus, classification corpus |
| Full game × full geometry state product explodes | Formal tools time out and evidence becomes superficial | Decomposed game, layout, transition, and refinement models; shared claims only |
| General tabletop ambitions displace Poche | Large engine effort without a playable checked game | P3-U27; Poche vertical slice precedes loose mode and generic extensions |
| Slash command injection or parser drift | Signed text executes a different action than UI/NDJSON | Versioned typed AST, canonical signed encoding, parser/help/codec parity, no raw string execution |
| Retrospective detector duplicates or rewrites history | Unstable accusations and divergent remedies | Stable action/evidence/finding IDs; append-only idempotent findings; replay corpus |
| Governance bypasses universe invariants | Votes create cards, forge principals, or corrupt history | C3; effect classification; negative amendment tests; hard validation after every event |
| Accused/disconnected player controls recovery | AFK/malicious actor deadlocks game or tally | G8; out-of-turn proposals; eligibility snapshot; visible/non-counting vote; dropout scenarios |
| Client automation secretly mutates shared state | Different preferences create divergent rooms | Automation may detect/propose only; committed effects follow typed authority/consensus |
| “Distributed” log equivocates or forks | Devices disagree on turns, scores, or membership | G5 formal gate; versioned experimental mode; quorum/fork rule; partitions and counterexamples |
| Device-key model recreates a singleton authority | Browser/desktop agency is lost or revocation ambiguous | G6; root/device certificates; simultaneous-device tests; explicit degraded custody |
| Gateway is described as an invisible relay but orders/reads state | False privacy/decentralization expectations | G9 threat inventory; end-to-end signatures/encryption; operator-visible metadata disclosure |
| SSE simplicity hides missing client-to-server or datagram needs | Ad hoc transport workarounds break replay/order | HTTP command path; reconnect IDs; choose WSS/WebTransport only from measured requirement |
| Mental-poker protocol is invented or misapplied | Card identities leak, duplicates appear, unfair deal, or collusion wins | G7 primary-source/security-review gate; audited libraries; vectors; no production claim before evidence |
| A kicked player withholds cryptographic shares | Trustless round cannot reveal/recover | Dropout-tolerant/threshold selection or explicit abort/redeal; test every phase; no magical recovery claim |
| Stable public card IDs cross a shuffle | Trajectories reveal hidden card identities | Atomic shuffle privacy boundary; re-randomized unlinkable handles; transcript/privacy tests |
| `big_space` adds complexity without benefit | Renderer/version burden for a metre-scale table | G12 measured precision gate; table-local origin default |
| Dirty sibling repositories leak uncommitted code into Poche | Non-reproducible build and unclear licensing | C11; exact crates.io or copied-attributed reviewed code only; clean-clone acceptance |
| Native 3D blocks browser/accessibility | Pretty client narrows who can play | Engine-neutral scene; semantic HTML remains first-class; acceptance matrix |
| Spatial rendering contaminates RL | Rollouts become slow/nondeterministic or reward changes | C7; dependency direction test; preserve spec/reward hashes and direct batch benchmarks |
| Veilid upstream work violates repository policy | Unacceptable AI-generated contribution | Read-only 7.3; no source edits/issues/MRs; user-owned upstream process |
| Plan compaction loses nuanced user intent | Later work quietly drops privacy, voting, or physical constraints | P3-U ledger, three-pass audits at creation and release, adjacent task evidence |
