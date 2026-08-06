<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Phase 3 release evidence

Phase 3 turns the earlier typed Poche engine into a checked spatial tabletop
slice and adds explicit governance, replicated-device, gateway, and hidden-card
research tracks. It does not replace the canonical game reducer with geometry,
an ECS, a transport, a vote, or a cryptographic transcript.

## Strength ladder

| Layer | Current result | Boundary |
| --- | --- | --- |
| Typed Poche and session reducers | Canonical executable state and transition authority | Invalid inputs, permissions, card conservation, and history integrity remain hard checks. |
| Spatial refinement | Exact-recipient state realizes to fixed integer objects/zones and classified input resolves back to typed intent | Geometry is checked evidence and interaction vocabulary, not a second game truth. |
| Native and HTML views | Bevy/Slug and ordinary semantic HTML render the same scene fingerprint | ECS transforms, text glyphs, and DOM positions cannot mutate canonical state. |
| Legality and governance | Rooms may prevent an illegal attempt, retain it for retrospective audit, or propose a typed remedy | A vote may change scores/rights/recovery but cannot create cards, rewrite history, or forge principals. |
| Replicated room experiment | Certified multi-device events converge under a named majority/non-equivocation model | This is deterministic in-process/formal evidence, not a deployed Byzantine-consensus network. |
| Hidden-card experiment | A bounded three-player full-deck shuffle/deal/play/reveal corpus is publicly verifiable | The prototype requires unanimous shares, has no same-hand dropout recovery, pins an unaudited library, and requires independent security review. |

## Current acceptance snapshot

- Rust, Alloy 6.2.0, NuSMV 2.7.1, and Scryer Prolog agree on every registered
  shared game, session, governance, spatial, and consensus claim; model-specific
  facts remain explicitly classified.
- The checked vertical slice contains 185 records and preserves scene hash
  `0a6de46fd21791260bb57c8516ce9ef5e1666e03e17d29cf6f8cda746c07baee`
  across native, HTML, and publication evidence.
- Typed and dragged `4♣` play reach the same typed action and exact endpoint;
  the native fixture exposes three authorized card faces and keeps 49 hidden.
- The in-process lifecycle consumes 178 inputs over all 13 deals. Five
  cryptographic dropout points enter immediate governable recovery without
  waiting for the missing actor.
- `poche-2p-v1` retains `round-score-v1`; the fresh network-free release sample
  and Burn CPU/WGPU smoke are empirical performance/learning evidence only.
- Locked dependency metadata contains zero missing license fields and zero
  external local paths. `cargo-deny` and `cargo-audit` were unavailable, so no
  advisory-database or vulnerability-free claim is made.

The complete machine-readable receipt is
[`evidence/phase-3-release.json`](evidence/phase-3-release.json). The release
commands and adjacent task evidence remain in
[`PLAN-3-DISTRIBUTED-TABLETOP-SPATIAL.md`](../PLAN-3-DISTRIBUTED-TABLETOP-SPATIAL.md).

## Reproduce the aggregate surfaces

```pwsh
cargo test --workspace --offline
cargo run -p poche-xtask --offline -- compare all --scope micro
cargo run -p poche-xtask --offline -- session compare all --scope lobby-micro
cargo run -p poche-xtask --offline -- session compare all --scope governance-micro
cargo run -p poche-xtask --offline -- spatial compare all --scope micro
cargo run -p poche-xtask --offline -- consensus compare all --scope micro
cargo run -p poche-xtask --offline -- trustless smoke --scenario dropout
cargo run -p poche-xtask --offline -- spatial vertical-slice
cargo run -p poche-xtask --offline -- pages build
```
