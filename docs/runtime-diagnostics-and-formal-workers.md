# Runtime diagnostics and formal-worker capabilities

The browser-facing diagnostic surface separates three state machines which can
change independently:

1. the room lifecycle (`Lobby`, `Countdown`, `Running`, `Paused`, `PostGame`,
   or `Closed`);
2. one viewer's transport (`Connected`, `Reconnecting`, `Disconnected`, or a
   retained `Replay`); and
3. the Poche game phase (`AwaitingDeal`, `Bidding`, `Playing`, `Scoring`, or
   `Finished`).

The server converts the typed runtime projection into a renderer-neutral
`StateMachineDiagram`, then emits accessible inline SVG. SVG is already the
browser-native image form; PNG rasterization belongs in an optional presentation
leaf and can never become game or authorization state.

The page also exposes a read-only `POCHE DIAGNOSTIC CONTEXT v1` summary and a
clipboard button. It includes the authority incarnation and revision so two
stale tabs can be distinguished. Command responses currently patch only the tab
which sent the request, so another tab can show an older incarnation or revision
until its viewer is refreshed. The copy-safe summary includes public and
client-local state but deliberately omits room codes, chat contents, and private
card faces.

## Evidence strength shown to the player

The live diagram is an annotation of the typed Rust projection accepted by the
runtime reducer. It is not a claim that Alloy, NuSMV, or Scryer Prolog ran for
that HTTP request. The UI must distinguish:

- **runtime accepted**: the current typed reducer and invariants accepted this
  state;
- **release checked**: the registered formal-model scopes passed for the
  source/model hashes named by a release receipt; and
- **checked now**: a capable worker actually ran a named query over this exact
  state witness and returned a verifiable receipt.

The independent formal models remain valuable even when a browser cannot run
their native tools. A browser can display release evidence or request fresh
diagnostic work from another device without making that result game authority.

## Capability-worker protocol boundary

A browser, Axum gateway, native Veilid peer, or loopback worker is a device with
advertised capabilities. A capable device may advertise a bounded formal-worker
profile such as:

```text
poche.formal-worker.v1
worker device key
supported tool/version/binary hashes
registered model IDs and source hashes
registered query/scope IDs
maximum input, wall-time, memory, and output sizes
privacy class accepted: public-only | exact-recipient-with-consent
expiry and worker signature
```

The normal request is data, not remotely supplied executable source:

```text
poche.formal-job.v1
job ID and requester device key
registered model ID + immutable source hash
registered claim/query ID + scope ID
canonical public state witness + semantic hash
resource limits no greater than the worker advertisement
reply route, expiry, and requester signature
```

The response is a signed evidence receipt:

```text
poche.formal-result.v1
job/request/state/model/tool hashes
satisfied | counterexample | inconclusive | rejected
normalized observations and optional bounded counterexample hash
actual resource use, qualification text, and worker signature
```

Multiple workers may run the same job and their receipts may be compared. Their
agreement is diagnostic evidence, not a vote which mutates the room. A result
may propose a typed accusation or recovery command, but the existing
authorization/governance path remains the only way to commit it.

## Why arbitrary Alloy/NuSMV/Prolog source is not the default FFI

Accepting `here is ABC.als; execute it` lets an untrusted room member select
parser/compiler attack surface, consume unbounded CPU/memory/disk, probe tool
versions, and potentially encode private state for exfiltration. Native solver
processes are not a safe game-message interpreter merely because their input is
declarative.

An explicit developer-only `unregistered-source` worker could be added later,
but it must be disabled for public rooms and require a separate local policy:
isolated process/container, no network, empty temporary directory, strict byte,
time, memory, process, and output limits, pinned tool binaries, no filesystem
includes, and a result labelled unregistered/untrusted. It must never inherit a
player's game-command authority.

## Browser, routes, and network boundaries

An HTTP path selects a resource and viewer adapter; it does not determine the
canonical room state. JavaScript, WASM, HTTP/SSE, Axum, Veilid, and native tools
are protocol boundaries with independently modelled failure modes:

```text
browser controls
  -> typed opaque command ID
  -> HTTP/SSE gateway
  -> signed device command
  -> Veilid or loopback transport
  -> authorization + pure reducer
  -> exact-recipient projection
  -> HTML/SVG/native rendering
```

Each arrow needs canonical encoding, identity/authority checks, replay and size
bounds, explicit failure states, and viewer privacy. The same typed diagnostic
context can annotate where a message currently sits without treating DOM,
route, SVG, or PNG state as the source of truth.
