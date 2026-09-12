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
the game elevated.

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
