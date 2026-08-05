# Veilid command transport boundary

Task 5.4 uses Veilid `AppCall` as an idempotent request/reply carrier. It does
not make Veilid routes, node IDs, DHT updates, or delivery order authoritative.
The stable application signature, session policy, command ID, expected
revision, and host-signed result remain the semantic boundary.

## Wire contract

`TransportCommandCall` carries one complete stable-key-signed
`CommandEnvelope`. `TransportCommandReply` binds the command ID, disposition,
base/current revisions, optional denial, and authority-ordered event frames.
Viewer projections and public errors may follow the events, but commands,
snapshots, and arbitrary frame mixtures are rejected in a command reply.

Both objects use strict `deny_unknown_fields` JSON, canonical re-encoding, the
30,000-byte Poche safe ceiling below Veilid's 32,768-byte operation limit, and
registered protocol envelope validation. A reply cannot claim a revision
advance without a contiguous event sequence. Denial cannot advance revision or
smuggle non-error frames.

The released Veilid 0.5.7 adapter provides:

- client `command_call` over the resolved private route;
- host `decode_command_call`, which deliberately ignores transport sender as
  an application identity;
- host `reply_command_call`, using the incoming call ID exactly once; and
- stable redacted mappings for `TryAgain`, timeout, no connection, invalid or
  stale target, shutdown, oversize, malformed wire, and permanent failure.

## Retry and recovery

`CommandRetryState` is bound to the command ID and the full canonical encoded
command, including its signature. Every retry must present identical bytes.
The authority's existing processed-command log then makes duplicate delivery
idempotent and rejects conflicting command-ID reuse.

| Failure | Action before budget exhaustion |
| --- | --- |
| `TryAgain`, timeout | Retry the same resolved route with the exact command |
| No connection, stale route, dead watch | Fetch and validate rendezvous data, replace the route, retry exact command |
| Shutdown | Stop as shutdown; never report a semantic denial |
| Oversize, invalid message, permanent failure | Stop permanently; never retry malformed input |

Budget exhaustion is its own result. It cannot be reclassified as a game-rule
denial or successful transition. A revision gap or `RecoveryRequired` reply
uses the signed snapshot + contiguous event-tail boundary from Task 5.3.

## DHT watches are hints

Resolved rooms keep their encrypted DHT record open while watched. A matching
live `ValueChange` means only `FetchAndValidate`; a dead/empty watch means
`RenewWatchThenFetchAndValidate`; and a dead current route means
`ReplaceRouteFromValidatedRendezvous`. Callback values and callback order are
discarded as authority evidence. Fetching still revalidates network, room,
stable host identity, session epoch, expiry, and route shape.

## Countdown and authorization

Clients may display a saturating estimate based on the latest authority tick
sample. When the estimate reaches zero it explicitly says it is awaiting the
authority transition. Only the authority clock may emit `CountdownExpired`,
and the session reducer rechecks the token and current prerequisites.

Chat, pause/unpause, countdown, and game commands all use the same
`CommandEnvelope`, application signature, authorization, decision, revision,
and reply path. There is no transport-only chat or game shortcut.

## Current evidence boundary

```powershell
cargo test -p poche-veilid --features veilid --offline
cargo clippy -p poche-veilid --all-targets --features veilid --offline -- -D warnings
```

These checks exercise strict call/reply codecs, exact-command retry binding,
every retry category, released 0.5.7 error mapping, watch/route hint semantics,
countdown display estimates, and the compiled AppCall/watch APIs. Existing
session/runtime tests independently prove duplicate idempotence, authorization,
authority-clock countdown, pause, chat, and full-game behavior.

The Task 5.4 checkbox remains open until the complete Phase 4 scenario crosses
a local multi-node Veilid harness. Veilid 0.5.7 private-route allocation
requires `PublicInternet`; ordinary loopback/LAN nodes do not satisfy it, and
the released virtual-network implementation is incomplete. The opt-in native
topology and separate-process evidence are therefore retained for Task 5.6
rather than relabeling the wire/unit harness as a native pass.
