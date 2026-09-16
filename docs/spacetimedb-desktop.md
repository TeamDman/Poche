# SpacetimeDB desktop table

The default Poche desktop path is now a Bevy 0.19.1 client connected directly
to SpacetimeDB 2.10.0. Two copies of the same `poche.exe` can create and join a
lobby, occupy distinct seats, receive distinct private rule-generated hands,
bid, play a complete first-round trick, and move/rotate cards with local
prediction and subscription-driven peer updates.

The current SpacetimeDB window draws those three-axis poses into a real Bevy 3D
scene: a perspective camera, the canonical `poche-spatial` table/seat/zone
geometry, dimensional cards,
avatars, shadows, and renderer-neutral filled Slug card labels. Network
millimetres and millidegrees cross into metres and quaternions only at this
rendering boundary. The table fills the window. Private cards peek from the
bottom edge through a transparent hand camera. These are presentation copies
of the same shared poses, not extra game objects. The hand view stays anchored
to the player's physical hand zone as the world camera moves.

Room information lives on world signs and a score sheet. Click notices to
read them close up. Click the room-code sign to copy its code. The compact
Speech control and rotation snap remain on screen; Escape opens other controls.

This remains a first-round vertical slice rather than the complete Poche game.
The module persists a deterministic shuffle seed and ordered typed action log;
each bid/play reducer reconstructs that log through the transport-free
`OracleEnvironment<2>` and commits only an accepted pure transition. Physical
poses remain independent latest-value rows. Dragging into PLAY proposes the
typed play, reveals the accepted card, and changes its logical location;
out-of-turn drops remain physical moves without changing logical state or
revealing the face. The existing Alloy/NuSMV/Prolog evidence checks the pure rules boundary,
not SpacetimeDB or Bevy execution.

## Choose Maincloud or local development

The same binary supports named authority profiles. Maincloud is convenient for
ordinary cross-device development and exercises the hosted topology players will
eventually use:

```powershell
target\debug\poche.exe --server maincloud
```

That shorthand selects `https://maincloud.spacetimedb.com` and the development
database `poche-6quz6`. A different hosted database can be selected explicitly:

```powershell
target\debug\poche.exe --server maincloud --database poche-staging-1
```

The same selection can be sent explicitly to the windowless two-device
acceptance harness:

```powershell
target\debug\poche-puppet.exe acceptance --server maincloud
```

Its JSON report records the resolved profile, URI, and database with the
latency evidence.

Publish module changes from the repository root with the existing Rust module;
no web or React template is involved:

```powershell
$spacetime = 'C:\Users\Teamy\AppData\Local\SpacetimeDB\spacetime.exe'
& $spacetime login show
wasm-opt --version
& $spacetime publish poche-6quz6 `
    --module-path crates\poche-spacetimedb-module `
    --server maincloud `
    --yes
