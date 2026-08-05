# Session rule coverage matrix

- Status: Phase 1 dispositions complete; implementation evidence pending
- Rule source: `session-rules.md`
- Scope policy: exact scopes and fairness assumptions are attached to later runs

Every rule has a disposition in every track. `planned` means the named phase
must supply executable evidence. `N/A` is reasoned non-applicability, not an
omission. Rows are never changed to `checked` merely because another track
passes.

Abbreviations: `unit` = focused Rust test; `exh` = bounded exhaustive Rust
exploration; `prop` = property/generative test; `fixture` = canonical protocol
fixture; `query` = forward/reverse Prolog query; `safety` = Alloy/NuSMV safety;
`live` = NuSMV conditional liveness; `e2e` = loopback/native/browser scenario.

## Room and composition

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-ROOM-001` | planned unit/exh | planned structure | planned lifecycle | planned successor/predecessor | planned create fixture | planned loopback/Veilid | planned host screen |
| `S-ROOM-002` | planned unit/exh | planned membership safety | planned lifecycle | planned invite query | planned redeem fixture | planned Veilid e2e | planned join/error |
| `S-ROOM-003` | planned unit/exh | planned unique-seat assertion | planned seat state | planned seat query | planned seat fixture | planned parity | planned seat control |
| `S-ROOM-004` | planned unit/exh | planned readiness assertion | planned lifecycle | planned ready query | planned ready fixture | planned parity | planned ready control |
| `S-ROOM-005` | planned unit/exh | planned cancel assertion | planned abort race | planned predecessor | planned unready fixture | planned parity | planned unready/countdown |
| `S-ROOM-006` | planned unit/exh | planned all-ready assertion | planned countdown | planned permission query | planned arm fixture | planned authority clock | planned host control |
| `S-ROOM-007` | planned unit/exh | planned abort assertion | planned abort race | planned predecessor | planned abort fixture | planned multi-client | planned every-player control |
| `S-ROOM-008` | planned unit/exh | planned at-most-once | planned expiry/live | planned predecessor | planned expiry fixture | planned authority clock | planned phase display |
| `S-ROOM-009` | planned unit | planned absence assertion | planned safety | planned impossible query | planned unknown-command fixture | N/A: no valid wire action | N/A: no control |
| `S-ROOM-010` | planned unit/prop | planned abstract game port | planned actor gate | planned legal successor | planned game fixture | planned parity | planned legal actions |
| `S-ROOM-011` | planned unit/exh | planned actor assertion | planned pause state | planned pause query | planned pause fixture | planned multi-client | planned every-player control |
| `S-ROOM-012` | planned unit/exh | planned actor assertion | planned resume/live | planned resume query | planned unpause fixture | planned multi-client | planned every-player control |
| `S-ROOM-013` | planned unit/exh | planned no-advance assertion | planned paused safety | planned denied-action query | planned paused-denial fixture | planned parity | planned disabled game controls |
| `S-ROOM-014` | planned composition unit | N/A: existing game semantics abstracted | planned abstract settle gate | N/A: existing game oracle owns score | planned score event fixture | planned parity | planned score display |
| `S-ROOM-015` | planned composition/replay | N/A: chance abstracted | planned abstract chance gate | N/A: existing game oracle owns chance | planned chance provenance fixture | planned parity | N/A: no player control |
| `S-ROOM-016` | planned unit/exh | planned phase assertion | planned terminal transition | planned predecessor | planned postgame fixture | planned parity | planned results screen |
| `S-ROOM-017` | planned unit/exh | planned reset safety | planned lifecycle | planned predecessor | planned reset fixture | planned parity | planned host reset |
| `S-ROOM-018` | planned unit/exh | planned membership cleanup | planned lifecycle | planned leave query | planned leave fixture | planned disconnect/leave | planned leave state |
| `S-ROOM-019` | planned unit/exh | planned connection abstraction | planned disconnect state | planned predecessor | planned disconnect event | planned fault harness | planned reconnect status |
| `S-ROOM-020` | planned unit/exh | planned key-bound member | planned reconnect state | planned reconnect query | planned reconnect fixture | planned restart e2e | planned reconnect status |
| `S-ROOM-021` | planned unit/exh | planned revocation cleanup | planned revoked state | planned permission/predecessor | planned removal fixture | planned revoke e2e | planned host moderation |
| `S-ROOM-022` | planned unit/exh | planned absorbing-close | planned closed state | planned close query | planned close fixture | planned shutdown e2e | planned closed screen |
| `S-ROOM-023` | planned failure unit | planned single-host invariant | planned host-loss counterexample | planned impossible migration query | planned unavailable event | planned host-loss e2e | planned unavailable message |
| `S-ROOM-024` | planned unit/exh | planned membership/seat assertions | planned bounded roles | planned role query | planned snapshot fixture | planned parity | planned member/seat list |

## Authorization and integrity

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-AUTH-001` | planned unit/exh/control defect | planned default-deny | planned denied transition | planned unknown-role/action queries | planned unknown fixture | planned attack e2e | planned reason display |
| `S-AUTH-002` | planned schema/unit | planned field abstraction | planned binding flags | planned validation query | planned golden/malformed | planned signed e2e | N/A: envelope internal |
| `S-AUTH-003` | planned crypto vectors | N/A: unforgeability assumed | planned valid-signature flag | planned valid-signature assumption query | planned bad-signature fixture | planned attack e2e | planned safe error |
| `S-AUTH-004` | planned canonical vectors/prop | N/A: byte encoding outside relational scope | N/A: byte encoding outside temporal scope | planned canonical relation only | planned exact byte vectors | planned cross-codec e2e | N/A: diagnostic only |
| `S-AUTH-005` | planned unit/exh | planned idempotence assertion | planned duplicate safety | planned duplicate query | planned duplicate fixture | planned retry e2e | planned stable result |
| `S-AUTH-006` | planned unit/exh | planned room-binding assertion | planned wrong-room flag | planned deny query | planned cross-room fixture | planned attack e2e | planned reason display |
| `S-AUTH-007` | planned unit/exh | planned epoch assertion | planned stale-epoch flag | planned deny query | planned stale fixture | planned reconnect/revoke e2e | planned reason display |
| `S-AUTH-008` | planned unit/exh | planned revision assertion | planned stale-revision flag | planned deny query | planned reorder fixture | planned reorder e2e | planned conflict display |
| `S-AUTH-009` | planned unit/control defect | planned deny-override | planned policy abstraction | planned deny-override query | planned decision fixture | planned parity | planned reason display |
| `S-AUTH-010` | planned unit/schema | planned decision completeness | planned decision outputs | planned explanation query | planned decision fixture | planned audit log | planned policy reason |
| `S-AUTH-011` | planned unit | planned no-authority assertion | planned audit flag | planned hypothetical query | planned audit fixture | planned parity | N/A: operator diagnostics only |
| `S-AUTH-012` | planned unit/control defect | planned transport-not-capability | planned peer flag | planned deny query | planned peer fixture | planned Veilid attack | planned auth error |
| `S-AUTH-013` | planned identity unit | planned stable-principal abstraction | planned route-rotation | planned identity query | planned identity fixture | planned restart/route e2e | planned identity display |
| `S-AUTH-014` | planned invite unit/prop | planned invite scope | planned invite state | planned issuance query | planned code vectors | planned DHT e2e | planned code display |
| `S-AUTH-015` | planned redeem unit/exh | planned key binding | planned replay/expiry | planned redemption query | planned invite attacks | planned join e2e | planned join reasons |
| `S-AUTH-016` | planned event crypto vectors | N/A: unforgeability assumed | planned signed-event flag | planned provenance query | planned signed event fixture | planned tamper e2e | planned integrity error |
| `S-AUTH-017` | planned unit/exh | planned total-order assertion | planned revision sequence | planned predecessor query | planned gap/conflict fixture | planned reorder/recovery | planned syncing state |
| `S-AUTH-018` | planned snapshot/hash unit | planned identity abstraction | planned snapshot revision | planned recovery query | planned snapshot vectors | planned recovery e2e | planned syncing state |
| `S-AUTH-019` | planned redaction/secret scan | N/A: secret bytes omitted by scope | N/A: secret bytes omitted by scope | planned non-generation query | planned redacted fixtures | planned capture scan | planned redaction scan |
| `S-AUTH-020` | planned fuzz/limits | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside query model | planned malformed corpus | planned oversize attacks | planned safe error |
| `S-AUTH-021` | planned transaction unit/exh | planned no-partial assertion | planned atomic transition | planned failure query | planned rejected-command fixture | planned fault e2e | planned unchanged view |
| `S-AUTH-022` | planned docs/assertion | planned malicious host outside claim | planned host-fault counterexample | planned trust query | planned threat metadata | planned host model disclosure | planned conspicuous disclosure |

