# Deterministic protocol transcripts and recovery

Task 2.4 adds a checked-in golden transcript at
`tests/fixtures/protocol/session-micro-v1.json` and verifies every JSON fixture
with:

```text
cargo run -p poche-xtask -- protocol replay --all
```

The 32-step fixture uses a deliberately small `SessionGame` implementation so
it can exercise the complete session/protocol lifecycle without duplicating the
full Poche oracle's gameplay fixtures. It covers create, secret-reference invite
redemption, seats, readiness, countdown abort and start, explicit chance,
pause/denied advance/unpause, chat, hand request/deny/grant/duplicate/revoke,
round-boundary expiry, disconnect/reconnect, player actions, settlement,
post-game, stale/reordered input, reset, and close.

Each step stores the secret-free typed input, command semantic hash,
authorization result, semantic outcome, emitted event-kind sequence, complete
semantic state hash, and the hash of every currently available viewer-scoped
projection. A duplicate command retains the same state/projection hashes. A
denied or stale command has no events and leaves them unchanged.

Privacy-relevant checkpoints additionally store the complete typed projection
payload for every connected viewer: game start, grant, revoke, the second grant,
disconnect, reconnect, round settlement/expiry, and close. These are explicitly
recipient-scoped fixture values; ordinary steps retain compact per-recipient
hashes to avoid repeating unchanged payloads.

## Snapshot v1

The first persisted snapshot representation favors inspectability over restore
speed. Its state bytes are canonical JSON for a bounded prefix of
`FixtureInput`; invite operations contain stable references such as `alice`, not
the invite verifier. The `SnapshotPayload` binds:

- the protocol schema hash;
- the complete semantic state hash;
- the authority tail revision, room phase, and public members;
- the secret-free canonical prefix bytes.

Restore decodes and replays the prefix through the same authorization,
decision, and apply path, then replays the remaining tail. The verifier compares
both the final semantic state hash and every final viewer projection hash with a
full genesis replay. This representation is intentionally not a claim of fast
binary hydration; a future snapshot schema may optimize it without weakening
the replay oracle.

The semantic state commitment covers membership/connection/seat state, invite
status without verifiers, processed command IDs and semantic hashes, event
counts, lifecycle phase, capability/request epochs, public history/state,
current private hands, policy count, and chat count. Processed chance-command
hashes bind hidden chance input not otherwise present in public projections.

## Failure evidence

Tests delete a line, reorder request/deny steps, and inject a controlled apply
defect that drops the revoke event. Verification reports the first divergent
step and field (for example `step 21 first divergence: state_hash`) rather than
only returning a final checksum mismatch.

Fixtures and rendered diagnostics are scanned to ensure neither runtime-only
invite verifier appears. No secret key material is part of the runner or file.
Task 2.5 will drive these same typed meanings through canonical NDJSON and any
enabled Phon codec; Task 2.4 does not claim that parity early.
