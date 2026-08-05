# Cross-model acceptance matrix

This is the first-milestone acceptance snapshot for the independently authored
Poche models and their published rulebook. It records what was actually
compared, at what strength and scope. The matrix does not upgrade bounded,
queried, or sampled evidence into exhaustive proof. Publication and contributor
guidance are complete; RL, generation, and legacy comparison remain deferred.

## Pinned tools

| Runtime/tool | Accepted version | Discovery |
| --- | --- | --- |
| Rust | Rust 1.96.0 | pinned toolchain |
| Alloy | Alloy 6.2.0 | `ALLOY_BIN` or `PATH` |
| NuSMV | NuSMV 2.7.1 | `NUSMV_BIN` or `PATH` |
| Prolog | Scryer Prolog 0.10.0-17-ge4d96925 | `SCRYER_PROLOG_BIN` or `PATH` |
| Typst | Typst 0.15.1 (`9dfd3a08`) | `TYPST_BIN` or `PATH`; official Linux archive digest-verified in Pages CI |
| Facet | 0.50.0-rc.5 | exact Cargo dependency |
| Phon | 0.2.0-rc.5 | exact Cargo dependency |
| Weavy | 0.2.2 | exact Cargo dependency |

The native runner records the resolved executable, actual version banner,
arguments, exit status, stdout, and stderr for every invocation. A version pin
is not a substitute for that per-run evidence.

## Rule and behavior evidence

