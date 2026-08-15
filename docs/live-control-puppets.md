# Unified executable, certified devices, captures, and puppets

Phase 5 makes `poche.exe` both the native game and the command-line entry
point. With no arguments (or with `desktop`) it opens the Bevy client. The
same executable also supplies room/game/chat commands, persistent baseline
agents, protected device profiles, cross-device captures, transcript tools,
and multi-player puppets.

This is not local window automation. A CLI, policy, browser, or graphical
client is an independently certified device. It observes an exact-recipient
projection, selects one action advertised at that revision, and submits the
ordinary signed request. Other devices perceive the result through committed
room history. No Vox server, named pipe, resident-window discovery, shared
device key, or privileged reducer entry point is involved.

## Build and launch one executable

```powershell
cargo build --locked --offline -p poche-cli
target\debug\poche.exe
target\debug\poche.exe desktop
target\debug\poche.exe --help
target\debug\poche.exe --output json puppet list
```

The first two commands open the native client. Text, JSON, and NDJSON output
remain available for PowerShell, redirected pipelines, and computer players.
Structured output is written to stdout; diagnostics are written separately.

## Create one player with several devices

```powershell
poche.exe identity create alice
poche.exe device create alice alice-desktop
poche.exe device create alice alice-cli
poche.exe device create alice alice-agent
poche.exe identity show
poche.exe --output json device list
```

The player root and every device signing key are generated independently.
Public profile metadata is stored beneath the platform's local application-data
directory; secret bytes stay in the operating-system credential vault. Poche
fails closed if that vault is unavailable and never falls back to plaintext key
files. `POCHE_PROFILE_ROOT` can relocate public metadata, but does not relocate
or export the protected secrets.

Every sibling can propose actions, receive its own private projection, manage
devices, and request/provide captures. Only the first active certificate for a
player root receives the ordinary `Vote` capability. The replicated reducer
also deduplicates by player, so opening more devices cannot create more quorum
weight. Devices remain independently attributable and revocable.

## Use GUI, CLI, and agent devices in one room

The currently packaged live-device carrier is the HTTP/gateway endpoint. Start
the local authority and browser surface in one terminal:

```powershell
cargo run --locked --offline -p poche-web-spike
```

The development certified-device fixture at that endpoint is
`certified-device-room`; its one-use fixture invite is
`certified-device-join-v1`. It exists for local integration, not as production
invite design. A representative two-player setup is:

```powershell
poche.exe --profile alice-desktop room host certified-device-room
poche.exe --profile alice-cli room show certified-device-room

poche.exe identity create bob
poche.exe device create bob bob-desktop
poche.exe --profile bob-desktop room join certified-device-room certified-device-join-v1

poche.exe --profile alice-desktop room take-seat certified-device-room 0
poche.exe --profile bob-desktop room take-seat certified-device-room 1
poche.exe --profile alice-desktop room ready certified-device-room
poche.exe --profile bob-desktop room ready certified-device-room
poche.exe --profile alice-desktop room countdown certified-device-room 3
```

Launch Alice's graphical sibling against the same room, inspect actions from
the CLI, or let a bounded baseline policy play:

```powershell
poche.exe --profile alice-desktop desktop --room certified-device-room
poche.exe --profile alice-cli --output json game actions certified-device-room
poche.exe --profile alice-cli game bid certified-device-room 1
poche.exe --profile alice-cli game play-card certified-device-room 3C
poche.exe agent run alice-agent certified-device-room first-legal
poche.exe --stop-after-ms 10000 agent run alice-agent certified-device-room seeded-random:41
```

`first-legal` and `seeded-random:<seed>` choose only from the current
advertised player-game actions. The phase-five agent command deliberately does
not load a Burn model. A future learned policy should implement this same
observe/actions/invoke/wait contract rather than receiving a second authority.

The desktop, typed convenience command, direct opaque action reference, and
policy path converge on identical canonical request bytes and reducer behavior.
A stale observation is denied; it is never silently reinterpreted against a
newer state.

## Request a sibling device's graphical view

Run a native provider using one profile:

```powershell
poche.exe device capture serve-native alice-desktop certified-device-room
```

The provider is windowless by default. Add `--show-window` only when a human
needs an interactive debugging window. In another terminal, discover the exact
device ID and request its view with a sibling profile:

```powershell
poche.exe --output json device capture providers alice-cli certified-device-room
poche.exe device capture request alice-cli certified-device-room <TARGET_DEVICE_ID> bidding-checkpoint
poche.exe device capture request alice-cli certified-device-room <TARGET_DEVICE_ID> bidding-checkpoint --output-dir D:\captures\poche
```

The request binds the room, session and membership epoch, same player root,
both exact device IDs, observed revision, expiry, privacy scope, requested
representations, byte limit, and a replay nonce. Both certificates and the
request signature are checked before the provider applies local consent and
privacy policy. The response is evidence about rendering at a revision; it is
not a room command, vote, formal proof, or state transition.

