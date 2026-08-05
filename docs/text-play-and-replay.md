# Text play and replay

Task 4.3 keeps text at the presentation boundary. Renderers accept only a
viewer-scoped `ProjectionPayload`, a supplied legal-action list, an attributed
chat item, or a logical countdown estimate. They cannot query authoritative
state, calculate legality, authorize a command, or read wall time.

The checked driver is
`tests/fixtures/protocol/session-micro-v1.script.ndjson`. Each LF-terminated
line is one canonical, secret-free `FixtureInput`. Invite references are names
resolved only inside the test runtime; invite verifier values never enter the
script, transcript, text, or logs.

Replay it for an inspectable view:

```text
cargo run -p poche-cli -- --output text transcript replay tests/fixtures/protocol/session-micro-v1.script.ndjson
```

Every numbered step names the actor, command ID, revision binding, action,
authorization decision, outcome, emitted event list, state hash, and every
current viewer-projection hash. The script includes two seated players and an
unseated spectator, an aborted and re-armed countdown, pause, a denied action
while paused, resume, chat, hand request/grant/revoke, disconnect/reconnect,
game completion, stale-command denial, reset, and close.

Machine consumers may select `--output json` for the replay summary or
`--output ndjson` for 32 complete step records plus one terminal summary.
`transcript inspect tests/fixtures/protocol/session-micro-v1.json` presents the
checked full golden transcript without executing it.

The release gate is:

```text
cargo run -p poche-xtask -- protocol replay --all
```

It requires every checked transcript JSON to have a companion script and
compares the script's typed replay, strict NDJSON-framed replay, and checked
state/projection hashes.
