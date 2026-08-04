# Complementary generated and larger-scope testing

Status: Phase 5.4 complete (2026-08-03).

These tests are deliberately labeled **sampled**. They complement the exhaustive
six-card graph, Alloy bounds, NuSMV temporal checks, and Prolog queries; passing
them is never described as a proof over the sampled full-deck configurations.

## Generated strict-model traces

`TraceSeed` implements `proptest::Arbitrary`, but arbitrary bytes are not decoded
into a raw `Game`. A seed selects one of the two refined prepared dealers, then
selects only from `Game::legal_actions()` at each explicit chance/player
boundary. Settlement remains deterministic. This preserves private constructors
and ensures every generated state/action trace respects the formal refinements.

For every generated complete 20-action micro-game:

- each state validates before use;
- chance selects an applicable exact 120/180 deal partition;
- applying the same state/action twice returns an identical transition;
- a direct Rust precondition independently checks phase, actor, bid bound, hand
  membership, and follow-suit;
- direct trump/lead/rank winner and score/payment formulas are compared with the
  Weavy-powered transition result;
- every catalog transition-local obligation is evaluated;
- the trace reaches `Finished`; and
- terminal absorption replays without changing state.

The proptest gate runs 16 generated seeds plus six fixed replay seeds. A failing
case reports its `u64` input directly; adding the minimized seed to
`REGRESSION_SEEDS` turns it into a permanent exact replay. The fixed corpus
already includes zero/one, the documentation seed, two mixing-boundary values,
and `u64::MAX`.

Pure Weavy kernel results are memoized by their complete finite input tuple.
The first occurrence is always evaluated by the lowered Weavy program; identical
later calls reuse that result. This changes neither accepted inputs nor outputs
and keeps generated/exhaustive checking practical.

## Sampled full-deck configurations

`LargerScopeCase` also implements `Arbitrary`. It selects a seed and one of four
complete 52-card conventional-oracle table sizes:

- 2 players (maximum hand seven, full 13-round schedule);
- 3 players (same maximum/schedule with another clockwise ring);
- 7 players (upper boundary of the seven-card schedule); and
- 51 players (largest supported table, one one-card round plus trump).

A local SplitMix64 stream deterministically shuffles all 52 unique cards and
selects only currently legal player actions. RNG state remains outside semantic
game state. Every state validates, every settlement exposes raw score events,
every run finishes within a fixed diagnostic bound, and re-running the same
seed/table yields the same steps, scores, pot, and winner mask.

The gate runs 40 generated cases and the six fixed seeds at all four table sizes.
`FullDeckSummary.confidence` is literally `"sampled"`; none of these outcomes is
merged with the exhaustive micro-scope result.

Run the Phase 5.4 gates with:

```pwsh
cargo test -p poche-model proptest_transition_equivalence
cargo test -p poche-model proptest_larger_scopes
```
