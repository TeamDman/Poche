# Deployment modes

This matrix records only modes demonstrated by executable evidence. “Present
in the dependency graph” is not treated as a deployable client.

| Mode | Status | Where it runs | Trust and anonymity | Entry point |
| --- | --- | --- | --- | --- |
| Rulebook | Published | GitHub Pages | Static public artifact; GitHub/CDN observes ordinary HTTP metadata | <https://teamdman.github.io/Poche/poche-rules.pdf> |
| Exact-projection replay, browser | Published by Pages run `31026114728` | GitHub Pages, static egui/WASM | No authority/network/user data; checked fixture only | <https://teamdman.github.io/Poche/replay/> |
| Exact-projection replay, native | Proven locally | Native egui process | No authority/network/user data; checked fixture only | `cargo run --release -p poche-ui --bin poche-replay` |
| Canonical spatial mirror, native | Proven locally in a Bevy 0.19 release window | Native process using the same checked exact-recipient replay | Renderer ECS mirrors typed locations and cannot reveal the 49 hidden faces in the acceptance fixture; this is not a live multiplayer client or physics authority | `cargo run --release -p poche-native-ui -- --debug-overlay`; see [`native-spatial-ui.md`](native-spatial-ui.md) |
| Browser-only Veilid | Unsupported; rechecked 2026-08-05 | A Pages HTTPS origin cannot use the passing insecure-WS development path | The public WSS bootstrap still resets before TLS. Veilid 0.5.7 deprecates WSS behind a feature and tracks still-open WebTransport work for secure origins; no companion is implied | Evidence in [`veilid-browser-feasibility.md`](veilid-browser-feasibility.md) |
| Host-colocated Datastar authority demo | Proven locally; self-hostable development mode | One Axum process plus ordinary browsers | Operator owns full state and can inspect every hand. Current named viewer routes and static invite codes are not authenticated, state is in-memory, and this is not anonymous/trustless production multiplayer | `cargo run --release -p poche-web-spike` |
| Signed browser-device gateway lab | Proven locally; compatibility experiment | The same Axum process at `/gateway`, a browser-local WebCrypto device, and a co-located native fixture | Browser command key is non-extractable and never uploaded; gateway still sees command/projection plaintext and is the disclosed host authority. Lab enrollment is not production root certification | [`browser-device-gateway.md`](browser-device-gateway.md) |
| Native live Veilid protocol | Acceptance proven; no packaged player client yet | Two native Veilid processes on the opt-in public network | Stable application keys authorize commands independently of node IDs/routes; DHT/private routes expose network metadata to Veilid peers, while exact-recipient projections remain encrypted | `cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-public` with the explicit environment guard; see `veilid-native-acceptance.md` |

## Datastar demo connection configuration

The server defaults to loopback at `127.0.0.1:4174`. Change the bind address
with `POCHE_WEB_SPIKE_ADDR`; the container sets it to `0.0.0.0:4174` internally.
For the proven safe development posture, publish only to the host machine:

```powershell
cargo run --locked --release -p poche-web-spike
```

Open <http://127.0.0.1:4174/> for the original host demo or
<http://127.0.0.1:4174/gateway> for the signed browser-device lab. Do not expose
this deterministic harness to an
untrusted network: `/live/{viewer}` selects an identity by name, invitation
codes are static, and there is no persistent store or production login. A
future deployable server must bind authenticated application principals to
connections, issue fresh invites, persist signed session state, apply origin
and abuse controls, and terminate TLS before this warning can be removed.

## Container reproduction

From the repository root:

```powershell
docker build --file deploy/poche-web-spike/Dockerfile --tag poche-web-spike:local .
docker run --rm --publish 127.0.0.1:4174:4174 poche-web-spike:local
```

The loopback host mapping is intentional. The multi-stage image compiles the
locked workspace with Rust 1.96.0 and copies only the release server into its
runtime stage. Generated image layers remain outside Git history. Base-image
tags identify the build recipe but registries can repoint tags; operators who
require byte-for-byte base immutability should mirror and digest-pin the images
under their own update policy.

The browser loads the pinned Datastar client module from jsDelivr. A production
deployment should vendor that generated/static dependency or apply an explicit
content-security and supply-chain policy; this development demo does not claim
that hardening.
