# Native canonical spatial mirror

Phase 8.1 adds a Windows-first Bevy 0.19 leaf application that renders the
same exact-recipient `SpatialScene` used by the neutral spatial checks. It is a
renderer and input adapter, not another game engine: the immutable scene keeps
stable object IDs, typed card locations, viewer-authorized faces, and semantic
text attachments. Bevy `Transform` values are derived presentation state.

![Poche native spatial acceptance with bounds overlay](assets/native-spatial-acceptance.png)

The screenshot is the original checked release-window acceptance artifact. Cyan boxes
are exclusive inner snap volumes; orange boxes are outer/dead-band volumes.
The committed `4♣` is in the central play zone. Only three face runs exist for
this exact viewer; 49 card objects remain backs with no face text. Current
windowless captures are generated under ignored `target/` evidence directories
rather than replacing this historical image in Git.

## September desktop input acceptance

The current desktop renderer uses Bevy **0.19.1**. Logical hand/deck/play
locations remain rules-engine state; authorized physical positions and full
rotations are a separate shared overlay. A card moved to PLAY out of turn can
stay there physically, still logically owned and face-hidden from the peer.
Only an accepted play publishes its face.

The hand inset and table are different cameras into the same world. A drag
retains the original world-space grab offset and uses the camera under the
pointer; entering the table viewport positions the same card under that
pointer. The inset is not an adjacent world region, so its accumulated pixel
motion is not carried as a leftover table offset. No logical card is duplicated
or automatically played merely because a new turn begins. Shift/Ctrl/Alt during
dragging change yaw/pitch/roll. A release is queued behind preceding poses if
necessary, retaining its original command ID and rules revision. Later pose-only
receipts do not erase a visible play rejection.

If an invocation returns a receipt but its follow-up snapshot fails, the native
worker retains that receipt while retrying reads, then delivers it with the
fresh projection. It does not resend the command or discard its known result.
A lost invocation ACK remains an unknown outcome: a newer revision alone is
not treated as a receipt. Closing the client also stops a worker waiting only
for confirmation reads. Deterministic tests exercise these boundaries against
the real loopback rules authority; they are not proof of public-network reliability.

Reproduce the **windowless GPU + signed mock Veilid** input test from the repo
root. Use a fresh output directory each time:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:POCHE_RENDERED_EVIDENCE_ROOT = "$PWD/target/my-rendered-play-run"
cargo test --locked -p poche-veilid --features device-service,veilid-mock-test,native-input-test --offline --test device_service rendered_pointer_denied_and_accepted_play -- --exact --ignored --nocapture --test-threads=1
```

This drives normal Bevy picking with pointer press/move/release on real
GPU-computed cameras, including a final move and release in the same frame.
It checks accepted poses and denial/commit results through the signed device
service, then compares an independent player's observation for pose equality
and face privacy. `denied/` and `accepted/` each contain `before.png`, `held.png`
and `after.png`, saved by the existing local screenshot/readback path. These
captures intentionally include the graphical viewer's private hand; treat
them as local developer evidence, not copy-safe/public room diagnostics.

The September 10 final run passed in 48.02s; inspected images live under ignored
`target/phase6-rendered-play-06`. It also asserts a six-degree change on each
rotation axis and waits for the exact final pose receipt, not a fixed frame
delay. Earlier runs exposed camera-offset drift, a
cleanup race, missing suit glyphs in feedback, and pose receipts erasing denial
feedback. Those paths were corrected and retested. This is not OS-input/menu/
clipboard acceptance, two simultaneously rendered public-network processes,
or complete lifecycle acceptance. PLAN-6 retains those remaining obligations.
The fixture launch commands and timings below are historical, not a substitute
for the current desktop lobby entry point.

## Rendered desktop menu checks

The desktop build explicitly enables Bevy's `system_clipboard` feature. Without
it, Copy/Paste use an in-process buffer and cannot exchange invitations between
two independently launched games. Clipboard errors remain visible; enabling the
feature does not guarantee that the OS will grant clipboard access.

Menu and action-bar buttons now use Bevy pointer-click events on both window
and image render targets. Invitation prefill queues normal text edits, preserving
the field configuration and updating rendered glyphs. Replacing the text editor
component had produced a populated-but-invisible invitation in an inspected
capture; the input harness now checks for both the expected value and glyphs.

Run the deterministic menu test with a fresh output directory:

```powershell
$env:WGPU_BACKEND = 'dx12'
cargo run --locked -p poche-native-ui --features input-probe --offline --example menu_input -- target/my-menu-input-run
```

It clicks actual GPU-laid-out controls and supplies keyboard messages through
Bevy's text-input pipeline. It checks blank-name validation, name entry,
unrelated clipboard refusal, invitation prefill without auto-join, explicit
Join, a visible connection failure and a Create retry. It opens no OS window:
Winit is disabled and the only Window entity is a headless keyboard endpoint.
Rendering/picking use the actual image camera, not synthetic UI geometry. The
clipboard is an explicitly isolated test buffer; your clipboard is untouched.
`menu.png`, `join-error.png` and `create-error.png` use the existing screenshot
readback/persistence path. This is local developer input evidence, not OS mouse
or cross-device capture-protocol acceptance.

The corresponding opt-in public-network test uses two independent processes and
the same protected connector as `poche desktop launch`:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST = 'I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
$env:POCHE_MENU_EVIDENCE_ROOT = "$PWD/target/my-public-menu-run"
cargo test --locked -p poche-cli --features native-input-test --offline --lib cli::desktop::connection::process_probe::protected_desktop_rendered_menu_two_process -- --exact --ignored --nocapture --test-threads=1
```

