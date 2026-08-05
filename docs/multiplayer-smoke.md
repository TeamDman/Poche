# In-process multiplayer acceptance smoke

Task 4.5 provides one release-style command for the no-socket multiplayer
slice:

```text
cargo run -p poche-xtask -- multiplayer smoke --transport in-process
```

The command records a secret-free symbolic transcript while driving the strict
canonical-NDJSON loopback transport. It then creates a fresh authority and
replays the exact inputs, requiring every disposition, revision, event count,
phase, and viewer-projection hash to agree. Invite references resolve to
runtime-only values which are scanned out of the serialized transcript.

The scenario creates a host, player, and spectator; joins and seats members;
exercises ready, unready, re-ready, an aborted countdown, and a completed
countdown; chats in lobby, countdown, running, paused, and post-game phases;
proves that a paused game action is denied; and uses different seated players
to pause and unpause. After the first seeded deal, the spectator requests the
host hand, the host grants it, the scenario compares the exact scoped cards,
and revocation removes future access. The player disconnects, establishes a
new route for the same principal, and reconnects before play continues.

The full two-player Poche oracle then completes all 13 deals by choosing the
first legal action in canonical order and using seed `0x5eed` for chance. The
run records terminal scores, resets the post-game room to a lobby, and closes
it. The normalized 178-input summary is pinned at
`evidence/multiplayer-in-process-smoke.json`; the complete transcript is
regenerated rather than committed because only its hash is acceptance
material.

After semantic replay, the same command requires the protocol fixture gate,
the 88-rule coverage audit, and zero-disagreement Rust/Alloy/NuSMV/Scryer
Prolog session comparison. The evidence file is compared structurally, so any
input count, revision, score, feature, transcript hash, or public-result hash
change fails until deliberately reviewed.
