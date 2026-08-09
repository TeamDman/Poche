# Poche phase 4: player-facing web experience

**Plan ID:** `poche-phase-4-player-web`
**Plan status:** Execution complete
**Primary implementation root:** `D:\Repos\Games\poche-3` on `model-checking`
**Last updated:** 2026-08-09 (America/Toronto)
**Intent audit:** Final three-pass audit passed 2026-08-08 against the web-player discussion and the completed phase-three plan
**Current implementation focus:** Complete; future production networking remains separately scoped

## Resumption rules

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

Update status, evidence, exact commands, and decisions together. Keep product
paths distinct from deterministic evidence harnesses. Do not weaken exact-
recipient privacy or reducer authority to make the UI convenient. A phase is
complete only when its observable flow, regressions, documentation, commit,
push, and intent audit are complete.

The plan is only ready once we have literally triple checked that no intent
from the user has been omitted without explicit direction from the user.

## Purpose

Replace the hard-coded Alice/Bob diagnostic fixture as Poche's root web mental
model with an actual game entry flow: choose a name, create or join an opaque
room, occupy a seat, ready, start, and play from two independent browser tabs.
Retain the typed reducer, exact-recipient projections, formal diagnostics, and
deterministic scenarios as explanatory evidence, but make them secondary to the
player experience.

## Authoritative user guidance ledger

| ID | Active guidance | Required consequence |
|---|---|---|
| P4-U1 | The website is a game window, not a hard-coded Alice/Bob scenario page. | `/` is a centered main menu; the fixture moves to `/lab`. |
| P4-U2 | A main menu should progressively reveal name, create, and join actions. | Name plus large create action are primary; join code is disclosed under `Join lobby`. |
| P4-U3 | A room code must be opaque, hard to guess, and independent of a predetermined player. | OS-random `PCH-…` room code admits newly generated principals; no identity-bound Alice/Bob code appears. |
| P4-U4 | Two tabs must act as different people instead of synchronizing one identity. | Names use tab-scoped `sessionStorage`; each admission receives a different secret session URL. |
| P4-U5 | Lobby and gameplay should fill the viewport, minimize scrolling, and resemble a game/mobile command surface. | Full-viewport HUD, seat map, table, turn banner, large current controls, responsive CSS. |
| P4-U6 | Cards should be recognizable HTML/CSS objects, not a textual hand dump; browser-native CSS/SVG geometry is welcome. | Semantic card, deck, trump, trick, hand-fan, seat, and countdown elements project typed state. |
| P4-U7 | Joining, occupied seats, countdown, denials, and other tabs' actions must visibly update. | Native exact-recipient SSE plus HTTP commands; all room mutations publish fresh per-session fragments. |
| P4-U8 | Diagnostics and formal state-machine explanations are valuable but must explain rather than dominate. | Activity, governance, and formal diagnostics are collapsed inspector surfaces; copy-safe context names both display viewer and principal. |
| P4-U9 | Tests must prove that presented controls can play, not merely render preprogrammed states. | A two-session test uses only retained visible control IDs from create through running gameplay. |
| P4-U10 | “Host” is the wrong product identity for decentralized intent; creator coordination is only a scoped compatibility capability. | Player UI says coordinator where needed and creates an ordinary named participant; docs preserve the authority qualification. |
| P4-U11 | The result must be documented, committed, and pushed. | README, focused design note, this durable plan, strict verification, clean commit, remote verification. |

## Decisions

1. **Display identity is not authority.** A human name, generated public
   principal, and secret tab session are separate values.
2. **The player path is dependency-light.** Native `EventSource` receives SSE;
   ordinary `POST` sends commands. Datastar remains an explicit `/lab`
   experiment rather than a required CDN runtime for playing.
3. **The room code is adapter rendezvous, not lasting authority.** The local
   slice uses 80 random bits and mints a fresh one-use reducer invite per join.
   Production expiry, checksums, gateway routing, abuse controls, and device
   keys remain governed by ADR 0009 and the identity/gateway tracks.
4. **Semantic state remains stronger than layout.** HTML/CSS geometry renders
   the exact typed projection. Moving pixels never creates a card or bypasses a
   retained typed control.
5. **The deterministic lab is preserved, not deleted.** It remains valuable for
   repeatable privacy, denial, transport, and scenario evidence at `/lab`.

## Work items and evidence

### [x] 1. Separate names, principals, and sessions

- `PresentationModel` and member presentation carry escaped display names while
  principal IDs remain available for comparison and diagnostics.
- `BrowserRooms` creates independent random room, principal, and session values.
- Two arbitrary names share a room without either name appearing in its code.

### [x] 2. Build the player entry and room lifecycle

