# Scryer Prolog oracle

`models/prolog/poche.pl` is a handwritten relational oracle derived directly
from the rulebook and rule ledger. It is independent of both Rust models and is
executed by the installed Scryer Prolog runtime.

The program provides finite relations for the standard deck, all supported
player counts and hand schedules, dealer rotation and score-sheet row values,
clockwise whole deals, legal bids and plays, trick winners, forward and reverse
round scoring, final winners, pot division, First Jack, and repeated-tie High
Card. A full-card-identity two-player one-card round supplies executable
`legal_action/2`, `step/3`, `predecessor/3`, and `replay/3` state queries.

## Native check

```pwsh
cargo run -p poche-xtask -- oracle check prolog
```

The runner discovers `scryer-prolog` through `SCRYER_PROLOG_BIN` or `PATH`,
runs the embedded sixteen-query corpus, requires the
`POCHE_PROLOG_OK tests=16` marker, and preserves the transcript under ignored
`target/prolog-oracle/native.log`.

## Productive query modes

The principal intended modes are:

- `max_hand(+Players, -Maximum)` and `max_hand(-Players, +Maximum)`;
- `hand_schedule(+Players, -Schedule)`;
- `deal_round(+N, +HandSize, +Dealer, +Deck, -Hands, -Trump, -Undealt)`;
- `legal_play(+Hand, +LeadSuit, -Card)`;
- `trick_winner(+TrumpSuit, +ClockwisePlays, -Winner)`;
- `round_score(+HandSize, ?Bid, ?Tricks, ?Points, ?Cell, ?MissDimes)`;
- `legal_action(+State, -Action)` and `step(+State, +Action, -Next)`; and
- `predecessor(+Next, +Action, -Previous)`, which is `step/3` read backward
  after the finite phase and action have been supplied.

For example, the corpus asks which bid/trick pair could produce the paper score
`13` in a four-card hand and obtains only `3-3`. It also replays a complete
round and then recovers the exact states before `bid(0,0)` and
`play(0,card(clubs,2))`.

An entirely unconstrained `step(Previous, Action, Next)` query is intentionally
not a useful mode: it lacks a finite phase/card boundary and can search a vast
term space. Supply a ground state, or a ground bounded successor and action.
No tabling is required for the current acyclic one-round transition corpus.

## Proof boundary

These are executable relational queries, not a universal temporal proof. The
schedule generator covers every player count from 2 through 51, and generic
deal/trick/score relations cover arbitrary valid finite arguments. The state
transition corpus deliberately fixes two players and one card each so forward
and reverse answer sets are finite and reviewable. Full-game liveness belongs
to NuSMV and exhaustive Rust exploration; full-deck structural provenance also
has Alloy evidence.

First Jack preserves seat-ordered, without-replacement draw support but assigns
no probability. High Card deals one card to every contender, narrows only tied
maximum ranks, enforces distinct cards, and is insensitive to play-list order.
The returned selection cards are represented as restoring the original deck
multiset before the physical reshuffle.
