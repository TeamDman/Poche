# Session rule coverage matrix

- Status: Phase 1 dispositions complete; implementation evidence in progress
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
| `S-ROOM-001` | checked Task 2.2 create/apply unit | checked Task 3.1 fixed-host/member structure | planned lifecycle | planned successor/predecessor | checked Task 2.4 create transcript | planned loopback/Veilid | planned host screen |
| `S-ROOM-002` | checked Task 2.2 invite units | planned membership safety | planned lifecycle | planned invite query | checked Task 2.4 secret-ref join transcript | planned Veilid e2e | planned join/error |
| `S-ROOM-003` | checked Task 2.2 seat/invariant units | checked Task 3.1 `SingleSeatOwnership` | planned seat state | planned seat query | checked Task 2.4 seat transcript | planned parity | planned seat control |
| `S-ROOM-004` | checked Task 2.2 readiness units | checked Task 3.1 ready-subset invariant | planned lifecycle | planned ready query | checked Task 2.4 ready transcript | planned parity | planned ready control |
| `S-ROOM-005` | checked Task 2.2 unready/cancel units | planned cancel assertion | planned abort race | planned predecessor | planned unready fixture | planned parity | planned unready/countdown |
| `S-ROOM-006` | checked Task 2.2 ready-gate units | checked Task 3.1 `NoStartWithoutReadiness` | planned countdown | planned permission query | checked Task 2.4 arm transcript | planned authority clock | planned host control |
| `S-ROOM-007` | checked Task 2.2 any-player abort | planned abort assertion | planned abort race | planned predecessor | checked Task 2.4 abort transcript | planned multi-client | planned every-player control |
| `S-ROOM-008` | checked Task 2.2 expiry/idempotence | checked Task 3.1 `AtMostOnceStart` | planned expiry/live | planned predecessor | checked Task 2.4 expiry transcript | planned authority clock | planned phase display |
| `S-ROOM-009` | checked Task 2.1 registered-tag decoder | checked Task 3.1 start is environment predicate, not allowed command | planned safety | planned impossible query | planned unknown-command fixture | N/A: no valid wire action | N/A: no control |
| `S-ROOM-010` | checked Task 2.2 actor/game gate | planned abstract game port | planned actor gate | planned legal successor | checked Task 2.4 game-action transcript | planned parity | planned legal actions |
| `S-ROOM-011` | checked Task 2.2 any-player pause | checked Task 3.1 player-gated pause | planned pause state | planned pause query | checked Task 2.4 pause transcript | planned multi-client | planned every-player control |
| `S-ROOM-012` | checked Task 2.2 any-player unpause | checked Task 3.1 player-gated resume | planned resume/live | planned resume query | checked Task 2.4 unpause transcript | planned multi-client | planned every-player control |
| `S-ROOM-013` | checked Task 2.2 controlled pause defect | checked Task 3.1 `PauseResumePreserveRoom` | planned paused safety | planned denied-action query | checked Task 2.4 paused-denial transcript | planned parity | planned disabled game controls |
| `S-ROOM-014` | checked Task 2.2 settle/score adapter | N/A: existing game semantics abstracted | planned abstract settle gate | N/A: existing game oracle owns score | checked Task 2.4 score transcript | planned parity | planned score display |
| `S-ROOM-015` | checked Task 2.2 explicit chance adapter | N/A: chance abstracted | planned abstract chance gate | N/A: existing game oracle owns chance | checked Task 2.4 explicit chance transcript | planned parity | N/A: no player control |
| `S-ROOM-016` | checked Task 2.2 terminal transition | planned phase assertion | planned terminal transition | planned predecessor | checked Task 2.4 postgame transcript | planned parity | planned results screen |
| `S-ROOM-017` | checked Task 2.2 reset unit | planned reset safety | planned lifecycle | planned predecessor | checked Task 2.4 reset transcript | planned parity | planned host reset |
| `S-ROOM-018` | checked Task 2.2 release/leave units | planned membership cleanup | planned lifecycle | planned leave query | planned leave fixture | planned disconnect/leave | planned leave state |
| `S-ROOM-019` | checked Task 2.2 transport-loss event | planned connection abstraction | planned disconnect state | planned predecessor | checked Task 2.4 disconnect transcript | planned fault harness | planned reconnect status |
| `S-ROOM-020` | checked Task 2.2 stable-key reconnect | planned key-bound member | planned reconnect state | planned reconnect query | checked Task 2.4 reconnect transcript | planned restart e2e | planned reconnect status |
| `S-ROOM-021` | checked Task 2.2 removal unit | planned revocation cleanup | planned revoked state | planned permission/predecessor | planned removal fixture | planned revoke e2e | planned host moderation |
| `S-ROOM-022` | checked Task 2.2 absorbing close | planned absorbing-close | planned closed state | planned close query | checked Task 2.4 close transcript | planned shutdown e2e | planned closed screen |
| `S-ROOM-023` | checked Task 2.2 host-loss/no-migration | planned single-host invariant | planned host-loss counterexample | planned impossible migration query | planned unavailable event | planned host-loss e2e | planned unavailable message |
| `S-ROOM-024` | checked Task 2.2 bounded member/seat invariants | checked Task 3.1 member/seat separation and unique seat | planned bounded roles | planned role query | planned snapshot fixture | planned parity | planned member/seat list |