This creates fresh protected test profiles, performs rendered Create/Copy and
Paste/Join, then continues with the existing scripted seat/ready/deal, shared
pose and participant-restart checks. Connection workers remain owned after each
renderer exits so teardown cannot destroy the room under the remaining checks.
Invitations cross processes through temporary test coordination files and are
pasted via isolated buffers: **this is not a test of cross-process OS clipboard
exchange**. Lobby screenshots do not prove rendered seat/ready/card input.
Menu captures may contain bearer invitations; keep the ignored evidence local.

The first September 10 public run reached both rendered lobbies but failed on a
subsequent action timeout (72.72s). Both `creator/lobby.png` and
`joiner/lobby.png` under `target/phase6-public-menu-01` were inspected. The exact
new executable had Public TCP/UDP Allow rules when checked afterward; this was
not another frozen-binary firewall experiment. Later-action failures remain
release blockers, not successful end-to-end acceptance. Consult PLAN-6 T3 for
the remaining OS-clipboard/lifecycle obligations. The diagnostic follow-up
(`target/phase6-public-menu-02`) passed in 74.96s, including the later scripted
deal/pose/participant-restart checks; its joiner lobby capture was inspected.
Only action-stage logging changed between these runs, not transport behavior.
The earlier timeout is therefore still unresolved. The final deterministic
menu input run (`target/phase6-menu-input-03`) also passed with the new glyph
assertion, and its prefilled menu was visually inspected.

The extended rendered-lobby test keeps both renderers running through seat,
Ready and deal. Use the same environment variables above, a fresh evidence
directory, and this test name:

```powershell
cargo test --locked -p poche-cli --features native-input-test --offline --lib cli::desktop::connection::process_probe::protected_desktop_rendered_lobby_two_process -- --exact --ignored --nocapture --test-threads=1
```

`target/phase6-public-lobby-01` passed in 108.69s, including the later scripted
shared-pose and participant-restart checks. Both `dealt.png` captures were
inspected. The driver clicks the GPU-laid-out seat/Ready/countdown buttons,
waits for this device's new committed response, then checks its actual seat,
ready flag and dealt hand. It does not treat another player's revision change
as proof that its own action succeeded. Timeout errors identify the waiting
action without logging player names, invitations or faces. This test still
uses isolated clipboard buffers; it does not yet render the subsequent card
drag/play or prove the whole goal complete.

For safe future OS clipboard testing, [Microsoft documents a clipboard per
window station](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations).
A separate noninteractive station may support real cross-process clipboard
testing without touching the interactive user's clipboard. This is researched,
not implemented: a read-only preflight found non-text formats in the current
clipboard, so a plain-text backup/restore was deliberately not attempted.

The continuous rendered-trick test adds bidding, an out-of-turn three-axis
hand-to-table drag, an accepted drag, and action-palette play of the previously
denied card. Each process keeps its original renderer and live connection from
the menu through scoring and the next deal. Run with the same public-network
opt-in above and a fresh `POCHE_MENU_EVIDENCE_ROOT`:

```powershell
cargo test --locked -p poche-cli --features native-input-test --offline --lib cli::desktop::connection::process_probe::protected_desktop_rendered_trick_two_process -- --exact --ignored --nocapture --test-threads=1
```

The recipient independently checks hidden-card motion without reveal or a
logical revision change; accepted play must publish that card's identity and
pose. Both devices compare their scored public history. Barriers carry expected
public metadata only, never private faces for denied plays or substitute game
commands. The overall test is bounded at eight minutes, each renderer at six,
and each trick stage at 110 seconds. Subsequent pose/restart checks are still
scripted, and invitation clipboard buffers are still isolated. Captures are
private local developer evidence, not a copy-safe diagnostic export. Test
implementation alone is not a passing network acceptance result. The first
public run (`target/phase6-public-trick-01`) failed after 293.02s awaiting the
legal drag's outcome, after peer-verified denied movement succeeded. The cause
is not yet isolated. Follow-up instrumentation checks that release actually
queues the action and records stage timings; this is not a transport fix.
See PLAN-6 T4b for the measured result and remaining evidence gates.

The post-receipt-fix run (`target/phase6-public-trick-03`) passed in 270.42s,
including both rendered clients completing the trick and agreeing on scoring,
then the scripted participant restart. Both next-round captures were inspected.
This does not prove the cause of earlier failures or smooth network behavior:
pose confirmation stages took 35.555s and 44.398s, with read retries still
occurring. Reducing avoidable queued motion is the next performance task;
OS clipboard exchange, visual polish and the other PLAN-6 gates remain open.

