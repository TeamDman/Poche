<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Inspectable spatial-tabletop vertical slice

The `poche-spatial-vertical-slice-v1` command assembles the smallest complete,
reproducible view of the distributed tabletop work. It is intended for a human
who wants to follow behavior and evidence without opening each implementation
crate separately.

Run from the repository root:

```pwsh
cargo run -p poche-xtask --offline -- spatial vertical-slice
```

The command runs native Alloy and the real Rust reducers, compares every stable
result with checked evidence, rejects known secret markers, and writes ignored
inspection artifacts to `target/spatial-vertical-slice/`:

| Artifact | Contents |
| --- | --- |
| `transcript.ndjson` | 178 typed lifecycle input/result pairs, followed by typed records for replicated devices, named play, drag-equivalent play, score text, delayed audit, recovery vote, and final replay |
| `semantic-tabletop.html` | Static exact-recipient semantic HTML built from the same checked scene fixture as native 3D |
| `alloy-overlap.html` | Rust-rendered view of the normalized retained Alloy counterexample |
| `receipt.json` | Scope, semantic hashes, counts, result summaries, artifacts, and exclusions |

The checked receipt is
[`docs/evidence/spatial-vertical-slice.json`](evidence/spatial-vertical-slice.json).
The normalized formal witness is
[`tests/fixtures/spatial/alloy-overlap-negative-control-v1.json`](../tests/fixtures/spatial/alloy-overlap-negative-control-v1.json).
Generated NDJSON and HTML remain ignored because the command can reproduce them.

## What is connected

- The in-process authority records 178 secret-free canonical inputs covering
  room creation/join, every two-player deal, pause and unpause by different
  players, chat, hand grant/revoke, disconnect/reconnect, settlement, reset,
  close, and independent replay to the same public state.
- The replicated runtime adds a three-player, five-device-authority,
  four-replica micro-scenario. It checks duplicate/reordered delivery,
  stale/revoked devices, minority failure, membership change, snapshot plus
  tail, convergence, and its retained non-equivocation counterexample.
- The exact-recipient replay fixture realizes one integer, table-local scene.
  Naming `4♣` and dragging its opaque object to the play zone resolve to the
  same typed play, sidecar, and endpoint. Score glyph records are interpreted
  back into the typed score cells.
- A rule-illegal off-suit action is initially unprovable. A later public club
  produces a `delayed_public_play` finding. Governance then records the accused
  target's visible rejection as uncounted and applies two eligible approvals to
  the typed `recover.kick(john)` effect.
- Alloy checks the full registered `layout-micro` suite. Rust reads the raw SAT
  instance for `OverlapNegativeControl`, requires deck and trump to share an
  outer cell in that instance, removes solver-local atom names, and renders the
  same normalized witness into HTML.

These tracks share typed contracts and a checked publication receipt. They are
not claimed to be one atomic live-network room execution.

## GitHub Pages build

Run the source-only Pages gate locally with:

```pwsh
cargo run -p poche-xtask --offline -- pages build
```

It writes `target/pages-site/`, replaces publication metadata, injects the
Rust-rendered endpoint and Alloy witness into `spatial.html`, copies the two
safe JSON evidence files, and rejects unresolved placeholders. GitHub Actions
runs the same builder into `site/`, then adds the pinned Typst PDF and egui/WASM
replay before deployment. Generated HTML, PDF, JavaScript, and WASM remain out
of Git history.

The resulting public page is
[teamdman.github.io/Poche/spatial.html](https://teamdman.github.io/Poche/spatial.html).
It intentionally contains no room secret, invite proof, private device key, or
live authority endpoint.

## Evidence boundaries

- The offline command opens no native window, browser/network connection,
  Veilid route, GPU frame, or external relay. Real-window and real-browser
  acceptance remain separately recorded in
  [`native-spatial-ui.md`](native-spatial-ui.md) and
  [`semantic-html-tabletop.md`](semantic-html-tabletop.md).
- The Alloy witness is a deliberate negative control in
  `layout-micro-2p-3v-8c-7z-7cells-8slots-int5`. It proves the checker can retain
  and explain a bad bounded instance; it is neither a concrete millimetre
  layout nor an unbounded theorem.
- The semantic HTML artifact is an exact-recipient static projection. Its
  inspectable forms do not submit to a live authority.
- Invite references in the NDJSON are labels for replay operations, not bearer
  invite proofs. Another viewer's hidden hand and all private keys remain absent.
