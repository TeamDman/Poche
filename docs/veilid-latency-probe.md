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

## Instrument the complete Poche path

The desktop command already supports timestamped NDJSON through `--log-file`.
With `--debug`, ordinary debug output remains concise on stderr while the file
also enables the `poche_latency=trace` target:

```powershell
cd D:\Repos\Games\poche-3
cargo run --release --locked --offline -p poche-cli -- `
  --debug --log-file target\latency\desktop.ndjson desktop
```

An explicit `--log-filter` still controls both destinations and conflicts with
`--debug`, as before. The latency events carry wall-clock timestamps plus
monotonic `Instant` deltas in `*_us` fields. A 16-hex BLAKE3 token correlates
the exact encoded request at the client and authority. It is diagnostic only:
the logs omit signed bytes, player and device identities, room and invitation
secrets, card faces and identifiers, chat, coordinates, and filenames.

The windowless two-process acceptance can write one log per process:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:CARGO_PROFILE_TEST_DEBUG = '1'
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST = 'I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
$env:POCHE_LATENCY_LOG_DIR = 'D:\Repos\Games\poche-3\target\latency-two-process'
cargo test --locked --offline -p poche-cli --features native-input-test --lib `
  cli::desktop::connection::process_probe::protected_desktop_native_drag_two_process `
  -- --exact --ignored --nocapture --test-threads=1
```

This opt-in test uses the public Veilid network, protected persistent test
profiles, two independent processes, isolated test coordination, and a
windowless draw target. It does not use the OS clipboard or open visible game
windows. `POCHE_LATENCY_LOG_DIR` is intentionally test-only. The command-local
test debug setting keeps Windows PDB size manageable after a clean build; it
does not change the application protocol or enable release optimizations.

The principal event boundaries are:

| Events | Boundary |
| --- | --- |
| `ui_pose_submitted` → `ui_pose_worker_dequeued` | render/input handoff and local coalescing queue |
| `client_pose_signed`, `client_exchange_encoded` | signing and encoding |
| `client_app_call_complete` | complete Veilid AppCall RTT |
| `authority_call_callback` → `authority_call_dequeued` → `authority_handler_started` → `authority_dispatch_worker_started` | callback queue and Tokio/blocking-task scheduling |
| `authority_room_operation` | room-lock wait, reducer, recovery snapshot, and persistence |
| `authority_checkpoint_saved` | protected-store lock/key lookup, encryption, temporary write, `sync_all`, and atomic persist |
| `ui_pose_confirmation_observed` | input through the forced follow-up observation |
| `ui_worker_event_applied` | worker result waiting for the Bevy/render thread |

### First instrumented application-path baseline

On 2026-09-12, the final debug acceptance run passed the public two-process
native drag test in 59.47 seconds. The run produced two physical-pose calls and
742 authority ticks while the service was active. Both child processes had one
early observation timeout and recovered; the complete scenario still passed.

| Measurement | Observed values |
| --- | ---: |
| Physical-pose AppCall RTT | 533.1 ms; 898.4 ms |
| Approximate client-to-authority callback leg | 71.4 ms; 69.1 ms |
| Approximate authority-reply-to-client leg | 58.5 ms; 234.0 ms |
| Authority physical-pose room operation | 101.7 ms; 103.9 ms |
| Pose persistence inside that operation | 76.5 ms; 78.9 ms |
| Input through submitting-device follow-up observation | 1,599.5 ms; 3,200.3 ms |
| Latest coalesced pose waiting in the local worker | 1,328.3 ms |
| Authority tick total, mean / p50 / p95 | 48.2 / 35.7 / 132.1 ms |
| Tick persistence, mean | 39.3 ms |
| Protected checkpoint, mean / p50 / p95 | 33.6 / 28.2 / 71.9 ms |
| Checkpoint encryption, mean / p50 / p95 | 28.7 / 24.1 / 67.3 ms |
| Async handler scheduling, mean / p50 / p95 / max | 245.2 / 33.1 / 738.0 / 2,702.7 ms |
| Blocking-worker scheduling, mean / p50 / p95 / max | 0.027 / 0.025 / 0.030 / 0.201 ms |

