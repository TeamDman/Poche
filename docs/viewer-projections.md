# Viewer projections and spectator hand capabilities

Task 2.3 defines one privacy boundary for native, web, CLI, replay, and later RL
clients. `project_viewer(state, recipient, expected_projection_epoch)` is pure:
it reads the authoritative session and returns the only ordinary network-safe
`ProjectionPayload` shape. The host receives no implicit privilege through this
function.

## Versioned public knowledge

Projection protocol v1 selects an ordered public event prefix. Every projection
contains:

- the current room phase and public member list;
- a typed `GamePublicStateWire` with phase, turn, dealer, round, hand counts,
  trump, current trick, bids, tricks won, scores, and pot;
- `PublicGameEventWire` entries, in authority order, for game start, every
  player bid/play, and every round score;
- the viewer's current hand only when the viewer owns a seat;
- current hands from exact active spectator grants addressed to the viewer.

Chance input, undealt cards, captured-card storage, and other players' hands do
not occur in public state/history. The existing oracle observation reports only
the current trick, so that observation alone was insufficient after a trick was
cleared. The ordered event prefix closes that gap for reconnecting humans and
memoryless policies without disclosing chance or private state. The same public
fields are therefore the legal knowledge source for RL and web rendering.

The protocol schema hash after adding these types is
`1489b2887acc117dd1a9e98d2891b8640fa4d901621b734d1f7654940c4e11ad`.

## Capability lifecycle

A connected unseated non-host spectator may request one active player's hand.
The request stores the request command ID, exact player, and exact recipient.
Only that seated player can grant or explicitly deny the matching pending
request. Denial closes the request without changing projection state. A grant
stores only `(player, recipient, grant_epoch)`; cards are looked up only while
building that recipient's future projection. A spectator may have at most one
pending request or active hand grant, so the private extension is always
exactly one player's hand.

The owner can revoke only the exact current grant. Projection epochs advance on
grant, revoke, and effective expiry, so a caller using an older epoch fails
closed. Revocation cannot change a projection value already delivered.

Pending requests and grants expire at:

- every round-score boundary;
- any relevant seat-role change, including release or a spectator taking a
  seat;
- membership loss of either the player or recipient.

Transport disconnect is not membership loss. A grant can survive a temporary
disconnect, but disconnected viewers cannot receive a projection; reconnect
reuses the stable membership and re-evaluates current entitlements. Every
expiry is an authority event with a public reason and no cards.

## Host diagnostics and trust

The host-authoritative process necessarily owns the full hidden game. Full-hand
inspection requires `LocalHostDiagnosticCapability` and returns
`LocalHostDiagnosticProjection`, neither of which implements the protocol
serialization/reflection surface. The ordinary host projection still contains
only the host's own hand when seated. This separation prevents a renderer or
network adapter from accidentally treating host diagnostics as an ordinary
viewer payload; it does not claim a malicious host is unable to inspect memory.

Room-wide `EventPayload::GameTransitioned` contains only a semantic state hash.
Public history contains actions/scores, never private hands. Full `SessionEvent`
values are host-local reducer/event-log values and must not be broadcast as
protocol events.

## Executable evidence

Focused tests establish:

- pairwise noninterference across a seated host, another player, one granted
  spectator, and one ungranted spectator;
- ordinary host projection versus separately authorized local diagnostics;
- stale-epoch rejection, grant visibility, future-only revoke, and round
  boundary expiry ordering;
- reconnect reconstruction of the identical public state/history plus only the
  reconnecting player's current hand;
- protocol validation for player/vector/card bounds and the pinned schema.

Network encryption to the exact recipient remains Task 5 evidence. Snapshot and
tail replay of these projections remains Task 2.4; neither is claimed here.
