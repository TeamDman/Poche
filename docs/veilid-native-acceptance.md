# Native Veilid acceptance

Status: Tasks 5.2, 5.4, and 5.6 complete (2026-08-05).

## September desktop firewall experiment

On September 9–10, 2026, the protected desktop two-process native-input test
was compared before and after accepting Windows firewall permission. This is
separate from the older August gates below; it does not mark the desktop MVP
complete.

| Exact-program Public-profile policy | Complete successes | Successful run wall times | Failed run |
| --- | --- | --- | --- |
| TCP/UDP inbound Block | 2/3 | 74.11s, 87.49s | Join admission transport unavailable, 39.33s |
| TCP/UDP inbound Allow | 2/3 | 54.27s, 48.43s | Create invocation transport unavailable, 15.74s |

One 62.55s passing prompt warm-up is excluded because permission changed during
that run. All six measured runs used the same executable path and SHA-256,
source at `1fa1af3`, native-input feature set, production connector and
protected-profile policy. Each run used fresh node identities; successful runs
included remote private-safe card motion and participant recovery after forced
termination. These were windowless pointer-pipeline tests, not GPU/OS-input or
complete gameplay acceptance.

Accepting the prompt did **not eliminate the intermittent failure**. The
post-approval successes were faster, but three trials per condition, roughly
22 hours between samples and uncontrolled public peer conditions do not establish a causal
performance improvement. The blocked-policy successes also do not identify
the route mechanism or establish reliable popup-free operation.

Only the two specifically approved block rules were removed, after full local
backups and narrowly scoped Windows administrator approval. The resulting
executable-specific Public TCP/UDP Allow rules remain installed; firewall
defaults, other rules, router configuration and the Ethernet profile were not
changed. The game/test did not run through that administrator helper. This
does not impose an administrator-only runtime requirement or automatically
grant access to other Poche executable paths.

The exact artifact, command, per-run timestamps, rule identifiers, backup
locations and interpretation limits are in
[PLAN-6, T2a](../PLAN-6-DESKTOP-VEILID.md#x-t2a-measure-the-effect-of-accepting-windows-firewall-permission).
Do not rebuild that artifact in the middle of a comparison or treat an allow
rule for a different binary/profile as the same condition. Further work should
investigate uncertain Create/join outcomes, without treating a missing reply
as proof that a state-changing action was rejected.

## Historical August gates

Poche has two deliberately different native Veilid gates. The default local
gate is network-free and reproducible. It runs all protocol, authorization,
formal-agreement, lifecycle, retry, projection, and crypto tests, then records
the released Veilid 0.5.7 isolated-topology limitation. The public gate is a
manual, explicit opt-in that proves real DHT, private-route, and `AppCall`
delivery between two distinct native nodes.

## Local gate

```powershell
cargo run -p poche-xtask --offline -- transport test veilid-local
cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-local
```

The transport command passed 26 Veilid units and two compile-fail docs. The
multiplayer command passed the 88-row session coverage audit, Rust/Alloy/
NuSMV/Scryer-Prolog agreement with zero disagreements, and the deterministic
in-process lifecycle (178 inputs, revision 177, 13 deals). The local probe then
started two isolated UDP nodes and reproduced zero peers, no public/local
readiness, and no private route.

That last outcome is the expected topology result, not an ignored transport
failure. Released 0.5.7 direct bootstrap returns PublicInternet peers and skips
LocalNetwork nodes, while private-route allocation requires PublicInternet
readiness. Its virtual-network path is incomplete. The diagnostic-only feature
uses `footgun-nodeid-target` to let isolated nodes start but never sends or
labels local application bytes as Veilid traffic.

## Public transport gate

This command contacts the public Veilid network and is intentionally absent
from ordinary unit tests, CI, and RL rollouts:

```powershell
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST='I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run -p poche-xtask --offline -- transport test veilid-public
```

On 2026-08-05, two clean temporary nodes reached public readiness (64 host
peers and 52 client peers). The host created and flushed a DFLT record and a
private route; the client resolved the encrypted record, imported the route,
sent an `AppCall`, and received the reply. Storage directories and node
identities were distinct and ephemeral.

## Full lifecycle gate

```powershell
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST='I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run -p poche-xtask --offline -- multiplayer smoke --transport veilid-public
```

The final successful precompiled run took 98.7 seconds and reported:

- 174 request/reply calls and 172 signed contiguous event frames;
- one exact duplicate reply and one authorization denial;
- one disconnect, validated DHT/private-route refresh, and reconnect;
- room creation, one-time code redemption, seats, ready/countdown/abort;
- all 13 rounds, player leave, host close, final revision 173, and preserved
  final score 40-20;
- any-player pause and unpause, with a game action denied while paused;
- three chat messages; and
- spectator request/grant/revoke with exact-recipient decryption succeeding
  only while the grant was current; and
- explicit `spectator-grant`, `spectator-revoke`, `player-leave`, and
  `room-close` verification flags in the strict report.

The authority is the real `InProcessAuthority<OracleSessionGame<2>>` reducer.
Every remote command crosses Veilid as transport schema v2 and includes a
stable application identity whose signature is revalidated on decode. Replies
carry signed event frames and, where authorized, one encrypted projection.
Timeout/TryAgain retries preserve the exact command bytes. No-connection,
stale-route, and watch-renewal recovery releases the stale route, rereads and
validates DHT state, imports the replacement route, and resends those same
bytes. Node and route identities never grant application authority.

## Limits

- This is native protocol acceptance, not a packaged end-user client.
- The public smoke shares one remote Veilid node among multiple stable
  application principals; host and remote nodes are separate, while player and
  spectator authorization remains independently keyed at the application
  layer.
- Disconnect/reconnect is a semantic client-session disconnect followed by a
  real private-route command after rendezvous refresh; persistent membership
  restart is separately covered by protected-store tests.
- Temporary acceptance nodes use Veilid's explicitly insecure development
  protected-store configuration. Production clients must configure durable,
  credentialed protected storage.
- Public Veilid peers can observe network metadata. Hand projections are
  encrypted for exact recipients, but neither traffic-analysis resistance nor
  formal verification of Veilid's cryptography is claimed.
- Browser-only HTTPS/WSS Veilid remains unsupported in 0.5.7; this native pass
  does not reopen the Datastar browser-topology decision.

Machine-readable measurements live in
`evidence/veilid-native-acceptance.json`.
