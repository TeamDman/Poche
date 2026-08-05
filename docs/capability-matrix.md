# Session capability and projection matrix

- Status: Phase 1 authorization contract complete; implementation evidence is
  tracked in `session-coverage.md`
- Rule source: `session-rules.md`
- Architecture source: `decisions/0003-session-network-rl-architecture.md`

This document turns the session rules into a default-deny authorization
contract. A transport connection, Veilid node ID, private route, room code, or
claimed role is never authority by itself. Authority comes from a verified
application principal with active capabilities in the current room and epoch.

## Principal kinds

Principal kinds classify authenticated application identities for policy
matching. They are not trusted claims supplied by a command. The authority
derives them from current membership and system state.

| Kind | Meaning | Authority source | Baseline projection |
| --- | --- | --- | --- |
| `unknown` | No current application membership or system role | None | None |
| `invite-holder` | Possesses a valid one-time/expiring invite but is not yet a member | Validated invite plus signed stable application key | Join response only |
| `host` | Stable application key that created and currently owns the room authority | Room genesis record and current membership epoch | Full trusted-host state |
| `member` | Connected, unseated room member | Current membership capability | Public room/game projection |
| `player` | Connected member occupying exactly one active seat | Current membership plus seat capability | Public projection plus own hand |
| `disconnected-member` | Durable member whose transport is unavailable | Current durable membership; no live connection | No live delivery; reconnect result only |
| `spectator` | Current member participating without a seat | Current membership plus spectator capability | Public room/game projection |
| `revoked` | Former member, invite, or capability holder whose epoch is no longer current | Revocation record | None |
| `authority-clock` | Runtime-owned logical-time adapter | Locally configured system identity | No viewer projection |
| `game-environment` | Runtime-owned chance and deterministic-settlement adapter | Locally configured system identity | No viewer projection |

An unrecognized kind is `unknown` and yields `D-UNKNOWN-ROLE`. An authenticated
key with no applicable membership/system role yields `D-UNKNOWN-PRINCIPAL`.
Neither case falls back to `member` or `spectator`.

## Capability vocabulary

Capabilities are room-bound, epoch-bound, revocable, and narrowly scoped. A
capability is only an input to policy evaluation; an enforce-mode deny still
overrides it.

| Capability scope | Holder | Permits | Does not permit |
| --- | --- | --- | --- |
| `room.host` | Current host | Host lifecycle and membership administration commands | Bypassing phase, revision, signature, or game legality checks |
| `room.member` | Current connected member | Room participation and commands shared by members | Taking a seat, playing, or viewing a hand by itself |
| `room.reconnect` | Durable current member key | Rebinding a new authenticated transport | Reusing a revoked membership or changing principal identity |
| `room.seat:<seat>` | Exactly one current member | Ready/unready and seat-scoped player status | Acting for another seat |
| `room.play:<seat>` | Current active seated player | Legal game action when that seat is the current actor | Chance, settlement, clock expiry, or action while paused |
| `room.chat` | Current non-revoked member | Bounded attributed chat | Lifecycle mutation or private-state disclosure by the application |
| `room.spectate` | Current unseated spectator | Public spectator projection and hand-view requests | Private hand access |
| `view.request` | Current spectator | Requesting one player's hand for the requesting recipient | Granting a request or accessing the hand |
| `view.hand:<player>:<recipient>:<epoch>` | Exact recipient after player grant | Future projections of exactly the named player's current hand | Other hands, other recipients, or projections after revocation/epoch change |
| `clock.countdown` | Authority clock | Emitting the matching logical countdown-expiry token | Player actions, pause, or direct game start |
| `game.environment` | Game environment | Explicit chance values and deterministic settlement requested by the game | Player action selection or lifecycle administration |

## Command authorization matrix

Every row is checked after structural, signature, replay, room, epoch, and
revision validation. `Actor` names the derived principal kinds that can reach
the command-specific policy. `Capability` is an exact required allow scope.
The listed denials are command-specific; the common denials in the evaluation
order below also apply.

