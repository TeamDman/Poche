# Exhaustive safety and injected-defect evidence

Status: Phase 5.2 complete (2026-08-03).

`poche-check::check_safety_catalog` evaluates the strict model's catalog in
deterministic graph order. It does not stop at the scripted examples used while
authoring the model.

## Complete reachable obligations

The following state-local obligations are evaluated on all 431,800 reachable
states in `micro-2p-2s-3r-6c-schedule-1-2-1`:

- card partition/conservation;
- legal action owner and nonempty owner-specific action set;
- viewer-private observation projection;
- trick credit/captured-card conservation;
- the local no-deadlock successor obligation;
- terminal absorption; and
- final maximum-score winner semantics.

The following transition obligations are evaluated on all 549,896 reachable
labeled edges (the semantic transition is replayed, not trusted from an edge
cache):

- announced bids remain fixed;
- follow-suit legality;
- trick winner credit;
- winner leads the next trick;
- score/payment events;
- nonterminal progress/no stuttering; and
- the unique terminal absorbing action.

This accounts for every catalog property except the graph-temporal
`universal-termination` SCC obligation. `deadlock-freedom` is checked locally
here; Phase 5.3 records the graph-level deadlock and SCC result. A failed state
or edge returns the deterministic first property, its rule origins, and a
shortest replay prefix through the exact violating edge.

## Controlled defects

The checker searches the reachable graph for a state that distinguishes each
fault, rather than asserting that an arbitrary broken implementation failed:

| Defect | Typed projection diff | Discriminating fact |
|---|---|---|
| `FollowSuitAllowsAnyCard` | `LegalActions` | The follower holds both lead-suit and off-suit cards; only the lead-suit play is legal. |
| `TrickWinnerUsesRankOnly` | `Transition` | A lower-ranked trump beats a higher-ranked non-trump lead. |
| `ObservationLeaksOpponentHand` | `Observation` | A viewer's two cards differ from the mutant union of both private hands. |
| `AllTricksUsesPartialScore` | `RoundScore` | Bid two/take two scores 22, not the partial-exact 12. |

Each witness is translated into `EvidenceBundleWire`, including the complete
G4 checkpoint state, both correct viewer observations, the correct legal action
set, an applicable transition with semantic state diffs, a typed
expected/actual projection diff, and rule/source origins. Current-round play
history is reconstructed from the BFS predecessor trace so completed trick
order and player/card provenance are not invented from captured sets.

The bundle is semantically validated, encoded with Phon, decoded, compared for
exact roundtrip equality, and validated again. The interchange schema now has a
general `ProjectionDiffWire` for state, observation, legal-action, transition,
round-score, and game-outcome disagreements.

Run the Phase 5.2 gates with:

```pwsh
cargo test -p poche-check safety_catalog
cargo test -p poche-check injected_defects
```
