# Veilid command transport boundary

Task 5.4 uses Veilid `AppCall` as an idempotent request/reply carrier. It does
not make Veilid routes, node IDs, DHT updates, or delivery order authoritative.
The stable application signature, session policy, command ID, expected
revision, and host-signed result remain the semantic boundary.

## Wire contract

Transport schema v2 `TransportCommandCall` carries the stable application's
public identity plus one complete stable-key-signed `CommandEnvelope`; decoding
revalidates the signature before the authority sees the command. A node ID or
route is never accepted as the caller. `TransportCommandReply` binds the
command ID, disposition,
base/current revisions, optional denial, and authority-ordered event frames.
Public errors may follow the events. Viewer state may appear only as one
host-signed exact-recipient `EncryptedProjectionPacket`; plaintext projection
frames, commands, snapshots, and arbitrary frame mixtures are rejected.

Both objects use strict `deny_unknown_fields` JSON, canonical re-encoding, the
30,000-byte Poche safe ceiling below Veilid's 32,768-byte operation limit, and
registered protocol envelope validation. A reply cannot claim a revision
advance without a contiguous event sequence. Denial cannot advance revision or
smuggle non-error frames or encrypted viewer state.

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

## Native acceptance boundary

```powershell
cargo test -p poche-veilid --features veilid --offline
cargo clippy -p poche-veilid --all-targets --features veilid --offline -- -D warnings
cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-local
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST='I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-public
```

These checks exercise strict call/reply codecs, exact-command retry binding,
every retry category, released 0.5.7 error mapping, watch/route hint semantics,
countdown display estimates, and the compiled AppCall/watch APIs. Existing
session/runtime tests independently prove duplicate idempotence, authorization,
authority-clock countdown, pause, chat, and full-game behavior. The guarded
public smoke then carries the complete scenario between distinct native Veilid
nodes over DHT discovery, private routes, and `AppCall`: 172 calls produced 170
contiguous signed event frames and final revision 171, with exact duplicate
handling, denial while paused, route refresh/reconnect, 13 rounds, chat, and
encrypted spectator grant/revocation ending at score 40-20.

The local command intentionally combines the network-free semantic/formal suite
with a reproducible released-0.5.7 topology diagnostic. It does not claim
private-route delivery because released local bootstrap excludes LocalNetwork
peers while private routes require PublicInternet readiness. Actual transport
delivery is owned by the explicit public opt-in command; it is excluded from
ordinary CI and RL. Full measurements and limitations are in
`docs/veilid-native-acceptance.md`.
