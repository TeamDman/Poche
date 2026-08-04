# Deadlocks and universal termination

Status: Phase 5.3 complete (2026-08-03).

The liveness result is for the exact scope
`micro-2p-2s-3r-6c-schedule-1-2-1`, from both prepared first dealers and without
fairness assumptions. It is derived from the raw, unreduced reachable graph.

## Graph result

The checker builds compressed sparse-row adjacency for all 431,800 states and
549,896 edges, then runs Tarjan SCC decomposition. It separately audits
nonterminal out-degree and the terminal action surface.

| Measurement | Result |
|---|---:|
| Nonterminal deadlocks | 0 |
| Malformed terminal edge sets | 0 |
| Strongly connected components | 431,800 |
| Cyclic components | 176 |
| Cyclic components containing a nonterminal state | 0 |
| Progress-rank violations | 0 |
| Maximum progress rank | 20 |

Every component is a singleton. The 176 cyclic components are exactly the 176
reachable `Finished` states, each with its semantic `Absorb` self-loop. Because
the finite reachable graph is total and has no nonterminal cycle, every path
from every prepared initial state must eventually enter `Finished`. No scheduler
or chance fairness is needed.

## Independent countdown rank

The phase-specific rank is the exact number of actions remaining on every
continuation:

- a round contributes one deal, two bids, two plays per card in each hand, and
  one settlement;
- `Bidding` subtracts bids already announced;
- `Playing` uses the cards still present in both hands plus settlement;
- `Scoring` has one remaining settlement action;
- future rounds contribute six, eight, or six actions for schedule `1,2,1`;
  and
- `Finished` has rank zero and may retain only its rank-preserving absorb edge.

Thus either initial dealer has rank 20, every nonterminal edge decreases the
rank by exactly one, and the maximum BFS depth is also 20. This rank is checked
on every edge; it corroborates rather than replaces the SCC proof.

## Lasso discrimination

For any nonterminal cyclic SCC, the checker reconstructs a shortest prefix to a
cycle entry and a closed state/edge cycle. Real edges retain their semantic
actions; controlled injected edges are explicitly actionless rather than
misrepresented as legal game actions.

Injecting a self-loop at the first prepared initial state produces:

- a depth-zero prefix;
- the closed cycle `[initial, initial]`;
- one actionless injected temporal edge;
- one nonterminal cyclic SCC; and
- one exact progress-rank violation.

The controlled defect therefore refutes universal termination with a replayable
prefix-plus-cycle lasso.

Run the Phase 5.3 gates with:

```pwsh
cargo test -p poche-check liveness_small_graphs
cargo test -p poche-check nonterminating_lasso
cargo run -p poche-xtask -- check rust-explicit --property game-terminates
```
