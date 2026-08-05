# Independent formal session models

The session oracles are handwritten from `session-rules.md`. They are not
generated from, and do not call, the Rust reducer. This separation makes a
cross-model agreement result useful rather than circular.

## Alloy room, authorization, and knowledge oracle

`models/alloy/session.als` is a bounded relational model with exactly five
named principals, two seats, and either two or four snapshots per command.
Every command uses a 5-bit integer scope. The native receipt is produced by:

```text
cargo run -p poche-xtask -- session oracle check alloy
```

The registered suite contains thirteen commands. `ValidRoomWitness` and
`PauseResumeWitness` are SAT. Eight safety assertions are UNSAT:
`DefaultDeny`, `SingleSeatOwnership`, `NoStartWithoutReadiness`,
`AtMostOnceStart`, `ScopedSpectatorGrant`,
`RevocationStopsFutureKnowledge`, `NoUnauthorizedKnowledge`, and
`PauseResumePreserveRoom`. The three deliberately invalid predicates
`DefectiveDuplicateSeatWitness`, `DefectiveStartUnreadyWitness`, and
`DefectiveRoomWideHandWitness` are SAT. The command receipt records each exact
scope and rejects omitted, renamed, duplicated, or outcome-mismatched commands.

The model covers the relational parts of `S-ROOM-001`, `S-ROOM-003` through
`S-ROOM-004`, `S-ROOM-006`, `S-ROOM-008` through `S-ROOM-009`,
`S-ROOM-011` through `S-ROOM-013`, `S-ROOM-024`, `S-AUTH-001`,
`S-VIEW-001` through `S-VIEW-002`, `S-VIEW-004` through `S-VIEW-007`,
`S-VIEW-010`, `S-VIEW-013`, `S-TIME-004`, `S-TIME-006` through
`S-TIME-007`, and `S-FAULT-007`. Other Alloy cells retain their explicit
planned or reasoned-N/A disposition in `session-coverage.md`.

The abstraction is deliberately current-state and future-delivery oriented.
`seesHand` is derived from self-knowledge plus exact owner-to-viewer grants;
revocation proves the absence of a later edge, not erasure of human memory.
Cryptographic bit strings, canonical bytes, network routes, chat text, game
card semantics, and an actively malicious host are outside this relational
scope. Membership and seats are distinct, but connection epochs and concrete
capability tokens are abstracted to membership and grant relations. Alloy
results are therefore bounded consistency evidence, not an unbounded theorem
or a transport-security proof.
