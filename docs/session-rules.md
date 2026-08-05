# Session and authorization rule catalog

- Status: Phase 1 catalog complete; implementation evidence is tracked in
  `session-coverage.md`
- Architecture source: `decisions/0003-session-network-rl-architecture.md`
- Human guidance: U30-U50 in `PLAN-2-MULTIPLAYER-RL-RENDERING.md`

These IDs are immutable evidence anchors. If semantics change, retain the old
row, mark it superseded, and add a new ID. A later implementation may split a
rule into smaller internal predicates, but it may not silently weaken the human
rule or merge away its deny behavior.

Classes are `session-semantic`, `transport`, `security-assumption`,
`game-semantic`, or `UI-only`. The session catalog gates access to the existing
game semantics; it does not redefine Poche bids, card play, scoring, or game
termination.

## Stable deny reasons

Every rejected command returns one stable reason and applies no partial event.
Reasons may gain structured detail, but their meaning does not change.

| ID | Meaning |
| --- | --- |
| `D-MALFORMED` | Bytes, UTF-8, JSON, canonical framing, or required fields are invalid. |
| `D-OVERSIZE` | Envelope, payload, or chat text exceeds its versioned bound. |
| `D-UNKNOWN-VERSION` | Protocol or signature-domain version is unsupported. |
| `D-UNKNOWN-COMMAND` | Command tag is not registered for this protocol version. |
| `D-UNKNOWN-ROLE` | Claimed principal/role kind is not registered. |
| `D-UNKNOWN-PRINCIPAL` | Principal key has no applicable membership or system role. |
| `D-BAD-SIGNATURE` | Strict signature verification failed or canonical signed bytes differ. |
| `D-WRONG-ROOM` | The command is bound to another room. |
| `D-STALE-EPOCH` | Session, membership, or capability epoch is no longer current. |
| `D-STALE-REVISION` | Expected authority/game revision is not current. |
| `D-REVOKED` | Membership, invite, or capability has been revoked. |
| `D-MISSING-CAPABILITY` | No active allow covers the exact action/resource scope. |
| `D-DENY-POLICY` | An applicable explicit deny overrides all allows. |
| `D-WRONG-PHASE` | The room lifecycle phase does not permit the command. |
| `D-CLOSED` | The room is absorbing and accepts no state-changing command. |
| `D-NOT-SEATED` | The principal is not an active seated player. |
| `D-SEAT-OCCUPIED` | The requested seat is already held by another principal. |
| `D-ALREADY-SEATED` | The principal already holds a seat. |
| `D-NOT-CONNECTED` | The command requires a currently connected member. |
| `D-NOT-READY` | Countdown readiness/minimum-player preconditions are false. |
| `D-COUNTDOWN-INACTIVE` | Abort/expiry refers to no current countdown. |
| `D-NOT-ACTOR` | The seated principal is not the current game actor. |
| `D-PAUSED` | A game/chance/settlement command is forbidden while paused. |
| `D-NOT-PAUSED` | Unpause requires the room to be paused. |
| `D-ALREADY-PAUSED` | Pause was requested after another command already paused this revision. |
| `D-INVITE-INVALID` | Invite format, checksum, verifier, room, or use count is invalid. |
| `D-INVITE-EXPIRED` | Invite deadline has passed. |
| `D-GRANT-SCOPE` | Hand request/grant/revoke actor, player, recipient, or capability does not match. |
| `D-CHAT-SIZE` | Chat text is empty or outside the byte/character bound. |
| `D-CHAT-RATE` | Principal exceeds the versioned chat rate window. |
| `D-ENVIRONMENT-ONLY` | Player/host attempted a clock, chance, or deterministic settlement action. |

Duplicate command IDs are not a denial: they return the previously recorded
decision/events without reapplying them.

