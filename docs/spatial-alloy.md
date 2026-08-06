# Spatial Alloy oracle

`models/alloy/spatial.als` is a handwritten, independent bounded model of the
`poche-spatial-v1` refinement contract. It is not generated from the Rust
types. Run its normalized evidence gate with:

```pwsh
cargo run -p poche-xtask --offline -- spatial alloy --scope layout-micro
```

The `layout-micro` scope is
`layout-micro-2p-3v-8c-7z-7cells-8slots-int5`:

- two players, two seats, and one spectator;
- deck, trump, play, two hand, and two won zones;
- eight cards/faces/slots and seven distinct coordinate cells;
- a 5-bit Alloy integer domain (`-16..=15`), used for finite cell coordinates,
  slot indices, and score cells; and
- one typed viewer projection and one realized spatial scene for the positive
  commands.

Cells are a finite relational abstraction of integer-labelled positions. They
let Alloy check nesting, pairwise zone separation, large-drop ambiguity, and
slot identity. They do not claim to prove arbitrary continuous AABB, OBB,
floating-point, mesh, renderer, or physics behavior. Rust separately checks
the exact millimetre layouts and all 52 card objects.

The suite requires one SAT canonical realization and seven UNSAT bounded
assertions covering:

- injective seats and owned hand/won zones;
- pairwise zone separation;
- unique card locations/slots;
- total semantic face and score-text attachment;
- deck/public/own-hand viewer knowledge; and
- typed projection → spatial realization → typed abstraction agreement.

Three deliberately false assertions must be SAT. Their retained Alloy
counterexamples show:

- a merely shape-complete layout can overlap zones;
- a broad drop can touch multiple separated zones, so choosing the first match
  is unsound; and
- face text attached to a different object is not a valid scene.

The runner fails closed on missing/extra commands, changed polarity, absent
scope text, or a SAT witness without its requested instance. Raw stdout/stderr,
normalized results, exact command sources, and Alloy's full `receipt.json`
instances are written to the ignored directory
`target/spatial-alloy-layout-micro/`. A green result is bounded evidence only;
it is not an unbounded proof.
