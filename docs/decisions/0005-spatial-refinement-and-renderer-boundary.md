# Spatial refinement and renderer boundary

- Status: Accepted; G1/G2/G3/G12 closed
- Date: 2026-08-05 (America/Toronto)
- Scope: `poche-phase-3`, `poche-spatial-v1`
- Supersedes: Nothing; ADRs 0001-0004 remain in force

## Decision

Typed Poche and session state remains canonical. An exact-recipient viewer
projection may be realized into a deterministic spatial scene. A classified
spatial interaction may propose typed intent, but raw geometry, glyphs, pixels,
renderer entities, and arbitrary component combinations cannot mutate game
truth directly.

```text
authorized viewer projection + registered layout
  -> realize
  -> SpatialScene (integer endpoint poses, objects, zones, text attachments)
  -> native 3D or semantic HTML

spatial input / CLI input
  -> classify + resolve against authorized semantic data
  -> typed command
  -> existing checked reducer
  -> successor projection
  -> realize successor
```

The central refinement law is
`abstract(realize(projection, layout), layout) == projection` for the portion of
the projection represented by `poche-spatial-v1`. Task 2.4 registers the exact
finite/property scopes. Transition commutation additionally requires that a
typed command followed by realization and a spatial intent resolved to that
same command have equal semantic successor and endpoint hashes.

## G1: `poche-spatial-v1`

### Units, origins, and poses

- Canonical linear units are signed integer millimetres in table-local space.
- Canonical yaw is an integer number of milli-degrees normalized to one turn.
- Pitch/roll, arbitrary quaternions, mesh vertices, and renderer floating-point
  transforms are not part of v1 semantic state.
- Rendering adapters may convert integer endpoints to `f32`/GPU coordinates.
  Those values never return as authority without integer classification and a
  typed command.
- Every table has its own origin. A network/session identity addresses the
  table; a globally enormous coordinate is unnecessary for one card game.

### Objects and identity

- Stable semantic identities name the table, seats, player anchors, zones,
  score sheet, surfaces, and text bindings.
- Card objects use opaque `(projection_epoch, ordinal)` handles independent of
  card face. A shuffle/privacy boundary may issue new unlinkable handles.
- Viewer-authorized face knowledge is `Option<CardFace>` on a card object. A
  hidden card has no face text run. Renderers may show a generic back.
- A renderer's ECS/entity/DOM/node ID is an adapter-local cache key, never a
  protocol, replay, attachment, or game identity.

### Zones, loose input, and snapping

- Typed card endpoints are `Deck`, `Hand(seat)`, `Play(seat)`, or
  `Won(seat, trick, index)`. These map to semantic zones plus deterministic
  local slots.
- Task 2.1 defines each zone with an inner snap volume and a separated outer
  boundary/dead band. Exactly one inner match resolves a zone; multiple matches
  are an invalid-layout/ambiguous finding; only boundary matches are ambiguous;
  no match is free/loose.
- `StrictPoche` commits only an interaction that resolves to a typed Poche
  action. `AuditLoose` may retain a loose manipulation as attempted evidence,
  but it does not make an invalid `Game` value or bypass hard invariants.
- Physics, collision impulses, gravity, and agreement on continuous paths are
  not part of v1. Simple AABBs/OBBs/planes are enough for layout and audit.

### Text, cards, and scores

- Text is a semantic UTF-8 run with an explicit binding, owner surface, and
  surface-local pose. Examples are `CardFace(card_object)`,
  `PlayerName(seat)`, and `PlayerScore(seat)`.
- The typed card face and score ledger remain authority. Slug/glyph curves are a
  native rendering of text runs; font selection, glyph outline, thickness, and
  nearest-surface distance cannot change identity or value.
- A score sheet can therefore look physical and be audited spatially while
  `interpret_score_sheet(realize_score_sheet(ledger)) == ledger` remains the
  semantic test.

### Animation and replay

- Consensus/replay records committed semantic source/destination endpoints and
  may include duration plus a stable easing ID.
- Intermediate tween frames, wall-clock samples, dropped frames, and local
  camera motion are presentation. Replaying the event log reconstructs exact
  endpoints without discretizing a five-second animation.

### Privacy and dependency direction

- `poche-spatial` accepts only viewer-scoped/adapter-owned inputs. It never reads
  full host authority state to decide what a renderer should see.
- The engine-neutral crate has no Bevy, GPU, window, network, protocol-codec, or
  RL dependency. Later adapters may depend on it; it never depends on them.
- RL keeps calling `GameEnvironment` directly. Spatial output may visualize a
  selected episode but cannot affect actions, chance, reward, or state hashes.

## G2: layout cardinalities

The first spatial contract registers deterministic layouts for 2 through 8
players, matching the existing phase-two session membership ceiling. Layout
support does not claim that the current strict game oracle, native formal
models, or `poche-2p-v1` RL spec already support every count.

- Task 1.1 provides the validated `LayoutId`/`SeatId` cardinality contract and a
  two-player viewer fixture.
- Task 2.1 supplies concrete poses, snap volumes, separation proofs, and layout
  fixtures for each advertised count.
- Each game/RL/formal model continues to state its own independent player scope.
  A three-player layout cannot silently change the immutable two-player RL
  observation/action shapes.

## Loose tabletop versus strict Poche

Poche remains the primary game. Generality is introduced by reusable spatial
objects and audit-mode interactions, not by replacing the reducer with a
generic scene graph. An off-suit card play is structurally representable as a
normal `Hand -> Play` move and can be prevented or audited by game policy. A
card dropped outside every zone is loose attempted interaction evidence; it is
not a valid strict Poche successor.

