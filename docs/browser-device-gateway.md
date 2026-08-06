# Signed browser-device gateway

Task 7.2 adds an explicitly labelled compatibility lab at
`http://127.0.0.1:4174/gateway`. It proves that an ordinary browser can retain
device agency while an Axum process performs HTTP/SSE and semantic-authority
work. It does **not** prove that the current room is decentralized: the lab
uses the existing host-authoritative Poche reducer, the gateway sees all
command and projection plaintext, and the host can order, censor, delay, or
inspect the whole room.

The page renders the canonical ADR-0009 host/browser-local/plaintext trust
profile before registration. An unsupported browser is blocked rather than
silently falling back to gateway custody. The demonstrated key is a
non-extractable, session-local WebCrypto Ed25519 private key; only its raw
public key and signatures cross HTTP. The page also lists a separately
identified co-located native fixture for the same `alice` player. Device count
does not change player voting weight. The native fixture can revoke the browser
device without revoking itself; replaying registration cannot reactivate the
revoked key.

Enrollment is intentionally a lab shortcut, not a root certificate. A
deployable replicated gateway must consume the root-signed device certificate
and membership transitions from ADR 0007 instead of accepting the fixed Alice
lab enrollment. The native fixture is co-located in this executable, so its
label demonstrates identity and independent revocation semantics rather than
process isolation.

## Signed HTTP ingress

`POST /gateway/command` accepts at most the router's 8 KiB body limit and then
decodes a deny-unknown-fields typed envelope:

- schema version, fixed lab player, 32-byte lower-hex device public-key ID;
- a bounded command ID and strictly increasing device sequence;
- one typed action: pause, unpause, bounded chat, spectator grant, or spectator
  revocation; and
- one 64-byte Ed25519 signature.

Signing bytes use the `poche.gateway-command.v1` domain followed by eight
four-byte-big-endian length-framed fields. Length framing makes text such as
chat unambiguous. The adapter verifies the registered, active device before it
bridges the already-typed action into `LiveDemo` and the canonical session
reducer. Raw command strings are never executed.

Command IDs provide semantic idempotency. An exact signed retry returns the
retained receipt with `duplicate=true` and does not execute the reducer or
publish another projection. Reusing the ID with different signed bytes fails
closed. Stale sequences, bad signatures, oversized chat, unknown devices, and
revoked devices also fail closed. This ingress signature authenticates the
browser-to-gateway hop; the current host-authoritative bridge still creates its
existing application command internally. Replicated mode must submit the
device-signed ADR-0007 candidate rather than claiming this bridge is consensus.

## Ordered recipient SSE

`GET /gateway/device/{device}/events?after=N` opens a persistent SSE stream.
Each JSON `projection` event has a monotonic SSE ID, the exact recipient device
ID, semantic status, and Alice's already-authorized semantic HTML projection.
The lab retains 128 full projection events. Reconnect accepts either the
explicit cursor or `Last-Event-ID` and replays only later events before
continuing live. Because each retained value is a full projection, falling
within the retained window restores current presentation without asking a
semantic command to run again.

The browser deliberately separates HTTP command availability from EventSource
state. Its “Drop SSE only” control closes the stream while signed POST remains
usable; “Reconnect after last event” then recovers a missed projection. Keepalive
comments traverse idle connections. HTTP+SSE met the measured need, so the lab
adds neither WSS nor WebTransport.

## Browser and runtime evidence

On 2026-08-05, the release binary was exercised in the Codex in-app Chromium
browser on loopback with ordinary DOM controls. The browser version was not
exposed by the acceptance surface and is therefore not guessed.

The acceptance performed the following sequence:

1. WebCrypto created and registered a browser-local key; the device table showed
   one browser and one native device for Alice.
2. A signed pause applied at revision 13 and SSE event 2.
3. An identical signed-body retry returned the same event 2 and left the
   accepted-command count at one.
4. SSE was closed after event 2; signed unpause applied at revision 14/event 3
   while the displayed projection correctly remained paused.
5. Reconnect after event 2 replayed exactly event 3 and displayed Running;
   counters reported two SSE connections and one reconnect.
6. Signed chat became visible in Alice's authorized projection, followed by a
   signed spectator hand grant and signed revocation.
7. The native fixture revoked only the browser key. Browser actions became
   disabled, the browser row read `revoked`, and the native row remained
   `active`.

The final operator metrics were five newly accepted commands, one exact
duplicate, two SSE connections, and one reconnect. The last command used 230
bytes including its signature; semantic processing took 335 microseconds in
this run. Its full recipient-event JSON was 2,569 bytes raw and 959 bytes under
deterministic gzip (37.3% of raw). The live spike deliberately leaves SSE
uncompressed: streaming compression is a deployment option only after its
flush/buffering behavior is measured. One browser-observed grant round trip was
2.9 ms; that includes local browser/application scheduling and is not
input-to-photon latency.

The final 4,073,984-byte release executable followed the accepted build; the
browser-accepted predecessor (before adding the re-registration regression
test/guard) used about 10.5 MB working set and 10.8 MB
peak working set during this bounded run. These figures are environment-specific
observations, not performance guarantees.

The DOM snapshot exposed named regions, headings, a labelled chat textbox,
ordinary buttons, a device table/caption, and live status announcements. Pause,
retry, drop/reconnect, chat, grant/revoke, and device revocation were all driven
through those controls. Browser console warning/error capture was empty.

## Reproduction

```pwsh
cargo test -p poche-web-spike --offline
cargo clippy -p poche-web-spike --all-targets --offline -- -D warnings
cargo build --locked --release -p poche-web-spike --offline
target/release/poche-web-spike
```

Open <http://127.0.0.1:4174/gateway>. Keep it on loopback. TLS, fresh invitation
enrollment, origin/CSRF policy, durable receipt/event storage, abuse controls,
root-certificate consumption, encrypted replicated projections, and gateway
failover remain deployment work rather than properties of this lab.
