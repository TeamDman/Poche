# Player-facing web client

The root of `poche-web-spike` is now a player entry point rather than a test
fixture. It presents a compact main menu, asks for a display name, and offers
`Create lobby` or `Join lobby`. The deterministic Alice/Bob/scenario console is
still useful, but is deliberately secondary at `/lab`.

## Player model

The adapter keeps three concepts separate:

| Value | Scope | Purpose |
|---|---|---|
| display name | chosen by one tab | Human-readable table label; not authority |
| principal | generated public identifier | Stable reducer identity for the process-local room |
| session token | generated secret URL bearer | Binds one browser tab to one exact-recipient projection |

The name is remembered with `sessionStorage`, not `localStorage`. Two tabs can
therefore use different names without synchronizing identity through shared
browser storage. Copying a secret `/game/{session}` URL currently copies that
development device authority; it must not be confused with the long-lived
multi-device key design in the application-identity and gateway documents.

## Room codes

Creating a room generates an 80-bit operating-system-random code formatted as
`PCH-XXXX-XXXX-XXXX-XXXX-XXXX`. It is independent of every participant name.
Joining with it causes the adapter to mint a fresh one-use reducer invite and a
fresh player principal; the typed session reducer still performs admission.

This is an honest local vertical slice, not the final routed invitation
protocol. The registry is memory-only, has no public rate limiter, and loses
rooms when the Axum process stops. Production codes still require the expiry,
checksum, routing, key-custody, and abuse decisions described by ADR 0009.

## Game surface

The room occupies the viewport instead of presenting a scrolling diagnostics
document. Its primary hierarchy is:

1. current viewer, room code with copy control, lifecycle phase, and revision;
2. players/scores and a prominent current-turn banner;
3. a lobby seat map or card table, depending on the typed room phase;
4. large controls that are legal for this exact viewer now;
5. collapsed activity, governance, diagnostics, and formal-evidence details.

The lobby visibly lists admitted names before they take seats. The card table
uses semantic HTML and CSS geometry: deck, trump, trick, hand fan, and clickable
or draggable legal cards. These are projections of typed state. CSS positions
do not become game authority and cannot invent cards or scores.

Human-facing ranks use suit glyphs (`3♣`, `Q♦`, `A♥`, `10♠`). Compact protocol
and historical event codes remain machine-oriented where they are canonical.
Every retained typed control appears in the `Choose an action` palette. A legal
card is therefore available both as the spatial card itself and as a `Play …`
button. Table/chat links are separated from infrequent room actions; destructive
leave, close, and remove controls carry an explicit confirmation prompt.

Chat is its own inspector accordion with attributed history and a real text
composer. The adapter accepts a form field, trims empty edges, constructs a
typed `Chat` command, and lets the existing protocol length/rate limits decide
authority. Viewer-controlled names and messages are escaped by the renderer.

## Synchronization

Each player page opens a native `EventSource` to its session-specific SSE route.
Commands are ordinary HTTP `POST`s. An accepted or denied command and every
countdown tick publish an update; the server independently re-renders the
exact-recipient fragment for each session. The browser replaces only
`#game-shell`. No player-facing command requires the external Datastar script.

Leaving and closing are retained adapter states rather than inferred from a
missing projection. A leaver sees `You have left this room`; every still-active
session sees `This room has been closed` after a close. Both terminal screens
remove command controls and offer `Return to main menu`. This prevents a removed
member's last Running projection from surviving as a misleading page whose
buttons only return `D-CLOSED`.

Clipboard handling is delegated across SSE replacements and has a selectable
textarea fallback, so both room-code and copy-safe diagnostic controls continue
to work after a projection patch.

The `/lab` developer harness retains its deterministic fixtures and Datastar
experiments. Keeping it available but off the product path prevents fixture
identities, pre-baked join codes, and scenario-reset controls from defining the
player mental model.

## Repeatable acceptance

The focused tests check both semantic layers:

- arbitrary names create and join one opaque room through different session
  tokens;
- only controls actually presented to each tab drive seat, ready, countdown,
  and transition to a running game;
- after game start exactly the current actor receives a game action;
- the rendered player document contains native SSE/HTTP wiring and no fixed
  Alice/Bob codes or CDN dependency;
- playable cards occur in both the hand and command palette, with suit glyphs;
- typed chat is attributed, escaped, and projected to other room members;
- close ends every active tab, leave ends only that device session, and neither
  terminal page contains stale command controls;
- destructive controls expose confirmation prompts and diagnostic copying uses
  the SSE-safe delegated clipboard handler;
- projection/privacy tests continue to exclude ungranted hands and hidden card
  faces.

Run them with:

```powershell
cargo test --locked -p poche-ui -p poche-web-spike --offline
```