## Authorization and integrity

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-AUTH-001` | checked Tasks 2.1-2.2 default-deny defect | checked Task 3.1 `DefaultDeny` | planned denied transition | planned unknown-role/action queries | planned unknown fixture | planned attack e2e | planned reason display |
| `S-AUTH-002` | checked Task 2.1 envelope schema | planned field abstraction | planned binding flags | planned validation query | planned golden/malformed | planned signed e2e | N/A: envelope internal |
| `S-AUTH-003` | planned crypto vectors | N/A: unforgeability assumed | planned valid-signature flag | planned valid-signature assumption query | planned bad-signature fixture | planned attack e2e | planned safe error |
| `S-AUTH-004` | checked Tasks 2.1/2.5 canonical parity/fuzz | N/A: byte encoding outside relational scope | N/A: byte encoding outside temporal scope | planned canonical relation only | checked Task 2.5 typed/NDJSON transcript | planned cross-codec e2e | N/A: diagnostic only |
| `S-AUTH-005` | checked Tasks 2.2/2.4 replay hashes | planned idempotence assertion | planned duplicate safety | planned duplicate query | checked Task 2.4 duplicate transcript | planned retry e2e | planned stable result |
| `S-AUTH-006` | checked Task 2.2 wrong-room unit | planned room-binding assertion | planned wrong-room flag | planned deny query | planned cross-room fixture | planned attack e2e | planned reason display |
| `S-AUTH-007` | checked Task 2.2 stale-epoch unit | planned epoch assertion | planned stale-epoch flag | planned deny query | planned stale fixture | planned reconnect/revoke e2e | planned reason display |
| `S-AUTH-008` | checked Tasks 2.2/2.4 stale/reorder | planned revision assertion | planned stale-revision flag | planned deny query | checked Task 2.4 stale/reorder transcript | planned reorder e2e | planned conflict display |
| `S-AUTH-009` | checked Task 2.2 deny-override defect | planned deny-override | planned policy abstraction | planned deny-override query | planned decision fixture | planned parity | planned reason display |
| `S-AUTH-010` | checked Task 2.2 immutable policy evidence | planned decision completeness | planned decision outputs | planned explanation query | planned decision fixture | planned audit log | planned policy reason |
| `S-AUTH-011` | checked Task 2.2 audit-only non-authority | planned no-authority assertion | planned audit flag | planned hypothetical query | planned audit fixture | planned parity | N/A: operator diagnostics only |
| `S-AUTH-012` | checked Task 2.2 unknown-principal defect | planned transport-not-capability | planned peer flag | planned deny query | planned peer fixture | planned Veilid attack | planned auth error |
| `S-AUTH-013` | checked Task 2.2 stable app-principal reconnect | planned stable-principal abstraction | planned route-rotation | planned identity query | planned identity fixture | planned restart/route e2e | planned identity display |
| `S-AUTH-014` | planned invite unit/prop | planned invite scope | planned invite state | planned issuance query | planned code vectors | planned DHT e2e | planned code display |
| `S-AUTH-015` | checked Task 2.2 redeem/replay/expiry units | planned key binding | planned replay/expiry | planned redemption query | planned invite attacks | planned join e2e | planned join reasons |
| `S-AUTH-016` | planned event crypto vectors | N/A: unforgeability assumed | planned signed-event flag | planned provenance query | planned signed event fixture | planned tamper e2e | planned integrity error |
| `S-AUTH-017` | checked Tasks 2.2/2.4 gap/reorder defects | planned total-order assertion | planned revision sequence | planned predecessor query | checked Task 2.4 first-divergence fixture | planned reorder/recovery | planned syncing state |
| `S-AUTH-018` | checked Task 2.4 prefix snapshot/tail hashes | planned identity abstraction | planned snapshot revision | planned recovery query | checked Task 2.4 snapshot payload vectors | planned recovery e2e | planned syncing state |
| `S-AUTH-019` | checked Tasks 2.1-2.4 secret/invite diagnostics | N/A: secret bytes omitted by scope | N/A: secret bytes omitted by scope | planned non-generation query | checked Task 2.4 secret-ref fixture scan | planned capture scan | planned redaction scan |
| `S-AUTH-020` | checked Task 2.1 bounded decoder fuzz | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside query model | planned malformed corpus | planned oversize attacks | planned safe error |
| `S-AUTH-021` | checked Tasks 2.2/2.4 atomic unchanged hashes | planned no-partial assertion | planned atomic transition | planned failure query | checked Task 2.4 rejected-command transcript | planned fault e2e | planned unchanged view |
| `S-AUTH-022` | checked ADR/docs; executable disclosure later | planned malicious host outside claim | planned host-fault counterexample | planned trust query | planned threat metadata | planned host model disclosure | planned conspicuous disclosure |

## Viewer projection

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-VIEW-001` | checked Task 2.3 player projection/noninterference | checked Task 3.1 self-knowledge relation | planned visibility flag | planned can-see query | checked Task 2.3 typed own-hand field | planned private delivery | planned own-hand screen |
| `S-VIEW-002` | checked Task 2.3 public spectator projection | checked Task 3.1 no-grant/no-edge assertion | planned spectator flag | planned can-see query | checked Task 2.3 absent private fields | planned spectator capture | planned public-only screen |
| `S-VIEW-003` | checked Task 2.3 exact request unit | planned request relation | planned request state | planned request query | checked Task 2.1 request schema | planned spectator e2e | planned request control |
| `S-VIEW-004` | checked Task 2.3 owner grant/deny/request/epoch gate | checked Task 3.1 owner/spectator grant relation | planned grant state | planned grant explanation | checked Task 2.3 exact grant/deny schema | planned private e2e | planned owner grant control |
| `S-VIEW-005` | checked Task 2.3 exact revoke unit | checked Task 3.1 `RevocationStopsFutureKnowledge` | planned revoke state | planned revoke explanation | checked Task 2.1 exact revoke schema | planned revoke e2e | planned owner revoke control |
| `S-VIEW-006` | checked Task 2.3 pairwise viewer test | checked Task 3.1 `ScopedSpectatorGrant` | planned grant flag | planned can-see query | checked Task 2.3 scoped hand shape | planned recipient capture | planned granted hand |
| `S-VIEW-007` | checked Task 2.3 revoke/expiry future test | checked Task 3.1 future-edge absence | planned post-revoke state | planned can-see false query | checked Task 2.3 projection epoch | planned capture after revoke | planned future removal |
| `S-VIEW-008` | checked Task 2.3 public-history/schema test | planned no-public-hand | planned broadcast flag | planned public visibility query | checked Task 2.3 event hash/public types | planned packet scan | planned client-state scan |
| `S-VIEW-009` | planned crypto adapter vectors | N/A: crypto assumed | planned recipient flag | planned recipient relation | planned encrypted metadata | planned wrong-key e2e | planned decrypt failure |
| `S-VIEW-010` | checked Tasks 2.3/2.4 reconnect/snapshot hashes | checked Task 3.1 current-entitlement derivation | planned reconnect state | planned reconstruction query | checked Task 2.4 scoped snapshot projections | planned reconnect e2e | planned restored view |
| `S-VIEW-011` | checked Task 2.3 observation/history audit | N/A: existing game knowledge compared | N/A: schema shape outside lifecycle | planned public-history query | checked Task 2.3 ordered prefix schema | N/A: transport carries projection | planned history rendering |
| `S-VIEW-012` | checked Task 2.3 immutable/future projection test | N/A: past knowledge cannot be erased | N/A: client memory outside session | planned future-only query | checked Task 2.3 epoch/expiry event | planned stop-delivery e2e | planned cache removal |
| `S-VIEW-013` | checked Task 2.3 four-viewer controlled defect | checked Task 3.1 scoped-grant noninterference | planned visibility safety | planned can-see matrix | checked Task 2.3 typed projection equality | planned multi-view capture | planned DOM/widget scan |
| `S-VIEW-014` | checked Task 2.3 local diagnostics separation | N/A: host owns state by model | planned host-full-state flag | planned host can-see query | N/A: host-local full state | planned disclosure only | planned host-trust notice |