| Surface | Scope and confidence | Reproducible evidence | Accepted result |
| --- | --- | --- | --- |
| Rule ledger | 61 stable rules × Rust/Alloy/NuSMV/Prolog applicability | `coverage audit --all` | every cell has direct evidence or reasoned `n/a`; zero `todo` cells |
| Conventional Rust | full 52-card games, 2..51 supported players; executable plus sampled outside focused cases | oracle tests and `oracle check rust` | complete two-player smoke game: 13 rounds/150 transitions; sampled larger scopes stay labelled sampled |
| Strict Rust/Weavy | exhaustive `micro-2p-2s-3r-6c-schedule-1-2-1` | explicit BFS, safety catalog, SCC/rank analysis | 431,800 states; 549,896 transitions; no nonterminal deadlock/cycle; universal termination |
| Conventional ↔ strict Rust | exact common `1,2` prefix plus classified boundaries | `compare rust-oracle rust-formal` | 14 matches, four classified scope/preparation differences; 28 observations, 10 legal sets, 14 transitions |
| Alloy base oracle | separately scoped 52-card bounded SAT | `oracle check alloy` | 15 commands: seven SAT witnesses and eight UNSAT assertions |
| Rust ↔ Alloy | exact two-player/52-card/one-round/one-or-two-trick commands, 7-bit integers | `compare rust alloy --scope micro` | 10 commands; 13 canonical relation groups; five invalid structures rejected; three controlled defects witnessed |
| NuSMV base oracle | exhaustive symbolic two-player `1..7..1` count abstraction | `oracle check nusmv` | 46 invariant/CTL/LTL properties hold, including fairness-free termination |
| Rust ↔ NuSMV | exhaustive shared two-player/six-card `1,2,1` temporal projection | `compare rust nusmv --scope micro` | 23 named properties; two initial states; nine Rust phase pairs; matching two-state stutter lasso and five-state deadlock prefix |
| Scryer base oracle | finite generic/bounded relational query modes | `oracle check prolog` | 16 named queries pass |
| Rust ↔ Scryer | ground two-player one-card transitions plus finite score/explanation relations | `compare rust prolog --fixtures tests/fixtures/prolog` | six fixtures; 264 exact order-independent rows; three ground bounded predecessors; 20 explanation bindings |
| Phon evidence | typed and semantically refined wire envelopes | interchange and injected-defect tests | model/rules/schema/scope/scoring/observation/backend/confidence identities survive roundtrip and are revalidated |
| Rulebook and replay publication | exact `model-checking` source commit, pinned compilers/digest/fonts, generated output untracked | Pages run [`31026114728`](https://github.com/TeamDman/Poche/actions/runs/31026114728) and [publication decision](pages-publication.md) | `200` landing/PDF/replay/WASM from commit `16f213252e3c447216582b462a498fbf27800379`; public browser reached the active replay and Pages explicitly disclaims live multiplayer |

## Applicable comparison surfaces

| Question | Rust | Alloy | NuSMV | Prolog | Cross-model disposition |
| --- | --- | --- | --- | --- | --- |
| Full card identity/partition | yes | yes within named bounds | counts only | yes in finite terms | Rust↔Alloy exact canonical partition; Prolog bounded identity answers; NuSMV compared only on counts |
| Valid structural round | typed construction/validation | relational facts/assertions | temporal/count projection | `valid_round_state/1` | Rust↔Alloy canonical round and invalid fixtures; Rust↔Prolog successor terms |
| Legal actions | exact finite sets | relational legality facts | `TRANS` restrictions | relational answer sets | Rust↔Prolog exact sets; Alloy/NuSMV checked through their native structural/transition obligations |
| Successor transitions | executable graph | six-step relational lifecycle | symbolic transition relation | `step/3` plus internal `collect` | Rust↔Prolog exact bounded successors; Rust↔NuSMV nine phase pairs; Alloy lifecycle projection |
| Predecessor explanations | replayable graph | not a native query role | counterexample prefixes | bounded ground reverse relation | three Rust↔Prolog predecessor/action answers; no invented Alloy/NuSMV reverse API |
| Safety properties | exhaustive micro catalog | bounded assertions | symbolic invariants | finite validation/query corpus | shared ground cases and controlled defects agree; confidence remains backend-specific |
| Deadlock/termination | exhaustive SCC and exact rank | selected lifecycle shape only | CTL/LTL plus `check_fsm` | finite trace recursion, not global model checking | Rust↔NuSMV correct results and both controlled temporal defects agree |
| Rule explanations | rule catalog/evidence bundles | named command/source scopes | named properties/traces | `rule_explanation/3` | Rust↔Prolog exact rule identity and nonempty independent explanations |
| Hidden observations | explicit per-player API | not applicable | not applicable | not exposed as a policy observation API | conventional/strict Rust projection and leak-discrimination evidence; no forced native surface |

## Known limits retained at acceptance

- Alloy results are bounded by each receipt-derived integer/atom scope.
- The full NuSMV oracle conserves 52 cards by counts and current-trick
  attributes, not cross-trick identity.
- The NuSMV conformance model is a separately named `1,2,1` projection; it does
  not replace or silently narrow the full `1..7..1` oracle.
- Prolog reverse search is accepted only for ground successor/action fixtures;
  `once/1` commits three deterministic first proofs and avoids unproductive
  unconstrained alternative-term search.
- Conventional full-deck Rust runs and 2/3/7/51-player Arbitrary cases are
  executable/sampled evidence, not exhaustive full-game proof.
- The strict Rust graph is exhaustive only for its declared six-card scope.
- Whole-deal transitions intentionally collapse physical card-at-a-time dealing
  because no modeled decision or observation occurs between cards.
- Indivisible-cent tie remainder and optional playoff mechanics remain written
  ambiguities; no model invents a rule.
- Native Typst HTML 0.15.1 is not published: the experimental target drops the
  selected template's page/two-column layout, title placement, spacing, and
  alignments. Landing HTML plus PDF is the accepted G13 format.
- RL, automatic target generation, and legacy-v2 comparison remain deferred.

The detailed classifications remain D-01 through D-11 in
[`oracle-audit.md`](oracle-audit.md).

## Model-family revisions

Each digest is BLAKE3 over an ordered list of repository-relative path bytes, a
zero separator, little-endian file length, and file bytes. This pins semantic
source families without hashing generated output or machine paths.

| Group | Revision | Contents |
| --- | --- | --- |
| rules | `blake3:04559f8bc9001b8ed9a56757dbc966f3d8aa382d683094b89cf5bfb551072580` | Typst authority and 61-rule ledger |
| rust-oracle | `blake3:ea709b4e644370f647cfb0ea1cf6266e4087dbaf13a2b309760c874694a7be99` | conventional full-deck Rust oracle |
| rust-formal | `blake3:c43a78dc2c2d2b8b31e1a6e6a9a1c56bcdb0076eb5ef4385236b6b8fe4341106` | strict model, Weavy kernels, explicit safety/liveness checker |
| contracts | `blake3:12c14c41657705d01e537a7c3abf492e67337db8ba4a49bbc3453b51d9f73a52` | finite domain, environment, formal dialect, Phon interchange |
| alloy | `blake3:9f5b657cf5fa8e8de43b4919ea1c854d6efcd51694e6ed3951afbdca9de50741` | base and conformance Alloy sources |
| nusmv | `blake3:d63cdb5928d4b785dcd6549f4b501b8f3d36477a6d8165bab258e39ca20aabc4` | base and conformance NuSMV sources |
| prolog | `blake3:df858eeaa772923fd41a4044d0290b79b05de4777077593c4631e5a6507b558c` | Scryer Prolog source |
| conformance | `blake3:984aaa037bad047a4d0f9e2c9e33d4ab6271ee711383d8bcd8752a03b8e08fdd` | native adapters/runners and pairwise comparison implementations |
| toolchain | `blake3:dbd785469e9116210313af6fe44301cf80e6d74233c96e774b9816157b79cb3c` | Cargo lock, Rust toolchain, native version pins |

Recompute and audit revisions with:

```pwsh
cargo run -p poche-xtask -- acceptance hashes
cargo run -p poche-xtask -- compare all --scope micro
cargo run -p poche-xtask -- guidance audit PLAN.md
```

`compare all --scope micro` runs the conventional Rust smoke game, all three
full native oracles, every pairwise comparison above, and rejects a stale hash
or required acceptance marker. Raw native evidence remains ignored under
`target/`; this document records stable claims, not generated transcripts.

The public rulebook is <https://teamdman.github.io/Poche/>. Contributor-facing
model roles, claim boundaries, native commands, observation/scoring contracts,
and LLM review requirements are in [`CONTRIBUTING.md`](../CONTRIBUTING.md).