```

Binaryen 131 is the currently verified optimizer. Binaryen 132 enables compact
imports through SpacetimeDB's `-all` invocation, which SpacetimeDB 2.10 cannot
load; track [SpacetimeDB issue #5828](https://github.com/clockworklabs/SpacetimeDB/issues/5828)
before upgrading it again. Do not add `--delete-data` to the ordinary update
workflow.

Local remains the safe default for offline work, repeatable automation, and
latency comparisons:

```powershell
target\debug\poche.exe --server local
```

An explicit HTTP(S) URL is also accepted by `--server`. The existing
`POCHE_SPACETIMEDB_URI` and `POCHE_SPACETIMEDB_DATABASE` environment variables
remain supported when no corresponding command-line option is supplied.

## Graphics backend on Windows

Poche defaults to DX12 on Windows. Bevy 0.19.1/wgpu 29 can otherwise select
Vulkan on NVIDIA and emit swapchain validation failures during startup about a
presented image remaining in `VK_IMAGE_LAYOUT_UNDEFINED`, followed by already-
signaled acquire semaphores. This has minimal upstream reproductions in
[Bevy #22733](https://github.com/bevyengine/bevy/issues/22733) and
[wgpu #9213](https://github.com/gfx-rs/wgpu/issues/9213); it is not caused by a
Poche reducer or card pose.

The backend remains explicit and reversible:

```powershell
target\debug\poche.exe --graphics-backend dx12
target\debug\poche.exe --graphics-backend vulkan
target\debug\poche.exe --graphics-backend auto
```

Use `vulkan` when testing a future upstream fix. `auto` restores Bevy/wgpu
selection (including `WGPU_BACKEND` handling). Windowless captures do not own a
presentation swapchain and therefore cannot by themselves prove this startup
problem fixed.

## Run a local table

The checked implementation pins its CLI, module bindings, and Rust SDK to
SpacetimeDB 2.10.0. The commands below deliberately use the absolute path of
the user's installed CLI so a restricted shell's `PATH` cannot select a
different version.

From `D:\Repos\Games\poche-4`, open three PowerShell terminals.

Terminal 1 starts a disposable local authority:

```powershell
$spacetime = 'C:\Users\Teamy\AppData\Local\SpacetimeDB\spacetime.exe'
& $spacetime --version
& $spacetime start --listen-addr 127.0.0.1:3000 --in-memory --non-interactive
```

Terminal 2 builds and publishes the module, then builds the two desktop tools:

```powershell
$spacetime = 'C:\Users\Teamy\AppData\Local\SpacetimeDB\spacetime.exe'
& $spacetime build --module-path crates\poche-spacetimedb-module --debug
& $spacetime publish poche-desktop-v1 --module-path crates\poche-spacetimedb-module --server local --anonymous --yes
cargo build --locked -p poche-spacetimedb-desktop --bins
```

An anonymous identity can create the database on a fresh in-memory server but
cannot update a database owned by a different anonymous invocation. During
this disposable phase, restart Terminal 1 before republishing an updated
module. A persistent development or hosted deployment should publish from a
saved owner identity instead.

Terminal 3 launches two independent player processes:

```powershell
Start-Process -FilePath .\target\debug\poche.exe
Start-Process -FilePath .\target\debug\poche.exe
```

Each window begins at **Who is playing?** before it opens a network
connection. Create or select Alice in the first window and Bob in the second.
If another window just created an account, use **Refresh identities**. The
title shows the selected account; its arrows switch accounts, and clicking
the account name opens the identity screen.

As Alice, choose **Create lobby** and copy the opaque `PCH-…` code. As Bob,
paste the code and choose **Join lobby**. Click different stools to sit. The round-one oracle
deal gives each player one private card while the peer sees an opaque `P` back.
Open Speech and select “I bid 0 tricks” or “I bid 1 trick” when it is your turn.
The dealer prompts the current bidder; accepted bids appear above the players.
During play, drag
your card from the private-hand inset into the highlighted central PLAY zone.
An accepted play reveals the face to both clients; after the second play both
cards move to the winner's logical won zone. Wiggling within the hand remains
only physical state. Hold Q or E while dragging to rotate around table-up Y.
The **Rotation snap** button cycles through off, 15°, 30°, 45°, 60°, and 90°.
Hover a card for an outline; grabbing lifts it. Drag from the bottom hand into
the world and back. A thin hand-drop indicator appears while dragging. Cards
outside that physical hand region leave the inset, but remain logically yours
until the authority accepts a play. Large hands compress card spacing without
shrinking the faces. Opposite rank/suit corners keep turned cards readable.
Hold Z to inspect zone volumes. These diagnostic shapes do not cast shadows.
Right-drag orbits around the camera's focal point; middle-drag and WASD pan that
point across a region twice the table-top extents. Space smoothly returns both
camera and focus to the viewer's seat-relative home. All camera changes
interpolate instead of teleporting. Local card motion is immediate while the
peer interpolates subscribed updates.

Arrow keys also orbit the camera. Perspective pitch can approach 3° above the
table for gap inspection. O retains the tactical orthographic view; Space
returns to the wider home view.

Create, join, and rejoin first show **Preparing the table**. Poche keeps that
loading surface until the authoritative room, this member, the private hand,
and the corresponding shared card poses form one coherent projection. While
that opaque loading surface remains visible, Poche creates the table and hand
cameras and reconciles the subscribed card/player entities behind it. The
table HUD is revealed only after the camera exists, rendered entity counts
match the projection, and Bevy's render world confirms that the table camera's
opaque 3D phase has compiled pipelines and completed a render-graph pass. The
confirmation carries a room-scene generation, so a completed frame from an
older lobby cannot unlock a newly joined one. This prevents a semantic-ready
but visually black table from appearing between the loading screen and the
first 3D frame; ordinary main-world update counts are not treated as rendering
evidence.

RMB vertical orbit is inverted by default, the opposite of the original
prototype response. Open **Escape → Options** and activate **Invert camera Y:
On/Off** to switch between the two signs. The setting is local to that running
game window and does not alter shared table state. Escape from Options returns
to the table menu; a second Escape resumes the table.

The player and Activity signs show the authoritative membership and newest public
room actions: create/join/leave, seat changes, deal start, bids, and played
cards. It never records an unplayed card face or a private-hand snapshot.
Only seated members have world avatars; unseated members remain visible in the
roster without appearing in the middle of the table. The table notepad shows
the current round in rulebook notation. Recorded totals remain authoritative;
this slice still stops at scoring, so the sheet labels pending scores explicitly.
Escape opens the table
menu. **Stand up** and **Options** live there rather than in the frequent action
bar. **Leave lobby** changes to **Confirm leave lobby** after the first click.
Successful leave clears the active-room
projection and shows **You have left the lobby** with an explicit **Return to
title** action while remaining peers see the roster update.

The replicated integer angle unit is one millidegree: one full turn is
`360_000`, so `18_000` is 18° and `180_000` is 180°. Radians are only a Bevy
trigonometry/quaternion boundary. A binary-turn newtype (for example, 65,536
ticks per turn) is a promising later schema because quarter/eighth turns are
exact and wrap naturally, but changing the database before the 3D orientation
contract is settled would create churn without changing the current renderer's
precision in a meaningful way.

The official SDK credential helper persists the actual SpacetimeDB token. A
separate non-secret catalogue defaults to
`%LOCALAPPDATA%\Poche\identity-vault-v1.json`; it contains immutable random
account IDs, labels, authority/database scope, and the observed public
principal, but never tokens. `--identity-vault PATH` selects an explicit
catalogue for testing. Display names are presentation, not credential keys.
Selecting the same account in another process therefore reconnects as the
same SpacetimeDB identity, seat, and private hand.

Each account also retains up to eight recently used lobby capabilities in this
local catalogue, newest first. Returning to the title after an explicit leave
shows **Recent lobbies** with direct rejoin actions. The list is per account and
authority. Its join codes are bearer secrets, so the catalogue remains local
and must not be attached to bug reports.

After account selection, the initial sender-scoped subscription is recovery
authority. If it contains an active room, Poche shows **Unfinished lobby
found** and waits for **Rejoin lobby** rather than silently opening the table.
The private room-secret table is never subscribed directly; a sender-scoped
`my_room_capability` view returns only the active room's join code to an
authenticated member. A resumed device can therefore display and copy the
code again without exposing other rooms' bearer capabilities.
Switching accounts disconnects that process but does not leave. Explicit
**Leave lobby** removes membership and active-room focus; a process crash or
disconnect does not. If a seated member explicitly leaves or stands during an
active deal, the authority abandons that incomplete deal and clears its cards
and action log. A later code-based join returns unseated to a coherent lobby;
occupying both seats starts a fresh deal. This avoids inheriting a departed
player's private hand or leaving the actor pointed at an empty seat. Server
presence is keyed by SDK connection ID, so a member remains online while any
process using that identity is connected.

When a resumed identity accepts **Rejoin lobby**, roster avatars and cards are
reconciled from the already-loaded subscription snapshot immediately; no bid or
other mutation is needed to make the scene appear. If a public game projection
says this player still owns cards but its sender-scoped hand rows have not yet
arrived, the HUD says that the private hand is synchronizing and suppresses bid
actions instead of claiming the round is complete.

## Logs and crash evidence

An ordinary windowed run writes a durable log to
`%LOCALAPPDATA%\Poche\logs\poche-<timestamp>-<process-id>.log`. Use
`--log-file FILE_OR_EXISTING_DIRECTORY` to select an explicit file or an
existing directory. Terminal output remains enabled. If Poche panics while
attached to an interactive terminal, it prints the log path and waits for
Enter so the terminal does not disappear before the error can be read;
redirected automation never waits.

The reported leave-to-title resize crash was an ownership problem at the DX12
surface boundary: inactive table and hand cameras remained bound to the window
after the table UI had gone away. Spatial cameras now exist only for the table
screen. A visible regression run created a Maincloud lobby, opened Escape,
confirmed leave, returned to the title, maximized the window, and successfully
observed the still-running process through file control. The failing reduced
run logged `ResizeBuffers ... window is in use`; the passing run produced no
render error.

A separate untouched-title minimize failure had the same DX12 symptom but a
different trigger. Windows reports a minimized winit client area as `0x0`;
reconfiguring the live swapchain for that transient extent failed with
`ResizeBuffers ... window is in use`. Poche now retains the last non-zero
physical extent in the render world while minimized and accepts the next
non-zero resize on restore. This does not resize or unminimize the OS window.
The file-control schema exposes `minimize` and `unminimize` so this exact
window-state boundary remains reproducible.

## Reproducible windowless acceptance

The acceptance puppet launches copies of the ordinary game binary with
Winit disabled and a GPU image target. It drives player actions through fresh,
per-instance file endpoints, never the OS clipboard or a privileged state
mutation API:

```powershell
target\debug\poche-puppet.exe acceptance
```

The command captures the initial identity gate, creates independent Alice and
Bob accounts, captures Alice's account-labelled title, creates a room, joins
Bob, and seats both devices. It verifies one rule-generated private card per
player and two face-free shared poses, moves
Alice's card, waits until Bob observes its exact position/rotation, submits two
legal bids through pointer clicks on the Speech picker and two legal plays,
verifies both devices converge on two revealed
cards in one winner's logical won zone, then starts a simultaneous second
Alice process from Alice's same vault. That process must receive the resume
offer and recover the same principal, seat, private hand, roster, public card
poses, activity history, and exact lobby code. After **Rejoin lobby**, the
acceptance contract also requires rendered avatar/card counts to match that
preloaded model before continuing.
After it disconnects, Bob must still observe Alice online through her original
connection. The puppet then captures the Escape table menu and armed leave
confirmation. It also opens the nested Options menu, captures inverted-Y On,
toggles and captures Off, and returns to the parent menu before explicitly
leaving Alice. It verifies the terminal screen and requires Bob to observe the
public leave event, one remaining member, no stranded game projection, and no
orphaned card poses.

The same run also grabs an inset card using real Bevy pointer input, taps Q,
moves it into the world, and brings it back. It checks the peer's exact angle,
lift/release heights, unchanged ownership, and absence of public face disclosure.
`hand-interaction.png` shows hover, rotation, world drag, return and low-angle
inspection in one image. The run writes:

- `target/poche-puppet/acceptance-contact-sheet.png` — seated and moved views
  for Alice and Bob in one image;
- `target/poche-puppet/acceptance-contact-sheet.json` — identities, counts,
  exact moved pose, authority time, peer-observation time, multi-device
  presence/resume assertions, and capture paths;
- `target/poche-puppet/identity-flow.png` — identity gate, selected-account
  title, and authoritative resume offer;
- a fresh ignored run directory with request/response transcripts and device
  logs.

The same endpoint supports ad-hoc exploration. Start a game with a fresh root:

```powershell
target\debug\poche.exe --control-root target\live\alice --instance-id alice-window
```

Then use the sibling tool from another shell:

```powershell
target\debug\poche-puppet.exe set-name target\live\alice Alice
target\debug\poche-puppet.exe create target\live\alice
target\debug\poche-puppet.exe observe target\live\alice --include-join-code
target\debug\poche-puppet.exe seat target\live\alice 0
target\debug\poche-puppet.exe move target\live\alice 0 60 40 500 18000
target\debug\poche-puppet.exe bid target\live\alice 0
target\debug\poche-puppet.exe play target\live\alice 0
target\debug\poche-puppet.exe menu target\live\alice
target\debug\poche-puppet.exe leave target\live\alice
target\debug\poche-puppet.exe leave target\live\alice
target\debug\poche-puppet.exe return-title target\live\alice
target\debug\poche-puppet.exe maximize target\live\alice
target\debug\poche-puppet.exe restore target\live\alice
target\debug\poche-puppet.exe minimize target\live\alice
target\debug\poche-puppet.exe unminimize target\live\alice
target\debug\poche-puppet.exe capture target\live\alice
target\debug\poche-puppet.exe stop target\live\alice
```

`set-name` is retained as an automation-compatible spelling for “create this
identity if absent, then select it”; it no longer makes the name a credential
key. A second process pointed at the same `--identity-vault` can instead use
`select-identity ROOT Alice`, followed by `resume ROOT` when its observation
reports the `resume_offer` surface.

`menu ROOT`, `options ROOT`, `invert-camera-y ROOT`, and `back ROOT` expose the
same nested menu path to ad-hoc windowless control and screenshot capture.

File-control schema 6 also accepts real input. On a `--windowless` instance:

```powershell
target\debug\poche-puppet.exe pointer target\live\alice 590 732 down
target\debug\poche-puppet.exe key target\live\alice Q down
target\debug\poche-puppet.exe key target\live\alice Q up
target\debug\poche-puppet.exe pointer target\live\alice 590 732 up
```

Coordinates are logical pixels within that instance's viewport. These commands
run through normal Bevy input and picking, not a pose reducer shortcut. Pointer
injection is rejected for OS windows so tests cannot move your mouse. The
observation includes the held card key and visible hand-copy count. Screenshots
and semantic observations still work for both windowed and windowless instances.

The final `move` argument is rotation about table-up Y in millidegrees,
matching the Q/E control and the card's visible orientation in the perspective
scene. The two `leave` invocations exercise the same arm-then-confirm path as
clicking the human-facing button twice.

Requests are atomically claimed from `requests/`, archived to `processed/`,
and answered in `responses/`. Every response includes a semantic observation
of the exact device, the public activity stream, and diagnostic counts for
rendered card/avatar entities. Join codes are omitted unless explicitly
requested, and the endpoint exposes only the controlling player's private hand.

## Trust, privacy, and licensing

SpacetimeDB is the trusted authority in this design. It orders transactions,
validates membership and card ownership, and can inspect private rows. Sender-
scoped views keep one player's faces out of another client's subscription,
diagnostics, and capture, but they do not hide those faces from the server
operator. This intentionally trades the Veilid design's decentralization for
a simpler low-latency product path.

Poche-authored code remains MPL-2.0. The inspected SpacetimeDB server source is
Business Source License 1.1 with an additional production-use grant for an
application using no more than one production instance and not offering a
database service; its stated change date is 2031-09-08. The Rust SDK inherits
SpacetimeDB's BSL file, while the module bindings directory identifies
Apache-2.0. This is a project note, not legal advice; re-check the license for
the exact version before distribution or deployment.

The Veilid implementation and its evidence remain in history and explicit
legacy packages. They are not in the default desktop package's dependency
graph.
