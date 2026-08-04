# Strict Facet/Weavy Rust model

`poche-model` is an independent finite implementation of the complete
three-round game induced by the six-card micro-deck: two seats, two unordered
suits, three ordered ranks, and hand schedule `1, 2, 1`.

The state is a phase enum whose payload changes with the rules:

- `AwaitingDeal` owns only the ledger;
- `Bidding` owns a validated card partition and exact two-step bid progress;
- `Playing` owns fixed hands, one of two trick-progress shapes, captured sets,
  fixed bids, and trick counts;
- `Scoring` structurally has no hands or partial trick; and
- `Finished` owns only final scores, pot, and the complete maximum-score tie mask.

Hands and card zones are six-bit `CardSet` values, seats and rounds are enums,
and every semantically stored collection has fixed cardinality. The only `Vec`
used by the game interface is a transient enumeration of legal choices. One-card
and two-card chance inputs are different refined variants: there are exactly 120
and 180 partitions respectively. This avoids accidentally trimming a two-card
partition in a way that excludes valid one-card deals.

The conventional Rust oracle is not a dependency. Core scalar decisions are
authored as cached pure programs in the restricted `poche-formal` dialect and
executed by Weavy:

- bid bounds;
- follow-suit legality;
- trump/lead/rank trick winner;
- round outcome, points, and missed-bid payment;
- final-round detection; and
- maximum-score winner masks.

Card movement is expressed by refined fixed-set operations and phase payload
construction rather than an opaque callback. Every transition validates card
partition, bid, capture, trick-count, score, pot, and winner invariants. Rule and
Typst origins are retained on both decisions and semantic diffs. `Finished` has
the sole total-transition self-loop; every nonterminal action must make progress.

The first dealer is an explicit prepared-state input. The physical First Jack or
High Card selection procedure remains comprehensively represented by the native
oracles; it is intentionally not hidden as RNG state in this finite gameplay
scope.
