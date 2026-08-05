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
  `b88ad8adf515300a67b7f0198d2eb63bc78d646b`; the portable spike uses the
  latest published `datastar` crate, 0.3.1, because 0.3.2 is not on crates.io;
- cursor-latency at `6c077050235a199af3d386d3c52523b8e04fef45`
  and ash at `a9a1fb17e98a0cde146caada86200d809306200d`;
- installed Edge 151.0.4129.59. Browser execution used the Codex in-app
  browser, whose version is not exposed; the installed Edge version is
  therefore not misattributed to those runs. The in-app browser also confirmed
  that npm has no published `veilid-wasm` package that could substitute for
  building the pinned source.

## Executable matrix

| Spike | State | Executable evidence | Size/latency | Accessibility and deployment | Exposure and exact limitation |
| --- | --- | --- | --- | --- | --- |
| Native egui projection replay | Pass | `cargo test -p poche-ui --offline`; `cargo build -p poche-ui --bin poche-replay --release --offline`; launching `target/release/poche-replay.exe` produced a live top-level window titled `Poche projection replay` with nonzero handle `3279528`. The process was then closed by the probe. | 5,590,016-byte release executable. A second probe reached a nonzero window handle in 499.831 ms and reported 20,885,504 bytes peak working set. This is process/window startup, not input-to-photon latency. | One Rust widget path, keyboard-focusable controls, AccessKit enabled. Windows build uses eframe Glow; no Vulkan API is required. Native artifact distribution is required. | Consumes only exact-recipient `ProjectionPayload`; there is no transport or additional operator in this static replay. |
| Pages-hostable egui/WASM replay | Pass | Installed `wasm32-unknown-unknown` for Rust 1.96 and `wasm-bindgen-cli` 0.2.126, then `crates/poche-ui/web/build.ps1` built and packaged the real target. A Python static server was the only local process; the in-app browser loaded `http://127.0.0.1:4173/`, reported no console errors, and exercised Bob's ungranted, granted (`2C 3C`), and revoked checkpoints. | 3,545,279-byte WASM plus 73,769-byte generated JS and a 1,763-byte static page. Reload-to-ready was 121 ms locally. Three automated click-and-screenshot sequences took 836-872 ms; this includes browser automation and capture and is not input-to-photon latency. | Generated output lives only under ignored `site/replay`, so Pages can build it without repository history churn. Current eframe 0.33.3 web output is a canvas: the accessibility tree exposes the application root but not ordinary button/text semantics because its web AccessKit update path is not implemented. This is acceptable for a static engineering replay but not selected for the accessible live browser UI. | Static fixture only: no live metadata, authority, or transport. The browser receives exactly the checked viewer projections embedded in the bundle. |
| Browser Veilid over HTTP/`ws://` | Pass | Built unchanged Veilid 0.5.7 at `76b21769` for Rust 1.96/WASM with exact `wasm-bindgen` 0.2.121 and `enable-protocol-wss`. From `http://127.0.0.1:4175`, the in-app browser connected directly to `ws://bootstrap-v1.veilid.net:5150/ws`. A first run reached `AttachedFull`, 28 peers, and public-ready in 7.171 s. Before the clean repeat, every process named Poche or Veilid was stopped/absent; it reached `AttachedFull`, 31 peers, and public-ready in 20.264 s. Only the Python static server remained locally. | Raw WSS-enabled bundle: 8,879,727-byte WASM, 407,153-byte JS, 2,153-byte page, and 5,166-byte probe before optional `wasm-opt`. Static server peak working set 26,083,328 bytes; browser memory was not measured. | Ordinary semantic probe controls/status; no native companion. HTTP is development-only and cannot protect room traffic on the public internet. Generated output remains ignored. | Veilid bootstrap and peers observe network metadata. Poche's application protocol must still encrypt/authenticate exact-recipient private projections; Veilid connectivity is not that proof by itself. |
| Browser Veilid over HTTPS/`wss://` | Fail; topology-selecting evidence | The same WSS-enabled browser artifact used `wss://bootstrap-v1.veilid.net:5150/ws` and remained `Attaching`, zero peers, not public-ready after 20.112 s. A read-only TLS probe resolved the documented host, connected to port 5150, then received a reset before the TLS handshake completed. The pinned upstream README independently labels HTTPS operation unimplemented pending outbound relays. | Same 9.29 MB raw JS+WASM payload. Failure deadline 20.112 s plus browser automation overhead; no connection latency can be claimed. | A Pages HTTPS origin cannot fall back to insecure `ws://`. A local page certificate would not create the missing public WSS bootstrap/relay. Providing one is new public infrastructure with certificate, uptime, abuse, and update obligations. | No companion was introduced. The failure selects the host-colocated Datastar authority for live browsers. A future WSS relay would observe network metadata and requires an explicit threat/operations decision before this row can be reopened. |
| Hostable Datastar Rust server | Pass | `poche-web-spike` uses released `datastar` 0.3.1 and Axum 0.8.9, not an absolute reference-repository path. Its host-colocated `InProcessAuthority<OracleSessionGame<2>>` accepts a typed `CreateRoom` through canonical NDJSON ingress and the pure session reducer, then patches the exact host projection. In the in-app browser, projection buttons proved Bob ungranted, granted (`2C 3C`), then revoked. `Create room` returned `applied; revision 1; events 1`; `Reset authority` restored pending state, and a second create succeeded. Independent projection roots were verified one each, with no duplicate DOM ID. | 3,013,632-byte release server and 9,023,488-byte measured peak working set. Initial page load was 2,814 ms, dominated by the external client CDN. Browser-observed click-to-patched-DOM snapshots took 297/307/286 ms for the three Bob views, 288 ms for create, 290 ms for reset, and 302 ms for recreate. These include browser automation/snapshot overhead, not transport-only latency. | Ordinary headings, regions, lists, buttons, live status, and card-code elements appear in the browser accessibility tree. Deployment requires a native room-host service plus TLS/reverse proxy. The spike pins Datastar JS 1.0.0-RC.7 on jsDelivr; production should self-host that pinned asset for availability and metadata control. | The server is the room authority colocated with the room host, so the topology adds no separate projection-reading operator. The CDN can observe page-asset fetch metadata but receives no Poche projections. A third-party server operator would be a different threat model requiring an explicit decision. |
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

The pure `PresentationModel` is the renderer boundary: native/WASM egui and the
semantic Datastar HTML adapter consume the same viewer-scoped input without
changing game/session semantics. The live browser evidence selects semantic
HTML for accessibility while retaining shared egui for native clients and the
static web replay. Direct Vulkan is retained as a targeted future latency
option, not the first UI implementation.

G26 and G32 are evidence-closed by ADR 0004. HTTP/WS browser Veilid works with
no companion, but the production HTTPS/WSS prerequisite fails. The selected
live browser topology is therefore a self-hostable, host-colocated Datastar
authority with semantic HTML. Shared egui remains the native/static-WASM
renderer, while Pages publishes the rulebook and static replay rather than
claiming direct Veilid multiplayer.
