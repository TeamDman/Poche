<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# ADR 0010: device-native orchestration and graphical capture

- Status: Accepted for phase 5 implementation
- Date: 2026-08-12 (America/Toronto)
- Scope: phase 5 gates G2 through G6
- Builds on: ADR 0007 player/device identity and replicated log
- Rejects: Vox/local-instance discovery and privileged window control

## Decision summary

Every Poche participant is a certified device connected to a room. The native
GUI, browser, CLI, persistent policy agent, and test puppet use the same typed
observe/actions/invoke/wait boundary. A CLI does not find and remote-control a
resident graphical process. Its commands become visible to graphical devices
because all of those devices observe the same committed room history through
their exact-recipient projections.

Graphical capture is non-authoritative device cooperation. One certified
device may sign an exact-target request to another active device of the same
player root. The provider applies its local capability, consent, and privacy
policy, then either signs a denial or returns hash-bound artifact descriptors.
Image-sized content uses a separate bounded private chunk transfer; raw PNG
bytes never ride an ordinary game-command reply. A shared `poche-capture`
pipeline, rather than Bevy or browser-specific code, validates, captions,
hashes, manifests, stores, cleans, and publishes artifacts.

This creates no new game authority and no extra player vote. Captures are
evidence about what one device rendered at a named revision. They are not
formal proofs, consensus events, or a way to change the room.

## One executable and command boundary (G2)

The installed product is one console-subsystem `poche.exe`:

- no arguments and `poche desktop` launch the native graphical client;
- `room`, `game`, `chat`, `command`, `spectator`, and `transcript` retain their
  existing text/JSON/NDJSON contracts;
- `agent` runs a persistent policy as a certified device;
- `device` manages profiles, enrollment, discovery, and capture cooperation;
- `puppet` runs semantic multi-player test scenarios as explicit devices.

The console subsystem is deliberate for this phase: redirected output, exit
codes, crash diagnostics, and PowerShell automation must remain dependable.
Explorer launch may detach or hide the console only through a reviewed Windows
launcher strategy later; a GUI-subsystem build must not make terminal output
unreliable. Structured failures go to stderr and the configured diagnostic
log. Secrets are loaded from protected profiles and never supplied in process
arguments, help, receipts, or crash text.

Facet/Figue owns reflected command/config parsing, help, and completions.
Top-level invocation owns cancellation, logging, output selection, and the one
dispatch point. Each subcommand has a discoverable `*_cli.rs` module following
the Teamy CLI layout. Exact published dependencies are used; dirty sibling
worktrees are references only.

## Device profiles and shared client (G3)

A protected device profile identifies:

- a player-root reference, never an exported root secret;
- one independently generated device key and root-signed certificate;
- custody class and certificate sequence/epoch;
- exact sorted capabilities;
- room membership/locator aliases and transport configuration; and
- durable idempotency, retry, and player-vote-lock evidence.

`poche-player-client` will expose renderer-neutral operations:

```text
observe(room) -> exact-recipient projection + revision
actions(room, revision) -> advertised typed actions
invoke(room, revision, action, idempotency) -> committed result or typed denial
wait(room, after_revision, predicate) -> later exact projection
cooperate(target_device, signed_request) -> signed denial or artifact transfer
```

In-process, HTTP/gateway, and Veilid adapters implement that boundary. Route,
URL, browser-connection, and Veilid node identifiers are locators, not player
or device identities. Revocation is checked at the request membership epoch.

Devices never add player voting weight. At most one active device certificate
per player/epoch should carry the `Vote` capability for ordinary operation,
and that device persists its player vote lock before publishing. The replicated
reducer still deduplicates by player and treats conflicting same-player signed
votes as equivocation evidence, so a profile/configuration defect cannot mint
extra quorum weight.

## Capture request and authorization (G4)

The version-one capture request binds all authority-relevant input:

- request, room, membership epoch, and player-root IDs;
- exact requester and provider device IDs;
- exact observed room/projection revision;
- an expiry instant and bounded total bytes;
- public-room or exact-player-view privacy scope;
- sorted requested representations and optional viewport;
- a bounded human-readable evidence label; and
- a device signature in the capture-request domain.

The requester certificate needs `RequestCapture`; the provider certificate
needs `ProvideCapture`. Both must be active, unrevoked, name the same player
root, and match the exact IDs in the request. The adapter verifies the request
signature against the requester's device key before pure session
authorization. The provider then applies local policy:

- a native provider may allow captures while its explicit provider toggle is
  enabled;
- a production browser advertises unavailable unless the user explicitly
  consents to the individual request;
- an enrolled Playwright/CDP test harness may advertise browser-harness
  capture for its disposable test profile;
- a headless device returns semantic structure and never claims pixels.

