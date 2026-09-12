# Veilid private-route latency probe

`poche-veilid-latency-probe` is an independent transport experiment. It starts
two distinct native Veilid nodes, waits for both to reach public-network
readiness, allocates one private route per node, and exchanges the route blobs
directly in process.

The experiment deliberately does **not** use:

- Veilid DHT publication, lookup, or watches;
- the Poche protocol, reducer, session authority, or recovery checkpoints;
- Bevy, any other UI, polling, or the clipboard; or
- fault injection.

It therefore separates Veilid's private-route data-plane latency from Poche's
application behavior. Setup and route-allocation time are printed separately
from steady-state samples.

## Run it

The opt-in is required because this attaches two real nodes to Veilid's public
network and opens two user-mode UDP listeners. It does not request Windows UAC
elevation or change firewall rules.

```powershell
cd D:\Repos\Games\poche-3
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST = 'I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run --release --locked --offline -p poche-veilid `
  --features veilid-latency-probe `
  --bin poche-veilid-latency-probe -- `
  --samples 20 --warmup 3 --payload-bytes 64 --timeout-ms 30000
```

The executable reports four different measurements for each sample:

| Metric | Boundary |
| --- | --- |
| `app_call_rtt` | Node A sends an `AppCall`; node B immediately echoes it with `app_call_reply`; node A receives the reply. |
| `app_message_dispatch` | Time until node A's `app_message` future returns. This is sender-side dispatch acceptance, **not delivery**. |
| `app_message_delivery` | Node A sends an `AppMessage`; node B's update callback receives it. Both nodes are in one process, so this is a one-way measurement on one monotonic clock. |
| `app_message_echo_rtt` | After receipt, node B echoes the `AppMessage`; node A receives the echo. |

The AppCall and AppMessage echo payloads carry a sequence number and are
byte-for-byte checked. The default routing context is intentional: it matches
Poche's current sender-safe routing behavior, while each receiver is addressed
through an imported private route.

If both end-to-end RTTs are small while the game takes seconds, the primary
delay is above Veilid's steady-state data plane—currently likely Poche's
request/observe sequence, polling, authority lock, and synchronous recovery
checkpoint. If the echo RTT itself takes seconds, route selection or the public
network path is independently implicated. Sender dispatch cannot answer this
question by itself.

## First local baseline

On 2026-09-12, the release build using Poche's pinned Veilid 0.5.7 commit
`99c96162` produced this 30-sample baseline from one Windows machine. Both
nodes used the public Veilid network; the numbers are observations of that run,
not universal performance promises.

| Metric | Minimum | Median | Mean | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: |
| AppCall RTT | 104.257 ms | 118.429 ms | 158.366 ms | 262.074 ms | 262.268 ms |
| AppMessage dispatch | 0.134 ms | 0.224 ms | 0.231 ms | 0.307 ms | 0.383 ms |
| AppMessage one-way delivery | 42.356 ms | 46.812 ms | 47.613 ms | 53.455 ms | 57.300 ms |
| AppMessage echo RTT | 90.436 ms | 99.969 ms | 101.090 ms | 112.483 ms | 134.330 ms |

Public readiness took 6.9-7.1 seconds. Private-route allocation then took
159-270 ms. Neither setup measurement is
included in the table.

This run does not reproduce Poche's multi-second steady-state interaction
delay. It establishes that sender dispatch makes `AppMessage` a candidate for
speculative pose previews and that neither delivery mechanism is intrinsically
instantaneous: authoritative UI should expect roughly network-RTT-scale
confirmation. The next diagnostic boundary is an instrumented Poche call that
times encode/sign, AppCall, authority queue/reducer, recovery persistence,
reply decoding, the follow-up observation, and render-thread receipt
independently.
