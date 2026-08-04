# Strict-model property catalog

The executable authority is `poche_model::property_catalog()`. Every declaration
contains stable identity, rules/source anchors, an inspectable expression, its
proof class, verification strategy, and equivalent native checks.

| ID | Class | Expression | First-scope strategy |
| --- | --- | --- | --- |
| `card-conservation` | reachable state | `state_card_partition` | exhaustive state predicate |
| `legal-actor` | reachable state | `state_legal_actor_and_actions` | exhaustive state predicate |
| `observation-confidentiality` | structural | viewer-only `Observation` shape | type/refinement proof |
| `fixed-bids` | transition | `transition_preserves_announced_bids` | exhaustive transition predicate |
| `follow-suit` | transition | `FollowSuitLegal` Weavy kernel | finite kernel and transition exhaustion |
| `trick-winner` | transition | `SecondCardWins` Weavy kernel | finite kernel and transition exhaustion |
| `winner-leads-next-trick` | transition | `transition_winner_becomes_leader` | exhaustive transition predicate |
| `trick-count-conservation` | reachable state | captured cards versus credits | exhaustive state predicate |
| `scoring-and-pot` | transition | `RoundScore` Weavy kernel | finite kernel and transition exhaustion |
| `phase-progress` | transition | nonterminal successor differs | exhaustive transition predicate |
| `deadlock-freedom` | temporal | `AG EX TRUE` with terminal-only absorb | deadlock search |
| `universal-termination` | temporal | `AF Finished` without fairness | nonterminal SCC/lasso search |
| `finished-absorbing` | transition | terminal-only self-loop | exhaustive successor check |
| `final-winner-semantics` | reachable state | `WinnerMask` Weavy kernel | finite kernel and reachable-state check |

Structural claims are not presented as state-space discoveries. Conversely,
`universal-termination` is not treated as proven merely because example games
finish: Task 5.3 must establish the SCC obligation for the named scope.