## Room and game-composition rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `S-ROOM-001` | U33, U35, G15, ADR composition | New host | Valid host application key; no existing room ID | Create `Lobby`, epoch 0, revision 0; host is a connected member with host capability | Duplicate/wrong identity is `D-MALFORMED` or `D-MISSING-CAPABILITY` | session-semantic |
| `S-ROOM-002` | U33, U40, G19 | Invite holder | Valid unexpired unused invite for this room and signed stable key | Add connected unseated membership bound to the stable key; consume one-time invite | `D-INVITE-INVALID`, `D-INVITE-EXPIRED`, `D-WRONG-ROOM`, or `D-REVOKED` | session-semantic |
| `S-ROOM-003` | U35, ADR identity | Host or authorized member policy | Lobby; connected unseated member; free seat | Bind exactly one principal to exactly one seat | `D-WRONG-PHASE`, `D-SEAT-OCCUPIED`, or `D-ALREADY-SEATED` | session-semantic |
| `S-ROOM-004` | U35, G22 | Seated member | Lobby; connected; seat held | Set ready true for that seat | `D-WRONG-PHASE`, `D-NOT-SEATED`, or `D-NOT-CONNECTED` | session-semantic |
| `S-ROOM-005` | U35, G22 | Seated member | Lobby or countdown; seat held | Set ready false and cancel any countdown | `D-WRONG-PHASE` or `D-NOT-SEATED` | session-semantic |
| `S-ROOM-006` | U35, G22 | Host | Lobby; minimum seats; every occupied seat connected and ready | Enter `Countdown` with a logical deadline/token | `D-NOT-READY`, `D-WRONG-PHASE`, or `D-MISSING-CAPABILITY` | session-semantic |
| `S-ROOM-007` | U35, G22 | Any active seated player | Current countdown | Cancel countdown and return to ready-state lobby without starting | `D-NOT-SEATED`, `D-COUNTDOWN-INACTIVE`, or stale revision | session-semantic |
| `S-ROOM-008` | U35, G22, ADR countdown | Authority clock | Matching current countdown token; readiness still true | Start exactly one game, enter `Running`, clear ready/countdown | `D-COUNTDOWN-INACTIVE`, `D-NOT-READY`, `D-ENVIRONMENT-ONLY`, or stale revision | session-semantic |
| `S-ROOM-009` | U35, G22 | Any principal | Any phase | No direct `StartGame` command exists; start can only follow accepted expiry | `D-UNKNOWN-COMMAND` or `D-ENVIRONMENT-ONLY` | session-semantic |
| `S-ROOM-010` | U32, G15, ADR composition | Current seated actor | Running and unpaused; exact current game/session revision; legal typed action | Apply one `GameEnvironment` player transition and emit resulting session event(s) | `D-NOT-ACTOR`, `D-WRONG-PHASE`, `D-PAUSED`, stale revision, or game rule error | session-semantic |
| `S-ROOM-011` | U48, G23 | Any active seated player | Running and unpaused | Enter `Paused`; preserve the exact game state | `D-NOT-SEATED`, `D-WRONG-PHASE`, or `D-ALREADY-PAUSED` | session-semantic |
| `S-ROOM-012` | U48, G23 | Any active seated player | Paused | Return to `Running`; preserve the exact game state | `D-NOT-SEATED`, `D-NOT-PAUSED`, or stale revision | session-semantic |
| `S-ROOM-013` | U48, G23, G30 | Any game/environment actor | Paused | Chat, reconnect, grants, leave, and unpause remain possible; game/chance/settlement does not advance | Game/chance/settlement returns `D-PAUSED` | session-semantic |
| `S-ROOM-014` | U31, G15 | Environment settlement | Running; game requests deterministic settlement | Apply existing game settlement; emit raw round scores unchanged | `D-ENVIRONMENT-ONLY`, `D-PAUSED`, or game error | game-semantic |
| `S-ROOM-015` | U37, G15 | Chance adapter | Running; game requests chance; explicit validated chance value | Apply existing chance transition and retain replay provenance | `D-ENVIRONMENT-ONLY`, `D-PAUSED`, or game error | game-semantic |
| `S-ROOM-016` | U31, ADR composition | Environment | Accepted game transition is terminal | Enter `PostGame`; retain outcome and room membership | Nonterminal transitions remain `Running` | session-semantic |
| `S-ROOM-017` | U33, ADR composition | Host | PostGame; room not closed | Clear prior game/results only as archived history; return seated members to unready `Lobby` | `D-WRONG-PHASE` or `D-MISSING-CAPABILITY` | session-semantic |
| `S-ROOM-018` | U35, U40 | Member | Room open | Remove active connection/seat/readiness; voluntary leave revokes current membership epoch; cancel countdown if affected | Unknown principal is `D-UNKNOWN-PRINCIPAL` | session-semantic |
| `S-ROOM-019` | U40, G20 | Runtime | Connected member loses route/transport | Mark disconnected and unready; retain durable membership; cancel countdown if affected | Unknown transport cannot invent membership | session-semantic |
| `S-ROOM-020` | U40, G20 | Stable member key | Valid current membership proof, new transport/route, room open | Restore connection without invite reuse; seat restoration follows recorded membership policy | `D-REVOKED`, `D-STALE-EPOCH`, `D-BAD-SIGNATURE`, or `D-CLOSED` | session-semantic |
| `S-ROOM-021` | U34, U40 | Host | Active target membership; host capability | Revoke/remove membership, release seat/grants, increment epochs, cancel affected countdown | `D-MISSING-CAPABILITY`, `D-UNKNOWN-PRINCIPAL`, or self/host policy denial | session-semantic |
| `S-ROOM-022` | U33, G18 | Host | Room open | Enter absorbing `Closed`; revoke routes/invites and reject later mutation | `D-MISSING-CAPABILITY` or `D-CLOSED` | session-semantic |
| `S-ROOM-023` | G18, no host migration | Host disconnect/leave | Host authority unavailable | Room cannot appoint a new host in this phase; clients preserve evidence and report unavailable/closed | No implicit election; network/liveness limitation is explicit | security-assumption |
| `S-ROOM-024` | U35, U41 | Any membership operation | Named fixed room scope | Membership and seat occupancy remain distinct; one key holds at most one seat; spectators do not count toward readiness | Conflicts deny with stable seat/role reasons | session-semantic |

