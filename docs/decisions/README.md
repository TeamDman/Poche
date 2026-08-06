<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Architecture decision index

These records freeze a boundary before an implementation can make it
accidental. “Accepted” means the decision governs its named scope; it does not
promote a bounded experiment into a production claim.

| ADR | Status | Decision in one sentence |
| --- | --- | --- |
| [0001 — Initial modeling](0001-initial-modeling-decisions.md) | Accepted | Typed Rust expressions and independently handwritten Alloy, NuSMV, and Prolog models share explicit finite domains and evidence contracts. |
| [0002 — GitHub head and Pages](0002-github-head-and-pages-policy.md) | Accepted and applied | `model-checking` is the project head and the only branch authorized to publish generated Pages artifacts. |
| [0003 — Session, network, rendering, and RL](0003-session-network-rl-architecture.md) | Accepted | Pure game state, room/session state, transport, viewer projection, rendering, and RL are separate layers. |
| [0004 — Live client topology](0004-live-client-topology-and-rendering.md) | Accepted | The first browser topology is a self-hosted Axum/Datastar authority; egui remains the portable replay baseline. |
| [0005 — Spatial refinement](0005-spatial-refinement-and-renderer-boundary.md) | Accepted | Typed state is canonical; exact integer geometry refines it, while Bevy ECS, transforms, pixels, and glyphs have no semantic authority. |
| [0006 — Typed commands and legality](0006-typed-commands-and-legality-policy.md) | Accepted | CLI text, UI controls, drag input, votes, and replay converge on one versioned command AST with prevent/audit/governance policies. |
| [0007 — Player/device identity and replicated log](0007-player-device-and-replicated-log.md) | Experimental track | Players may own several certified devices without gaining voting weight; the accountable log is majority-based and not Byzantine fault tolerant. |
| [0008 — Hidden-card prototype](0008-hidden-card-prototype.md) | Research-only track | A bounded published mental-poker construction demonstrates verifiable shuffle/deal/reveal, but unanimous reveal and missing security review prevent a production claim. |
| [0009 — Routed room codes and gateway trust](0009-routed-room-codes-and-gateway-trust.md) | Accepted for composition | A route credential may locate direct Veilid or an explicit gateway, but it grants neither membership nor later event authority. |

When a later ADR supersedes one row, preserve the old scope and state exactly
which evidence identities, wire versions, and public claims change.