- `/` renders the main menu; `/game/create`, `/game/join`, and secret
  `/game/{session}` paths drive process-local rooms.
- Names persist only within a tab. Join codes normalize case, spaces, and
  hyphens but reject malformed or unknown rooms.
- The creator and joiner can independently take seats and ready. The creator's
  coordinator capability may arm the logical countdown.

### [x] 3. Build the game-like presentation

- Compact HUD includes viewer, copyable room code, lifecycle, transport, and
  revision.
- Lobby includes connected names, two spatial seats, ready state, and countdown.
- Running view includes a viewer-relative circular table, player avatars,
  opaque opponent hands, dealer-relative deck/trump, integrated private hand,
  actor cue, paper-style score sheet, and phase-appropriate primary actions.
- Developer evidence is collapsed and the inspector width is subordinate to
  the table.

### [x] 4. Synchronize exact-recipient tabs

- Each tab opens its own native EventSource route.
- Commands and drag actions use ordinary HTTP POST.
- Every command outcome and countdown change broadcasts an invalidation; the
  server re-renders separately for each session before replacing `#game-shell`.
- Copying the room code provides visible `Copied` feedback.

### [x] 5. Verify behavior and boundaries

Executed on 2026-08-08 with Rust 1.96:

```powershell
cargo test --locked -p poche-ui -p poche-web-spike --offline
cargo clippy --locked -p poche-ui -p poche-web-spike -p poche-xtask --all-targets --offline -- -D warnings
cargo test --workspace --locked --offline --no-run
cargo build --locked -p poche-web-spike --offline
```

The final focused run passes 17 `poche-ui` and 30 `poche-web-spike` tests. The new
acceptance uses the controls presented to two independent sessions to take
seats, ready, count down, and reach running gameplay; it confirms exactly the
eligible actor receives a game action. The rendered-document test requires
native SSE/POST wiring, a chosen name and random room code, and absence of the
fixed lab identities and Datastar CDN.

The full workspace behavioral invocation was attempted twice but its Cargo
wrappers stalled under the managed shell without spawning compiler or test
children; both exact PIDs were stopped. The replacement `--no-run` gate then
compiled every workspace test target successfully. No failure is represented
as a pass.

The in-app browser had already validated the new menu, arbitrary `Teamy` and
`Morgan` tabs, distinct session URLs, shared random room code, and lobby DOM.
That pass exposed the CDN-dependent update gap and caused the native
EventSource change. A final live-server pass could not run because Windows
processes launched by the managed shell stalled at loader initialization with
`0xc0000142`; the exact test children were stopped. The same binary builds and
the focused executable test harness runs. Launch from the user's ordinary
terminal remains the environment-specific manual acceptance path.

### [x] 6. Document and publish

- `README.md` contains the two-tab play path.
- `docs/player-web-client.md` distinguishes the implemented local slice from
  production identity/rendezvous claims.
- `docs/live-client.md` distinguishes `/` from `/lab`.
- Completion commit and remote branch verification are required release gates.

## Final three-pass intent audit

- **Pass 1 — extraction:** Re-read the reported hard-coded code/name behavior,
  two-tab failures, stuck/stale lifecycle reports, game-menu expectations,
  responsive viewport/card requests, and testing concern. P4-U1 through P4-U11
  each preserve a distinct requested outcome or qualification.
- **Pass 2 — traceability:** Checked every guidance row against a route, type,
  renderer, test, document, or named authority qualification. Checked the
  reverse direction: random adapter credentials, native EventSource, CSS card
  geometry, and `/lab` placement all trace to user-visible defects or existing
  architecture boundaries.
- **Pass 3 — adversarial omission:** Specifically checked likely omissions:
  names are not credentials; tabs do not share local identity; room codes no
  longer encode Bob; unseated joiners are visible; occupied-seat controls are
  removed by fresh projections; countdown advances; denied controls cause a
  fresh status projection; cards are semantic objects; diagnostics remain
  available but collapsed; formal tools are not falsely claimed to execute in
  browsers; coordinator is not advertised as decentralized consensus; and the
  managed-shell loader failure is disclosed rather than hidden. No active user
  intent was omitted or silently weakened.

## Contact-hardening follow-up — 2026-08-08

The first ordinary two-tab play session added six requirements without
superseding the original phase guidance:

