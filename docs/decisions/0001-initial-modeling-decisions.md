# Initial modeling decisions

- Status: Accepted
- Date: 2026-08-03
- Scope: Current formal-modeling goal

These decisions close the initial architecture gates. A later decision may
supersede one, but must preserve the original gate and explain the effect on
model identity, evidence, and rule coverage.

| Gate | Decision | Consequence |
| --- | --- | --- |
| G1 | Begin with an explicit typed Rust builder/expression graph. Add macro sugar only after semantics stabilize. | Formal behavior remains visibly graph construction and independently interpretable. |
| G2 | Define a canonical `FiniteDomain` abstraction plus semantic refinement validators. | Encodings are finite and deterministic, and invalid bit patterns are rejected. |
| G3 | Pin published crates.io releases exactly: `facet = 0.50.0-rc.5`, `phon = 0.2.0-rc.5`, and `weavy = 0.2.2`; commit `Cargo.lock`. | Builds do not depend on sibling checkouts or their uncommitted changes. |
| G4 | First exhaustive scope: two players, two suits, three ranks per suit, two cards per player, one trump card, and one undealt card. | Every exhaustive claim names this scope or another explicit finite scope. |
| G5 | `docs/main.typ` states human intent. No computerized model wins a disagreement automatically. | Discrepancies are preserved and classified against stable rule IDs. |
| G6 | Universal termination means every path from every initial state reaches absorbing `Finished`; the first scope permits no stuttering. | Rust SCC analysis and NuSMV check the same liveness semantics. |
| G7 | License repository source and native/formal model artifacts under MPL-2.0. | Root and package metadata agree before implementation source is imported. |
| G8 | Defer legacy-v2 comparison to a later goal. | Legacy structure cannot shape or block the formal core. |
| G9 | A player observes table-public information plus only their private hand; acting-player identity and chance actions are explicit. | Hidden cards cannot leak through the ordinary environment contract. |
| G10 | Defer the RL API. Preserve raw per-player end-of-round score as the intended future intermediary reward signal. | Formal scoring remains authoritative and independent of future reward projections. |
| G11 | A comprehensive track gives every stable full-game rule a modeled, checked/queried, or reasoned-not-applicable disposition. | Full-rule coverage is audited separately from finite-scope exhaustiveness. |
| G12 | Defer automatic Rust/Weavy-to-native-language generation. | Independent handwritten oracles remain the maintained baseline for this goal. |
| G13 | Publish a small HTML landing page and generated PDF first; add native Typst HTML only after a fidelity comparison. | The pretty-view URL remains stable without committing generated output. |
| G14 | After the first branch push, make `model-checking` the repository default and deploy Pages with GitHub Actions through the protected `github-pages` environment. | Repository defaults and documentation deployment share the intended project head. |

## Dependency evidence

On 2026-08-03, `cargo search` and `cargo info` confirmed all three versions above
are published. Each requires Rust 1.92 and is licensed `MIT OR Apache-2.0`;
Poche pins Rust 1.96.0 and remains MPL-2.0. The inspected local Facet repository
was intentionally not used as a dependency because it contained unrelated user
changes.
