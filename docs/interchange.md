# Phon interchange and evidence identity

`poche-interchange` is the common transport boundary for handwritten native
oracles, the strict Rust model, explicit-state checking, and conformance tools.
Facet describes each Rust wire shape and Phon encodes it. A successful Phon
decode establishes memory/schema shape only; `ValidatedEvidence::try_from` must
then re-establish Poche's semantic refinements.

The root payload pairs a model-neutral fixture with a backend result. It includes
schemas for:

- model/rules/schema/scoring/observation identities and semantic hashes;
- named finite scopes, backend versions, subjects, rule origins, and confidence;
- complete states, per-player observations, player and chance actions;
- raw round scores, money outcomes, transitions, state diffs, and traces;
- typed state/observation/action/score projection diffs on counterexamples;
- Prolog bindings, normalized solver status, statistics, and raw diagnostics.

Cross-model comparison intentionally permits the fixture and result to name
different model families, backend versions, and confidence kinds. It requires
their schema, rule, scope, subject, scoring, observation, and rule-origin
identities to agree. This is what prevents an Alloy bound, a NuSMV symbolic run,
an exhaustive Rust micro-scope, and a sampled full-deck run from being confused.

Semantic validation also checks the finite player/deck partition, card
uniqueness, phase/actor ownership, bids, trick shapes/counts, observation
projection, action ownership, chance permutations, trace continuity, winner ties,
pot division, and rule-originated diffs. Native adapters can therefore consume a
fixture from bytes without linking to a Rust oracle or trusting the producer.