## Architecture and controls

- `poche-slug` is the MPL-2.0 extraction of Teamy Terminal's pinned outline,
  directional-band, packed-word, metadata, and independent CPU coverage
  contract. It accepts explicit font bytes and has no terminal, Ash, Bevy, or
  absolute path dependency.
- `poche-native-ui` is the only Bevy-dependent crate. Each entity mirrors a
  canonical `ObjectId`, endpoint `PoseMm`, and, for cards, `CardLocation`.
  Presentation-only drag offsets and tween clocks cannot edit those records.
- Semantic `TextRun` records are children of their exact card/score-sheet
  surface with the registered local z offset. Slug extracts the actual
  Caskaydia Cove outlines, including `♣♦♥♠`, and produces buffer-ready curve,
  band, and metadata packets. Bevy rasterizes the same directional-band
  analytic coverage into transparent antialiased textures, then places those
  filled surfaces on the owning card or score sheet. This is deliberately not
  a claim that the Teamy Vulkan analytic shader was transplanted.
- Hold the middle mouse button and drag to pan across the table. `WASD` moves
  the camera target in its current ground-plane frame, the arrow keys rotate
  yaw and pitch, and `Space` eases the camera back to the registered home view
  over 550 ms. These are presentation-only camera transforms.
- Press `P` for the first owned typed play, drag an owned card onto the play
  volume for the picking path, and press `F3` for the spatial audit overlay.
  Both card-input paths call the same `resolve_card_play`/`resolve_drag_play`
  contract and reconstruct the same 300 ms smooth-step endpoint.

Run the interactive release application with:

```pwsh
cargo run --release -p poche-native-ui -- --debug-overlay
```

Reproduce the bounded acceptance artifact without calling it input-to-photon
latency:

```pwsh
cargo build -p poche-native-ui --release --offline
target\release\poche-native-ui.exe `
  --play-card first `
  --debug-overlay `
  --screenshot target\acceptance\native-spatial.png `
  --acceptance-report target\acceptance\native-spatial.json `
  --exit-after-seconds 6
```

## Verification and measured evidence

The 2026-08-05 release-window run produced the raw checked receipt in
[`evidence/native-spatial-acceptance.json`](evidence/native-spatial-acceptance.json)
and screenshot SHA-256
`ab9e2c4c0e02bb609e4411531a000b90a191c89e7c80e920a795b6386ddcad7d`.
The historical receipt also carries its renderer-independent scene fingerprint
`0a6de46fd21791260bb57c8516ce9ef5e1666e03e17d29cf6f8cda746c07baee`.
The 2026-08-15 score-sheet orientation correction intentionally changes the
current fixture fingerprint to
`d1216416d9fe0fdad1412512a2b5cf273b883843276ec77113931b6e1b4e0576`;
historical receipts remain immutable. See
[semantic-html-tabletop.md](semantic-html-tabletop.md) for the earlier
cross-renderer acceptance.

| Observation | Result |
| --- | ---: |
| Process start to first Bevy update | 716.681 ms |
| Named semantic resolve/commit | 500 ns (0 µs at integer-microsecond precision) |
| Sampled presented frames | 733 |
| Mean sampled frame interval | 7.212 ms |
| p95 sampled frame interval | 7.486 ms |
| Scene objects / cards / semantic text runs | 13 / 52 / 7 |
| Authorized face runs / hidden cards without face runs | 3 / 49 |

Focused native tests cover camera reset interpolation, CLI face parsing, real
suit outlines and packed metadata, Slug all-curves/banded CPU parity, opaque
glyph interiors with antialiased edges, binding-specific surface bounds,
named/drag transition equality, and absence of invented hidden-face text or
canonical scene mutation. The two
extracted Slug source tests, strict focused Clippy, and optimized offline build
also pass.

The 2026-08-15 post-release regression ran
`external-devices-full-game --surface native --seed 95` through the real
windowless Bevy provider. It completed at revision 160 with five certified
devices, 132 steps, 138 public events, and six requester-published PNGs. Image
inspection confirmed centered filled card faces and horizontal, bounded
score-sheet rows; the generated catalog remains under ignored
`target/poche-text-fix-final2`.

The timing is local release-process evidence. The semantic number measures the
resolver-to-accepted-record path for the startup CLI action; frame intervals
measure Bevy presentation sampling. No OS input-injection timestamp or display
instrument was used, so this is deliberately not an input-to-photon claim.

## Scope boundary

This is a faithful projection of a checked replay checkpoint, not a packaged
live Veilid player client. It has no physics authority and cannot create cards,
change scores through glyph placement, or make a loose transform into a legal
Poche transition. A direct analytic GPU Slug shader and further visual polish
remain later work; the current filled surfaces are CPU-rasterized from that
same checked coverage contract. Browser parity
for the checked scene semantics is now covered by the linked phase-8.2 evidence.
