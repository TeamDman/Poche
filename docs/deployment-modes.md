# Deployment modes

This matrix records only modes demonstrated by executable evidence. “Present
in the dependency graph” is not treated as a deployable client. “Experimental”
and “research-only” rows are intentionally not deployment recommendations.

| Mode | Status | Where it runs | Trust and anonymity | Entry point |
| --- | --- | --- | --- | --- |
| Rulebook | Published | GitHub Pages | Static public artifact; GitHub/CDN observes ordinary HTTP metadata | <https://teamdman.github.io/Poche/poche-rules.pdf> |
| Exact-projection replay, browser | Published from `model-checking` by Pages | GitHub Pages, static egui/WASM | No authority/network/user data; checked fixture only | <https://teamdman.github.io/Poche/replay/> |
| Spatial evidence, browser | Published from `model-checking` by Pages | Static semantic HTML, SVG, and checked JSON | One exact-recipient scene endpoint and bounded Alloy overlap witness; no live room, hidden-hand transport, or unbounded geometry theorem | <https://teamdman.github.io/Poche/spatial.html> |
| Exact-projection replay, native | Proven locally | Native egui process | No authority/network/user data; checked fixture only | `cargo run --release -p poche-ui --bin poche-replay` |
| Canonical spatial mirror, native | Proven locally in a Bevy 0.19 release window | Native process using the same checked exact-recipient replay | Renderer ECS mirrors typed locations and cannot reveal the 49 hidden faces in the acceptance fixture; this is not a live multiplayer client or physics authority | `cargo run --release -p poche-native-ui -- --debug-overlay`; see [`native-spatial-ui.md`](native-spatial-ui.md) |
| Browser-only Veilid | Unsupported; rechecked 2026-08-05 | A Pages HTTPS origin cannot use the passing insecure-WS development path | The public WSS bootstrap still resets before TLS. Veilid 0.5.7 deprecates WSS behind a feature and tracks still-open WebTransport work for secure origins; no companion is implied | Evidence in [`veilid-browser-feasibility.md`](veilid-browser-feasibility.md) |
| Host-colocated Datastar authority demo | Proven locally; self-hostable development mode | One Axum process plus ordinary browsers | Operator owns full state and can inspect every hand. Current named viewer routes and static invite codes are not authenticated, state is in-memory, and this is not anonymous/trustless production multiplayer | `cargo run --release -p poche-web-spike` |
| Signed browser-device gateway lab | Proven locally; compatibility experiment | The same Axum process at `/gateway`, a browser-local WebCrypto device, and a co-located native fixture | Browser command key is non-extractable and never uploaded; gateway still sees command/projection plaintext and is the disclosed host authority. Lab enrollment is not production root certification | [`browser-device-gateway.md`](browser-device-gateway.md) |
| Native live Veilid protocol | Acceptance proven; no packaged player client yet | Two native Veilid processes on the opt-in public network | Stable application keys authorize commands independently of node IDs/routes; DHT/private routes expose network metadata to Veilid peers, while exact-recipient projections remain encrypted | `cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-public` with the explicit environment guard; see `veilid-native-acceptance.md` |
| Replicated player/device log | Experimental semantics and formal micro-scope; not deployable | Deterministic in-process replicas and independent Alloy/NuSMV/Prolog/Rust models | Multiple certified devices retain agency without adding player voting weight. Progress assumes a strict player majority, eventual delivery, and non-equivocation; a retained equivocation witness shows this is not Byzantine fault tolerance | [`replicated-runtime.md`](replicated-runtime.md) and [`consensus-coverage.md`](consensus-coverage.md) |
| Verifiable hidden-card round | Research-only bounded prototype; not deployable | Deterministic three-player cryptographic corpus | Verifies a 52-card shuffle/deal/reveal and final audit without a trusted dealer, but pins unaudited experimental cryptography, requires all reveal shares, and still needs independent security review | [`hidden-card-prototype.md`](hidden-card-prototype.md) and [`trustless-recovery.md`](trustless-recovery.md) |

## Trust vocabulary

| Label | Meaning in this repository |
| --- | --- |
| Static | No room authority, identity, command ingress, or live transport exists. |
| Host-trusted | One process orders state and sees every hand; signatures authorize clients but do not constrain the host's private-state knowledge. |
| Gateway-facilitated | A disclosed gateway locates/relays or, in an explicit degraded profile, may custody a device key. A route code alone grants no membership or event authority. The current browser lab is the host-trusted profile, not the future replicated relay profile. |
| Replicated experimental | Devices verify a certified log under exact quorum/fork assumptions; availability and Byzantine safety are not implied beyond the checked model. |
| Cryptographically private prototype | The dealerless card identity claim comes from the named proof construction and corpus, subject to unanimous-share and security-review limits. |
| Unsupported | A prerequisite failed in the production-equivalent environment; no hidden companion or downgrade is assumed. |

The routed `p3r-` credential can carry one initial direct-Veilid or exact gateway
locator. It remains an expiring rendezvous capability: later membership,
device certification, event quorum, projection encryption, and key revocation
are separate layers. See
[`ADR 0009`](decisions/0009-routed-room-codes-and-gateway-trust.md).

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