## Viewer projection

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-VIEW-001` | planned projection unit/prop | planned knowledge assertion | planned visibility flag | planned can-see query | planned player projection | planned private delivery | planned own-hand screen |
| `S-VIEW-002` | planned projection unit/prop | planned no-knowledge assertion | planned spectator flag | planned can-see query | planned public projection | planned spectator capture | planned public-only screen |
| `S-VIEW-003` | planned unit/exh | planned request relation | planned request state | planned request query | planned request fixture | planned spectator e2e | planned request control |
| `S-VIEW-004` | planned unit/exh | planned owner/scope assertion | planned grant state | planned grant explanation | planned grant fixture | planned private e2e | planned owner grant control |
| `S-VIEW-005` | planned unit/exh | planned future-revoke assertion | planned revoke state | planned revoke explanation | planned revoke fixture | planned revoke e2e | planned owner revoke control |
| `S-VIEW-006` | planned pairwise prop | planned exact-recipient | planned grant flag | planned can-see query | planned scoped projection | planned recipient capture | planned granted hand |
| `S-VIEW-007` | planned future-delivery prop | planned future-no-edge | planned post-revoke state | planned can-see false query | planned post-revoke fixture | planned capture after revoke | planned future removal |
| `S-VIEW-008` | planned public-wire scan | planned no-public-hand | planned broadcast flag | planned public visibility query | planned event corpus scan | planned packet scan | planned client-state scan |
| `S-VIEW-009` | planned crypto adapter vectors | N/A: crypto assumed | planned recipient flag | planned recipient relation | planned encrypted metadata | planned wrong-key e2e | planned decrypt failure |
| `S-VIEW-010` | planned reconnect projection | planned entitlement assertion | planned reconnect state | planned reconstruction query | planned snapshot fixture | planned reconnect e2e | planned restored view |
| `S-VIEW-011` | planned observation audit | N/A: existing game knowledge compared | N/A: schema shape outside lifecycle | planned public-history query | planned observation manifest | N/A: transport carries projection | planned history rendering |
| `S-VIEW-012` | planned client-model unit | N/A: past knowledge cannot be erased | N/A: client memory outside session | planned future-only query | planned epoch event | planned stop-delivery e2e | planned cache removal |
| `S-VIEW-013` | planned pairwise prop/control defect | planned noninterference | planned visibility safety | planned can-see matrix | planned projection hashes | planned multi-view capture | planned DOM/widget scan |
| `S-VIEW-014` | planned threat assertion | N/A: host owns state by model | planned host-full-state flag | planned host can-see query | N/A: host-local full state | planned disclosure only | planned host-trust notice |

## Chat

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-CHAT-001` | planned unit/exh | planned permission assertion | planned chat action | planned may-chat query | planned chat fixture | planned e2e | planned composer |
| `S-CHAT-002` | planned fuzz/bounds | N/A: text omitted | planned bounded flag | N/A: text omitted | planned boundary corpus | planned oversize e2e | planned validation error |
| `S-CHAT-003` | planned rate unit | planned bounded counter | planned rate state | planned rate query | planned rate fixture | planned burst e2e | planned retry display |
| `S-CHAT-004` | planned attribution unit | planned attribution assertion | planned sender state | planned who-sent query | planned attributed fixture | planned parity | planned sender display |
| `S-CHAT-005` | planned retention unit | N/A: durable history out of scope | planned bounded count | planned recent-chat query | planned truncation fixture | planned restart e2e | planned ephemeral notice |
| `S-CHAT-006` | planned parser fuzz | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside scope | planned newline/control corpus | planned injection e2e | planned literal text render |
| `S-CHAT-007` | planned phase matrix | planned phase permission | planned phase transitions | planned may-chat query | planned multi-phase fixture | planned e2e | planned multi-phase composer |
| `S-CHAT-008` | planned abstraction audit | planned bounded metadata only | planned bounded metadata | planned permission/count query | N/A: modeling rule | N/A: content not formal | N/A: content still rendered |
| `S-CHAT-009` | planned redaction/secret scan | N/A: secrets omitted | N/A: secrets omitted | planned no-auto-source query | planned safe system text | planned capture scan | planned UI scan |

