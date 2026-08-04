# Conventional/strict Rust conformance

Status: Phase 4.4 complete (2026-08-03).

The conventional Rust oracle and the strict Facet/Weavy model are independent
implementations with deliberately different finite universes. Conformance does
not assume that the newer or stricter implementation wins a disagreement.
`poche-conformance` instead compares a declared common projection and returns an
error for any mismatch that is not represented by a named difference class.

## Exact shared projection

The strict six-card deck is embedded into the conventional 52-card deck as
Clubs/Diamonds and ranks Two/Three/Four. Cards outside the embedding stay in the
conventional stock and cannot affect the compared first two rounds. Both models
therefore execute the same two-player hand-size prefix `1, 2` with the same:

- dealer, actor, phase, round ordinal, and hand size;
- viewer-private hand, public counts, trump, current trick, bids, trick counts,
  cumulative scores, and communal pot;
- legal bid/play action set before every player choice;
- player/chance/environment transition outcome; and
- raw round-score category, points, and missed-bid payment.

The executable prefix compares 28 viewer observations, 10 player legal-action
sets, and 14 transitions. It discriminates exact zero bids, all-tricks scores,
partial exact scores, missed bids/payments, mandatory follow-suit, void trump
play, an ineligible off-suit card, winner-led continuation, dealer rotation,
and cumulative score/pot updates.

Both models are also run independently through their complete schedules. The
harness checks that each finishes after a normally scored one-card round, marks
exactly all maximum-score seats as winners, and divides the separate communal
pot arithmetically (including any remainder).

## Classified differences

| Class | Fixtures | Rules | Reason |
|---|---|---|---|
| `ScheduleScope` | `schedule-boundaries` | R-GAME-001, R-HAND-001..004 | The named strict scope is `1,2,1`; the full two-player rulebook schedule is `1..7..1`. The exact comparison stops after the shared `1,2` prefix. |
| `CardUniverseScope` | `full-deck-conservation` | R-GAME-002, R-DEAL-004, R-TRICK-012, R-ADVANCE-001 | Each model conserves its declared universe, but six abstract cards are not literally the standard 52-card deck. |
| `PreparedDealerBoundary` | `first-jack-seat-order`, `high-card-repeated-tie` | R-RANDOM-002..006 | The strict transition model begins after physical first-dealer selection and accepts the selected dealer explicitly. The conventional oracle retains both selection procedures. |

These are modeling boundaries, not silent passes. Expanding the strict scope or
moving setup selection into its transition graph must retire or narrow the
corresponding classification.

## Inventory coverage

Every one of the 15 `[[scenario]]` entries in
`fixtures/oracle-inventory.toml` has a conformance result. The cross-Rust
property `private-observation-does-not-leak` and query
`legal-actions-from-state` match; `full-deck-conservation` carries the declared
card-universe difference. Inventory entries assigned only to NuSMV, Prolog, or
Phase 5 are not relabeled as Rust evidence.

Run the gate with:

```pwsh
cargo test -p poche-conformance rust_models
cargo run -p poche-xtask -- compare rust-oracle rust-formal
```