## Chat

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-CHAT-001` | checked Task 2.2 member chat gate | planned permission assertion | planned chat action | planned may-chat query | checked Task 2.4 chat transcript | planned e2e | planned composer |
| `S-CHAT-002` | checked Task 2.1 UTF-8/size validation | N/A: text omitted | planned bounded flag | N/A: text omitted | planned boundary corpus | planned oversize e2e | planned validation error |
| `S-CHAT-003` | checked Task 2.2 logical rate unit | planned bounded counter | planned rate state | planned rate query | planned rate fixture | planned burst e2e | planned retry display |
| `S-CHAT-004` | checked Task 2.2 attributed event | planned attribution assertion | planned sender state | planned who-sent query | checked Task 2.4 attributed transcript | planned parity | planned sender display |
| `S-CHAT-005` | planned retention unit | N/A: durable history out of scope | planned bounded count | planned recent-chat query | planned truncation fixture | planned restart e2e | planned ephemeral notice |
| `S-CHAT-006` | checked Task 2.1 newline/control fuzz | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside scope | planned newline/control corpus | planned injection e2e | planned literal text render |
| `S-CHAT-007` | checked Task 2.2 phase-preserving chat | planned phase permission | planned phase transitions | planned may-chat query | checked Task 2.4 running-phase fixture | planned e2e | planned multi-phase composer |
| `S-CHAT-008` | planned abstraction audit | planned bounded metadata only | planned bounded metadata | planned permission/count query | N/A: modeling rule | N/A: content not formal | N/A: content still rendered |
| `S-CHAT-009` | planned redaction/secret scan | N/A: secrets omitted | N/A: secrets omitted | planned no-auto-source query | planned safe system text | planned capture scan | planned UI scan |

## Logical time

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-TIME-001` | checked Task 2.2 pure logical reducer | planned logical ordering | planned logical clock | planned event query | planned logical fields | planned runtime boundary | N/A: no wall clock authority |
| `S-TIME-002` | planned clock-adapter unit | N/A: external time abstracted | planned fair expiry | planned authority query | planned expiry fixture | planned timed harness | planned estimate only |
| `S-TIME-003` | checked Task 2.2 same-deadline race unit | planned abort-wins assertion | planned race safety | planned predecessor query | planned race transcript | planned simultaneous harness | planned no false start |
| `S-TIME-004` | checked Task 2.2 duplicate-start defect | checked Task 3.1 `AtMostOnceStart` | planned duplicate expiry | planned start explanation | planned duplicate expiry | planned retry e2e | planned single transition |
| `S-TIME-005` | planned presentation-model unit | N/A: UI estimate | N/A: UI estimate | N/A: UI estimate | planned projected deadline | planned skew harness | planned browser/native test |
| `S-TIME-006` | planned checker assumptions | checked Task 3.1 bounded safety only; exact scopes documented | planned live/counterexamples | planned fairness explanation | planned evidence metadata | planned fault evidence | planned limitation text |
| `S-TIME-007` | checked Task 2.2 paused persistence/gate | checked Task 3.1 SAT pause/resume witness | planned unconditional counterexample | planned paused successor loop | N/A: claim metadata only | planned partition/pause harness | planned persistent paused state |
| `S-TIME-008` | checked Task 2.4 deterministic replay | planned logical steps | planned deterministic trace | planned event sequence query | checked Task 2.4 transcript hashes | planned in-process only | planned replay parity |

