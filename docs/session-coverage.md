# Session rule coverage matrix

- Status: Phase 3 formal-track audit complete; later protocol, network, and UI tracks remain planned
- Rule source: `session-rules.md`
- Scope policy: exact scopes and fairness assumptions are attached to later runs

Every rule has a disposition in every track. Formal columns use `checked`,
`queried`, reasoned `N/A`, bounded-scope `abstracted`, or explicitly
`deferred`; they contain no silent Phase 3 gaps. `planned` is reserved for the
named later protocol/network/UI phase that must supply executable evidence.
Rows are never changed to `checked` merely because another track passes.

Abbreviations: `unit` = focused Rust test; `exh` = bounded exhaustive Rust
exploration; `prop` = property/generative test; `fixture` = canonical protocol
fixture; `query` = forward/reverse Prolog query; `safety` = Alloy/NuSMV safety;
`live` = NuSMV conditional liveness; `e2e` = loopback/native/browser scenario.

## Room and composition

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-ROOM-001` | checked Task 2.2 create/apply unit | checked Task 3.1 fixed-host/member structure | abstracted Task 3.2: lifecycle | abstracted Task 3.3: successor/predecessor | checked Tasks 2.4/4.3 create script | checked Task 4.2 loopback; Veilid pending Task 5.2 | checked Task 4.3 lobby text; egui pending |
| `S-ROOM-002` | checked Task 2.2 invite units | abstracted Task 3.1: membership safety | abstracted Task 3.2: lifecycle | abstracted Task 3.3: invite query | checked Task 2.4 secret-ref join transcript | checked Task 4.2 loopback join; Veilid pending Task 5.2 | planned join/error |
| `S-ROOM-003` | checked Task 2.2 seat/invariant units | checked Task 3.1 `SingleSeatOwnership` | abstracted Task 3.2: seat state | abstracted Task 3.3: seat query | checked Task 2.4 seat transcript | checked Task 4.2 multi-client seats | planned seat control |
| `S-ROOM-004` | checked Task 2.2 readiness units | checked Task 3.1 ready-subset invariant | checked Task 3.2 readiness lifecycle | queried Task 3.3 ready decision/steps | checked Task 2.4 ready transcript | checked Task 4.2 multi-client ready | planned ready control |
| `S-ROOM-005` | checked Task 2.2 unready/cancel units | abstracted Task 3.1: cancel assertion | checked Task 3.2 unready cancellation | queried Task 3.3 reverse-capable unready step | checked Task 4.5 replayed smoke input | checked Task 4.5 multi-client unready/re-ready | planned unready/countdown |
| `S-ROOM-006` | checked Task 2.2 ready-gate units | checked Task 3.1 `NoStartWithoutReadiness` | checked Task 3.2 `countdown_is_ready` | queried Task 3.3 allow/deny arm explanations | checked Tasks 2.4/4.3 arm script | checked Task 4.2 manual authority clock | checked Task 4.3 countdown text; controls pending |
| `S-ROOM-007` | checked Task 2.2 any-player abort | abstracted Task 3.1: abort assertion | checked Task 3.2 abort-before-expiry | queried Task 3.3 abort successor/history | checked Tasks 2.4/4.3/4.5 abort replay | checked Task 4.5 player abort | checked Task 4.3 abort replay; controls pending |
| `S-ROOM-008` | checked Task 2.2 expiry/idempotence | checked Task 3.1 `AtMostOnceStart` | checked Task 3.2 start/expiry safety and liveness | queried Task 3.3 expiry predecessor | checked Tasks 2.4/4.3 expiry script | checked Task 4.2 logical expiry delivery | checked Task 4.3 phase text; egui pending |
| `S-ROOM-009` | checked Task 2.1 registered-tag decoder | checked Task 3.1 start is environment predicate, not allowed command | checked Task 3.2 expiry-only start transition | queried Task 3.3 no player start relation | planned unknown-command fixture | N/A: no valid wire action | N/A: no control |
| `S-ROOM-010` | checked Task 2.2 actor/game gate | abstracted Task 3.1: abstract game port | checked Task 3.2 abstract accepted action | queried Task 3.3 abstract action successors | checked Tasks 2.4/4.3 game script | checked Task 4.2 full real game | checked Task 4.3 legal-action text; controls pending |
| `S-ROOM-011` | checked Task 2.2 any-player pause | checked Task 3.1 player-gated pause | checked Task 3.2 pause state/gate | queried Task 3.3 both player predecessors | checked Tasks 2.4/4.3/4.5 pause replay | checked Task 4.5 player pause | checked Task 4.3 paused text; controls pending |
| `S-ROOM-012` | checked Task 2.2 any-player unpause | checked Task 3.1 player-gated resume | checked Task 3.2 resume/liveness | queried Task 3.3 both player predecessors | checked Tasks 2.4/4.3/4.5 resume replay | checked Task 4.5 other-player resume | checked Task 4.3 resume replay; controls pending |
| `S-ROOM-013` | checked Task 2.2 controlled pause defect | checked Task 3.1 `PauseResumePreserveRoom` | checked Task 3.2 paused action cannot advance | queried Task 3.3 paused-action default deny | checked Tasks 2.4/4.5 paused denial | checked Task 4.5 unchanged paused projection hashes | planned disabled game controls |
| `S-ROOM-014` | checked Task 2.2 settle/score adapter | N/A: existing game semantics abstracted | abstracted Task 3.2: abstract settle gate | N/A: existing game oracle owns score | checked Tasks 2.4/4.3 score replay | checked Task 4.2 all 13 settlements | checked Task 4.3 score text; egui pending |
| `S-ROOM-015` | checked Task 2.2 explicit chance adapter | N/A: chance abstracted | abstracted Task 3.2: abstract chance gate | N/A: existing game oracle owns chance | checked Task 2.4 explicit chance transcript | checked Task 4.2 seeded explicit chance | N/A: no player control |
| `S-ROOM-016` | checked Task 2.2 terminal transition | abstracted Task 3.1: phase assertion | checked Task 3.2 abstract post-game transition | queried Task 3.3 post-game successor | checked Tasks 2.4/4.3 postgame replay | checked Task 4.2 real post-game | checked Task 4.3 result text; egui pending |
| `S-ROOM-017` | checked Task 2.2 reset unit | abstracted Task 3.1: reset safety | abstracted Task 3.2: lifecycle | abstracted Task 3.3: predecessor | checked Tasks 2.4/4.5 reset replay | checked Task 4.5 post-game reset | planned host reset |
| `S-ROOM-018` | checked Task 2.2 release/leave units | abstracted Task 3.1: membership cleanup | abstracted Task 3.2: lifecycle | abstracted Task 3.3: leave query | planned leave fixture | planned disconnect/leave | planned leave state |
| `S-ROOM-019` | checked Task 2.2 transport-loss event | abstracted Task 3.1: connection abstraction | checked Task 3.2 disconnect state | queried Task 3.3 route-loss step | checked Tasks 2.4/4.5 disconnect replay | checked Task 4.5 route-loss harness | planned reconnect status |
| `S-ROOM-020` | checked Task 2.2 stable-key reconnect | abstracted Task 3.1: key-bound member | checked Task 3.2 durable-member reconnect | queried Task 3.3 reconnect relation | checked Tasks 2.4/4.5 reconnect replay | checked Task 4.5 new-route same-principal reconnect; restart Task 5 | planned reconnect status |
| `S-ROOM-021` | checked Task 2.2 removal unit | abstracted Task 3.1: revocation cleanup | abstracted Task 3.2: revoked state | abstracted Task 3.3: permission/predecessor | planned removal fixture | planned revoke e2e | planned host moderation |
| `S-ROOM-022` | checked Task 2.2 absorbing close | abstracted Task 3.1: absorbing-close | checked Task 3.2 `closed_is_absorbing` | queried Task 3.3 host/nonhost close policy | checked Tasks 2.4/4.5 close replay | checked Task 4.5 shutdown transition | planned closed screen |
| `S-ROOM-023` | checked Task 2.2 host-loss/no-migration | abstracted Task 3.1: single-host invariant | checked Task 3.2 fixed durable host; no migration | abstracted Task 3.3: impossible migration query | planned unavailable event | planned host-loss e2e | planned unavailable message |
| `S-ROOM-024` | checked Task 2.2 bounded member/seat invariants | checked Task 3.1 member/seat separation and unique seat | checked Task 3.2 fixed two-seat scope | abstracted Task 3.3: role query | planned snapshot fixture | planned parity | planned member/seat list |

## Authorization and integrity

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-AUTH-001` | checked Tasks 2.1-2.2 default-deny defect | checked Task 3.1 `DefaultDeny` | abstracted Task 3.2: denied transition | queried Task 3.3 default-deny and outsider defect | planned unknown fixture | planned attack e2e | planned reason display |
| `S-AUTH-002` | checked Task 2.1 envelope schema | abstracted Task 3.1: field abstraction | abstracted Task 3.2: binding flags | abstracted Task 3.3: validation query | planned golden/malformed | planned signed e2e | N/A: envelope internal |
| `S-AUTH-003` | checked Task 5.1 strict signature/wrong-key/revision attacks | N/A: unforgeability assumed | abstracted Task 3.2: valid-signature flag | abstracted Task 3.3: valid-signature assumption query | checked Task 5.1 canonical command domain | checked Task 5.1 protected-store adapter; live attack Task 5.4 | planned safe error |
| `S-AUTH-004` | checked Tasks 2.1/2.5 canonical parity/fuzz | N/A: byte encoding outside relational scope | N/A: byte encoding outside temporal scope | abstracted Task 3.3: canonical relation only | checked Task 2.5 typed/NDJSON transcript | planned cross-codec e2e | N/A: diagnostic only |
| `S-AUTH-005` | checked Tasks 2.2/2.4 replay hashes | abstracted Task 3.1: idempotence assertion | abstracted Task 3.2: duplicate safety | abstracted Task 3.3: duplicate query | checked Task 2.4 duplicate transcript | checked Task 4.2 duplicate authority delivery; Veilid retry pending | planned stable result |
| `S-AUTH-006` | checked Task 2.2 wrong-room unit | abstracted Task 3.1: room-binding assertion | abstracted Task 3.2: wrong-room flag | abstracted Task 3.3: deny query | planned cross-room fixture | planned attack e2e | planned reason display |
| `S-AUTH-007` | checked Task 2.2 stale-epoch unit | abstracted Task 3.1: epoch assertion | abstracted Task 3.2: stale-epoch flag | abstracted Task 3.3: deny query | planned stale fixture | planned reconnect/revoke e2e | planned reason display |
| `S-AUTH-008` | checked Tasks 2.2/2.4 stale/reorder | abstracted Task 3.1: revision assertion | abstracted Task 3.2: stale-revision flag | abstracted Task 3.3: deny query | checked Task 2.4 stale/reorder transcript | checked Task 4.2 deterministic reorder; Veilid pending | planned conflict display |
| `S-AUTH-009` | checked Task 2.2 deny-override defect | abstracted Task 3.1: deny-override | abstracted Task 3.2: policy abstraction | abstracted Task 3.3: deny-override query | planned decision fixture | planned parity | planned reason display |
| `S-AUTH-010` | checked Task 2.2 immutable policy evidence | abstracted Task 3.1: decision completeness | abstracted Task 3.2: decision outputs | queried Task 3.3 stable rule/explanation rows | planned decision fixture | planned audit log | planned policy reason |
| `S-AUTH-011` | checked Task 2.2 audit-only non-authority | abstracted Task 3.1: no-authority assertion | abstracted Task 3.2: audit flag | abstracted Task 3.3: hypothetical query | planned audit fixture | planned parity | N/A: operator diagnostics only |
| `S-AUTH-012` | checked Task 2.2 unknown-principal defect | abstracted Task 3.1: transport-not-capability | abstracted Task 3.2: peer flag | abstracted Task 3.3: deny query | planned peer fixture | planned Veilid attack | planned auth error |
| `S-AUTH-013` | checked Tasks 2.2/5.1 stable public-key principal/restart | abstracted Task 3.1: stable-principal abstraction | abstracted Task 3.2: route-rotation | abstracted Task 3.3: identity query | checked Task 5.1 public identity schema | checked Task 5.1 store restart; route e2e Task 5.3 | planned identity display |
| `S-AUTH-014` | deferred integration phases: invite unit/prop | abstracted Task 3.1: invite scope | abstracted Task 3.2: invite state | abstracted Task 3.3: issuance query | planned code vectors | planned DHT e2e | planned code display |
| `S-AUTH-015` | checked Task 2.2 redeem/replay/expiry units | abstracted Task 3.1: key binding | abstracted Task 3.2: replay/expiry | abstracted Task 3.3: redemption query | planned invite attacks | planned join e2e | planned join reasons |
| `S-AUTH-016` | checked Task 5.1 event signing/verifier implementation; vectors Task 5.4 | N/A: unforgeability assumed | abstracted Task 3.2: signed-event flag | abstracted Task 3.3: provenance query | checked Task 5.1 canonical event domain | planned tamper e2e | planned integrity error |
| `S-AUTH-017` | checked Tasks 2.2/2.4 gap/reorder defects | abstracted Task 3.1: total-order assertion | abstracted Task 3.2: revision sequence | abstracted Task 3.3: predecessor query | checked Task 2.4 first-divergence fixture | planned reorder/recovery | planned syncing state |
| `S-AUTH-018` | checked Task 2.4 prefix snapshot/tail hashes | abstracted Task 3.1: identity abstraction | abstracted Task 3.2: snapshot revision | abstracted Task 3.3: recovery query | checked Task 2.4 snapshot payload vectors | planned recovery e2e | planned syncing state |
| `S-AUTH-019` | checked Tasks 2.1-2.4/5.1 non-diagnostic secret types and snapshot scan | N/A: secret bytes omitted by scope | N/A: secret bytes omitted by scope | abstracted Task 3.3: non-generation query | checked Tasks 2.4/5.1 public-only identity schema | checked Task 5.1 protected-store-only secret blob | planned redaction scan |
| `S-AUTH-020` | checked Task 2.1 bounded decoder fuzz | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside query model | planned malformed corpus | planned oversize attacks | planned safe error |
| `S-AUTH-021` | checked Tasks 2.2/2.4 atomic unchanged hashes | abstracted Task 3.1: no-partial assertion | abstracted Task 3.2: atomic transition | abstracted Task 3.3: failure query | checked Task 2.4 rejected-command transcript | planned fault e2e | planned unchanged view |
| `S-AUTH-022` | checked ADR/docs; executable disclosure later | abstracted Task 3.1: malicious host outside claim | abstracted Task 3.2: host-fault counterexample | abstracted Task 3.3: trust query | planned threat metadata | planned host model disclosure | planned conspicuous disclosure |