Another player, a stale/replayed request, wrong room/epoch/revision, revoked or
expired certificate, missing capability, expired request, oversized request,
or substituted response fails closed. Denial is signed and inspectable but is
not committed game history. Cancellation is a signed advisory cooperation
message; it does not roll back a completed transfer or a room transition.

The same-player rule permits one player's CLI device to see what that player's
GUI is allowed to see. It does not grant cross-player private-hand capture.
Such sharing would require a separately designed explicit grant protocol.

## Private bounded transfer (G5)

Existing acceptance PNGs measure roughly 58 KiB to 596 KiB. That is already
well above the protocol's 30,000-byte safe ordinary AppCall/frame ceiling. The
selected transfer is therefore a content-addressed, resumable sequence of
private chunks on a cooperation lane:

- artifact descriptors bind transfer ID, exact byte length, chunk size/count,
  and BLAKE3 content hash;
- each chunk is at most 24 KiB before adapter encryption/framing;
- one artifact is at most 64 MiB and one request returns at most four sorted
  representations;
- the receiver may request missing chunk indices and safely deduplicates
  repeated chunks;
- out-of-range, reordered-without-index, mismatched, expired, or oversized
  content is rejected before publication;
- temporary partial files are scoped to the request and removed on expiry,
  cancellation, validation failure, or successful finalization.

“Cooperation lane” is a semantic adapter interface, not a new authority. The
loopback adapter can transfer chunks in memory, the web adapter can use a
private authenticated endpoint, and a Veilid adapter can use separately
encrypted direct operations or a content-addressed blob locator. No adapter
may disguise the entire PNG as an ordinary command/AppCall reply. Public
Veilid support remains qualified until interruption, resume, duplicate,
reorder, mismatch, cleanup, and measured-size tests pass on that adapter.

## Provider and common artifact contract (G6)

A renderer provider receives an already authorized request plus its exact
projection and returns renderer-owned raw representations and metadata. It may
read Bevy render targets/camera state or browser viewport/DOM/accessibility
state. It does not choose filenames, retain history, publish files, infer game
authority from ECS/DOM state, or sign on behalf of another device.

The shared pipeline owns:

- representation validation and normalized PNG/UTF-8 structure;
- a caption containing scenario, player/device, room revision, viewport,
  provider, and qualified evidence boundary;
- BLAKE3 hashes for the raw projection/scene/structure and final content;
- one versioned run manifest and per-artifact descriptors;
- deterministic safe names beneath
  `target/poche-puppets/<run>/<scenario>/<player>/<device>/`;
- privacy scanning, exact-recipient labels, retention, temporary cleanup, and
  contact-sheet generation; and
- CI/Pages publication without committing generated images or PDFs to Git.

Generated runs are ignored. A deliberately curated baseline is reviewed and
committed separately, never silently updated by a passing test. Browser and
Bevy artifacts must pass the same validator and be readable by ordinary image
tooling. Semantic HTML, accessibility, and layout companions remain selectable
text/JSON rather than being flattened into pixels.

## Runtime, formal, and evidence boundaries

Capture manifests bind the runtime revision, exact-projection hash, renderer
scene/DOM hash where available, content hashes, and the release-evidence
version. That permits a diagnostic tool to say which runtime state and diagram
node the pixels represent.

It does not permit the page to claim that Alloy, NuSMV, or Prolog ran for that
request. “Checked now” evidence must include a real checker receipt. Otherwise
the artifact may only cite the versioned release/developer evidence that
covered the relevant transition family. Screenshots establish presentation
behavior; transcripts and reducers establish semantic execution; formal tools
establish only the properties and bounds their models actually checked.

## Rejected alternatives

- **Vox/named-pipe control of a resident window:** creates a second local
  authority, works only near one process, and bypasses the multi-device model.
- **A CLI steals or copies the GUI device key:** destroys independent device
  revocation and makes attribution misleading.
- **One capture authority on the host/gateway:** recreates trusted host
  visibility and cannot faithfully capture a remote browser/native surface.
- **Silent production-browser screenshots:** ordinary page JavaScript does
  not have that capability and pretending otherwise would erase consent.
- **Renderer-specific save/caption code:** guarantees divergent privacy,
  naming, cleanup, and manifest behavior.
- **PNG bytes in command replies:** violates measured transport bounds and
  couples diagnostic artifacts to authoritative command latency.
- **Captures in the replicated game log:** inflates consensus state with
  private, nondeterministic evidence that does not affect game semantics.

## Required implementation evidence

Acceptance requires canonical signing vectors; same-player positive and
cross-player/revoked/stale/replay/wrong-room/non-capable negative cases;
interrupted/resumed/reordered/duplicate/oversized/hash-mismatch transfer
tests; one artifact validator for Bevy and browser providers; privacy scans;
and a single puppet invocation that completes a multi-player game through
headless, browser, and native surfaces. Public Veilid evidence is a separately
guarded qualification, while deterministic CI uses loopback/in-process
adapters and the identical device/cooperation schema.
