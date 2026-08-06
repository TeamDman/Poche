# Spatial Scryer Prolog oracle

`models/prolog/spatial.pl` is a handwritten finite relational oracle for
`poche-spatial-v1` query and explanation roles. Run the complete normalized
corpus with:

```pwsh
cargo run -p poche-xtask --offline -- spatial prolog --scope query-micro
```

The exact scope ID is
`query-micro-2p-3v-8cards-8drops-4layouts-7transitions`. It contains two
players, one spectator, eight named cards/locations, eight classified drop
cases, four layout findings, and seven named committed-state transitions.
Every query is finite and grounded by an explicit corpus relation.

The seven answer sets cover:

- card identity, location, slot, and authority classification;
- viewer-specific face text plus public name/score-sheet attachments;
- named and drag command resolution to the same `play(c0)` intent;
- allowed play, wrong zone, dead band, ambiguous Deck/Trump, free,
  out-of-bounds, unauthorized, and unknown-object drop explanations;
- a valid layout and overlap, duplicate-seat, and missing-zone findings;
- all named committed-state successors; and
- the exact reverse predecessor relation, including pause/recovery paths.

Every decision/finding/transition row carries a stable `because/2` term. The
ambiguous result retains both implicated zones and never chooses a first match.
The attachment gate also rejects answer sets that expose player one's hand to
player zero or expose either deck card as face text.

The Rust runner invokes only lowercase ground fixture atoms through the
existing fail-closed Scryer protocol. It sorts and deduplicates every answer
set, checks exact counts and essential witnesses, and emits length-framed
BLAKE3 digests per fixture and for the corpus. The current receipt is 63 rows,
55 explanation rows, and corpus digest
`blake3:5dbf46198a011fe7815f1ea92c7378b925d088d9a7a8d3c189d504290dc2aa54`.
Raw and normalized evidence stays under ignored
`target/prolog-conformance/spatial-*/` directories.

This model intentionally uses unification, finite enumeration, negation-free
positive query rules, and reverse use of `spatial_step/5`. It does not claim
arbitrary real arithmetic, nonlinear constraints, collision/mesh solving,
continuous motion, or completeness beyond the named finite corpus. Those are
not silently delegated to a Rust callback.