| ID | Observed intent | Implemented consequence |
|---|---|---|
| P4-U12 | `3C` is unnecessarily opaque to a player; use suit glyphs. | Human-facing card, trump, hand, and action labels use `♣ ♦ ♥ ♠`; canonical event history remains untouched. |
| P4-U13 | `Choose an action` should be a command-palette entry point for everything, including a button for each playable card. | Every retained typed control appears exactly once in a partitioned palette; legal plays additionally remain clickable/draggable in the hand. |
| P4-U14 | Chat should be a complete accordion surface rather than a canned send button. | Added attributed history, arbitrary text composer, typed chat endpoint, escaping, and visible denial status. |
| P4-U15 | Leave/close are serious and need confirmation plus explicit intermediate states. | Danger group carries confirmation prompts; sessions retain `LeftRoom` or `RoomClosed` screens with a main-menu action. |
| P4-U16 | Diagnostic copying stopped working after the native SSE rewrite. | Player script now delegates diagnostic and room-code copying across fragment replacements with a textarea fallback and visible status. |
| P4-U17 | A closed room left another tab in stale Running state producing repeated `D-CLOSED`. | Adapter marks every still-active session closed immediately; terminal fragments contain no stale controls and close their SSE stream. |

The command-palette audit also found that ordinary rooms displayed governance
sidecar buttons whose IDs only exist in `TabletopLab`. Governance command
availability is now explicit: the ordinary player page keeps its explanatory
rules/votes surface but does not advertise commands its adapter cannot execute.

Focused regression evidence covers human suit labels, duplicate spatial/palette
access to each legal play, typed and escaped chat, close-all versus leave-one
terminal behavior, confirmation attributes, terminal documents without command
IDs, and the delegated diagnostic clipboard path. The browser server was not
running during the final in-app inspection attempt (`ERR_CONNECTION_REFUSED`),
so live visual acceptance remains qualified; source-level JavaScript syntax,
Rust renderer tests, strict Clippy, and the built server binary are the
repeatable gates.

The three-pass intent audit was rerun for P4-U12 through P4-U17: each sentence
in the follow-up was mapped to code and tests; each material new mechanism was
mapped back to an observed defect; and an adversarial pass checked that room
closure, chat privacy, action completeness, destructive confirmation, copy
fallback, and canonical/history notation were not silently conflated.

## Tabletop and action-loop follow-up — 2026-08-08

A second ordinary play pass added seven presentation and operability
requirements:

| ID | Observed intent | Implemented consequence |
|---|---|---|
| P4-U18 | The phase/turn banner belongs next to the available actions rather than above the play area. | The DOM and renderer regression now require table → turn banner → action palette order. |
| P4-U19 | Bid actions should read as complete game actions. | Typed action labels render `Bid 0 Tricks`, singular `Bid 1 Trick`, and plural higher bids. |
| P4-U20 | The visible score sheet should follow the rulebook's round/dealer/cards/player layout and notation. | The presentation retains typed dealer and completed round-score rows; the inspector renders active bids, `●` failures, scored cells, rotating dealer, scheduled hand size, and totals. |
| P4-U21 | The play area should look like a top-down projection with the viewer at the bottom, opponent card quantities, dealer-local deck, and an animated actor cue. | The renderer computes a viewer-relative orbit, shows only opaque opponent backs/counts, integrates the authorized hand, locates deck/trump beside the dealer, and labels bidding/playing decisions with reduced-motion support. |
| P4-U22 | Narrow screens should reserve more room for the action surface and avoid requiring horizontal command scrolling. | Below 900px the table height is bounded, command groups wrap, and compact cards/avatars preserve the table projection; a 640×800 browser pass fit every action group without a horizontal action scrollbar. |
| P4-U23 | Adapter result text such as `COMMAND · applied` is diagnostic activity, not HUD-level game information. | The latest client result moved into the collapsed Activity panel while retaining its live status semantics. |
| P4-U24 | Arbitrary chat text must submit successfully from the enhanced browser form. | The delegated client now URL-encodes `FormData` and sends the exact `application/x-www-form-urlencoded;charset=UTF-8` media type required by Axum's `Form` extractor. |

Verification for this follow-up:

```powershell
cargo test --locked -p poche-ui -p poche-web-spike --offline
cargo build --locked -p poche-web-spike --offline
```

The focused tests pass 17/17 and 30/30. A live in-app browser acceptance then:

- created and joined one opaque room from independent named tabs;
- seated/readied both players and reached Running through the real countdown;
- completed a round entirely through presented `Bid … Trick(s)` and `Play …`
  buttons;
- observed the score sheet advance from active bids to `●` / `10`, rotate the
  dealer, and create the two-card round;
- submitted `hello from the browser form` through Room chat and observed the
  attributed message with no content-type denial;
- visually checked the active-player desktop view and a 640×800 layout.

The earlier managed-shell qualification remains part of the historical record,
but it no longer describes the current acceptance boundary: the local server
launched successfully in this pass and the browser exercised the real player
adapter. The workspace-wide suite was also started; its existing
`poche-conformance` binary outlived the shell window and the user interrupted
that run. This is recorded as incomplete, not as a test failure or pass.