## Fault and recovery

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-FAULT-001` | checked Task 2.4 duplicate/reorder mutations | planned idempotence contract | planned fault abstraction | planned reorder predecessor | checked Task 2.4 golden mutation corpus | planned fault harness | planned syncing state |
| `S-FAULT-002` | planned adapter unit | N/A: DHT outside model | planned hint/non-authority flag | planned refresh query | planned watch metadata | planned Veilid watch test | planned refreshing state |
| `S-FAULT-003` | planned retry unit | N/A: API errors outside model | planned retry abstraction | planned retry-category query | planned retry metadata | planned forced failures | planned structured error |
| `S-FAULT-004` | checked Task 2.2 route-neutral reconnect state | planned stable-key abstraction | planned route rotation | planned reconnect query | planned route metadata | planned rotation e2e | planned reconnect status |
| `S-FAULT-005` | checked Task 2.4 genesis/snapshot-tail parity | planned snapshot contract | planned gap/recovery | planned recovery query | checked Task 2.4 snapshot+tail corpus | planned gap e2e | planned syncing/conflict |
| `S-FAULT-006` | planned boundary unit/fuzz | N/A: byte size outside model | planned oversize flag only | N/A: bytes outside query | planned max-size vectors | planned Veilid limit e2e | planned safe error |
| `S-FAULT-007` | checked Task 2.2 host-loss/no-election unit | checked Task 3.1 fixed singleton host/no election transition | planned liveness counterexample | planned impossible-host query | planned unavailable event | planned host shutdown | planned host-unavailable screen |
| `S-FAULT-008` | planned evidence classification | N/A: network fact outside relational result | planned partition counterexample | planned availability query | planned disposition metadata | planned unavailable harness | planned honest limitation |
| `S-FAULT-009` | planned no-network assertion/benchmark | N/A: RL path outside session model | N/A: rollout path outside lifecycle | N/A: rollout path outside query model | planned direct-call parity only | planned network-spy zero calls | N/A: renderer optional |
| `S-FAULT-010` | checked Task 2.1 fuzz/no semantic entry | N/A: parser outside model | N/A: parser outside model | N/A: parser outside model | checked Tasks 2.1/2.5 malformed corpus/parity | planned malformed e2e | planned safe error |
| `S-FAULT-011` | planned redaction/error unit | N/A: diagnostics outside model | N/A: diagnostics outside model | planned public-reason query | planned error fixtures | planned logs/capture scan | planned redacted error |

## Completion protocol

Task 3.5 may change a `planned` cell only after the named evidence exists. It
must preserve exact scope/result counts, fixture hashes, native tool versions,
and controlled-defect outcomes. A reasoned `N/A` may be revised only by adding a
new applicable contract; absence of an implementation is never itself an N/A.