## Viewer projection

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-VIEW-001` | checked Task 2.3 player projection/noninterference | checked Task 3.1 self-knowledge relation | abstracted Task 3.2: visibility flag | queried Task 3.3 own-hand matrix | checked Task 2.3 typed own-hand field | planned private delivery | planned own-hand screen |
| `S-VIEW-002` | checked Task 2.3 public spectator projection | checked Task 3.1 no-grant/no-edge assertion | abstracted Task 3.2: spectator flag | queried Task 3.3 public visibility matrix | checked Task 2.3 absent private fields | checked Task 4.2 full-game spectator projection | checked Task 4.3 public-only text; egui pending |
| `S-VIEW-003` | checked Task 2.3 exact request unit | abstracted Task 3.1: request relation | abstracted Task 3.2: request state | queried Task 3.3 request successor/chain | checked Tasks 2.1/4.5 request replay | checked Task 4.5 spectator request | planned request control |
| `S-VIEW-004` | checked Task 2.3 owner grant/deny/request/epoch gate | checked Task 3.1 owner/spectator grant relation | abstracted Task 3.2: grant state | queried Task 3.3 grant explanations | checked Tasks 2.3/4.5 exact grant replay | checked Task 4.5 owner grant | planned owner grant control |
| `S-VIEW-005` | checked Task 2.3 exact revoke unit | checked Task 3.1 `RevocationStopsFutureKnowledge` | abstracted Task 3.2: revoke state | queried Task 3.3 revoke explanations | checked Tasks 2.1/4.5 exact revoke replay | checked Task 4.5 revoke e2e | planned owner revoke control |
| `S-VIEW-006` | checked Task 2.3 pairwise viewer test | checked Task 3.1 `ScopedSpectatorGrant` | abstracted Task 3.2: grant flag | queried Task 3.3 exact can-see matrix | checked Tasks 2.3/4.5 scoped hand hash | checked Task 4.5 exact recipient/card comparison | checked Task 4.3 granted-hand text; egui pending |
| `S-VIEW-007` | checked Task 2.3 revoke/expiry future test | checked Task 3.1 future-edge absence | abstracted Task 3.2: post-revoke state | queried Task 3.3 revoke produces false can-see | checked Tasks 2.3/4.5 projection epoch | checked Task 4.5 absent post-revoke projection | planned future removal |
| `S-VIEW-008` | checked Task 2.3 public-history/schema test | abstracted Task 3.1: no-public-hand | abstracted Task 3.2: broadcast flag | abstracted Task 3.3: public visibility query | checked Task 2.3 event hash/public types | planned packet scan | planned client-state scan |
| `S-VIEW-009` | deferred integration phases: crypto adapter vectors | N/A: crypto assumed | abstracted Task 3.2: recipient flag | abstracted Task 3.3: recipient relation | planned encrypted metadata | planned wrong-key e2e | planned decrypt failure |
| `S-VIEW-010` | checked Tasks 2.3/2.4 reconnect/snapshot hashes | checked Task 3.1 current-entitlement derivation | abstracted Task 3.2: reconnect state | queried Task 3.3 visibility from current grant | checked Task 2.4 scoped snapshot projections | planned reconnect e2e | planned restored view |
| `S-VIEW-011` | checked Task 2.3 observation/history audit | N/A: existing game knowledge compared | N/A: schema shape outside lifecycle | abstracted Task 3.3: public-history query | checked Task 2.3 ordered prefix schema | N/A: transport carries projection | planned history rendering |
| `S-VIEW-012` | checked Task 2.3 immutable/future projection test | N/A: past knowledge cannot be erased | N/A: client memory outside session | abstracted Task 3.3: future-only query | checked Task 2.3 epoch/expiry event | planned stop-delivery e2e | planned cache removal |
| `S-VIEW-013` | checked Task 2.3 four-viewer controlled defect | checked Task 3.1 scoped-grant noninterference | abstracted Task 3.2: visibility safety | queried Task 3.3 matrix and broadcast defect | checked Task 2.3 typed projection equality | planned multi-view capture | planned DOM/widget scan |
| `S-VIEW-014` | checked Task 2.3 local diagnostics separation | N/A: host owns state by model | abstracted Task 3.2: host-full-state flag | abstracted Task 3.3: host can-see query | N/A: host-local full state | planned disclosure only | planned host-trust notice |

## Chat

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-CHAT-001` | checked Task 2.2 member chat gate | abstracted Task 3.1: permission assertion | abstracted Task 3.2: chat action | queried Task 3.3 member chat policy | checked Task 2.4 chat transcript | checked Task 4.4 member/outsider loopback | planned composer |
| `S-CHAT-002` | checked Task 2.1 UTF-8/size validation | N/A: text omitted | abstracted Task 3.2: bounded flag | N/A: text omitted | checked Task 4.4 strict NDJSON boundary | checked Task 4.4 oversize rejection | planned validation error |
| `S-CHAT-003` | checked Task 2.2 logical rate unit | abstracted Task 3.1: bounded counter | abstracted Task 3.2: rate state | abstracted Task 3.3: rate query | checked Task 4.4 rate denial | checked Task 4.4 burst sequence | planned retry display |
| `S-CHAT-004` | checked Task 2.2 attributed event | abstracted Task 3.1: attribution assertion | abstracted Task 3.2: sender state | abstracted Task 3.3: who-sent query | checked Tasks 2.4/4.3/4.4 attributed replay/export | checked Task 4.4 attributed tail | checked Task 4.3 escaped sender text; egui pending |
| `S-CHAT-005` | checked Task 4.4 bounded tail/truncation | N/A: durable history out of scope | abstracted Task 3.2: bounded count | abstracted Task 3.3: recent-chat query | checked Task 4.4 public tail export | checked Task 4.4 in-memory runtime; restart loss specified | planned ephemeral notice |
| `S-CHAT-006` | checked Task 2.1 newline/control fuzz | N/A: parser outside scope | N/A: parser outside scope | N/A: parser outside scope | checked Task 4.4 escaped single-frame export | checked Task 4.4 duplicate/injection loopback | checked Task 4.3 literal escaped text; egui pending |
| `S-CHAT-007` | checked Task 2.2 phase-preserving chat | abstracted Task 3.1: phase permission | abstracted Task 3.2: phase transitions | queried Task 3.3 phase-preserving self-loop | checked Tasks 2.4/4.3/4.5 phase replay | checked Task 4.5 lobby/countdown/running/paused/post-game | planned multi-phase composer |
| `S-CHAT-008` | checked Task 4.4 content-free bounded runtime tail | abstracted Task 3.1: bounded metadata only | abstracted Task 3.2: bounded metadata | abstracted Task 3.3: permission/count query | N/A: modeling rule | N/A: content not formal | N/A: content still rendered |
| `S-CHAT-009` | checked Task 4.4 export secret scan | N/A: secrets omitted | N/A: secrets omitted | abstracted Task 3.3: no-auto-source query | checked Task 4.4 public-only export type | checked Task 4.4 capture scan | planned UI scan |