## Authorization, identity, and integrity rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `S-AUTH-001` | U34, G21 | Policy engine | Any attempt | Default deny unless an active exact allow applies | Unknown principal, role, command, or scope returns its stable deny reason | session-semantic |
| `S-AUTH-002` | U32, ADR authority | Command author | Registered protocol version | Envelope binds version, room, session epoch, command ID, principal key, expected revision, payload, and signature metadata | Missing/malformed fields return `D-MALFORMED` | security-assumption |
| `S-AUTH-003` | U34, G21 | Verifier | Structurally valid bounded command | Strictly verify application signature before authorization/reduction | `D-BAD-SIGNATURE`; no mutation | security-assumption |
| `S-AUTH-004` | G16, ADR authority | Codec/verifier | Registered signature-domain version | Produce versioned length-framed canonical signed bytes with stable payload tags | Unknown domain is `D-UNKNOWN-VERSION`; diagnostic JSON is never signed as-is | security-assumption |
| `S-AUTH-005` | Security invariants | Authority | Previously recorded command ID for same room/epoch/principal | Return the recorded decision/events without advancing revision | Conflicting reuse is `D-MALFORMED` or `D-BAD-SIGNATURE` | session-semantic |
| `S-AUTH-006` | Security invariants | Authority | New command | Require exact room ID | `D-WRONG-ROOM`; no partial event | session-semantic |
| `S-AUTH-007` | Security invariants | Authority | New command | Require current session/membership/capability epoch as applicable | `D-STALE-EPOCH`; no partial event | session-semantic |
| `S-AUTH-008` | Security invariants | Authority | New command | Require exact expected authority/game revision | `D-STALE-REVISION`; no partial event | session-semantic |
| `S-AUTH-009` | U34, G21 | Policy engine | Multiple policies apply | Any enforce-mode deny overrides every allow | `D-DENY-POLICY` with winning policy ID/reason | session-semantic |
| `S-AUTH-010` | U34, G21 | Policy engine | Every attempt | Emit immutable allow/deny, stable policy ID, reason, and audit-only results | Missing reason is an implementation defect | session-semantic |
| `S-AUTH-011` | U34, G21 | Audit policy | Audit-only policy evaluates | Record hypothetical result without changing enforce result | Audit allow never grants authority | session-semantic |
| `S-AUTH-012` | U34, Veilid findings | Runtime | Authenticated transport peer or Veilid operation | Treat peer/route/signature as transport evidence only | No app capability means `D-MISSING-CAPABILITY` | security-assumption |
| `S-AUTH-013` | U40, G20 | Identity layer | Stable app public key | Derive `PrincipalId` independently of Veilid node ID/private route | Route/node changes cannot change membership identity | security-assumption |
| `S-AUTH-014` | U33, G19 | Invite issuer | Host capability; room open | Issue versioned checksummed locator plus one-time/expiring verifier | Secret never grants permanent membership by itself | security-assumption |
| `S-AUTH-015` | U40, G19-G20 | Invite redeemer | Valid invite and app signature | Bind durable membership to stable app key; consume/restrict invite | Invalid, replayed, expired, cross-room, or revoked invite denies | session-semantic |
| `S-AUTH-016` | G18, security invariants | Host authority | Accepted decision/reduction | Sign event with room, epoch, revision, causation/correlation, payload, and semantic identity | Unsigned/invalid event is rejected by clients | security-assumption |
| `S-AUTH-017` | G18, security invariants | Host authority | Event emission | Form one gap-free monotonically increasing revision sequence | Duplicate/gap/conflict triggers recovery, never blind application | session-semantic |
| `S-AUTH-018` | Security invariants | Snapshotter | Current state/revision | Commit snapshot to room, epoch, revision, schema/semantic hashes, and event-tail boundary | Hash/revision mismatch rejects recovery | security-assumption |
| `S-AUTH-019` | U40, security invariants | Identity/log layers | Any output path | Secret keys and invite verifiers never enter logs, errors, snapshots, fixtures, DHT public values, or room-wide events | Detection is a security test failure; output is redacted | security-assumption |
| `S-AUTH-020` | G16, Veilid limit | Decoder | Untrusted bytes | Enforce UTF-8, one-frame, nesting/length, envelope, and transport-size bounds before allocation/reduction | `D-MALFORMED`, `D-OVERSIZE`, or `D-UNKNOWN-VERSION` | transport |
| `S-AUTH-021` | U34, G21 | Reducer transaction | Accepted immutable decision | Apply all resulting state/events atomically or none | Any error leaves state/revision unchanged | session-semantic |
| `S-AUTH-022` | G18, ADR threat model | Human/operator | Host-authoritative room | UI/docs disclose that host owns full hidden state and can censor/equivocate | No claim of trustless dealing or malicious-host prevention | security-assumption |

