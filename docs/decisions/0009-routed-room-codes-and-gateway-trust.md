<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# ADR 0009: routed room codes and explicit gateway trust

- Status: Accepted for phase 3 gateway composition
- Date: 2026-08-05 (America/Toronto)
- Scope: phase 3 gate G9 and tasks 7.1-7.3
- Supersedes: no existing code or topology; refines ADRs 0004 and 0007

## Decision summary

Poche adds a distinct `p3r-` room credential that selects one initial
rendezvous path: direct Veilid, a Poche HTTPS gateway, or explicit loopback
HTTP. Existing `p3-` codes retain their byte-for-byte v1 meaning as direct
Veilid DHT credentials. We do not wrap an old code with a URL: it is already
near the 256-byte `InviteProof` bound, and doing so would either break the bound
or silently reinterpret deployed text.

A route is a replaceable locator. It is not a player, device, membership,
capability, event-ordering vote, or room authority. The routed credential also
contains the expiring invite secret and expected host principal, so its holder
can reach the selected redeemer and then run the same application-level invite,
identity, membership, and device authorization checks. After redemption, the
credential is disposable; signed membership/rendezvous records can rotate
routes without moving authority.

Every browser gateway publishes a canonical machine-readable trust disclosure.
The disclosure uses explicit visibility, ability, and quorum enums rather than
ambiguous security booleans. A decoder recomputes the selected profile and
rejects any document that understates gateway access.

## Route-code contract

`p3r-` version 1 binds:

- Poche protocol v1 and rendezvous schema v1;
- one route kind and bounded canonical payload;
- authority-clock expiry;
- the expected 32-byte player-root/host public principal;
- a 32-byte OS-random invitation secret; and
- an eight-byte domain-separated corruption checksum.

The checksum detects transcription/copy corruption; it is not a signature or a
substitute for verifying the secret and host credential at redemption. Codes
and locator values are redacted from `Debug`, and fixed secret buffers are
zeroed on drop. The maximum encoded text remains 256 bytes.

The three initial route payloads are:

| Route | Payload | Preconditions | Meaning |
| --- | --- | --- | --- |
| Direct Veilid | Public/local network plus encrypted DHT record key | Native client has or can obtain Veilid attachment | Resolve the signed Poche rendezvous record; the locator is not the host key |
| Gateway HTTPS | Exact `https://` origin plus opaque room handle | Reachable TLS origin | Submit to the Poche gateway/redeemer; no claim that the browser itself runs Veilid |
| Explicit loopback HTTP | `http://localhost`, `127.0.0.1`, or `[::1]` origin plus handle | User explicitly chose local/insecure development | Test or same-device fallback only; remote plaintext HTTP is unrepresentable |

A Veilid DHT key/private route does not attach a fresh Veilid node to the
network. Direct clients still require Veilid bootstrap or already-known peers.
The room code removes an application rendezvous lookup after attachment; it
does not replace Veilid's network bootstrap. Conversely, an HTTPS browser can
reach the named Poche gateway directly without Veilid, while the gateway may
itself use Veilid to participate on that browser device's behalf.

## Six separate layers

The implementation and UI name these boundaries independently:

1. **Veilid bootstrap:** attaches a Veilid node to a network; no Poche room or
   player authority follows.
2. **Poche rendezvous:** locates the current room ingress and expected host.
3. **Application membership:** consumes the invite and binds a stable player
   principal to the room/session epoch.
4. **Device authorization:** a player root certifies separately revocable local
   or gateway-held device keys; multiple devices do not add player votes.
5. **Event consensus/authority:** host-authoritative ordering or ADR 0007's
   experimental quorum rules accept effects.
6. **Projection protection:** exact-recipient state is either gateway plaintext
   or end-to-end encrypted to a device. Transport reachability alone grants no
   visibility.

## Gateway threat matrix

All gateway profiles see connection/IP/timing metadata, receive plaintext
signed HTTP commands in phase 7, can censor/delay/reorder deliveries, and can
replay bytes. Stable command IDs, signatures, and reducers prevent replay from
creating a second semantic effect; they do not prevent denial of service.

| Mode | Device key | Projection | Gateway can do | Gateway cannot claim |
| --- | --- | --- | --- | --- |
| Host-authoritative | Browser-local | Gateway plaintext | Learn full authoritative state and every hand; construct projections; unilaterally order as the disclosed host | It is anonymous, trustless, or unable to inspect hidden state |
| Replicated facilitator | Browser-local | Device end-to-end encrypted | See command/transport metadata and censor/reorder; relay signed commands and ciphertext | Forge the browser device, read its projection, add player voting weight, or commit without the named quorum |
| Replicated degraded custody | Gateway-custodied | Gateway-readable even when transported as device ciphertext | Impersonate that one disclosed device and read its authorized projection | Hold the player root, impersonate other devices, or silently become extra voting power |

The phase-7 HTTP command body is signed but not application-encrypted, so the
gateway sees command plaintext in every current profile. A future encrypted
ingress version would require a new disclosure schema rather than flipping a
label. End-to-end projection encryption is meaningful only with a browser-local
key; a gateway holding that recipient key can of course decrypt it.

## Custody, recovery, and simultaneous devices

The default browser device key remains browser-local where platform APIs and
persistence permit. A gateway-custodied certificate is an explicit degraded
mode using ADR 0007's `GatewayCustodied` class. It can never contain the player
root or another device secret. Escape is export-and-rotate: create and certify a
new local device, verify it through the player root/another authorized device,
then revoke the gateway device in a later membership epoch. Copying one key to
two locations does not create independent agency.

Gateway loss does not revoke the player. A browser can reconnect to a rotated
gateway route, use another already-known signed rendezvous route, or receive a
fresh invite. A native device can remain connected directly at the same time.
Route failover does not bypass membership or device revocation, and a partition
does not redefine consensus membership.

## Transport choice

Phase 7 uses bounded HTTP POST for browser-to-gateway commands and reconnectable
SSE for ordered exact-recipient projections. This is the Datastar-compatible
simple path, supports ordinary HTTP compression/caching infrastructure, and
keeps directionality visible. WSS or WebTransport is added only if measured
requirements such as true bidirectional streaming, unreliable datagrams, or
head-of-line constraints cannot be met. Neither transport changes the
application security model.

Direct browser Veilid over production HTTPS/WSS remains unsupported under ADR
0004 until task 7.3 repeats the no-companion public-network probe against the
pinned release. A `p3r-` direct locator does not change that status.

## Executable evidence

Five `room_code` tests cover old-prefix preservation, direct/gateway/loopback
round trips, the 256-byte bound, canonical re-encoding, redaction, expiry,
mutation, HTTPS origin restrictions, and remote-HTTP rejection. Three
deterministic `p3r-` vectors have digest
`blake3:b06e4dd25ce49e12663f02054a03b8e802f7dfbf36cd44985de32127b671560a`.

Four `gateway` protocol tests cover the three profiles, custody recovery, exact
canonical JSON, host/end-to-end incompatibility, root/other-device exclusion,
and softened-disclosure rejection. The three canonical disclosure profiles
have digest
`blake3:b07ec7cb405aaa5fbfd14f82ba7368b4fbe981eb697914f9b06b197fcb499df9`.

## Consequences

Task 7.2 must show the exact disclosure before live browser participation and
must implement the local/device-custody behavior it advertises. Task 7.3 may
change the direct-browser support row only from production-equivalent HTTPS/WSS
evidence. Route strings never enter device signing bytes, membership identity,
or consensus voting weight.