| Command | Actor | Capability | Additional preconditions | Accepted semantic effect | Principal deny reasons |
| --- | --- | --- | --- | --- | --- |
| `CreateRoom` | New authenticated host key | Genesis operation; creates `room.host` | No existing room ID; valid protocol and key | Create revision-0 lobby and host membership | `D-MALFORMED`, `D-MISSING-CAPABILITY` |
| `RedeemInvite` | `invite-holder` | Exact valid invite proof | Correct room, unexpired, unused, current verifier | Consume invite and bind connected membership to stable key | `D-INVITE-INVALID`, `D-INVITE-EXPIRED`, `D-WRONG-ROOM`, `D-REVOKED` |
| `TakeSeat` | `member`, `host`, or `spectator` | `room.member` | Lobby, connected, unseated, requested seat free | Bind `room.seat:<seat>` and player kind | `D-WRONG-PHASE`, `D-SEAT-OCCUPIED`, `D-ALREADY-SEATED`, `D-NOT-CONNECTED` |
| `ReleaseSeat` | `player` | Exact `room.seat:<seat>` | Lobby/countdown; caller owns seat | Release seat, readiness, play scope; cancel affected countdown | `D-NOT-SEATED`, `D-WRONG-PHASE`, `D-MISSING-CAPABILITY` |
| `Ready` | `player` | Exact `room.seat:<seat>` | Lobby, connected | Mark owned seat ready | `D-WRONG-PHASE`, `D-NOT-SEATED`, `D-NOT-CONNECTED` |
| `Unready` | `player` | Exact `room.seat:<seat>` | Lobby or countdown | Mark unready and cancel current countdown | `D-WRONG-PHASE`, `D-NOT-SEATED` |
| `ArmCountdown` | `host` | `room.host` | Lobby, minimum seats, every occupied seat connected/ready | Enter countdown with logical deadline/token | `D-NOT-READY`, `D-WRONG-PHASE`, `D-MISSING-CAPABILITY` |
| `AbortCountdown` | Any active `player` | Exact `room.seat:<seat>` | Current countdown | Return to ready-state lobby without starting | `D-NOT-SEATED`, `D-COUNTDOWN-INACTIVE` |
| `CountdownExpired` | `authority-clock` | `clock.countdown` | Matching active token and readiness; player abort/unready wins same-deadline ordering | Start exactly one game and consume countdown | `D-ENVIRONMENT-ONLY`, `D-COUNTDOWN-INACTIVE`, `D-NOT-READY` |
| `Pause` | Any active `player` | Exact `room.seat:<seat>` | Running and unpaused | Enter paused without changing game state | `D-NOT-SEATED`, `D-WRONG-PHASE`, `D-ALREADY-PAUSED` |
| `Unpause` | Any active `player` | Exact `room.seat:<seat>` | Paused | Return to running without changing game state | `D-NOT-SEATED`, `D-NOT-PAUSED` |
| `GameAction` | Current active `player` | Exact `room.play:<seat>` | Running, unpaused, caller is current actor, action legal | Apply one pure player transition | `D-NOT-SEATED`, `D-NOT-ACTOR`, `D-WRONG-PHASE`, `D-PAUSED` |
| `ApplyChance` | `game-environment` | `game.environment` | Running, unpaused, game requests chance, value valid | Apply explicit replayable chance transition | `D-ENVIRONMENT-ONLY`, `D-WRONG-PHASE`, `D-PAUSED` |
| `Settle` | `game-environment` | `game.environment` | Running, unpaused, game requests deterministic settlement | Apply settlement and emit raw round score | `D-ENVIRONMENT-ONLY`, `D-WRONG-PHASE`, `D-PAUSED` |
| `Chat` | `host`, `member`, `player`, or `spectator` | `room.chat` | Room open; bounded text; within rate window | Emit attributed ephemeral side-stream event | `D-CHAT-SIZE`, `D-CHAT-RATE`, `D-REVOKED`, `D-CLOSED` |
| `RequestHand` | `spectator` | `view.request` and `room.spectate` | Target is active player; recipient is caller | Create exact attributed pending request | `D-GRANT-SCOPE`, `D-REVOKED`, `D-MISSING-CAPABILITY` |
| `GrantHand` | Target `player` | Exact `room.seat:<target>` | Matching current request and exact recipient | Create `view.hand:<target>:<recipient>:<epoch>` | `D-GRANT-SCOPE`, `D-NOT-SEATED`, `D-STALE-EPOCH` |
| `RevokeHand` | Granting target `player` | Exact `room.seat:<target>` | Exact current grant | Revoke grant and advance grant epoch before future delivery | `D-GRANT-SCOPE`, `D-NOT-SEATED`, `D-STALE-EPOCH` |
| `Reconnect` | `disconnected-member` | `room.reconnect` | Current signed membership proof; new route; room open | Rebind transport and restore only current entitlements | `D-REVOKED`, `D-STALE-EPOCH`, `D-BAD-SIGNATURE`, `D-CLOSED` |
| `Leave` | `host`, `member`, `player`, or `spectator` | `room.member` | Room open and caller is current member | Revoke caller epoch; release seat/grants; cancel affected countdown | `D-UNKNOWN-PRINCIPAL`, `D-REVOKED`, `D-CLOSED` |
| `RemoveMember` | `host` | `room.host` | Active target membership and host-removal policy | Revoke target epoch, seat, grants, and affected countdown | `D-MISSING-CAPABILITY`, `D-UNKNOWN-PRINCIPAL`, `D-DENY-POLICY` |
| `ResetLobby` | `host` | `room.host` | Post-game and room open | Archive result; return seated members to unready lobby | `D-WRONG-PHASE`, `D-MISSING-CAPABILITY`, `D-CLOSED` |
| `CloseRoom` | `host` | `room.host` | Room open | Enter absorbing closed phase and revoke invites/routes | `D-MISSING-CAPABILITY`, `D-CLOSED` |