## Viewer projection and spectator rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- |
| `S-VIEW-001` | Prior G9, U46 | Active player viewer | Current membership/seat | Project public state plus only that player's private hand | Opponent hands absent from object graph and serialization | session-semantic |
| `S-VIEW-002` | U41, G25 | Spectator viewer | Current spectator membership; no hand grant | Project public room/game information and no private hand | Private field access is absent, not merely hidden by widget | session-semantic |
| `S-VIEW-003` | U41 | Spectator | Current spectator membership; target is active player | Create attributed pending request for exactly target player/recipient | `D-GRANT-SCOPE`, `D-REVOKED`, or wrong phase | session-semantic |
| `S-VIEW-004` | U41 | Target hand owner | Matching pending request; current seat/hand; recipient exact | Create capability ID and incremented grant epoch for that player-recipient pair | Non-owner, wrong recipient, stale request, or no hand is `D-GRANT-SCOPE` | session-semantic |
| `S-VIEW-005` | U41, G25 | Hand owner | Current exact grant | Revoke capability and advance epoch before later projection | Missing/non-owner/stale grant is `D-GRANT-SCOPE` | session-semantic |
| `S-VIEW-006` | U41, G25 | Projection engine | Current exact grant | Include only grantor's current hand for only named recipient and epoch | Grant to one spectator never authorizes another | session-semantic |
| `S-VIEW-007` | U41, G25 | Projection engine | Revoked/expired grant | Omit hand from every later projection/delivery | Already observed information is not claimed erased | session-semantic |
| `S-VIEW-008` | Security invariants | Event/projection layer | Any room event | Public events never contain private hands; private projections are per-recipient | Room broadcast of a hand is a failing security invariant | security-assumption |
| `S-VIEW-009` | U41, ADR threat model | Network adapter | Private projection | Encrypt/authenticate to exact stable recipient key or prove equivalent end-to-end construction | Wrong recipient/capability/epoch cannot decrypt/use payload | transport |
| `S-VIEW-010` | U40-U41 | Snapshot/reconnect | Authorized reconnecting viewer | Rebuild viewer projection from public history plus current entitlements | Never include cards that were neither public nor currently granted | session-semantic |
| `S-VIEW-011` | U37, U46 | Observation schema | Human reconnect or memoryless policy | Preserve sufficient public played-card/history information to reconstruct legal knowledge | Missing history triggers schema-version change, not hidden-state leakage | game-semantic |
| `S-VIEW-012` | U41 | Client cache | Grant revocation or membership epoch change | Remove private material from active future client state and stop new delivery | Cannot promise secure erasure of screenshots/memory already observed | UI-only |
| `S-VIEW-013` | G25 | Projection tests | Any pair of viewers | Pairwise noninterference: changing one hidden hand cannot alter an unauthorized viewer projection | Difference is a privacy defect | security-assumption |
| `S-VIEW-014` | G18, ADR threat model | Host viewer | Host authority owns complete game | Host process may inspect all hands by design | UI must not imply host-blind or trustless semantics | security-assumption |

