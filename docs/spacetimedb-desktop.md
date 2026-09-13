# SpacetimeDB desktop table

The default Poche desktop path is now a Bevy 0.19.1 client connected directly
to SpacetimeDB 2.10.0. Two copies of the same `poche.exe` can create and join a
lobby, occupy distinct seats, receive distinct private five-card hands, and
move and rotate cards with local prediction and subscription-driven peer
updates.

This is the physical-table multiplayer slice, not a claim that the complete
Poche rules have been ported into SpacetimeDB. The existing transport-free
Rust rules and Alloy/NuSMV/Prolog evidence remain the authority for Poche game
behavior. The new module currently owns room membership, two seats, recipient-
scoped sample hands, and face-free physical poses. A future durable play/drop
reducer must call the pure Rust transition boundary; moving a card presently
does not change its logical `hand` location.

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

In the first window, enter a name and choose **Create lobby**, then copy the
opaque `PCH-…` code. In the second window, enter a different name, paste the
code, and choose **Join lobby**. Take different seats. Each player sees only
their own card faces; the peer sees opaque `P` card backs. Drag a card in one
window and use Q/E while dragging to rotate it. The initiating window predicts
the pose immediately, while the other window interpolates toward updates from
the server subscription.

The local profile token is persisted by the official SDK credential helper.
Reusing a profile name on the same machine reconnects as the same SpacetimeDB
identity. Display names are not authentication secrets. Explicit **Leave
lobby** removes membership; a process crash or disconnect does not.

## Reproducible windowless acceptance

The acceptance puppet launches two copies of the ordinary game binary with
Winit disabled and a GPU image target. It drives player actions through fresh,
per-instance file endpoints, never the OS clipboard or a privileged state
mutation API:

```powershell
target\debug\poche-puppet.exe acceptance
```

The command creates a room, joins Bob, seats both devices, verifies five
private cards per player and ten face-free shared poses, moves Alice's first
card, waits until Bob observes the exact position and rotation, and writes:

- `target/poche-puppet/acceptance-contact-sheet.png` — seated and moved views
  for Alice and Bob in one image;
- `target/poche-puppet/acceptance-contact-sheet.json` — identities, counts,
  exact moved pose, authority time, peer-observation time, and capture paths;
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
target\debug\poche-puppet.exe move target\live\alice 0 650 160 -250 18000
target\debug\poche-puppet.exe capture target\live\alice
target\debug\poche-puppet.exe stop target\live\alice
```

Requests are atomically claimed from `requests/`, archived to `processed/`,
and answered in `responses/`. Every response includes a semantic observation
of the exact device. Join codes are omitted unless explicitly requested, and
the endpoint exposes only the controlling player's private hand.

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
