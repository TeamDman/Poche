# Slug source provenance

`src/slug.rs` was originally extracted from Teamy Terminal at revision
`8aede3a196d46354e253ecc0e9446fd4ff00fe74`:

- source: `crates/teamy-terminal-font/src/slug.rs`;
- source SHA-256: `8bd99bbb93c7a3c69376af1338306b00d716a02a10b7b57be07c123660f2d9a4`;
- reference shader: `crates/teamy-terminal-renderer/shaders/gpu_slug.wgsl`;
- shader SHA-256: `24c46c2278190537779e263357dba1e8fd0d5650d5f49e4fe35556e471c5df74`;
- source license: MPL-2.0.

The degenerate-quadratic linear threshold was synchronized from the public
[Teamy-Slug reference implementation](https://github.com/TeamDman/Teamy-Slug)
at revision `5e1f2eeaa07e63fcc5ecd673824e9baa123a3c7d`. That revision separates the
`0.015625` quadratic-linear classification threshold from the smaller general
coverage epsilon, preventing false line/striation artifacts caused by
floating-point cancellation at large font-design coordinates. Poche carries a
focused source-level regression for that policy.

The extraction removes the reference implementation's embedded-font singleton.
Poche callers must pass explicit licensed font bytes to `SlugFont::parse`. The
curve, directional-band, packed-word, metadata, and independent CPU coverage
contracts otherwise remain file-level MPL-2.0 source.
