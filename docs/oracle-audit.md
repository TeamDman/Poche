# Native oracle completeness audit

- Status: Phase 6 cross-model acceptance complete
- Date: 2026-08-03
- Authority: `docs/main.typ` plus stable IDs in `docs/rules-coverage.md`

All 61 stable rules have a valid disposition in Rust, Alloy, NuSMV, and Scryer
Prolog. “Comprehensive” follows gate G11: every rule is modeled, checked,
queried, or explicitly inapplicable with a reason. It does not erase each
tool's scope or turn a bounded result into an unbounded proof.

## Evidence summary

| Oracle | Native scope | Strongest current evidence | Card identity | Transition view |
| --- | --- | --- | --- | --- |
| Conventional Rust | Generic 2..51 players; deterministic and sampled full games | Focused tests, a 150-transition full two-player game, and 2/3/7/51-player deterministic replay samples | Full 52-card identity and ownership | Typed executable environment |
| Strict Rust/Weavy | Exact two-player, six-card, `1,2,1` schedule | 431,800 states, 549,896 transitions, exhaustive safety/liveness, sampled larger-scope boundary tests | Full six-card micro identity | Typed phase graph and exact action enumeration |
| Alloy 6.2.0 | Exact 52 cards; named two/three-player scopes; 7-bit integers | 15 base commands plus 10 structural-conformance commands | Full identity inside each bounded command | Complete relational snapshots plus six-step phase trace |
| NuSMV 2.7.1 | Full two-player `1..7..1` count abstraction plus exact micro temporal conformance | 46 full-oracle properties plus 23 named micro properties and normalized defect traces | Conserved counts plus current trick suit/rank | Exhaustive symbolic transition systems |
| Scryer Prolog | Generic finite relations; full-identity bounded transition/query corpora | 16 base queries plus six fixtures and 264 exact normalized rows | Full identity in generic deals and bounded round states | Relational successor/predecessor queries |

## Cross-model agreement

| Common surface | Compared evidence | Result |
| --- | --- | --- |
| Conventional Rust ↔ strict Rust | 28 viewer observations, 10 legal-action sets, 14 transitions across the exact common prefix | 14 inventory matches and four explicit scope/preparation differences; zero unclassified differences |
| Rust ↔ Scryer Prolog | legal actions, complete successors, bounded ground predecessors, trick winners, exhaustive finite score causes, rule explanations | six fixtures and 264 exact order-independent rows agree |
| Rust ↔ Alloy | valid/invalid structures, one complete round projection, aggregate assertion, controlled defects | 10 scoped commands and 13 relation groups agree; five malformed fixtures rejected and three weakened-rule witnesses found |
| Rust ↔ NuSMV | prepared initial states, phase transitions, progress, deadlocks, universal termination, defect traces | 23 named properties; two-state stutter lasso and five-state deadlock prefix agree exactly |
| Rule coverage | every stable rule by Rust/Alloy/NuSMV/Prolog applicability | 61 rows × four tracks have direct evidence or a reasoned `n/a`; zero `todo` cells |

Pairwise checks are intentionally limited to surfaces with shared meaning.
Prolog proof modes are not forced into a temporal state-machine API, NuSMV
count abstraction is not treated as card identity, and an Alloy bound is never
reported as Rust-style exhaustive graph coverage.

## Explicit differences and underspecification

| ID | Classification | Difference | Current disposition |
| --- | --- | --- | --- |
| D-01 | scope limitation | Alloy commands exhaust only their named atom/integer scopes. | Preserve scope in every receipt/result; never generalize the bounded result. |
| D-02 | scope limitation | The full NuSMV transition oracle fixes two players even though a separate symbolic parameter checks schedule formulas for 2..51. | Completed temporal conformance uses a separately named `1,2,1` micro projection and preserves the full oracle unchanged. |
| D-03 | tool-role limitation | NuSMV conserves card counts and current trick attributes but not card identity across prior tricks. | Alloy, Rust, and Prolog retain identity; compare only the shared projection. |
| D-04 | scope limitation | Prolog's bid/play `step/3` corpus fixes two players and one card while its schedules, deals, winners, and scores are generic finite relations. | Reverse queries require a ground bounded successor/action; full-game liveness remains NuSMV/Rust work. |
| D-05 | scope limitation | Conventional full-deck Rust tests/samples are not exhaustive over 52-card games. | The distinct strict six-card scope is exhaustively checked; full-deck results remain labeled deterministic or sampled. |
| D-06 | expected semantic difference | Whole-deal transitions collapse physical one-card-at-a-time dealing in Rust/Alloy/NuSMV. | Accepted because no decision or order-sensitive observation occurs mid-deal; Prolog `deal_round/7` still constructs clockwise layers. |
| D-07 | ambiguous written rule | “Divide the bowl as evenly as practical” does not specify indivisible-cent ownership. | Rust and Prolog expose quotient/remainder; Alloy/NuSMV identify all recipients; no oracle invents a remainder winner. |
| D-08 | ambiguous written rule | Playoff rounds are allowed only if agreed before play, but mechanics are unspecified. | Baseline models use shared winners and no extra round; a future house rule must update the rulebook and ledger. |
| D-09 | tool-role limitation | Paper score-cell glyphs and physical score-sheet placement are not native concerns for Alloy/NuSMV. | Rust/Prolog retain semantic cell variants; all models retain numeric scores; Typst owns presentation. |
| D-10 | expected semantic difference | First Jack is order-biased, but no probability distribution is specified or needed for legal-state support. | Every model preserves possible selected players/seat order where applicable and makes no uniformity claim. |
| D-11 | tool-role limitation | Hidden-information observations are a Rust environment concern rather than an Alloy/NuSMV/Prolog surface. | Conventional/strict observation projections agree, validation rejects leaks, and a controlled strict-Rust leak witness proves discrimination; no irrelevant native API is invented. |

No row above is a missing rule encoding. A newly discovered omission must return
the relevant coverage cell to `todo` instead of being relabeled as a benign
difference.

## Accepted shared inventory

`fixtures/oracle-inventory.toml` names the scenarios, properties, and reverse
queries used by the Phon interchange and normalized native adapters. Its track
selectors are checked by the 53-pair native adapter registry; specialized
Prolog, Alloy, and NuSMV conformance fixtures add the comparisons summarized
above. Exact model-family revisions and acceptance commands are recorded in
[`acceptance-matrix.md`](acceptance-matrix.md).

## Reproduction

```pwsh
cargo run -p poche-xtask -- coverage audit --all
cargo run -p poche-xtask -- oracle check rust
cargo run -p poche-xtask -- oracle check alloy
cargo run -p poche-xtask -- oracle check nusmv
cargo run -p poche-xtask -- oracle check prolog
cargo run -p poche-xtask -- oracle check all
cargo run -p poche-xtask -- oracle report
cargo run -p poche-xtask -- compare all --scope micro
cargo run -p poche-xtask -- acceptance hashes
```
