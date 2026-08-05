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

## Scryer Prolog policy and predecessor oracle

`models/prolog/session.pl` is a handwritten relational model over one host,
one other player, one spectator, and one outsider. Its bounded room term keeps
phase, two readiness flags, player-one connectivity, one pending request, one
exact hand grant, start count, and a two-step abstract game. `step/5` is usable
forward and backward for bounded ground states; `predecessor/5` is the same
relation with its arguments oriented for explanation queries. `decision/4`,
`can_see/5`, `projection_chain/5`, `revocation_chain/5`, and
`possible_history/2` remain native Prolog relations and do not call Rust.

```text
cargo run -p poche-xtask -- session oracle check prolog
```

The command executes seven sorted answer-set fixtures and pins both count and
BLAKE3 digest:

| Query | Rows | BLAKE3 |
| --- | ---: | --- |
| `policy_decisions` | 11 | `bc866fb812a92afa20d26f197a356bc86307b1479af6d836624117b6402e04f6` |
| `successors` | 13 | `6d2fdd3044e24e09d532d3faf37652fd668fdf5d2f205f2e870413259d6372cb` |
| `predecessors` | 12 | `fce25d53cce3449006108f711a5c28957363064699b57aec87dc519956fb9387` |
| `visibility` | 16 | `8ebef323c2929242b53ddb774569c67e6facd4f1b232b70c5eb6c3e7ba8e72cc` |
| `grant_chains` | 4 | `98d2652614fc505cbfab64245a6f35325effbeabadfb04b6f3780a9689c1b00d` |
| `history_causes` | 3 | `a35c7c6a3428854928cc3eaa02f59014a2f2e7212231da4913f9a456c86244b5` |
| `controlled_defects` | 4 | `7883641e7f9687f4765beae11ed34ca4b6b2e3b1db0d51d7e504acfec721b7e9` |

Every allow, denial, transition, visibility edge, and explanation carries a
stable `S-*` rule ID. The defect corpus separately witnesses an outsider-pause
policy mistake and room-wide hand disclosure while the correct relations
return default deny and require an exact grant. The fixture runner only accepts
repository-local models and lowercase module/goal atoms, then checks its framed
row count; pinned digests detect any later missing or extra solution.

The Prolog scope covers query-oriented parts of `S-ROOM-004` through
`S-ROOM-013`, `S-ROOM-016`, `S-ROOM-019` through `S-ROOM-020`,
`S-ROOM-022`, `S-AUTH-001`, `S-AUTH-010`, `S-VIEW-001` through
`S-VIEW-007`, `S-VIEW-010`, `S-VIEW-013`, `S-CHAT-001`,
`S-CHAT-007`, `S-TIME-003` through `S-TIME-004`, and `S-TIME-008`.
It does not infer cryptographic validity, network delivery, arbitrary-length
histories, chat text, or concrete card legality.
