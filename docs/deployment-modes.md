# Deployment modes

This matrix records only modes demonstrated by executable evidence. “Present
in the dependency graph” is not treated as a deployable client.

| Mode | Status | Where it runs | Trust and anonymity | Entry point |
| --- | --- | --- | --- | --- |
| Rulebook | Published | GitHub Pages | Static public artifact; GitHub/CDN observes ordinary HTTP metadata | <https://teamdman.github.io/Poche/poche-rules.pdf> |
| Exact-projection replay, browser | Published after Task 6.4 workflow passes | GitHub Pages, static egui/WASM | No authority/network/user data; checked fixture only | <https://teamdman.github.io/Poche/replay/> |
| Exact-projection replay, native | Proven locally | Native egui process | No authority/network/user data; checked fixture only | `cargo run --release -p poche-ui --bin poche-replay` |
| Browser-only Veilid | Unsupported | A Pages HTTPS origin cannot use the passing insecure-WS development path | No companion is implied; the tested public WSS bootstrap reset before TLS and Veilid 0.5.7 documents the missing outbound-relay HTTPS topology | Evidence in `rendering-topology-spike.md` |
| Host-colocated Datastar authority demo | Proven locally; self-hostable development mode | One Axum process plus ordinary browsers | Operator owns full state and can inspect every hand. Current named viewer routes and static invite codes are not authenticated, state is in-memory, and this is not anonymous/trustless production multiplayer | `cargo run --release -p poche-web-spike` |
| Native live Veilid client | Not released | Intended native process | Transport adapters and crypto boundaries exist, but separate-process lifecycle/security acceptance in Task 5.6 is still open | Do not advertise/download yet |

## Datastar demo connection configuration

The server defaults to loopback at `127.0.0.1:4174`. Change the bind address
with `POCHE_WEB_SPIKE_ADDR`; the container sets it to `0.0.0.0:4174` internally.
For the proven safe development posture, publish only to the host machine:

```powershell
cargo run --locked --release -p poche-web-spike
```

Open <http://127.0.0.1:4174/>. Do not expose this deterministic harness to an
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
