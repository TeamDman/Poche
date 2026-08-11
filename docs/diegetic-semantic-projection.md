# Diegetic semantic projection

Poche's command palette and tabletop are two views of one typed action space.
The palette optimizes discoverability and accessibility. The tabletop grounds
the same actions in objects: seat, hand, card, play zone, speech bubble, clock,
score paper, rules paper, chat bubble, and door. Neither renderer invents a
second reducer.

## Direction of authority

1. The typed session/game state and retained command IDs define truth and legal
   mutations.
2. The semantic spatial scene realizes that state as stable objects, zones,
   visibility, and relationships in canonical integer units.
3. Native 3D, semantic HTML/CSS, text, and future renderers project those
   objects into renderer-native affordances.
4. Interaction maps an affordance back to an already-retained typed command.

The HTML projection is therefore not required to rasterize a camera. It should
use selectable text, forms, buttons, CSS transforms, SVG, and browser layout
where those are the browser's stronger primitives. A card drag and `Play 3♣`
button converge on the same `GameAction::Play`; geometry cannot manufacture a
card identity or bypass authorization.

## Action completeness

For the ordinary player adapter, every currently offered command must remain
reachable through the `Choose an action` palette. Game-world actions should
also have at least one tabletop affordance. This is intentionally asymmetric:
the palette is exhaustive, while a renderer may substitute an accessible
world affordance appropriate to its medium.

Today, a bid control is an atomic typed action. A fuller tabletop protocol may
split it into:

1. the player communicates an intended number through text, speech, voice, or
   a hand-sign gesture;
2. the scorekeeper proposes the written transcription on the score sheet;
3. participants receive a bounded opportunity to acknowledge or challenge;
4. the accepted transcription becomes canonical game state.

Those communication modalities must not become four inconsistent rule paths.
They produce evidence for one proposal/acknowledgement transition. Until that
protocol exists, the current direct bid control is a deliberate simplification,
not a claim that real table speech and scorekeeping are identical.

## Presence versus consensus history

Remote cursor poses, hover targets, hesitant movement around a hand, voice
activity, and camera/frustum pose improve social legibility. They are normally
ephemeral presence streams: loss or reordering must not change turn, score, or
card ownership. A recorder may optionally retain sampled presence beside a
canonical transcript for replay, but consensus operates on the typed command
history. Private-hand cursor projection must avoid leaking card identities or
precise hidden-card targeting to unauthorized viewers.

## Geometry and overlap

Canonical spatial layouts give objects and exclusive zones integer positions
and extents. Rust validates registered layouts and the Alloy spatial oracle
checks a bounded relational abstraction, including seeded overlap defects.
Renderer layout adds another refinement obligation. The web adapter labels
visible semantic footprints and measures real DOM bounding rectangles after
projection replacement and viewport resize. Any unexpected intersection is
named on the table element for tests and diagnostics.

Exact browser AABBs depend on viewport, fonts, zoom, and engine behavior, so
the runtime check complements rather than replaces the canonical spatial and
formal models. A useful future conformance receipt will bind viewport/browser,
semantic scene hash, footprint rectangles, allowed containment relations, and
zero unexpected intersections.

## Lifecycle and network boundary

Closing a tab or choosing `Exit table` means transport loss and retains durable
membership. `Reconnect` authenticates the same principal and restores only its
current exact-recipient projection. `Leave room` ends the current membership,
not the person's future access: a valid room code can admit a newly generated
spectator principal during any open phase. Unseated spectators may leave during
active play. A seated membership cannot leave during an active game because no
dropout/substitution transition yet resolves its abandoned hand and turn;
recoverable exit is the supported operation there. Seat claims are currently
restricted to the lobby before dealing.

The player web registry is currently process-local Axum state. NuSMV and Prolog
have explicit durable disconnect/reconnect abstractions; the Rust reducer and
transport tests exercise the concrete stable-principal behavior. The session
Alloy oracle abstracts connection into membership and does not independently
prove leave/reconnect traces. Native Veilid and the signed browser-device
gateway are separate transport/identity evidence; they are not yet the backing
store or authority adapter for `BrowserRooms`.
