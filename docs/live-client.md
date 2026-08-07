# Deterministic live client

Task 6.2 attaches the renderer-neutral `poche-ui::LiveClientPresentation` to a
real two-seat `InProcessAuthority<OracleSessionGame<2>>`. The executable
adapter is `poche-web-spike`: one self-hosted Axum process owns the room
authority and serves a semantic HTML/Datastar client. This is the topology
selected by ADR 0004. It is deliberately not represented as peer-to-peer or as
a service hosted by GitHub Pages.

## Semantic boundary

The authority remains the only component that changes game/session semantics:

1. the exact-recipient `ProjectionPayload` and public supplements reduce to a
   pure `PresentationModel`;
2. `LiveClientPresentation` derives phase-appropriate controls whose retained
   values are typed `CommandPayload`s;
3. semantic HTML contains only an opaque control ID and endpoint path;
4. the server resolves that ID against a fresh presentation and submits the
   retained typed command through canonical NDJSON loopback ingress;
5. policy/reducer results produce new exact-recipient projections.

The HTML never serializes `CommandPayload`, `InviteProof`, or an invitation
secret into a command endpoint. The deliberately visible demo join code is
display data for the current candidate/coordinator. Invitation values never enter the
canonical projection transcript. A stale or invented control ID fails closed.
Removing a button is not an authorization mechanism: the focused test submits
a typed spectator `Pause` directly past the UI and the authority returns
`D-NOT-SEATED` without changing the running phase.

Native egui and semantic HTML consume the same presentation object. Neither
renderer reads authoritative hidden state. The shared projection widgets cover
members, readiness, countdown, public table, pot/scores, exact authorized
hands, legal actions, history, chat, connection state, and policy notices. The
live shell adds identity, room/join codes, create/join/seat/ready/countdown,
pause/resume, game actions, chat, hand request/grant/deny/revoke,
leave/close/reset, reconnect, transcript export, and replay controls.

## Deterministic demonstration

Run the checked release server from the repository root:

```powershell
cargo run --release -p poche-web-spike
```

Then open `http://127.0.0.1:4174/`, which is Alice's ordinary peer client. Bob
is at `/client/bob`, and the spectator is at `/client/spectator`. Alice or Bob
may select `Create room`; the first accepted creator becomes the room's current
lifecycle coordinator and may also take a player seat. The other peer then
receives its own invite and `Join this room` command through the live stream.
Each client path is durable, so the browser URL and rendered identity agree.
Every displayed demo invite has its own copy button and clipboard status
message. No `/client/host` identity exists.

`Coordinator` is an explicit compatibility-mode capability, not a magic demo
device and not a claim that replicated consensus is already deployed. The
current self-hosted process still orders authoritative state. The creator may
start/reset/close the room until those lifecycle powers are migrated to the
replicated voting model.

`POCHE_WEB_SPIKE_ADDR` changes the listen address. The scenario buttons are an
explicit development harness. They reset the local authority and prepare
pending, lobby, countdown, or running state through ordinary typed commands.
Chance and settlement are explicit environment commands. The authority clock
is logical rather than wall time. While the executable is running, a one-second
adapter timer advances an active countdown by one logical tick. Reducer tests
can still advance the same clock directly without waiting for wall time.

Each client opens a long-lived Datastar SSE request. An accepted command,
denied stale control, scenario change, disconnect, or countdown tick publishes
a fresh exact-recipient patch to every open client. The server renders each
patch independently for that viewer. It does not broadcast one player's
private projection to other clients. Reconnecting the SSE request starts with
the latest projection, so a missed event cannot leave the tab stale.

If a tab submits a control that became stale before the update arrived, the
adapter rejects it before constructing a typed command. It then refreshes the
client and shows a `STALE-CONTROL` status. Authority denials use the same
visible client-event surface.

The three interactive identities are Alice, Bob, and spectator. Static one-use
demo codes are intentionally unsuitable for deployment. Before a room exists,
peers see only `Create room`, not a meaningless join command or code. After
creation, the coordinator sees codes for the other identities and each
nonmember sees only its own join code. A real room must issue fresh protected
invitations through the rendezvous flow described in `veilid-rendezvous.md`.

The transcript endpoint exports canonical NDJSON projection/error frames for
one exact recipient. It intentionally omits commands, invitation proofs, full
authority snapshots, chat text, and other viewers' projections. The replay
endpoint renders only that recipient's retained projection history. Revocation
stops future hand delivery; a transcript checkpoint captured while a viewer
was authorized remains historical knowledge and is not claimed to be erased.

## Task 6.2/6.3 verification

On 2026-08-05, Rust 1.96 ran:

```powershell
cargo test -p poche-web-spike -p poche-ui -p poche-runtime
cargo clippy -p poche-web-spike -p poche-ui -p poche-runtime --all-targets -- -D warnings
cargo build --release -p poche-web-spike
```

The focused run passed 18 runtime, 11 UI, and 8 web-spike unit/integration tests,
plus the runtime integration and documentation tests. The web tests cover
repeatable presentation reduction, opaque typed-control resolution, direct
authority denial independent of control visibility, logical countdown abort,
different-player pause/resume, attributed chat, reconnect, exact projection
histories, and the ungranted/granted/revoked spectator sequence. At this point
the release server was 3,165,184 bytes and its observed peak working set during
the browser exercise was 8,003,584 bytes; these are environment-specific
measurements, not performance guarantees.

The in-app browser (whose version is not exposed by the test surface) then
exercised the release server as ordinary semantic HTML:

- pending Alice and Bob each rendered only `Create room`;
- deterministic running setup rendered a dynamic coordinator, two seated players, one
  spectator, public pot/scores/trump, and exact private player hands;
- the ungranted spectator DOM contained no own or granted hand;
- spectator request -> Alice grant added only Alice's `3C` to the spectator
  DOM; Alice revoke removed it from the next and later spectator views;
- logical countdown showed tick `0`, deadline `3`, and three ticks remaining;
  Alice aborted it back to lobby;
- Alice paused, Bob resumed, and Bob submitted legal typed `bid 0`, changing
  the public actor and bid state;
- spectator chat rendered as attributed `spectator: hello from spectator`;
- forced spectator transport loss rendered the durable membership as
  disconnected and offered reconnect; the typed reconnect restored connected
  delivery;
- the exact-recipient replay link rendered the spectator's ordered checkpoints;
  transcript HTTP response was `application/x-ndjson`, downloadable, 13 lines
  in that run, and contained no `POCHE-LAB` invitation value or secret field.

Automation waits and snapshot work are included in observed browser timings,
so they are not reported as transport or input-to-photon latency.

The privacy test decodes every spectator projection frame before a grant and
requires both private-hand fields to be empty. After revoke it decodes every
new frame and applies the same requirement. It separately confirms that the
authorized interval contains exactly Alice's projected cards. Thus rendered
text, deterministic client state, server-side projection history, and network
payload access are checked at the same exact-recipient boundary.
