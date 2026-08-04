# Rust and Scryer Prolog conformance

Phase 6.2 compares native relational answers from the handwritten Scryer model
with answers computed independently by the conventional Rust oracle and strict
Rust property catalog. The adapter invokes `run_conformance_fixture/1`; it does
not generate or rewrite Prolog model source.

## Ground common scope

Transition fixtures use two players, first dealer seat 1, the standard ordered
52-card deck, and the first one-card round. That makes the physical deal exact:
seat 0 receives Clubs 2, seat 1 receives Clubs 3, and Clubs 4 is trump.

Prolog exposes `collect` as a separate deterministic micro-step after the second
play. Conventional Rust resolves/captures a complete trick inside that play
transition. The successor adapter composes only Prolog `play -> collect` and
`collect -> settle` at this boundary; every card zone, bid, actor, winner,
score, and pot value on either side remains in the compared state term.

Six tracked manifests under `tests/fixtures/prolog/` declare the native goal,
rule IDs, and productive query mode. Native rows are sorted into sets before
comparison, so Prolog proof order is irrelevant.

| Fixture | Exact rows | Comparison |
| --- | ---: | --- |
| `legal-actions-from-state` | 19 | deal, all bid choices, all canonical one-card plays, settlement |
| `canonical-round-successors` | 15 | every legal decision successor through the one-card round |
| `predecessors-for-action-and-state` | 3 | ground bid/bid/play target-and-action explanations |
| `trick-winner-relations` | 4 | trump, lead, rank, and off-suit eligibility cases |
| `score-causes` | 203 | every `(hand size, bid, tricks)` tuple for hand sizes 1 through 7 |
| `rule-explanations` | 20 | relation/rule identity plus nonempty native explanation keys |

The scoring fixture is an exhaustive finite input comparison of the score
relation, including reverse score causes. It includes misses, successful zero
bids, exact partial bids, all-tricks bonuses, and missed-bid payments.

## Reverse-query and explanation limits

`predecessor/3` is productive here only after both successor and action are
ground. The corpus asks for the deterministic first proof at three checkpoints
using `once/1`. Attempting to exhaust alternative unconstrained previous terms
was deliberately rejected: it expands the term search without strengthening
the bounded game claim. This is a recorded mode/constraint limit, not an empty
answer treated as agreement.

`rule_explanation/3` is a finite fact relation and supports variables in the
relation, rule-ID, or explanation positions. Exact rule identities are compared
with the Rust property catalog. Both sides must carry nonempty explanations,
but independent English/atom wording is not required to be byte-identical.

Raw stdout/stderr, exact commands, versions, and normalized answer rows are
retained under ignored `target/prolog-conformance/<fixture-id>/` directories.
The protocol fails closed on missing markers, malformed counts, duplicate rows,
or unknown protocol lines.

## Reproduction

```pwsh
cargo test -p poche-conformance prolog
cargo run -p poche-xtask -- compare rust prolog --fixtures tests/fixtures/prolog
```

Both commands compare 264 normalized rows with zero unclassified differences.