This identifies application architecture, rather than Veilid alone, as the
dominant delay in that run. The 50 ms authority interval currently enters the
same durability wrapper as a mutation: it serializes and encrypts a complete
recovery checkpoint, calls `sync_all`, atomically replaces it, and holds the
room lock even when no state changed. Its mean duration exceeded its requested
period. Because this synchronous work runs in the async service loop, incoming
calls waited hundreds of milliseconds for task execution. Across 87 calls, the
callback-to-async-handler scheduling delay reached a 738.0 ms p95 and 2.70 s
maximum. Once the handler submitted blocking work, the blocking-worker queue
added only 0.027 ms on average and 0.201 ms at worst. This distinguishes Tokio
service-loop starvation from blocking-pool saturation. The native worker then
serialized every accepted pose behind a 50 ms delay and another AppCall
snapshot before it could send the next coalesced pose.

The two physical calls also show why private-route transport is not the whole
delay. Their outbound network legs were about 69-71 ms. The first handler ran
almost immediately; the second waited 385.9 ms after dequeue before it began.
The room operation then consumed about 102-104 ms, mostly persistence. The
reply legs were 58.5 and 234.0 ms. Veilid latency is visible, but application
scheduling, persistence, and the forced confirmation request account for the
larger interaction delay.

These exact durations are **not** release-performance claims. The test used an
unoptimized debug build; debug cryptography is slower, and synchronous trace
file writes perturb the measured process. In particular, checkpoint encryption
dominated the local protected-store duration in this build. The monotonic
durations captured inside each operation and scheduler boundary are more
reliable than subtracting adjacent file timestamps. The causal findings do not
depend on that qualification: an unchanged 20 Hz tick performs durable
encrypted I/O, the synchronous tick competes with network request scheduling,
and each sent pose is followed by a blocking snapshot before the next pose. A
release run should be recorded before setting a numerical performance target.

### Idle-tick checkpoint experiment

The first experiment kept the 50 ms scheduler and every certified rules path,
but stopped creating and saving a recovery checkpoint when
`drive_authority_services_elapsed` returned `Ok(0)`. An idle service
observation can still advance an in-memory delivery-only projection ID. That
ID is excluded from the semantic projection hash and is captured by the next
meaningful checkpoint. A tick that commits one or more actions, or returns an
error after possible partial mutation, still requests persistence.

The otherwise identical public-network acceptance passed in 41.95 seconds,
down from 59.47 seconds (29%):

| Measurement | Before | Idle-write suppression |
| --- | ---: | ---: |
| Encrypted checkpoints | 831 | 90 |
| Authority ticks | 742 | 621 |
| Ticks requesting persistence | 742 | 1 |
| Tick total, mean / p50 / p95 | 48.2 / 35.7 / 132.1 ms | 8.2 / 0.149 / 63.3 ms |
| Async-handler scheduling, mean / p50 / p95 / max | 245.2 / 33.1 / 738.0 / 2,702.7 ms | 0.027 / 0.018 / 0.028 / 0.287 ms |
| Invoke AppCall, mean | 593.9 ms (7 calls) | 385.9 ms (7 calls) |
| Observe AppCall, mean | 749.4 ms (78 calls) | 471.5 ms (77 calls) |
| Physical-pose AppCall, mean | 715.8 ms (2 calls) | 450.1 ms (3 calls) |
| Pose input through confirmation, mean | 2,399.9 ms | 1,361.1 ms |
| Complete acceptance scenario | 59.47 s | 41.95 s |

The changed run wrote 90 checkpoints: the initial recovery attachment, the
ordinary request operations, and one tick that committed a real authority
transition. The remaining 620 idle ticks wrote none. Blocking-worker
scheduling remained negligible at a 0.027 ms p95.

This comparison was not aided by a faster public route. The changed run's
three measured pose requests took roughly 173-199 ms to reach the authority
and 162-197 ms for the reply leg, compared with roughly 69-71 ms outbound and
59-234 ms inbound in the earlier run. Despite that network variation, removing
idle persistence eliminated the large async scheduling backlog and improved
the complete interactions.

The next implementation experiments should preserve the certified rules
boundary while measuring each change separately:

1. keep actual mutation acknowledgement behind durable save, but move scheduled
   blocking work off the Tokio service-loop thread;
2. deliver the already-authoritative `PhysicalPoseState` reply directly to the
   submitting renderer instead of requiring an immediate snapshot;
3. separate high-rate, latest-wins physical pose replication from durable game
   commands, then measure AppMessage push/preview against AppCall settlement;
4. replace peer snapshot polling with a pushed invalidation or pose stream,
   retaining snapshots for reconciliation and restart.