An ordinary production browser does not have silent screenshot authority. It
must explicitly consent to a supported individual capture or return a stable
unavailable/denied result. Disposable browser puppets may use the separately
identified Playwright/CDP harness capability. Bevy and browser providers both
return raw representations to the common pipeline; renderers do not select
publication paths or retain their own evidence history.

Capture content is sent on a bounded cooperation lane rather than embedded in
an ordinary game reply. Each artifact has a content hash, exact length, chunk
shape, fresh content key, and requester-device key wrapping. Interrupted
receivers checkpoint authenticated ciphertext and public transfer metadata
only. After restart they reacquire the protected key, reauthenticate every
retained chunk, and resume from the contiguous boundary. Completion,
cancellation, expiry, integrity failure, and publication failure remove partial
state. The default live publication root is `target/poche-captures/live`.

The measured capture carriers are in-process/loopback and the HTTP gateway.
Public Veilid capture transfer is not currently a supported claim.

## Run windowless multi-player puppets

```powershell
poche.exe puppet list
poche.exe puppet show two-player-full-round
poche.exe --output json puppet run two-player-full-round --surface headless --transport loopback-ndjson --seed 1
poche.exe --output json puppet run external-devices-full-game --surface web,native --seed 84
poche.exe puppet artifacts path
poche.exe puppet artifacts open
```

Native puppets render to a Bevy image target with no primary window or Winit
event loop. Browser puppets use headless browser mode. The default therefore
does not flash windows or steal focus. `--show-window` opts into a native debug
window for a run.

One invocation creates several player roots and distinct devices, including
graphical siblings, CLI/policy participants, the spectator, and explicit
authority-service devices. It plays only through exact observations and
advertised signed actions. Runs have action deadlines, a whole-run watchdog,
bounded semantic steps, cancellation, and atomic publication. A cancelled or
timed-out run leaves no partial published evidence.

To author a scenario, add a static catalog entry in `poche-puppet`, construct
explicit roots/devices/memberships, and drive `PlayerDeviceClient` rather than
calling a reducer directly. Record each pending observation and completed
commit, preserve lifecycle operations separately from authoritative history,
and request graphical checkpoints through the same cooperation API used by a
real device. A scenario must declare its surfaces, transport, deterministic
seed, bounds, and evidence boundary.

## Inspect the artifacts

Every successful run is published atomically beneath
`target/poche-puppets/<run-id>/` with:

- `run.json`: final status, players/devices, scores, hashes, and qualification;
- `steps.ndjson`: exact observation, action, commit, and cross-device witnesses;
- `lifecycle.ndjson`: route disconnect/rebind and membership readmission facts;
- `manifest.json`: hashes and byte lengths for every retained file;
- `index.html`: a selectable contact sheet; and
- capture subdirectories containing PNG/HTML/accessibility/layout artifacts and
  their common capture manifests when the surface supports them.

The parent `catalog.json` and `index.html` are regenerated only after every run
manifest has been rehashed. Git ignores these generated artifacts. GitHub Pages
generates a deterministic headless contact sheet and manifest during CI, so the
public evidence can be inspected without committing high-churn screenshots or
PDF output.

## Evidence and deployment boundaries

| Track | What it establishes | What it does not establish |
| --- | --- | --- |
| In-process/direct typed and loopback NDJSON | Deterministic reducer, projection, client, policy, capture-schema, and failure behavior without sockets | External network delivery |
| HTTP/gateway live devices | Independently signed protected profiles can observe, act, wait, cooperate, and transfer captures through the packaged CLI | Host privacy or decentralized consensus; the gateway sees plaintext it serves |
| Native/public Veilid acceptance | A separately guarded native game-lifecycle scenario crossed real DHT/private routes | Routine offline CI, browser Veilid, public capture transfer, or network availability theorem |
| Experimental replicated log | Player-deduplicated certified voting, convergence under named assumptions, and attributable signed forks | Deployed Byzantine consensus or guaranteed progress under partition |
| Headless puppet evidence | Exact observations, actions, commits, histories, revisions, and device convergence | Pixels or layout |
| Bevy/browser puppet evidence | The named renderer produced the retained pixels/structure at a bound revision | Game authority, formal verification, or pixel equality between renderers |
| Alloy/NuSMV/Prolog receipts | Only the named properties and declared finite/symbolic/query scopes | A checker run for every page request or screenshot |

The default local Veilid topology gate is intentionally isolated and may report
zero public peers. The real public-network gate requires an explicit opt-in and
is never part of an ordinary test, puppet, or reinforcement-learning rollout.
Direct secure-origin browser Veilid remains unsupported in Veilid 0.5.7; live
browser play uses the disclosed host-colocated HTTP/SSE gateway instead.

See [ADR 0010](decisions/0010-device-orchestration-and-capture.md) for the
security decision, [deployment modes](deployment-modes.md) for trust topology,
and [native Veilid acceptance](veilid-native-acceptance.md) for the separately
guarded public-network evidence.
