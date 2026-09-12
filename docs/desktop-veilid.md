# Play Poche in two native windows

The default `poche.exe` experience is the Bevy 0.19.1 desktop client. Its room
connection is native Veilid: the renderer sends signed typed actions through
the same device client used by the deterministic harness, while game rules and
private projections remain outside Bevy ECS.

## Build and open two players

From the repository root:

```powershell
cargo build --locked --offline -p poche-cli
target\debug\poche.exe
```

Run `target\debug\poche.exe` a second time for the other player. The two
processes must use different names.

1. In the first window, enter a name and choose **Create lobby**.
2. Choose **Copy lobby invitation** at the table.
3. In the second window, enter a different name and choose **Paste invitation
   from clipboard**. The editor is filled only when the clipboard contains a
   syntactically valid Poche invitation; pasting never joins automatically.
4. Review the invitation and choose **Join**.
5. Take different seats, mark both players ready, and arm the countdown from
   the creator's action bar.
6. Bid and play with the advertised action buttons. A hand card can also be
   dragged from the lower hand viewport into the table. Hold Shift, Control or
   Alt while dragging to adjust yaw, pitch or roll.

A card pose and its rules location are deliberately different. An authorized
drag is shared with the other process without changing the game revision. A
legal drop can then play the card. An out-of-turn drop leaves the accepted pose
where the player put it, keeps the card logically in that player's hand,
reveals no face to the other player, and reports **Not played**. It is never
silently replayed when the turn changes.

The invitation is an admission bearer secret, not a player identity or a
recovery key. Avoid posting it in logs or screenshots while the room is live.
The entered name selects a local protected profile; the cryptographic root and
device keys—not the spelling of the name—authorize later actions.

## Recovery and room lifetime

- If a participant process exits or crashes while the creator remains, relaunch
  it, enter the exact same local name, paste the original invitation and choose
  **Join**. Its protected identity reconnects to the same member and restores
  only its entitled private state.
- If the creator crashes while another member remains, relaunch it with the
  exact same name and choose **Create lobby**. Poche validates the encrypted
  checkpoint, obtains a fresh signed response from a surviving member, rotates
  the private route, and resumes the same room. The remaining player cannot
  finalize a two-player quorum alone during the outage.
- Choosing **Leave and disband room** as the final member closes the room. If
  every process disappears unexpectedly, the creator checkpoint has a
  60-second survivor grace period and is then terminalized when recovery is
  attempted without a surviving peer. A later Create produces a fresh room and
  invitation; old protected data cannot resurrect the expired room.
- A connected spectator may leave and later use the still-valid invitation to
  return. A seated player cannot abandon an active round, because doing so
  would strand the fixed game seat; the existing dropout/governance path is the
  recovery mechanism for that case.

The recovery material is encrypted at rest and deliberately has no export or
human-chosen recovery-code path in this phase.

## Windows firewall prompt

Veilid runs as the ordinary desktop user; Poche does not require an
Administrator token. Windows may ask whether this particular executable may
accept inbound traffic on the active network profile. Accepting that prompt is
reasonable for the desktop game, but the controlled experiment did not show it
to be either necessary or sufficient: two of three runs passed with an inbound
block and two of three passed with an inbound allow. Public-peer conditions
were not controlled across the small sample. See
[Native Veilid acceptance](veilid-native-acceptance.md) for the exact evidence
and limits.

Each rebuilt executable path can be treated separately by Windows Firewall, so
development builds may prompt again. This is executable-specific network
permission, not a request to disable the firewall, reconfigure a router, or run
the game elevated. Running a development command outside the Codex filesystem
sandbox is likewise not Windows elevation: Poche never invokes UAC or requires
an Administrator token.

## Incremental live control for development

The opt-in `dev-control` feature lets a developer or Codex explore retained
Bevy instances one action at a time. It is deliberately different from the
semantic puppet scenarios: there is no predeclared action list. Each instance
watches one fresh explicit directory for bounded typed JSON, drives measured
Bevy pointer or keyboard input, and returns a correlated observation or GPU
capture. Veilid still carries every room/game action between processes.

This surface never reads or changes the OS clipboard, never executes command
strings, never writes `EditableText` directly, and is not a player device. The
ordinary Copy/Paste buttons remain available to humans in non-automated play.
Developer observations are private local artifacts; only an explicit
`--include-invitation` request returns the room bearer secret.

Build the opt-in executable, then start two instances in separate terminals.
Omit `--dev-control-windowless` when you want to see and interact with the
ordinary game windows:

```powershell
cargo build --locked --offline -p poche-cli --features dev-control

target\debug\poche.exe desktop `
  --dev-control-root target\live-poche\alice `
  --dev-control-instance alice `
  --dev-control-windowless

target\debug\poche.exe desktop `
  --dev-control-root target\live-poche\bob `
  --dev-control-instance bob `
  --dev-control-windowless
```

From other terminals, make incremental decisions. Roots must be fresh for each
run; they contain private evidence and already live under ignored `target/`:

```powershell
$poche = 'target\debug\poche.exe'
$alice = 'target\live-poche\alice'
$bob = 'target\live-poche\bob'

& $poche --output json puppet live type $alice name Alice
& $poche --output json puppet live click $alice create
$created = & $poche --output json puppet live observe $alice --include-invitation |
  ConvertFrom-Json
$invitation = $created.observation.room_invitation

& $poche --output json puppet live type $bob name Bob
& $poche --output json puppet live type $bob invitation $invitation
& $poche --output json puppet live click $bob join

& $poche --output json puppet live observe $alice
& $poche --output json puppet live observe $bob
& $poche --output json puppet live click $alice seat:0
& $poche --output json puppet live click $bob seat:1
& $poche --output json puppet live capture $alice
& $poche --output json puppet live capture $bob
& $poche --output json puppet live stop $alice
& $poche --output json puppet live stop $bob
```

For dynamic action-bar controls, read `available_actions` from Observe and use
`action:ID`, for example `puppet live click $alice action:room-ready`. A click
response means the pointer gesture reached the presented control; a later
observation distinguishes an accepted authority revision from a pending,
denied, or stale command. That distinction intentionally revealed concurrent
seat requests during acceptance instead of silently replaying one.

## What has been accepted

The retained September 12 acceptance used two independent protected processes,
two continuously alive windowless Bevy renderers and public Veilid. It drove
the graphical menu, seating, readiness, deal, bidding, an out-of-turn
three-axis drag, a legal drag, action-button play, trick scoring and participant
restart. It passed in 214.56 seconds. Separate current-source probes passed
creator crash recovery, all-peer expiry, implicit empty-room replacement,
explicit final departure and genuine live private-route retirement.

Those are empirical system tests, not a proof of Internet availability,
Veilid's cryptography, polished art, or peer-elected Byzantine consensus.
Ordinary tests and reinforcement-learning rollouts remain network-free.
