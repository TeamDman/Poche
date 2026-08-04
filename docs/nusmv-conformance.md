# Rust and NuSMV temporal conformance

Phase 6.4 compares Rust's exhaustive explicit micro graph with the separately
handwritten `models/nusmv/conformance.smv` temporal projection. The projection
uses exactly two players and the strict model's three-round `1,2,1` schedule.
It is not generated from Rust and does not replace the independent full
`models/nusmv/poche.smv` oracle.

## Common temporal projection

The conformance model retains dealer rotation, round and hand size, both hand
counts, trick count, leader, and seven phase refinements:

`awaiting_deal -> bid_first -> bid_dealer -> play_lead -> play_follow`

The follower transition returns to `play_lead` or enters `scoring`; settlement
enters the next `awaiting_deal` state or the absorbing `finished` state. A
derived progress rank is exactly 20 in either prepared initial state, decreases
by one on every correct nonterminal transition, and is zero in `finished`.

NuSMV checks 23 named properties:

- 12 exact round/phase one-step obligations, including terminal absorption;
- three correct-mode state invariants;
- correct-mode deadlock freedom, CTL universal termination, LTL universal
  termination, and terminal reachability;
- two intentionally false initial-state formulae that expose dealer `p0` and
  dealer `p1` as native one-state witness traces; and
- two intentionally false defect properties for stuttering and deadlock.

The 19 correct-mode properties must be true. The four witness/defect properties
must be false and must each carry a native counterexample. The runner calls
`show_property` so every normalized result is joined to its stable source
`NAME`; result order or an unnamed property cannot silently pass.

Rust exhaustively projects all 431,800 reachable states and 549,896 transitions.
Its edges contain exactly nine coarse phase pairs, and every nonterminal edge
decreases the same rank. Both prepared dealers are compared exactly against the
native initial witness states.

## Matching temporal defects

The source has three frozen modes. Conditional properties isolate correct
semantics from two controlled mutations:

- `stutter` adds a nondeterministic self-loop only to a prepared first-round
  state. NuSMV refutes `AF finished` with a two-state loop at rank 20. Rust adds
  the same edge to the matching initial state and returns the same normalized
  two-state lasso.
- `deadlock` removes all successors from the first `play_follow` state. NuSMV's
  `check_fsm` reports the transition relation non-total and supplies the
  deadlock assignment; a deliberately false invariant supplies the five-state
  path `awaiting_deal, bid_first, bid_dealer, play_lead, play_follow`. Rust
  removes successors from the state with that same projection and returns the
  identical prefix plus that exact nonterminal deadlock.

NuSMV's CTL evaluation of `AG EX TRUE` is retained for the isolated correct
mode, but the mixed-mode defect claim relies on `check_fsm`, the native command
specifically responsible for transition totality and deadlock diagnostics.
This distinction is recorded rather than treating a vacuous/engine-specific
CTL interpretation as deadlock evidence.

## Trace normalization and evidence

Native traces print only changed variables after their first state. The runner
materializes inherited values into every normalized state, records loop-start
indices, associates traces with named false properties, and fails closed when a
false property lacks a trace. It also parses the named property catalog and
`check_fsm` assignment separately.

The ignored `target/nusmv-conformance/` directory retains the exact command
script, executable/version/exit status, raw stdout/stderr,
`normalized-results.txt`, and `normalized-counterexamples.txt`.

## Scope boundary

This temporal comparison deliberately projects card zones to counts. Rust's
micro graph still retains all six card identities, legal actions, scoring, and
observations. The full NuSMV oracle separately checks the complete two-player
`1..7..1` schedule with 52-card conservation by counts; Alloy and Rust retain
full-deck identity. Agreement here is exhaustive for the shared temporal micro
projection, not a claim that these internal representations are identical.

## Reproduction

```pwsh
cargo test -p poche-conformance nusmv
cargo run -p poche-xtask -- compare rust nusmv --scope micro
```

Both commands require NuSMV 2.7.1 on `PATH` or in `NUSMV_BIN`.
