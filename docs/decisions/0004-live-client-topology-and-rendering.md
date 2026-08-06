# Live client topology and rendering

- Status: Accepted
- Date: 2026-08-05 (America/Toronto)
- Scope: `poche-phase-2`, G26 and G32
- Supersedes: Only the provisional G26/G32 dispositions in `0003`; all other
  decisions in `0001`-`0003` remain in force

## Decision

The first supported live browser topology is a self-hostable native Rust room
authority serving an ordinary semantic-HTML client over HTTPS. Axum and
Datastar are the current thin adapter. The server is colocated with the room
host/authority; it is not an additional projection-reading relay. It consumes
typed commands and emits exact-recipient projections through the same protocol
and pure reducers as every other adapter.

Browser-only Veilid is not the production Pages topology. It is proven to work
from an HTTP origin over direct `ws://`, but Veilid 0.5.7 has no usable public
WSS bootstrap/outbound-relay path for an HTTPS origin. We will not require an
on-device native companion to conceal that limitation. A future Veilid release
or Poche-operated WSS relay may reopen this decision only after repeating the
same no-companion HTTPS test and updating the threat/operations model.

Rendering remains downstream of the renderer-neutral `PresentationModel`:

- shared egui is the native client and static WASM replay implementation;
- semantic HTML is the live browser implementation because it supplies
  ordinary headings, regions, lists, controls, and live status to assistive
  technology;
- direct ash/Vulkan is reserved for a separately measured latency experiment,
  not the game/session boundary or initial UI.

GitHub Pages publishes the rulebook and static replay. It may link to a room's
separately hosted HTTPS authority, but it does not claim that its own origin is
a direct Veilid multiplayer client.

## Executable evidence

The pinned, unchanged Veilid 0.5.7 source at
`76b2176926dc24e30f9427540384a04ae22e590c` was built for
`wasm32-unknown-unknown` with Rust 1.96, `wasm-bindgen` 0.2.121, and
`enable-protocol-wss`. `crates/poche-veilid/web-spike` packages the generated
artifact into ignored output and exposes attachment state without a native
Poche or Veilid process.

On `http://127.0.0.1:4175`, the browser connected directly to
`ws://bootstrap-v1.veilid.net:5150/ws`. A first run became public-ready with 28
peers in 7.171 seconds. A clean repeat first proved that no process named Poche
or Veilid existed, then reached `AttachedFull`, 31 peers, and public readiness
in 20.264 seconds. Only the Python static-file server remained locally.

The same WSS-enabled artifact against
`wss://bootstrap-v1.veilid.net:5150/ws` remained `Attaching` with zero peers
after 20.112 seconds. A separate TLS handshake to the documented host resolved
its two public addresses, connected to port 5150, and was reset before TLS
could complete. The pinned upstream README also labels HTTPS operation not
implemented pending outbound relays. Thus a real HTTPS origin has no permitted
`ws://` fallback and no working `wss://` bootstrap prerequisite; fabricating a
local trusted page certificate would not supply the missing public relay.

The WSS-enabled raw release package is 8,879,727 bytes of WASM plus 407,153
bytes of generated JavaScript before the unavailable optional `wasm-opt` pass.
The probe page and script add 7,319 bytes. The static server peaked at
26,083,328 bytes; this is not browser process memory.

The Datastar alternative executed in an ordinary browser. Its 3,013,632-byte
release binary holds the typed host authority, accepts `CreateRoom` through
canonical NDJSON ingress and the pure reducer, and patches the exact host
projection (`applied; revision 1; events 1`). Reset/recreate and Bob's
ungranted/granted/revoked views all passed. Browser-observed command-to-patched
DOM snapshots were 286-307 ms including automation and snapshot overhead.

Native egui, egui/WASM, semantic HTML, Datastar, and direct Vulkan measurements
and reproduction commands are retained in `docs/rendering-topology-spike.md`.

## Privacy and operations consequences

The room host already owns authoritative hidden state under ADR 0003. Serving
that host's exact-recipient projections does not add a new trusted party. TLS
is mandatory outside loopback. Deployments should self-host the pinned Datastar
client rather than disclose page-fetch metadata to a CDN. If someone other than
the room host operates the server, that is a new projection-reading party and
requires an explicit threat-model decision.

The server operator must provide a reachable HTTPS endpoint, certificate,
persistence policy, backups if desired, and native-process updates. Native
Veilid remains available for native clients and server-side rendezvous, but it
does not erase those browser-server operational requirements.

## Gate dispositions

| Gate | Disposition |
| --- | --- |
| G26 | Closed: HTTP/WS browser Veilid passes without a companion; HTTPS/WSS direct Pages fails because the public WSS bootstrap/outbound-relay prerequisite is absent. Select the self-hostable, host-colocated Datastar authority for live browsers. |
| G32 | Closed: keep a renderer-neutral presentation model, shared egui for native/static WASM, semantic HTML for the accessible live browser, and raw Vulkan only as a bounded future experiment. |

## Primary references

- [Veilid WASM limitations](https://gitlab.com/veilid/veilid/-/blob/v0.5.7/veilid-wasm/README.md)

## Phase-3 re-evaluation, 2026-08-05

Task 7.3 leaves this decision unchanged. Veilid 0.5.7 is the newest crates.io
release. Its secure-origin README still says HTTPS/WSS browser operation is not
implemented. Upstream has since deprecated WSS behind an opt-in feature and
selected WebTransport as the intended replacement, but issue #460's bootstrap,
wire, codec, config, native, and WASM checklist remains entirely open. A fresh
bounded probe reached `101 Switching Protocols` over documented development
WS, while the same public endpoint reset before TLS over HTTPS/WSS. See
[`veilid-browser-feasibility.md`](../veilid-browser-feasibility.md) for exact
versions, source/issue links, endpoint evidence, and upstream-versus-Poche scope.
- [Veilid 0.5.7 source](https://gitlab.com/veilid/veilid/-/tree/v0.5.7)
- [Datastar Rust 0.3.1](https://docs.rs/datastar/0.3.1/datastar/)
