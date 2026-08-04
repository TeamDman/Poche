# Alloy oracle

`models/alloy/poche.als` is a handwritten Alloy 6.2.0 oracle derived directly
from `docs/main.typ` and the stable IDs in `docs/rules-coverage.md`. It is not
generated from either Rust model.

The model combines four relational views:

- a complete 52-card deck, explicit rank order, unranked suits, and a cyclic
  player ring;
- complete-round snapshots containing initial hands, the revealed trump,
  bids, ordered tricks, legal plays, winners, captured piles, scores, ante and
  missed-bid payments, and restored cards;
- a rising-and-falling game plan plus a bounded phase trace; and
- First Jack and repeated-tie High Card dealer-selection relations.

## Native check

Run every command with:

```pwsh
cargo run -p poche-xtask -- oracle check alloy
```

The runner discovers `alloy` from `ALLOY_BIN` or `PATH`, invokes the installed
Alloy CLI with overflow rejection enabled, and writes only ignored evidence
under `target/alloy-oracle/`.

The shared native runner preserves `stdout.log`, `stderr.log`, the exact command,
version, and exit code separately. Its normalized results retain SAT/UNSAT,
instance counts, and the exact receipt-derived command source/scope. Any missing,
duplicate, or unfamiliar command/result is an `Unknown` failure, never success.

## Recorded scopes

Every command uses a seven-bit integer scope, the real 52-card deck, and six
ordered lifecycle steps. Complete-round checks use exactly two players, one
round, and two tricks (a two-card micro-round). Dealer-selection witnesses use
exactly three players. The final-winner assertion also uses exactly three
players. Each exact scope remains visible in the `.als` command and in Alloy's
generated `receipt.json`.

Expected native results:

| Kind | Command | Expected result |
| --- | --- | --- |
| witness | `CompleteRoundWitness` | SAT |
| witness | `UnrestrictedBidWitness` | SAT |
| witness | `ZeroBidSuccessWitness` | SAT |
| witness | `SharedWinnerWitness` | SAT |
| witness | `FirstJackWitness` | SAT |
| witness | `RepeatedHighCardWitness` | SAT |
| witness | `ParameterBoundaryWitness` | SAT |
| assertion | `CompleteDeckIsExactly52` | UNSAT (no counterexample) |
| assertion | `CardConservationAndPartition` | UNSAT (no counterexample) |
| assertion | `FollowSuitIsEnforced` | UNSAT (no counterexample) |
| assertion | `DealerBidsLastInClockwiseOrder` | UNSAT (no counterexample) |
| assertion | `WinnerIsEligibleAndHighest` | UNSAT (no counterexample) |
| assertion | `ScoreAndPaymentAgree` | UNSAT (no counterexample) |
| assertion | `ScheduleBoundariesAndFeasibility` | UNSAT (no counterexample) |
| assertion | `FinalWinnersAreExactlyTheMaxima` | UNSAT (no counterexample) |

The separate [Rust/Alloy bounded-conformance suite](alloy-conformance.md)
imports this oracle and adds role-labelled valid/invalid fixtures plus
controlled weakened-rule witnesses. It compares a canonical receipt instance
with an executed Rust round without generating either oracle.

## Proof boundary

The structural rule facts are general over their signatures, but the commands
above are bounded SAT checks, not unbounded proofs. In particular, a two-card
round is exhaustive only within the named atom and integer scopes. The
`ParameterBoundaryWitness` and `ScheduleBoundariesAndFeasibility` assertion
exercise the formula over all representable player-count integers from 2
through 51, while they do not instantiate 51 Player atoms.

The whole-deal relation intentionally collapses the physical one-card-at-a-time
deal because no legal decision or observation occurs between dealt cards. First
Jack preserves seat-ordered draw support but makes no false claim that the
method is uniform. High Card is represented as simultaneous per-contender draws
with recursive tie narrowing. Paper placement, score-sheet writing, reshuffle
randomness, and indivisible-cent tie remainders remain documentary or physical
concerns, as recorded in the coverage ledger.