The three-pass audit was rerun for P4-U18 through P4-U24. Pass one mapped each
requested layout, wording, sheet, projection, responsive, activity, and chat
detail above. Pass two traced every row to typed presentation data, renderer
markup/CSS, delegated browser code, and executable evidence. Pass three checked
the main failure risks: viewer-relative layout does not alter canonical seat
order; opponent card faces remain private; score rows are built from typed
events rather than parsed labels; active bids are not misrepresented as scored
cells; `Content-Type` matches the body encoding; and visual browser acceptance
is not inflated into a claim that the interrupted workspace conformance run
passed. No active user intent was omitted or silently weakened.

## Explicitly deferred

- durable rooms and accounts;
- public deployment, rate limiting, and production room-code security;
- replacing host-authoritative ordering with the experimental replicated log;
- spectator entry from the player main menu;
- polished animation, sound, settings, and native/web visual parity;
- production multi-device key custody and Veilid/gateway routing.

These remain future work; none is implied by the local player vertical slice.

## Grounded lifecycle and diegetic-projection follow-up — 2026-08-09

| ID | Observed intent | Implemented consequence |
|---|---|---|
| P4-U25 | Bidding must be performable in the play area as well as the complete action palette. | The current bidder receives typed `Bid … Trick(s)` controls in a speech/gesture console between player and table; non-actors see the animated status cue. |
| P4-U26 | Returning to a session after leaving must not expose raw `SeatMap` or 404 failures. | Vacant retained game seats realize safely; unknown sessions get a terminal explanatory page; recoverable exit now preserves membership and a recent-session address. |
| P4-U27 | Formal and Veilid support for leave/rejoin must be stated exactly. | Documentation distinguishes Rust and NuSMV/Prolog durable reconnect evidence, Alloy's current abstraction, destructive leave, process-local `BrowserRooms`, and the separate Veilid/gateway prototypes. |
| P4-U28 | The action palette is complete, but actions should also be grounded in the table world. | Seat, countdown, bid, play, pause/reset clock, chat, score/rules paper, exit, and destructive room controls have semantic HTML table affordances that retain the same opaque command IDs. |
| P4-U29 | Ready must be available in the play area. | The viewer's occupied seat contains Ready/Not ready and Stand up controls in addition to the palette. |
| P4-U30 | Browser-native HTML should be a projection of semantic 3D objects, not a rasterized replacement for them. | `docs/diegetic-semantic-projection.md` fixes canonical semantics → spatial scene → renderer-native projection as the direction of authority. |
| P4-U31 | Responsive layouts need an executable non-intersection constraint. | Visible semantic DOM objects expose named footprints; runtime AABB auditing publishes exact collision count/pairs and browser acceptance exercises representative viewports. |
| P4-U32 | Shared cursors and speech/chat/voice/gesture may later communicate player intent. | The design note separates ephemeral presence/communication from canonical game history while retaining a path to optional replay evidence. |
| P4-U33 | A spoken/gestured bid and the scorekeeper's written bid are distinct, challengeable acts. | The design note preserves this future two-step proposal/acknowledgement model; the present typed bid remains an explicitly simplified atomic action. |

The three-pass audit for this follow-up checked the user's concrete failures,
the requested affordances, and the longer-term grounding discussion separately.
It then traced each implemented claim to reducer, projection, DOM, browser, or
documentation evidence and checked the reverse direction for accidental new
authority. Finally it adversarially checked the likely omissions: exit is not
leave; disconnected sessions do not retain chat/exit controls; restarting the
memory-only server is not called reconnect; a vacant seat does not disclose a
departed hand; DOM collision checks are not inflated into arbitrary CSS proof;
and future cursors, voice, gestures, scorekeeper acknowledgement, and challenges
remain recorded requirements rather than falsely completed features.

Verification on 2026-08-09 passed 26 `poche-session`, 18 `poche-ui`, and 32
`poche-web-spike` tests plus strict Clippy for session/UI/web/xtask. Independent
Alloy (13 bounded results), NuSMV (16 named properties), and Prolog (all seven
pinned answer sets) session gates passed. A real two-tab browser flow used the
diegetic seat, Ready, countdown, and bid controls, then checked actual named DOM
footprints in Bidding and both exact-recipient Playing views at 1280×720,
1280×650, 900×700, 640×800, and 390×844: all 15 phase/view/viewport samples
reported zero unexpected intersections. Recoverable exit exposed a recent-table
link; resume showed one Reconnect and no Chat/Exit controls. An unknown session
rendered the explanatory terminal page with no browser-console error.
