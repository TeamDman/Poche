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

## NuSMV lifecycle and conditional liveness oracle

`models/nusmv/session.smv` is an independent temporal abstraction with one
durable host/player, one durable reconnecting player, one countdown token, six
room phases, and a two-action abstract game. Commands cover readiness,
unready/abort/expiry ordering, game action, pause/resume, player-one route
loss/reconnect, settlement abstraction, and absorbing close. The model does not
represent real time, packet delivery, cards, chat content, or cryptographic
bytes.

```text
cargo run -p poche-xtask -- session oracle check nusmv
```

NuSMV 2.7.1 reports a total, deadlock-free transition system and recognizes
all 16 named properties. Nine lifecycle/safety properties hold. Both CTL and
LTL conditional termination hold in `fair_all`. Unconditional CTL termination
is false, as required: an idle lobby, partitioned participant, or unresumed
pause can persist. Four additional CTL properties are false in precisely named
`missing_readiness`, `missing_expiry`, `missing_action`, and `missing_resume`
modes. The CLI requires a nonterminal lasso in each matching mode, so merely
reporting `false` without the intended witness fails the task.

The modes are explicit finite scheduler assumptions, not hidden NuSMV
`FAIRNESS` clauses. `fair_all` supplies eventual readiness/arm, expiry, accepted
game action, and resume, and deliberately visits `Paused` before the second
abstract game action. Each omission mode retains the deterministic prefix from
the other obligations and then stutters exactly where the omitted obligation
would be required. This proves the conditional claim only for the stated
abstraction; it does not claim an arbitrary network session must finish.

The temporal model covers the lifecycle portions of `S-ROOM-004` through
`S-ROOM-013`, `S-ROOM-016`, `S-ROOM-019` through `S-ROOM-020`,
`S-ROOM-022` through `S-ROOM-024`, `S-TIME-001` through `S-TIME-004`,
`S-TIME-006` through `S-TIME-008`, and `S-FAULT-004`, `S-FAULT-007`,
and `S-FAULT-008`. The coverage matrix retains planned or reasoned-N/A
dispositions for unrelated protocol, cryptographic, knowledge, chat-content,
and concrete-game rules.
