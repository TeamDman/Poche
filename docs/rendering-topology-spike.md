# Rendering and live-topology spike

This is the resumable evidence log for Task 6.1. A matrix row is complete only
after its executable has run in the named environment. Source inspection is
recorded as a prediction or prerequisite, never as equivalent evidence.

## Reproduction baseline

Recorded on 2026-08-05 on Windows x86-64:

- Rust/Cargo 1.96.0, host `x86_64-pc-windows-msvc`;
- eframe/egui 0.33.3 with Glow, AccessKit, default fonts, and the web screen
  reader enabled;
- Veilid WASM 0.5.7 at local reference commit
  `76b2176926dc24e30f9427540384a04ae22e590c`;
- Datastar Rust 0.3.2 at local reference commit
  `b88ad8adf515300a67b7f0198d2eb63bc78d646b`;
- cursor-latency at `6c077050235a199af3d386d3c52523b8e04fef45`
  and ash at `a9a1fb17e98a0cde146caada86200d809306200d`;
- installed Edge 151.0.4129.59. No browser version is assigned to an unrun
  row. The in-app browser was used to confirm that npm has no published
  `veilid-wasm` package that could substitute for building the pinned source.

## Executable matrix

| Spike | State | Executable evidence | Size/latency | Accessibility and deployment | Exposure and exact limitation |
| --- | --- | --- | --- | --- | --- |
| Native egui projection replay | Pass | `cargo test -p poche-ui --offline`; `cargo build -p poche-ui --bin poche-replay --release --offline`; launching `target/release/poche-replay.exe` produced a live top-level window titled `Poche projection replay` with nonzero handle `3279528`. The process was then closed by the probe. | 5,590,016-byte release executable. A second probe reached a nonzero window handle in 499.831 ms and reported 20,885,504 bytes peak working set. This is process/window startup, not input-to-photon latency. | One Rust widget path, keyboard-focusable controls, AccessKit enabled. Windows build uses eframe Glow; no Vulkan API is required. Native artifact distribution is required. | Consumes only exact-recipient `ProjectionPayload`; there is no transport or additional operator in this static replay. |
| Pages-hostable egui/WASM replay | Source ready; execution open | `poche-ui` exports an eframe `WebHandle`, embeds the checked projection fixture, and includes `crates/poche-ui/web/index.html` plus a pinned packaging contract. `wasm32-unknown-unknown` and the `wasm-bindgen` CLI are not installed, so no WASM build, bundle size, console log, DOM/accessibility tree, or browser interaction is claimed. | Not measured. | Intended output is generated under ignored `site/replay`; Pages workflow is deliberately unchanged until a local browser pass. | Static fixture only: no live metadata or transport. |
| Browser Veilid over HTTP/`ws://` | Open | No executable artifact exists locally and the package is not published on npm. The pinned Veilid README says browser nodes have WebSocket-only networking, no DNS-TXT bootstrap, and require a direct `ws://.../ws` bootstrap on HTTP. | Not measured. | Would need a local HTTP server, the pinned WASM build, and a reachable WS bootstrap/relay. | Browser network peers and bootstrap can observe ordinary network metadata; Poche private projections would remain application-encrypted. Source constraints are not a passing test. |
| Browser Veilid over HTTPS/`wss://` | Open; source predicts failure | The pinned Veilid README labels HTTPS operation “Not currently implemented” because browser-trusted WSS/outbound relay support is required. This has not been upgraded to executable failure evidence because the pinned WASM artifact cannot yet be built. | Not measured. | A GitHub Pages origin cannot use insecure `ws://`; it needs production-equivalent WSS bootstrap/relay infrastructure with trusted certificates. | Running such a relay is additional public infrastructure and exposes network metadata. No on-device companion is silently introduced. |
| Hostable Datastar Rust server | Open | Clean reference APIs and the 0.3.2 Axum examples were inspected. The crate is not in the local Cargo cache; no portable Poche dependency or executable has been fabricated from an absolute local path. | Not measured. | Ordinary accessible HTML/SSE can work in any browser; operator must build and host a native Rust service. | Candidate is a room-host-colocated authority adapter, not a projection-reading third-party relay. Exact trust and TLS evidence remains required. |
| Direct ash/Vulkan reference | Pass, bounded | From the unchanged cursor-latency reference: `cargo run --release --offline -- --present-mode immediate --frames-in-flight 1 --hide-os-cursor --stop-after-duration 3s` opened and exited normally, reporting `IMMEDIATE`, one frame in flight, and hidden OS cursor. | 3,819,008-byte executable. The renderer is 1,301 Rust source lines with 56 `unsafe` tokens; no input-to-photon hardware measurement was attempted. | Windows/Linux native Vulkan distribution; no browser target or AccessKit semantics. | Direct control includes polling, a latest Win32 cursor query, process/thread priority, 1 ms timer request, explicit acquire/submit/present, and selectable FIFO/relaxed/mailbox/immediate modes. That control is useful for a dedicated latency experiment, not for the game-state boundary. |

## Timed native probe

The native row records a stopwatch from process launch until a nonzero
top-level window handle, along with exit status and peak working set. The
command is intentionally external to unit tests because it creates a real
window:

```pwsh
$watch = [Diagnostics.Stopwatch]::StartNew()
$process = Start-Process target/release/poche-replay.exe -PassThru
do {
  Start-Sleep -Milliseconds 10
  $process.Refresh()
} while (-not $process.HasExited -and $process.MainWindowHandle -eq 0)
$watch.Stop()
$process | Select-Object Id, HasExited, MainWindowTitle, MainWindowHandle, PeakWorkingSet64
$watch.Elapsed.TotalMilliseconds
Stop-Process -Id $process.Id
```

This is startup latency, not click-to-presentation or input-to-photon latency.
Live command round trips must be measured in the eventual selected topology.

## Provisional architectural reading

The pure `PresentationModel` and egui widget tree are the renderer boundary:
native, WASM, or an eventual Datastar HTML adapter may consume the same
viewer-scoped input without changing game/session semantics. Direct Vulkan is
retained as a targeted future latency option, not the first UI implementation.

G32 remains provisional until the WASM row runs. G26 remains open until both
HTTP/WS and production-equivalent HTTPS/WSS Veilid rows execute and the
Datastar alternative runs. Consequently Pages still advertises only the
rulebook, and no direct live browser topology has been selected.