## Logical time

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-TIME-001` | planned dependency/source check | planned logical ordering | planned logical clock | planned event query | planned logical fields | planned runtime boundary | N/A: no wall clock authority |
| `S-TIME-002` | planned clock-adapter unit | N/A: external time abstracted | planned fair expiry | planned authority query | planned expiry fixture | planned timed harness | planned estimate only |
| `S-TIME-003` | planned race unit/exh | planned abort-wins assertion | planned race safety | planned predecessor query | planned race transcript | planned simultaneous harness | planned no false start |
| `S-TIME-004` | planned unit/exh | planned at-most-once | planned duplicate expiry | planned start explanation | planned duplicate expiry | planned retry e2e | planned single transition |
| `S-TIME-005` | planned presentation-model unit | N/A: UI estimate | N/A: UI estimate | N/A: UI estimate | planned projected deadline | planned skew harness | planned browser/native test |
| `S-TIME-006` | planned checker assumptions | planned bounded safety only | planned live/counterexamples | planned fairness explanation | planned evidence metadata | planned fault evidence | planned limitation text |
| `S-TIME-007` | planned infinite-trace witness | planned persistent paused instance | planned unconditional counterexample | planned paused successor loop | N/A: claim metadata only | planned partition/pause harness | planned persistent paused state |
| `S-TIME-008` | planned deterministic replay | planned logical steps | planned deterministic trace | planned event sequence query | planned transcript hash | planned in-process only | planned replay parity |

## Fault and recovery

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-FAULT-001` | planned transport prop | planned idempotence contract | planned fault abstraction | planned reorder predecessor | planned duplicate/reorder corpus | planned fault harness | planned syncing state |
| `S-FAULT-002` | planned adapter unit | N/A: DHT outside model | planned hint/non-authority flag | planned refresh query | planned watch metadata | planned Veilid watch test | planned refreshing state |
| `S-FAULT-003` | planned retry unit | N/A: API errors outside model | planned retry abstraction | planned retry-category query | planned retry metadata | planned forced failures | planned structured error |
| `S-FAULT-004` | planned identity/route unit | planned stable-key abstraction | planned route rotation | planned reconnect query | planned route metadata | planned rotation e2e | planned reconnect status |
| `S-FAULT-005` | planned recovery/hash prop | planned snapshot contract | planned gap/recovery | planned recovery query | planned snapshot+tail corpus | planned gap e2e | planned syncing/conflict |
| `S-FAULT-006` | planned boundary unit/fuzz | N/A: byte size outside model | planned oversize flag only | N/A: bytes outside query | planned max-size vectors | planned Veilid limit e2e | planned safe error |
| `S-FAULT-007` | planned host-loss test | planned no-migration | planned liveness counterexample | planned impossible-host query | planned unavailable event | planned host shutdown | planned host-unavailable screen |
| `S-FAULT-008` | planned evidence classification | N/A: network fact outside relational result | planned partition counterexample | planned availability query | planned disposition metadata | planned unavailable harness | planned honest limitation |
| `S-FAULT-009` | planned no-network assertion/benchmark | N/A: RL path outside session model | N/A: rollout path outside lifecycle | N/A: rollout path outside query model | planned direct-call parity only | planned network-spy zero calls | N/A: renderer optional |
| `S-FAULT-010` | planned fuzz/no-mutation | N/A: parser outside model | N/A: parser outside model | N/A: parser outside model | planned malformed corpus | planned malformed e2e | planned safe error |
| `S-FAULT-011` | planned redaction/error unit | N/A: diagnostics outside model | N/A: diagnostics outside model | planned public-reason query | planned error fixtures | planned logs/capture scan | planned redacted error |

## Completion protocol

Task 3.5 may change a `planned` cell only after the named evidence exists. It
must preserve exact scope/result counts, fixture hashes, native tool versions,
and controlled-defect outcomes. A reasoned `N/A` may be revised only by adding a
new applicable contract; absence of an implementation is never itself an N/A.

