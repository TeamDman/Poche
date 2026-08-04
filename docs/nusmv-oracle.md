# NuSMV oracle

`models/nusmv/poche.smv` is a handwritten NuSMV 2.7.1 transition-system
oracle derived directly from the rulebook and rule ledger. It is independent of
the Rust and Alloy implementations.

## Scope and abstraction

The model fixes exactly two players and executes the complete thirteen-round
`1,2,3,4,5,6,7,6,5,4,3,2,1` schedule. It represents all 52 cards through
conserved counts across undealt, two hands, the public center, captured piles,
and the revealed trump. The current trick retains suit and rank, including
follow-suit legality and trump/lead/rank winner computation. Card identity
across different tricks is abstracted; full identity and without-replacement
provenance remain in Alloy and Rust.

The environment nondeterministically supplies dealer-selection method, trump,
bids, played-card attributes, and whether the follower holds lead suit.
`TRANS` constraints remove out-of-range bids, illegal failure to follow suit,
and replay of the current lead card. There is no strategy or probability
distribution in this oracle.

## Temporal meaning

Every nonterminal phase advances unconditionally and `finished` is absorbing,
so no fairness assumption is needed. NuSMV checks both `AF finished` and the LTL
property `F finished`: every legal path in this exact abstraction completes all
thirteen rounds. `AG EX TRUE` is the explicit reachable-deadlock check.

Safety invariants cover the schedule, phase/actor order, card conservation,
trick counts, follow suit, winner computation, score formulas, missed-bid
payments, opening antes, final one-card round, tied winners, and bowl recipients.
Existential CTL properties retain witnesses for an unrestricted total bid, a
successful zero bid, a trump-over-lead win, both named random-selection methods,
and final-state reachability.

## Native check and evidence

```pwsh
cargo run -p poche-xtask -- oracle check nusmv
```

The runner discovers `NuSMV` through `NUSMV_BIN` or `PATH`, invokes NuSMV with
sound cone-of-influence reduction, and rejects any result reported false. It
preserves native stdout and stderr separately under `target/nusmv-oracle/` so a
future counterexample trace is retained. It also writes typed normalized
property lines to `normalized-results.txt` for stable review and cross-model
tooling. The returned result count must exactly match the handwritten source
property count; unfamiliar truth values or malformed/missing lines fail as
`Unknown`. All paths are ignored build evidence.

The checked result is exhaustive for the reachable state space of this fixed
two-player symbolic abstraction. It is not a proof for other player counts or
for cross-trick card identity. Those limits are intentionally different from
Alloy's named bounded atom scopes and Rust's typed executable oracle.

The separate [Rust/NuSMV temporal-conformance suite](nusmv-conformance.md)
uses the strict Rust model's smaller `1,2,1` scope to compare initial states,
phase transitions, progress/termination, deadlocks, and matching controlled
counterexamples. It preserves this full oracle's broader schedule as an
independent check rather than changing its scope to fit Rust.
