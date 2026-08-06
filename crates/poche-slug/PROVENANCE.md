# Slug source provenance

`src/slug.rs` was extracted from Teamy Terminal at revision
`8aede3a196d46354e253ecc0e9446fd4ff00fe74`:

- source: `crates/teamy-terminal-font/src/slug.rs`;
- source SHA-256: `8bd99bbb93c7a3c69376af1338306b00d716a02a10b7b57be07c123660f2d9a4`;
- reference shader: `crates/teamy-terminal-renderer/shaders/gpu_slug.wgsl`;
- shader SHA-256: `24c46c2278190537779e263357dba1e8fd0d5650d5f49e4fe35556e471c5df74`;
- source license: MPL-2.0.

The extraction removes Teamy Terminal's embedded-font singleton. Poche callers
must pass explicit licensed font bytes to `SlugFont::parse`. The curve,
directional-band, packed-word, metadata, and independent CPU coverage contracts
otherwise remain file-level MPL-2.0 source.