This separation preserves both goals:

1. invalid strict Poche states remain difficult or impossible to construct;
2. a future tabletop interface may show, accuse, vote on, or recover from
   attempted behavior outside ordinary rules.

## ECS consequence

The earlier Bevy prototype represents `InHand`, `InDeck`, `Played`, and
ownership as separately composable components, then derives one positioning
behavior. That storage permits contradictory component combinations. The
lesson is not that ECS or Bevy is unsuitable; it is that renderer ECS state
must mirror one validated spatial/card-location record and must not be the
semantic source.

The native adapter will use Bevy rather than building another window/input/GPU
shell. Bevy remains downstream of `poche-spatial`; its ECS mirrors a validated
scene and may cache renderer resources, but a component query cannot directly
change canonical game/session state.

## G3: native renderer and Slug packaging

### Bevy selection

The native adapter will pin `bevy = "=0.19.0"`, disable default features, and
enable Bevy's published `3d` feature profile. The adapter lives in the future
`poche-native-ui` crate and is not a dependency of domain, model, environment,
formal, protocol, network, browser, or RL crates. Windows is the first
executable target; Bevy's portable renderer boundary remains available without
claiming untested platform support.

The exact-version probe in `tools/probes/bevy-0.19` compiled with Rust 1.96.
Its isolated lock resolved 501 packages, which is a meaningful build and audit
cost and another reason to keep Bevy at the leaf. The probe uses a Bevy
`Component` and `Transform`, so it checks the actual intended seam rather than
only dependency metadata.

### Slug selection and provenance

Poche will extract the renderer-neutral Slug curve, directional-band, packed
word, and CPU-oracle contract into a first-party MPL-2.0 `poche-slug` crate.
That crate will accept explicit licensed font bytes and will not depend on
Teamy Terminal's embedded-font singleton, terminal cell model, snapshots,
Ash/Vulkan lifecycle, or dirty/absolute sibling paths. `poche-native-ui` will
own the Bevy GPU material/buffer adapter and parity tests. Semantic `TextRun`
attachments and typed card/score values remain upstream authority.

The extraction source is pinned for provenance, not used as a path dependency:

- Teamy Terminal revision `8aede3a196d46354e253ecc0e9446fd4ff00fe74`;
- `crates/teamy-terminal-font/src/slug.rs`, SHA-256
  `8bd99bbb93c7a3c69376af1338306b00d716a02a10b7b57be07c123660f2d9a4`;
- reference WGSL
  `crates/teamy-terminal-renderer/shaders/gpu_slug.wgsl`, SHA-256
  `24c46c2278190537779e263357dba1e8fd0d5650d5f49e4fe35556e471c5df74`;
- Teamy Terminal code is MPL-2.0. Any selected font asset must preserve its own
  license and hash beside the extracted crate.

The renderer-neutral source is reusable; the existing retained GPU renderer is
terminal-specific and is architecture evidence only. Task 8.1 must perform a
file-level provenance/licence review while extracting because MPL-2.0
modifications remain MPL-2.0 source files.

## G12: origin and precision

`big_space` is not admitted for the Poche vertical slice. Canonical positions
remain integer millimetres in a `TableId`-local frame; Bevy receives derived
`f32` metres. The executable probe checks every integer millimetre endpoint in
`-10_000..=10_000`: all round-trip to the original integer, with maximum error
`0 mm`. A conservative `f32` epsilon at two metres is
`0.000238419 mm`, far below the one-millimetre semantic grid.

Multiple rooms/tables therefore use separate table-local frames rather than
astronomically separated renderer coordinates. Reopen G12 only if one rendered
scene must span sufficiently separated tables that a measured interaction or
visual tolerance fails. At this decision date, the published `big_space`
compatibility table also stops at Bevy 0.18 (`big_space` 0.12), while the chosen
Bevy is 0.19; taking that integration burden without a precision need would be
counterproductive.

## Support and license consequences

- Bevy 0.19.0 declares Rust 1.95 and `MIT OR Apache-2.0`; the Poche workspace
  uses Rust 1.96 and MPL-2.0 first-party files.
- No MPL-incompatible source is copied by selecting Bevy. Third-party notices
  remain a distribution/release responsibility.
- `cargo-deny` was not installed when this gate closed, so no `cargo deny`
  result is claimed. Cargo's resolved package metadata was inspected instead;
  the final release gate still requires the repository's license audit.
- Bevy, Slug GPU data, render transforms, and glyph proximity do not change the
  G1/G2 semantic contract.

## Initial executable evidence

Task 1.1 validates the dependency-free crate and two-player fixture with:

```pwsh
cargo test -p poche-spatial --offline
cargo clippy -p poche-spatial --all-targets --offline -- -D warnings
cargo fmt --all --check
```

The fixture includes a table, two seats and hand zones, deck, play zone, score
sheet, cards in two hands/deck/play, authorized face text, hidden cards without
face text, and explicit name/score attachments. Negative tests reject hidden
face text, missing face text, invalid epochs/attachments, and prove touching
card poses cannot reassign semantic text.

Task 1.2 validates the selected leaf dependency and precision boundary with:

```pwsh
cargo run --manifest-path tools/probes/bevy-0.19/Cargo.toml --locked --offline
cargo tree --manifest-path tools/probes/bevy-0.19/Cargo.toml --locked --offline
```
