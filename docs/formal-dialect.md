# Pure formal expression dialect

`poche-formal` is the deliberately small executable boundary between ordinary
Rust and portable formal computation. It uses Weavy as a lowered-program carrier
and runner, but defines its own finite Poche-oriented type and instruction
vocabulary.

Accepted computations contain only:

- typed constants and named typed inputs;
- finite records, enums, and fixed arrays;
- equality and finite integer/enum ordering;
- Boolean operations and pure conditionals;
- statically in-bounds fixed indexing; and
- explicitly finite `all`, `any`, and bounded-integer `sum` folds.

The builder attaches a rule ID, source location, and one of `State`,
`Observation`, `LegalAction`, `Transition`, `Scoring`, or `Property` to every
node. Lowering preserves those origins on every Weavy instruction, and runtime
diagnostics add the exact instruction offset.

There is intentionally no escape hatch for an arbitrary Rust callback. Opaque
host calls, effects, dynamic allocation, unbounded iteration, hidden randomness,
and unsupported arithmetic are rejected. Chance is data supplied through an
explicit finite input; it is never an implicit RNG operation. Fixed folds are
unrolled into a finite operand list, which lets later backends inspect the whole
computation graph.

This crate supplies the reusable expression mechanism. The phase-specific Poche
state machine and its complete rule computations are introduced by the strict
model tasks in Phase 4 of `PLAN.md`.