## Chat rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-CHAT-001` | U39, G24 | Current member | Room open; active membership | Emit attributed chat event on session side stream | `D-UNKNOWN-PRINCIPAL`, `D-REVOKED`, or `D-CLOSED` | session-semantic |
| `S-CHAT-002` | U39, security invariants | Chat decoder | Valid UTF-8 | Enforce versioned nonempty byte/character limit before storage/fanout | `D-CHAT-SIZE` or `D-MALFORMED` | transport |
| `S-CHAT-003` | U39, security invariants | Rate policy | Current member under window quota | Accept and account bounded message | `D-CHAT-RATE` with retry metadata | session-semantic |
| `S-CHAT-004` | U39 | Authority | Accepted chat | Bind message ID, principal, room, epoch, revision/order metadata, and text | Unattributed chat is invalid | session-semantic |
| `S-CHAT-005` | U39, G24 | Runtime | Accepted chat | Retain bounded ephemeral history sufficient for current clients; exclude durable social history | Restart/history truncation is documented, not silently promised durable | transport |
| `S-CHAT-006` | U32, U39 | NDJSON/protocol parser | Chat text contains newlines/control-like text | Encode it strictly as payload data; it cannot create a second envelope/control frame | Ambiguous/control injection is `D-MALFORMED` | security-assumption |
| `S-CHAT-007` | U39, G24 | Member | Lobby, countdown, running, paused, or post-game | Chat remains available without advancing game/session lifecycle | Closed or revoked membership denies | session-semantic |
| `S-CHAT-008` | G24, G31 | Formal model | Chat applicable | Model only permission, bounded count/rate state, and lifecycle interaction | Text content is reasoned-not-applicable to game/formal semantics | security-assumption |
| `S-CHAT-009` | Security invariants | Application output | Any generated chat/system text | Never interpolate keys, invite verifiers, unredacted state, or private hands into chat | Deliberate user-entered disclosure is outside prevention; automatic disclosure is a defect | security-assumption |