## Logical time

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-TIME-001` | checked Task 2.2 pure logical reducer | abstracted Task 3.1: logical ordering | checked Task 3.2 logical command steps | abstracted Task 3.3: event query | planned logical fields | checked Task 4.2 manual runtime boundary | N/A: no wall clock authority |
| `S-TIME-002` | deferred integration phases: clock-adapter unit | N/A: external time abstracted | checked Task 3.2 eventual-expiry assumption | abstracted Task 3.3: authority query | planned expiry fixture | checked Task 4.2 manual harness; native clock pending | planned estimate only |
| `S-TIME-003` | checked Task 2.2 same-deadline race unit | abstracted Task 3.1: abort-wins assertion | checked Task 3.2 abort-wins/stale-expiry safety | queried Task 3.3 abort/rearm history | planned race transcript | planned simultaneous harness | planned no false start |
| `S-TIME-004` | checked Task 2.2 duplicate-start defect | checked Task 3.1 `AtMostOnceStart` | checked Task 3.2 one-start invariant | queried Task 3.3 expiry start explanation | planned duplicate expiry | planned retry e2e | planned single transition |
| `S-TIME-005` | deferred integration phases: presentation-model unit | N/A: UI estimate | N/A: UI estimate | N/A: UI estimate | planned projected deadline | planned skew harness | planned browser/native test |
| `S-TIME-006` | deferred integration phases: checker assumptions | checked Task 3.1 bounded safety only; exact scopes documented | checked Task 3.2 named conditional CTL/LTL assumptions | abstracted Task 3.3: fairness explanation | planned evidence metadata | planned fault evidence | planned limitation text |
| `S-TIME-007` | checked Task 2.2 paused persistence/gate | checked Task 3.1 SAT pause/resume witness | checked Task 3.2 unconditional and missing-resume counterexamples | abstracted Task 3.3: paused successor loop | N/A: claim metadata only | planned partition/pause harness | checked Task 4.3 paused text/replay; egui pending |
| `S-TIME-008` | checked Task 2.4 deterministic replay | abstracted Task 3.1: logical steps | checked Task 3.2 deterministic scheduler traces | queried Task 3.3 three causal event histories | checked Task 2.4 transcript hashes | checked Task 4.2 typed/NDJSON no-socket delivery | planned replay parity |

## Fault and recovery

| Rule | Rust | Alloy | NuSMV | Prolog | Protocol | Network | UI |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `S-FAULT-001` | checked Task 2.4 duplicate/reorder mutations | abstracted Task 3.1: idempotence contract | abstracted Task 3.2: fault abstraction | abstracted Task 3.3: reorder predecessor | checked Task 2.4 golden mutation corpus | checked Task 4.2 duplicate/reorder/disconnect harness | planned syncing state |
| `S-FAULT-002` | deferred integration phases: adapter unit | N/A: DHT outside model | abstracted Task 3.2: hint/non-authority flag | abstracted Task 3.3: refresh query | planned watch metadata | planned Veilid watch test | planned refreshing state |
| `S-FAULT-003` | deferred integration phases: retry unit | N/A: API errors outside model | abstracted Task 3.2: retry abstraction | abstracted Task 3.3: retry-category query | planned retry metadata | planned forced failures | planned structured error |
| `S-FAULT-004` | checked Task 2.2 route-neutral reconnect state | abstracted Task 3.1: stable-key abstraction | checked Task 3.2 route-neutral disconnect/reconnect | abstracted Task 3.3: reconnect query | planned route metadata | planned rotation e2e | planned reconnect status |
| `S-FAULT-005` | checked Task 2.4 genesis/snapshot-tail parity | abstracted Task 3.1: snapshot contract | abstracted Task 3.2: gap/recovery | abstracted Task 3.3: recovery query | checked Task 2.4 snapshot+tail corpus | planned gap e2e | planned syncing/conflict |
| `S-FAULT-006` | deferred integration phases: boundary unit/fuzz | N/A: byte size outside model | abstracted Task 3.2: oversize flag only | N/A: bytes outside query | planned max-size vectors | planned Veilid limit e2e | planned safe error |
| `S-FAULT-007` | checked Task 2.2 host-loss/no-election unit | checked Task 3.1 fixed singleton host/no election transition | checked Task 3.2 fixed-host liveness limitation | abstracted Task 3.3: impossible-host query | planned unavailable event | planned host shutdown | planned host-unavailable screen |
| `S-FAULT-008` | deferred integration phases: evidence classification | N/A: network fact outside relational result | checked Task 3.2 unconditional persistence counterexample | abstracted Task 3.3: availability query | planned disposition metadata | planned unavailable harness | planned honest limitation |
| `S-FAULT-009` | deferred integration phases: no-network assertion/benchmark | N/A: RL path outside session model | N/A: rollout path outside lifecycle | N/A: rollout path outside query model | planned direct-call parity only | checked Task 4.2 no-socket dependency graph; RL benchmark pending | N/A: renderer optional |
| `S-FAULT-010` | checked Task 2.1 fuzz/no semantic entry | N/A: parser outside model | N/A: parser outside model | N/A: parser outside model | checked Tasks 2.1/2.5 malformed corpus/parity | planned malformed e2e | planned safe error |
| `S-FAULT-011` | deferred integration phases: redaction/error unit | N/A: diagnostics outside model | N/A: diagnostics outside model | abstracted Task 3.3: public-reason query | planned error fixtures | planned logs/capture scan | planned redacted error |

## Completion protocol

The Task 3.5 audit requires all 88 catalog IDs exactly once and rejects
`planned` or unclassified formal cells. It preserves exact scope/result counts,
fixture hashes, native tool versions, and controlled-defect outcomes. A
reasoned `N/A` may be revised only by adding a new applicable contract; absence
of an implementation is never itself an N/A. Later tracks retain descriptive
`planned` gates until their named phases produce evidence.
