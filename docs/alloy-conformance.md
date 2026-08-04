# Rust and Alloy bounded conformance

Phase 6.3 compares the conventional Rust oracle with a separate Alloy fixture
module, `models/alloy/conformance.als`. The fixture imports the independently
handwritten `models/alloy/poche.als`; neither Alloy source file is generated
from Rust.

## Canonical relational projection

`CanonicalOneCardRustFixture` selects a complete two-player, one-card round.
Stable `CanonicalRoles` relations label the semantic atoms, so comparison never
depends on Alloy-generated names such as `Card$17`.

The matching Rust execution starts with dealer seat 1 and the standard ordered
deck. Seat 0 receives Clubs 2, seat 1 receives Clubs 3, and Clubs 4 is revealed
as trump. The players bid 0 and 1 and play their cards in that order. The Rust
adapter parses the Alloy receipt and compares 13 relation groups exactly:

- dealer, first player, and the three labelled cards;
- initial hands and bids;
- trick leader, both plays, and winner;
- round scores and captured piles; and
- the restored set of all 52 distinct cards.

Rust computes the lifecycle by applying its public `Game<2>` transitions. Alloy
describes the whole round relationally. Conformance is therefore over the
observable lifecycle projection, not accidental agreement between internal
state layouts.

## Invalid structures and discriminating defects

The suite contains five individually named malformed predicates. Each `run`
must be UNSAT under the core Poche facts:

| Fixture | Rejected condition |
| --- | --- |
| `InvalidDuplicateHandCard` | one card appears in two initial hands |
| `InvalidTrumpInHand` | the revealed trump also occurs in a hand |
| `InvalidFollowSuitPlay` | a player discards while holding the lead suit |
| `InvalidWinner` | the recorded winner is not trump/lead eligible |
| `InvalidRoundScore` | an exact partial bid is not scored as 10 plus bid |

`CoreRejectsKnownInvalidStructures` checks their conjunction as one assertion
and must also be UNSAT (no counterexample).

Three controlled weakened-rule predicates must instead be SAT:

| Witness | What the witness demonstrates | Rust-side discriminator |
| --- | --- | --- |
| `UnrestrictedFollowSuitDefect` | a follower can hold both lead-suit and off-suit cards | the off-suit action returns `MustFollowSuit` |
| `RankOnlyWinnerDefect` | a higher off-suit card can exist beside the correct lead-suit winner | `trick_winner` keeps the eligible lead card |
| `PartialAllTricksBonusDefect` | an exact partial bid differs from the 20-plus-bid formula | `score_round(1, 1, 2)` returns 11, not 21 |

The invalid predicates being UNSAT are expected success results for this suite;
they are not confused with ordinary witness commands that require SAT. The
typed runner accepts an explicit expected command kind and polarity and fails
closed on missing, duplicate, extra, or malformed output.

## Scope and evidence

All ten commands use seven-bit integers, exactly two players, all 52 cards,
exactly one round, six lifecycle steps, no game result, and no dealer-selection
process. Each command separately fixes one or two tricks as its predicate
requires. These are bounded results, not unbounded proofs.

The exact source and scope for every command is read back from Alloy's
`receipt.json` and returned in `AlloyConformanceReport::command_scopes`. Raw
stdout/stderr, command, version, exit status, normalized results, and receipt
remain under the ignored `target/alloy-conformance/` directory. On Windows the
runner validates the model's canonical repository containment but removes the
`\\?\` path spelling before invoking Alloy's Java launcher.

## Reproduction

```pwsh
cargo test -p poche-conformance alloy
cargo run -p poche-xtask -- compare rust alloy --scope micro
```

Both commands require Alloy 6.2.0 on `PATH` or in `ALLOY_BIN`.
