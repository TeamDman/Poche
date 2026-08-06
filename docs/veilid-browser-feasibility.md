# Veilid secure-browser feasibility, phase 3 re-evaluation

This note re-evaluates direct, no-companion Veilid from a production secure
browser origin as of 2026-08-05. The result remains **unsupported**, but the
likely upstream direction is now clearer: Veilid has deprecated WSS behind an
opt-in feature and tracks WebTransport as its intended secure-origin transport.
WebTransport is not implemented in Veilid 0.5.7 or current upstream main.

This work was read-only. Poche did not edit `D:\Repos\rust\veilid`, generate an
issue, or draft an upstream merge request. Veilid's `AGENTS.md` says that the
project does not accept fully or predominantly AI-generated contributions. A
human who chooses to pursue upstream work must first read the upstream
[`CONTRIBUTING.md`](https://gitlab.com/veilid/veilid/-/blob/main/CONTRIBUTING.md),
understand the design, search existing issues, and discuss the approach with
the maintainers.

## Exact versions and source state

- Poche pins crates.io `veilid-core = 0.5.7`; the official crates.io API
  reported 0.5.7 as both newest and maximum, published/updated on 2026-07-19.
- Official tag `v0.5.7` is commit
  `f5cdcca38cecf4845eb9bd5e21ddca382a357a75`.
- The clean local read-only reference and official upstream `main` both pointed
  to `76b2176926dc24e30f9427540384a04ae22e590c` (“update version”, committed
  2026-07-31). The Poche browser spike previously built this exact main commit,
  whose crate version is 0.5.7.
- `cargo run -p poche-xtask --offline -- doctor` passed with Rust 1.96.0,
  Alloy 6.2.0, NuSMV 2.7.1, Scryer Prolog 0.10.0-17-ge4d96925, and Typst 0.15.1.

The local reference remains a reference, not a dependency source. Poche's
reproducible native dependency remains the exact crates.io release.

## What upstream says and implements

The current and v0.5.7
[`veilid-wasm/README.md`](https://gitlab.com/veilid/veilid/-/blob/v0.5.7/veilid-wasm/README.md)
still documents these browser constraints:

- browser nodes have WebSockets but no TCP/UDP sockets;
- browser nodes cannot use DNS TXT bootstrap and must receive a direct
  WebSocket bootstrap URL;
- `ws://` is usable only from an insecure HTTP origin, while an HTTPS origin
  requires certificate-valid `wss://`; and
- “Running WASM on HTTPS sites” remains marked not implemented.

Upstream issue
[#487](https://gitlab.com/veilid/veilid/-/issues/487) and merged MR
[#461](https://gitlab.com/veilid/veilid/-/merge_requests/461) changed the
direction after that README text was written: WSS was deprecated behind the
`enable-protocol-wss` feature because of deployment/privacy roadblocks, with a
goal of eventual removal. The local 0.5.7 manifests confirm WSS is no longer a
default protocol feature. Source still contains its gated dial type, codecs,
native listener, and WASM connector; Poche's old probe intentionally opts in.

The replacement is official issue
[#460, WebTransport Support](https://gitlab.com/veilid/veilid/-/issues/460).
As checked on 2026-08-05, it remained open and every listed item remained
unchecked: WebTransport bootstrap, Cap'n Proto and core protocol/dial types,
encoders/decoders, listener/config URL, UDP/TCP-port multiplexing, native
implementation, and WASM implementation. A source search found only an
explanatory WebTransport comment in framing types—no WebTransport protocol or
dial implementation. Open issue
[#430](https://gitlab.com/veilid/veilid/-/issues/430) also states that outbound
relaying is not implemented while asking that future work use the relay worker.

The older closed PWA-server issue
[#23](https://gitlab.com/veilid/veilid/-/issues/23) describes serving a PWA plus
local WS/WSS relay configuration. Its closure is not executable secure-browser
support: current README, source, feature surface, issue #487, and the open
WebTransport checklist all contradict such a claim.

## Fresh endpoint evidence

The existing 0.5.7 browser artifact is 8,879,727 bytes of WASM plus 407,153
bytes of generated JavaScript and uses the opt-in WSS feature. The previous ADR
0004 browser run already proved that this exact 0.5.7/main artifact attaches
over development HTTP/WS without a companion and cannot attach from HTTPS/WSS.

The mandatory public endpoint prerequisite was probed again on 2026-08-05:

- `bootstrap-v1.veilid.net` resolved to `170.64.186.46` and `159.223.237.84`;
- a correctly formed `ws://bootstrap-v1.veilid.net:5150/ws` upgrade returned
  HTTP `101 Switching Protocols`, then remained open until the bounded
  five-second client timeout; and
- `https://bootstrap-v1.veilid.net:5150/ws` connected to the port but reset
  before the Schannel TLS handshake completed (`curl` error 35).

The WSS failure occurs before Veilid or Poche application traffic. A browser on
an HTTPS/Pages origin cannot legally downgrade to the passing `ws://` endpoint,
so rerunning the same browser attach cannot cross this failed prerequisite.
This is a bounded current endpoint result, not a claim that nobody can privately
operate a certificate-valid WSS peer.

## What a general upstream solution would require

Poche should not create a private fork that merely removes browser mixed-content
checks or restores WSS by default. A general secure-browser Veilid solution
would need the upstream-owned issue-460 work at minimum:

1. versioned WebTransport protocol/dial representation in the Cap'n Proto wire
   schema and Rust API, with compatibility and unsupported-peer behavior;
2. native QUIC/HTTP/3 listener and WASM browser connector implementations;
3. a secure bootstrap response that gives browser nodes reachable
   certificate-valid WebTransport dial information without DNS TXT access;
4. routing/relay selection, liveness, reconnect, and abuse/resource limits for
   browser nodes that cannot accept sockets;
5. an operator story for DNS, publicly trusted certificates, endpoint uptime,
   upgrades, rate limiting, denial of service, and transport metadata; and
6. production HTTPS browser tests covering bootstrap, DHT/routing, private
   projections/application crypto, partitions, failover, and multiple browser
   engines without a companion process.

WebTransport solves browser reachability constraints; it does not automatically
make Poche membership, device authorization, hidden projections, or shared-state
consensus secure. Those remain Poche application layers.

## Difference from the Poche gateway

An upstream improvement would make a browser a real Veilid node capable of the
general Veilid API. Poche's `/gateway` is narrower: it accepts a bounded,
browser-device-signed Poche action, bridges it to a disclosed authority mode,
and sends exact-recipient Poche HTML over reconnectable SSE. It does not expose
the Veilid API, proxy arbitrary DHT operations, or claim the browser itself is a
Veilid peer.

The narrow adapter is therefore not throwaway work. The same typed command,
device certificate, idempotency, projection, and trust-disclosure boundaries
remain useful if future browsers connect through WebTransport: only the route
adapter and deployment profile change. Until issue #460 is implemented and a
production-equivalent HTTPS probe passes, Poche keeps direct browser Veilid
unsupported and recommends either native Veilid or the explicitly trusted
self-hosted HTTP/SSE gateway.
