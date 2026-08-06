# Semantic HTML tabletop

Phase 8.2 projects the same engine-neutral `SpatialScene` used by the native
Bevy leaf into ordinary HTML. The HTML is not a rasterized camera view and does
not receive transforms as layout instructions. It derives semantic regions for
the deck, trump/current trick, exact-recipient hands, score sheet, public
history, chat, audit findings, proposals, and votes; the browser then performs
its normal responsive layout.

Run the local lab and open `/tabletop/alice`, `/tabletop/bob`, or
`/tabletop/spectator`:

```pwsh
cargo run --locked -p poche-web-spike --offline
```

Every command is a native button in an ordinary `POST` form. A card button is
also draggable, but drag/drop merely submits the opaque control ID retained by
the server; disabling JavaScript leaves click and keyboard activation intact.
The adapter resolves that ID to `CommandPayload`, and the ordinary session,
game, audit, or governance reducer decides whether it applies. Neither HTML nor
CSS position is game authority.

At the bottom of each page, a server-derived SVG highlights the independent
room, viewer-transport, and game-phase states. A read-only copy-safe diagnostic
summary carries the authority incarnation/revision, public state, available
commands, public history, and viewer-local events while omitting join codes,
chat contents, and private card faces. See
[runtime diagnostics and formal workers](runtime-diagnostics-and-formal-workers.md)
for the evidence-strength and capable-peer boundary.

## Shared scene evidence

`poche_ui::embedded_spatial_fixture` selects the checked exact-recipient replay
checkpoint once for both native and HTML adapters. `poche_spatial` hashes its
validated canonical field encoding with the domain
`poche-spatial-scene-hash-v1`. Both adapters report:

```text
0a6de46fd21791260bb57c8516ce9ef5e1666e03e17d29cf6f8cda746c07baee
```

The hash is over authorized scene semantics, not pixels, HTML, Bevy entities,
GPU buffers, or animation frames. Different recipients can and should have
different hashes when their authorized face knowledge differs.

## Real-browser acceptance

The 2026-08-06 acceptance used a fresh local Axum process and Microsoft Edge.
It inspected the accessibility tree and exercised:

- a legal card play and resulting public history/trick;
- pause as Bob and unpause as Alice;
- room chat;
- spectator request, grant, exact Alice-hand visibility, revoke, and removal;
- a confirmed retrospective follow-suit accusation;
- a typed redeal proposal and visible counted vote;
- transport loss, a reconnect-only mutation surface, and restored agency; and
- keyboard `Enter` activation of a command.

The first pass caught a real integration defect: disconnected players still
saw audit/governance forms, and the governance overlay had not received the
connection change. The renderer now requires a connected seated viewer before
showing those forms, while `TabletopLab` synchronizes disconnect/reconnect into
`GovernanceState`. Unit tests and the second browser pass verify the correction.
The machine-readable receipt is
[`evidence/semantic-html-acceptance.json`](evidence/semantic-html-acceptance.json).

## Scope

This is an inspectable loopback vertical slice. Viewer names in development
URLs are not production authentication. The Axum process remains the trusted
host authority for the live game and sees exact recipient projections; the
separate signed browser-device gateway experiment documents its own custody and
transport boundary. The page demonstrates semantic/adaptive presentation,
typed authorization, and recipient privacy behavior, not anonymity, Byzantine
consensus, trustless dealing, or a production deployment.