There is deliberately no `StartGame` command and no command for appointing a
replacement host. Either tag is unknown in this protocol version and returns
`D-UNKNOWN-COMMAND`. A host cannot invoke `CountdownExpired`, `ApplyChance`, or
`Settle` merely because it holds `room.host`; those require the distinct local
system capabilities.

## Authorization evaluation order

The authority evaluates an immutable attempt and records an immutable decision
before reduction. The order is normative so implementations cannot turn a
malformed or unauthorized request into a partial state change.

1. Enforce transport and codec bounds, exactly one frame, UTF-8, canonical
   shape, registered protocol/signature-domain version, and registered command
   tag. Fail with `D-OVERSIZE`, `D-MALFORMED`, `D-UNKNOWN-VERSION`, or
   `D-UNKNOWN-COMMAND`.
2. Verify the application signature over canonical signed bytes. Fail with
   `D-BAD-SIGNATURE`.
3. Resolve duplicate command ID. Return the prior recorded decision/events for
   an exact duplicate; reject conflicting reuse without reapplying it.
4. Require exact room, current session/membership/capability epochs, and exact
   expected revision. Fail with `D-WRONG-ROOM`, `D-STALE-EPOCH`, or
   `D-STALE-REVISION`.
5. Derive the principal kind and active capabilities from authority state.
   Unknown principal and unknown role fail with `D-UNKNOWN-PRINCIPAL` and
   `D-UNKNOWN-ROLE`; neither gains a baseline allow.
6. Evaluate every applicable policy. Any enforce-mode deny wins and produces
   `D-DENY-POLICY` with stable policy ID/reason. Audit-only decisions are
   recorded but cannot grant or deny authority.
7. Require an exact active capability for the command and resource scope. Fail
   with `D-REVOKED`, `D-MISSING-CAPABILITY`, or the command-specific stable
   reason in the matrix.
8. Check lifecycle, logical-time, actor, and game preconditions. Record one
   stable denial on failure.
9. Atomically apply the pure reduction and append the gap-free signed event(s),
   or apply nothing. Diagnostics contain only redacted public context.

## Viewer projection matrix

Projection is a server-side/object-graph boundary, not a widget visibility
flag. The renderer, text protocol, network adapter, and replay viewer receive
only the projection for the exact viewer.

| Viewer/destination | Public room and game state | Own hand | Another player's hand | Delivery notes |
| --- | --- | --- | --- | --- |
| Trusted `host` process | Yes | If seated | Yes, because the authoritative host owns full hidden state | The UI must disclose this non-trustless model |
| Active `player` | Yes | Yes, exact owned seat only | No, unless separately granted as an exact spectator-style recipient | Projection omits unauthorized fields before serialization |
| Connected unseated `member` | Yes | No | No | May later take a seat or spectate subject to policy |
| `spectator` without grant | Yes | No | No | A pending request reveals no hand |
| `spectator` with exact current grant | Yes | No | Only `<player>` named by `view.hand:<player>:<recipient>:<epoch>` | Future delivery stops immediately after revoke/epoch change |
| `disconnected-member` | No live delivery | No live delivery | No live delivery | Reconnect rebuilds from public history plus current entitlements only |
| `revoked` or `unknown` | No | No | No | No projection or live room stream |
| Public DHT/rendezvous record | Locator/routing metadata only | Never | Never | No chat, secrets, private hands, or full snapshot |
| Room-wide event stream | Public event data only | Never | Never | Private projections use exact recipient delivery |
| UI/text renderer | Whatever is present in its supplied viewer projection | Never fetched independently | Never fetched independently | Rendering cannot query authoritative hidden state |

Revocation prevents future projection and delivery; it does not claim to erase
information the recipient already saw, copied, or captured. Reconnect never
replays historical private cards unless they are now public or covered by a
current grant.

## Default-deny completion checks

Phase 2 and later evidence must demonstrate all of the following:

- every registered command maps to exactly one semantic command type and one
  matrix row;
- every unknown command tag, unknown role tag, and unknown principal is denied
  before reduction;
- adding a new principal kind or command without an explicit policy/matrix
  entry cannot inherit a wildcard allow;
- a capability for one room, epoch, seat, player, recipient, or grant epoch
  cannot authorize another;
- deny override, audit-only non-authority, idempotent duplicate handling, and
  atomic no-partial-event behavior are tested independently;
- projections are pairwise tested so changing hidden state cannot affect an
  unauthorized viewer's serialized result.
