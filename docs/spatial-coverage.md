# Spatial refinement coverage

This ledger classifies every phase-three spatial obligation independently for
Rust, Alloy, NuSMV, and Scryer Prolog. An applicable cell names that track's
native evidence kind. `N/A` is an explicit scope exclusion, not evidence from a
different backend. Cross-track comparison occurs only where at least two
applicable cells express the same discrete claim. No backend is an oracle for
another.

The scopes are deliberately different: Rust samples all registered 2-8 player
layouts plus complete seeded two-player traces; Alloy bounds a static two-player
layout; NuSMV symbolically explores a two-card transition machine; Prolog
queries a finite ground corpus. Renderer pixels, continuous geometry, mesh
collision, physics, and interpolation frames are outside every formal claim.

| Obligation | Meaning | Rust | Alloy | NuSMV | Prolog |
| --- | --- | --- | --- | --- | --- |
| `P3-S-IDENTITY` | Stable card/object/viewer identities do not alias. | sampled: registered layouts and traces | bounded: distinct card and slot atoms | symbolic: two stable card and viewer identities | queried: ground object and viewer identities |
| `P3-S-CONSERVATION` | The complete typed 52-card universe is realized exactly once. | sampled: every realized complete-deck viewer scene | N/A: eight-card bound is not the complete deck | N/A: two identity-only cards omit the deck | N/A: eight-card query corpus omits the deck |
| `P3-S-ZONE-EXCLUSIVITY` | Canonical endpoints classify into one semantic zone. | sampled: all registered layout endpoints plus injected overlap fault | bounded: exclusive finite coordinate cells and overlap counterexample | N/A: endpoint labels abstract coordinates | N/A: corpus reports findings but does not exhaust placements |
| `P3-S-AMBIGUITY` | Ambiguous spatial input is rejected and explained. | sampled: injected equal-distance placement | bounded: multi-zone relation and faulty first-choice counterexample | N/A: classifier ambiguity is outside transition abstraction | queried: ambiguous drop returns competing candidates |
| `P3-S-ATTACHMENT` | Explicit attachment identity wins over geometric proximity. | sampled: attached semantic text and injected wrong-owner fault | bounded: attachment ownership assertion | N/A: text/surface attachment omitted | queried: attachment explanations include exact owner |
| `P3-S-PRIVACY` | A viewer scene contains only authorized hidden card faces. | sampled: every viewer pair across supported layouts | bounded: viewer/face knowledge relation | symbolic: cross-owner and negative-control traces | queried: visibility explanations per viewer |
| `P3-S-ROUNDTRIP` | Abstracting a canonical realization recovers typed zones. | sampled: every registered canonical viewer scene | bounded: realization/abstraction relation | N/A: coordinates and classifier omitted | N/A: corpus answers individual resolutions, not all realizations |
| `P3-S-PLAY-EQUIVALENCE` | Typed `/play-card` and drag intent resolve to one semantic play. | sampled: paired endpoint transitions | N/A: static model has no input path | N/A: input syntax and drag resolution omitted | queried: paired command/drop answers agree |
| `P3-S-ONE-CARD-MOVE` | A play changes exactly the selected card's semantic endpoint. | sampled: all checked play transitions | N/A: temporal mutation omitted | symbolic: one-card endpoint transition invariant | N/A: named explanations are not an exhaustive transition theorem |
| `P3-S-PAUSE-IMMOBILE` | Pause cannot advance semantic card endpoints. | N/A: current spatial trace scope omits session phase | N/A: static model omits pause | symbolic: paused transition invariant | N/A: explanatory fact is not structurally linked to endpoints |
| `P3-S-RECOVERY-IMMOBILE` | Governance recovery cannot partially move cards. | N/A: current spatial trace scope omits governance | N/A: static model omits recovery | symbolic: recovery transition invariant | N/A: explanatory fact is not structurally linked to endpoints |
| `P3-S-TRANSIT-PRESENTATION` | Transit is presentation; committed semantics stay at endpoints. | sampled: replay/tween endpoint commutation | N/A: static model omits transit | symbolic: transit preserves semantic endpoint | N/A: continuous/transit semantics are excluded |
| `P3-S-FAIR-CONVERGENCE` | Animation converges only under the named scheduler assumption. | N/A: sampled endpoint tests do not prove liveness | N/A: static model has no liveness | symbolic: conditional `fair_animation` liveness | N/A: query corpus has no scheduler |
| `P3-S-PREDECESSOR` | Queries recover actions that could precede an observed state. | N/A: forward refinement gate has no reverse query | N/A: static model has no action history | N/A: model checks paths but exposes no relational predecessor API | queried: reversible transition relation and explanations |
| `P3-S-CONTINUOUS-RENDERER` | Pixel, mesh, collision, and continuous-frame fidelity. | N/A: exact semantic AABBs/endpoints only | N/A: finite coordinate cells only | N/A: finite endpoint labels only | N/A: finite ground relations only |

The machine audit verifies all 15 rows, all 60 track cells, their evidence-kind
prefixes, and exact agreement between applicable cells and the claims registered
by each source gate. The comparison command runs all four native tools first,
retains every native scope and exclusion, reports single-track claims separately,
and returns contradictions as data instead of selecting a preferred backend.
