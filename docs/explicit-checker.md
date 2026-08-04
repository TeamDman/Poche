# Deterministic explicit-state checker

Status: Phase 5.1 complete (2026-08-03).

`poche-check` constructs the complete reachable graph of the strict Rust model
without random sampling or symmetry reduction. The checked scope has the stable
ID `micro-2p-2s-3r-6c-schedule-1-2-1`:

- two fixed seats;
- two unordered suits and three ordered ranks per suit;
- all six distinct cards;
- exact `1,2,1` three-round schedule;
- every 120 one-card and 180 two-card deal partition at its applicable chance
  boundary; and
- both possible prepared first dealers.

This is an exhaustive result for that named micro-game, not for a 52-card game
or any other player count.

## Exploration contract

The checker uses deterministic breadth-first search. `Game` and every semantic
action/state component have structural `Eq`/`Hash`; graph-local state IDs are
assigned in first-discovery order. The standard hash map is used only for
membership lookup and is never iterated, so its randomized internal bucket order
cannot affect discovery, action, or trace order.

For each discovered state the checker enumerates the strict model's complete
legal-action surface in its canonical order. Chance partitions, player actions,
deterministic settlement, and the sole terminal `Absorb` self-loop are explicit
edges. If an advertised legal action is rejected by transition semantics, the
run fails closed with the source state, action, and semantic error.

The first incoming edge records a shortest-path predecessor and action. A
counterexample is reconstructed as its initial state, ordered actions, and every
successor. The injected false invariant “every reachable phase is
`AwaitingDeal`” is first violated at depth one; its one-step trace replays to the
same `Bidding` state.

An optional state limit exists only for diagnostic tests. A limited run reports
`StateLimitReached` and cannot be called exhaustive. Proof-producing runs must
report `ReachableStateSpaceExhausted`.

## Raw micro-scope counts

The unbounded command reports:

| Measurement | Count |
|---|---:|
| Prepared initial states | 2 |
| Distinct reachable states | 431,800 |
| Labeled transitions | 549,896 |
| Transitions to an already discovered state | 118,098 |
| Maximum shortest-path depth | 20 |
| Reachable `Finished` states | 176 |
| Termination reason | `ReachableStateSpaceExhausted` |

No player/card symmetry quotient is applied. A future reduction must first prove
that the quotient preserves the properties and counterexample projections being
checked, then retain these raw counts as the equivalence baseline.

Run the Phase 5.1 gate with:

```pwsh
cargo test -p poche-check explicit_small_graphs
cargo test -p poche-check shortest_counterexample
cargo run -p poche-xtask -- check rust-explicit --scope micro
```