## Logical-time and liveness rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-TIME-001` | U35, G22 | Pure reducer | Any transition | Read only logical event data; never read wall clock or sleep | Wall-clock access in semantic crate is forbidden | session-semantic |
| `S-TIME-002` | G22, ADR countdown | Authority runtime | Armed countdown and reached wall deadline | Enqueue matching logical `CountdownExpired` token | Non-authority emission is `D-ENVIRONMENT-ONLY` | transport |
| `S-TIME-003` | U35, ADR countdown | Authority scheduler | Player abort/unready and expiry observed at same logical deadline | Order player command first; resulting expiry becomes stale/inactive | Starting despite accepted abort is a defect | session-semantic |
| `S-TIME-004` | G22 | Reducer | Matching active expiry token and readiness | Start at most once and consume token | Duplicate/stale expiry is idempotent denial/no-op | session-semantic |
| `S-TIME-005` | U35 | Client renderer | Countdown projection with authority deadline/estimate | Display an estimate only; never authoritatively start locally | Client clock disagreement cannot mutate state | UI-only |
| `S-TIME-006` | G30 | Formal claims | Session liveness statement | Name eventual tick, delivery, player-action, reconnect, and unpause assumptions used | Removing an assumption must expose the expected counterexample | security-assumption |
| `S-TIME-007` | U48, G30 | Any execution | Session paused or partitioned | Permit infinite persistence when fairness assumptions are absent | Never claim unconditional session termination | security-assumption |
| `S-TIME-008` | U37 | Loopback/replay runtime | Same initial state and logical event sequence | Produce identical deadlines/tokens/events without real-time sleeps | Wall timing is not part of semantic hash | transport |

## Fault, recovery, and transport rules

| Rule | Source | Actor | Preconditions | Accepted effect | Failure result | Class |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-FAULT-001` | U33, security invariants | Transport | Drop, duplicate, reorder, or delay | Authority IDs/revisions and replay make accepted semantic result deterministic | Missing gaps trigger recovery; duplicates do not reapply | transport |
| `S-FAULT-002` | Veilid findings | DHT watch consumer | Watch notification | Treat as a hint to fetch and validate current rendezvous/snapshot metadata | Notification order never becomes event authority | transport |
| `S-FAULT-003` | U33 | Veilid adapter | `TryAgain`, timeout, no connection, stale route, or watch renewal | Classify retry, preserve command ID, and retry only when safe/idempotent | Exhaustion reports transport failure, not semantic denial/pass | transport |
| `S-FAULT-004` | U40, G20 | Route manager | Private route/node ID rotates or dies | Re-rendezvous and authenticate stable app key; membership identity unchanged | Route possession alone cannot reconnect | transport |
| `S-FAULT-005` | G18, security invariants | Recovering client | Gap/reconnect; trusted signed snapshot and tail available | Validate hashes/revision, apply snapshot once, then contiguous tail | Conflicting snapshot/tail is rejected and surfaced | transport |
| `S-FAULT-006` | Veilid payload bound | Network codec | Outbound/inbound message near 32,768-byte Veilid limit | Keep envelopes below selected safe bound; authenticated chunking only if separately specified | `D-OVERSIZE`; no ad hoc reassembly | transport |
| `S-FAULT-007` | G18, no host migration | Clients | Host unavailable permanently | Stop making progress, preserve transcript/evidence, expose host unavailable | Never elect or trust a replacement silently | security-assumption |
| `S-FAULT-008` | G30 | Formal/runtime report | Network partition or absent external service | Report conditional/unavailable evidence separately from semantic results | External-network failure cannot be labeled a semantic pass | security-assumption |
| `S-FAULT-009` | U37 | RL/ordinary tests | Training or deterministic acceptance | Use direct/in-process transport only; create no Veilid/public-network traffic | Network access in ordinary rollout path is a defect | transport |
| `S-FAULT-010` | U32, G16 | Decoder | Invalid UTF-8/JSON, unknown version/tag, extra frame, or trailing control data | Reject whole input before policy/reducer | `D-MALFORMED` or `D-UNKNOWN-VERSION`; no partial mutation | transport |
| `S-FAULT-011` | Security invariants | Diagnostics | Any decode/policy/network failure | Return structured public reason with redacted context | Secret/invite/hand leakage in diagnostics is a defect | security-assumption |

