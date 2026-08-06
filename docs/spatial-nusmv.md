# Spatial NuSMV oracle

`models/nusmv/spatial.smv` is a handwritten symbolic transition model for the
presentation/refinement boundary of `poche-spatial-v1`. Run it through the
strict normalized gate with:

```pwsh
cargo run -p poche-xtask --offline -- spatial nusmv --scope transition-micro
```

The exact scope ID is
`transition-micro-2cards-2viewers-endpoints-transit-pause-recovery`. It has two
stable card identities, two owners/viewers, per-card hand/play/won endpoints,
optional finite presentation transit, and running/paused/recovery/finished
phases. It abstracts card faces to cross-viewer knowledge booleans and one
logical command per transition. It does not model all 52 identities, arbitrary
coordinates, renderer frames, clocks, network delivery, or the full Poche
round lifecycle.

Canonical and presentation state are deliberately distinct:

- `committed0/committed1` contain only typed endpoint zones and are the state
  that rules, replay, and consensus would use;
- `present0/present1` may briefly contain `transit0/transit1`; and
- a legal play updates the committed endpoint immediately, while presentation
  converges independently. Transit cannot become game authority.

The suite checks four invariants and six CTL/LTL properties:

- each card remains in its owner's endpoint family;
- a private hand face is not known by the other viewer;
- transit refines an already committed play endpoint;
- one legal play moves exactly one card;
- paused and recovery phases preserve committed card state;
- the full mixed-mode transition relation is total and deadlock-free; and
- presentation converges in both CTL and LTL under the named
  `fair_animation` scheduling mode.

There is no global NuSMV `FAIRNESS` declaration. `fair_animation` is an
explicit deterministic environment assumption: it schedules one play, one
transit state, and then endpoint service. The controlled `stuck_animation`
mode removes exactly that service obligation and must produce a three-state
lasso. This makes the liveness conclusion conditional and inspectable.

Two further false properties must retain two-state counterexamples: player
zero moves player one's card into player zero's play endpoint, and viewer zero
learns player one's still-private face. The Rust runner validates the decisive
assignments in all three traces, rather than accepting any false result.

Raw output, normalized named properties, carried-forward counterexample states,
loop indices, and `check_fsm` diagnostics are written under the ignored
`target/spatial-nusmv-transition-micro/` directory. These are symbolic results
over the declared finite abstraction, not proof about transport scheduling or
continuous rendering.
