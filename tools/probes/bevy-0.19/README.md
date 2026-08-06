# Bevy 0.19 renderer-boundary probe

This independent Cargo workspace makes plan gate G3 reproducible without
making Bevy part of the canonical game, spatial, formal, protocol, or RL
crates. It verifies the exact dependency/features selected for the later
`poche-native-ui` adapter and measures the conversion from canonical integer
millimetres into renderer-local `f32` metres.

Run from the Poche repository root:

```pwsh
cargo run --manifest-path tools/probes/bevy-0.19/Cargo.toml --locked --offline
```

The accepted result for every integer millimetre in `-10_000..=10_000` is a
zero-millimetre rounded endpoint error. Intermediate floating-point transforms
remain presentation data and never flow back into canonical game state.
