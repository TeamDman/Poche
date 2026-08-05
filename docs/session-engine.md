# Pure session engine

Task 2.2 implements the room lifecycle and authorization boundary in
`poche-session`, with the existing full-rule Poche environment connected by
`OracleSessionGame` in `poche-runtime`.

## State shape

`SessionState<G>` separates bounded membership records from a strong phase enum:

```text
Uninitialized
  -> Lobby
  -> Countdown { logical deadline, token }
  -> Running { game }
  <-> Paused { game }
  -> PostGame { game }
  -> Lobby (host reset)
  -> Closed
```

Countdown data cannot exist in a lobby, and a game value cannot exist before
start. Connection state remains orthogonal: a stable member may be connected or
disconnected while retaining at most one seat. The engine validates a maximum
of eight memberships, unique principal/seat bindings, connected seated
readiness, countdown readiness, one retained host identity, and consistent
accepted-command records after every applied event.

The no-host-migration decision is executable. A host transport loss leaves the
same stable host membership disconnected; it never promotes another member.
The room may stop progressing until that key reconnects.

## Pure command path

The three stages have no clock, network, renderer, random generator, or global
mutable input:

```text
authorize(state, signed command) -> immutable PolicyDecision
decide(state, AuthorizedCommand) -> [SessionEvent]
apply(state, SessionEvent) -> new state
```

Authorization checks exact duplicate IDs before current revision, then exact
room, session epoch, revision, derived principal/capability, custom policies,
and default deny. An enforce deny overrides every allow. Audit-only results are
retained in evidence but never change authority. Unknown principals are denied
before a policy for an `Unknown` kind can grant access.

Every event carries command semantic hash, principal, correlation ID, base
revision, event index, and the immutable allow decision. Apply requires a
gap-free index/revision and is atomic. Reapplying an exact recorded event is a
no-op; conflicting command-ID reuse or changed event content fails closed.

## Lifecycle behavior

Executable tests cover:

- create, one-time/expiring/revoked invite redemption, seating, readiness,
  release, voluntary leave, host removal of another member, reset, and close;
- host-armed countdown, abort by any seated player, authority-clock expiry, and
  same-deadline ordering in which an accepted unready/abort makes expiry stale;
- pause and unpause by any connected seated player, including different
  players, while game/chance/settlement cannot advance in `Paused`;
- player actor checks, environment-only chance/settlement, terminal post-game,
  and raw round-score propagation;
- trusted transport-loss events that cancel an affected countdown, plus
  reconnect by the same stable application principal without another invite;
- attributed chat in every open initialized phase with five messages per
  20-logical-revision window; limits are reducer state, never wall time;
- exact duplicate replay, stale room/epoch/revision, stable command-specific
  denials, and all 31 registered deny codes through immutable policy evidence.

Controlled defects directly construct invalid readiness, duplicate-start,
paused-advance, event-order, duplicate-seat, and attempted unknown-principal
allow states. Each is rejected by validation, authorization, or apply.

## Full Poche composition

`OracleSessionGame<const PLAYERS>` adapts the existing `OracleEnvironment` to
the session's pure `SessionGame` port. It validates dense seat composition,
maps typed bid/play commands, reconstructs and validates a complete 52-card
deck, verifies seeded chance provenance against the explicit deck, preserves
raw round scores, and reports terminal state. The session calls those methods
only from `Running` and only for the current action owner.

This keeps the protocol/session engine generic without replacing the already
checked Poche rules with a second gameplay implementation.

## Deliberate next boundaries

Task 2.3 adds viewer grants and projections; until then those three typed
commands remain default-denied. Task 2.4 owns bounded transcripts, snapshots,
recovery tails, and replay. Task 5 supplies real Ed25519 verification and invite
issuance/storage around the canonical bytes; Task 2.2 tests authorization
semantics but does not claim cryptographic authenticity or trustless hosting.
