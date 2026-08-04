# Native oracle completeness audit

- Status: Phase 2 complete
- Date: 2026-08-03
- Authority: `docs/main.typ` plus stable IDs in `docs/rules-coverage.md`

All 61 stable rules have a valid disposition in Rust, Alloy, NuSMV, and Scryer
Prolog. “Comprehensive” follows gate G11: every rule is modeled, checked,
queried, or explicitly inapplicable with a reason. It does not erase each
tool's scope or turn a bounded result into an unbounded proof.

## Evidence summary

| Oracle | Native scope | Strongest current evidence | Card identity | Transition view |
| --- | --- | --- | --- | --- |
| Conventional Rust | Generic 2..51 players; deterministic full games exercised at 2 and 51 | 10 focused tests plus a 150-transition full two-player game | Full 52-card identity and ownership | Typed executable environment |
| Alloy 6.2.0 | Exact 52 cards; two-player/two-trick round checks; three-player selection/winner checks; 7-bit integers | 7 SAT witnesses and 8 UNSAT assertion checks | Full identity inside each bounded command | Complete relational snapshots plus six-step phase trace |
| NuSMV 2.7.1 | Exact two-player full 13-round schedule; independent symbolic table parameter 2..51 | 46 true invariant/CTL/LTL properties, including fairness-free universal termination | Conserved counts plus current trick suit/rank | Exhaustive symbolic transition system |
| Scryer Prolog | Generic finite rule relations; full-identity two-player/one-card transition corpus | 16 passing forward/reverse queries | Full identity in generic deals and bounded round states | Relational successor/predecessor queries |

## Explicit differences and underspecification

| ID | Classification | Difference | Current disposition |
| --- | --- | --- | --- |
| D-01 | scope limitation | Alloy commands exhaust only their named atom/integer scopes. | Preserve scope in every receipt/result; never generalize the bounded result. |
| D-02 | scope limitation | NuSMV transitions fix two players even though a separate symbolic parameter checks schedule formulas for 2..51. | Cross-model transition conformance begins at the named micro-scope. |
| D-03 | tool-role limitation | NuSMV conserves card counts and current trick attributes but not card identity across prior tricks. | Alloy, Rust, and Prolog retain identity; compare only the shared projection. |
| D-04 | scope limitation | Prolog's bid/play `step/3` corpus fixes two players and one card while its schedules, deals, winners, and scores are generic finite relations. | Reverse queries require a ground bounded successor/action; full-game liveness remains NuSMV/Rust work. |
| D-05 | scope limitation | Conventional Rust tests selected deterministic actions/decks and are not yet exhaustive. | Phase 5 adds explicit-state exhaustive checking in the named G4 micro-scope. |
| D-06 | expected semantic difference | Whole-deal transitions collapse physical one-card-at-a-time dealing in Rust/Alloy/NuSMV. | Accepted because no decision or order-sensitive observation occurs mid-deal; Prolog `deal_round/7` still constructs clockwise layers. |
| D-07 | ambiguous written rule | “Divide the bowl as evenly as practical” does not specify indivisible-cent ownership. | Rust and Prolog expose quotient/remainder; Alloy/NuSMV identify all recipients; no oracle invents a remainder winner. |
| D-08 | ambiguous written rule | Playoff rounds are allowed only if agreed before play, but mechanics are unspecified. | Baseline models use shared winners and no extra round; a future house rule must update the rulebook and ledger. |
| D-09 | tool-role limitation | Paper score-cell glyphs and physical score-sheet placement are not native concerns for Alloy/NuSMV. | Rust/Prolog retain semantic cell variants; all models retain numeric scores; Typst owns presentation. |
| D-10 | expected semantic difference | First Jack is order-biased, but no probability distribution is specified or needed for legal-state support. | Every model preserves possible selected players/seat order where applicable and makes no uniformity claim. |
| D-11 | tool-role limitation | Hidden-information observations currently have executable evidence only in conventional Rust. | G9 remains binding; Phase 3 defines the shared observation boundary and Phase 6 compares observable projections. |

No row above is a missing rule encoding. A newly discovered omission must return
the relevant coverage cell to `todo` instead of being relabeled as a benign
difference.

## Initial shared inventory

`fixtures/oracle-inventory.toml` names the scenarios, properties, and reverse
queries that become shared Phon fixtures or normalized backend checks in Phases
3 and 6. At this stage it is an inventory of native evidence—not yet a claim
that serialized fixtures or all pairwise comparisons exist.

## Reproduction

```pwsh
cargo run -p poche-xtask -- coverage audit --all
cargo run -p poche-xtask -- oracle check rust
cargo run -p poche-xtask -- oracle check alloy
cargo run -p poche-xtask -- oracle check nusmv
cargo run -p poche-xtask -- oracle check prolog
cargo run -p poche-xtask -- oracle check all
cargo run -p poche-xtask -- oracle report
```
