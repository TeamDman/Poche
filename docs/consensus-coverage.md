# Replicated consensus formal evidence and coverage

The `consensus-micro` comparison runs four independently authored sources
before constructing neutral agreement evidence. Rust is not the oracle for the
other tracks, and no track inherits a claim it did not actually check.

Run both gates with:

```pwsh
cargo run -p poche-xtask --offline -- consensus compare all --scope micro
cargo run -p poche-xtask --offline -- consensus coverage audit --all
```

The coverage audit requires every cell below to begin with the track's native
evidence kind or with a reasoned `N/A:`. It cross-checks applicable cells against
the exact machine-emitted claim inventory.

| Obligation | Meaning | Rust explicit/runtime | Alloy | NuSMV | Scryer Prolog |
| --- | --- | --- | --- | --- | --- |
| `P3-C-DEVICE-WEIGHT` | Multiple devices never create extra player votes. | sampled: five-device runtime plus player-deduplicated admission | bounded: three players and five devices | symbolic: device/player signer counters | queried: duplicate Alice device vote explanation |
| `P3-C-QUORUM` | Only strict player majority certifies an event. | sampled: micro denial plus exhaustive subset intersection for 2–8 players | bounded: valid-certificate relation and intersection assertion | symbolic: commit requires epoch quorum | queried: committed and denied certificate explanations |
| `P3-C-STALE-REVOKED` | Inactive, stale, and revoked devices cannot contribute current authority. | sampled: exact stale and revoked denials | bounded: per-epoch owner/activity/validity assertion | symbolic: separate stale and revoked scheduler modes | queried: stable denial reasons for both attempts |
| `P3-C-CANONICAL-BATCH` | Arrival order and duplicate semantics cannot multiply or conflict effects. | sampled: reversed arrival and transition-key collapse | bounded: one command hash per key | symbolic: three proposals yield at most one effect | queried: accepted/superseded proposal explanations |
| `P3-C-FORK-SAFETY` | Conflicting commits are excluded only under non-equivocation; removal retains a witness. | sampled: verified fork halt and retained two-majority witness | bounded: majority intersection plus equivocation witness | symbolic: safe invariant plus expected-false defect mode | queried: retained/removed assumption explanations |
| `P3-C-RECOVERY` | Governance can kick the current actor and change membership out of turn. | sampled: joint-quorum current-actor kick | bounded: Alice/Bob kick witness excluding Carol | symbolic: reachable committed kick while Carol is actor | queried: three-step recovery explanation |
| `P3-C-CONDITIONAL-LIVENESS` | Eventual delivery and connected quorum imply progress; unfair delivery does not. | sampled: partition/reconnect convergence with named assumptions | N/A: static relational model has no delivery trace | symbolic: conditional AF convergence and expected-false unfair scheduler | queried: eventual-delivery removal counterexample explanation |
| `P3-C-SNAPSHOT-TAIL` | An already-certified snapshot plus canonical tail reaches the same head. | sampled: browser height-one snapshot plus one tail event | N/A: snapshot payload/log prefix excluded from bounded relation | N/A: scheduler model abstracts snapshot installation | N/A: explanation corpus does not reconstruct snapshot bytes |
| `P3-C-LOCAL-PROPOSAL` | Device-local automation is proposal input, never hidden shared mutation. | sampled: three device proposals collapse before one certified event | bounded: three-proposal common-key witness | symbolic: proposals/effect counters remain distinct | queried: each local proposal has accepted/superseded evidence |
| `P3-C-TWO-PLAYER-LIMIT` | A two-player epoch needs both players, so a 1–1 split cannot safely progress. | sampled: minority denial and quorum calculation | bounded: E2 certificate requires Alice and Bob | symbolic: epoch-two quorum is two | queried: stalled partition explanation |

## Native scopes

- Rust executes
  `replicated-micro-3players-5devices-4events-majority-partition-snapshot-tail`
  and exhaustively enumerates strict-majority subset intersection for two
  through eight players. The delivery transcript is a deterministic sample,
  not exhaustive over networks or schedules.
- Alloy checks three players, five fixed devices, two epochs, two values,
  proposal batches, and 5-bit integers. Its nine commands include satisfiable
  recovery/automatic/equivocation witnesses and six unsatisfiable assertion
  searches. It has no temporal fairness or snapshot-payload model.
- NuSMV symbolically explores ten finite scheduler modes over five steps and
  four replica heights. Twelve named properties include two expected-false
  counterexamples: equivocation breaks fork safety and unfair delivery breaks
  liveness. This is not an unbounded network proof.
- Scryer Prolog returns 34 normalized, finite ground explanation rows. Reverse
  predecessor queries explain which votes/proposals can lead to commits. It is
  queried evidence, not a temporal or cryptographic proof.

## Claim boundary

“Fork safety” always means safety under authenticated player votes and
non-equivocation. With three players, two strict majorities intersect, but that
intersection alone does not stop the shared player from signing two values.
The implementation records signed fork evidence and halts; it does not pretend
to resolve Byzantine disagreement. “Conditional liveness” additionally needs
eventual delivery, a connected strict majority, and eventually responsive
proposers. In a two-player room one disconnected player removes the only safe
quorum. None of these models claims Sybil resistance, asynchronous Byzantine
consensus, real network availability, or cryptographic security beyond the
separate signed-vector/session-verifier evidence.
