# Poche phase 5: unified executable, device orchestration, captures, and puppets

**Plan ID:** `poche-phase-5-live-control-puppets`
**Plan status:** Execution in progress
**Primary implementation root:** `D:\Repos\Games\poche-3` on `model-checking`
**Last updated:** 2026-08-12 (America/Toronto)
**Intent audit:** Passed 2026-08-12 against the available original Poche/SFM,
desktop, browser, CLI, computer-player, multi-device, capture, puppet, Figue,
Veilid, and planning instructions in this task
**Current implementation focus:** Task 3.1; turn Bevy into a live certified
player device and common-contract graphical capture provider

## How to update this plan

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

Update a work item's heading, evidence, validation, and completion notes
together. Keep at most one implementation item `[~]` unless the plan explicitly
records independent owners. A phase is complete only when every item in it is
`[x]`.

The plan is only ready once we have literally triple checked that no intent
from the user has been omitted without explicit direction from the user.

The room protocol, player/device authority, cross-device cooperation,
presentation adapters, artifact storage, and test-only orchestration are
separate capabilities. Never simplify a test by silently merging those
boundaries. Record decisions and evidence beside the work item they affect
rather than appending a detached chronological log.

## Purpose

Ship one installed `poche.exe` that launches the native graphical game and
also provides CLI, device-agent, and puppet commands. A CLI or computer policy
participates as a real certified Poche device: it observes its exact-recipient
view, selects an advertised action, submits an ordinary signed command through
the configured Poche transport, and sees the same committed history as the
player's graphical devices. It does not remote-control a local window through
a second IPC authority.

Add a transport-neutral cooperation protocol through which one certified
device can request a visual capture from another certified device of the same
player. Bevy, browser, and future renderers implement one capture-provider
contract. They return renderer-specific pixels and structure to one shared
capture pipeline that owns authorization, transfer, validation, captioning,
hashing, manifests, persistence, and publication.

Build one semantic puppet harness that fans out into distinct devices for
multiple players. The harness drives ordinary room actions, asks rendering
devices for captures through the same cooperation protocol, and compares
headless, semantic-HTML, and Bevy evidence without constructing a second game
engine. Bevy remains an intermediary between typed game state and the devices
and inputs through which humans experience it.

## Authoritative user guidance ledger

| ID | Guidance | Required plan consequence | Superseded by |
| --- | --- | --- | --- |
| P5-U1 | Research `sfm-propagate-changes`, Minecraft puppets, and the newer `sfm` live-game CLI as inspiration for Poche automation and integration. | Retain SFM's useful semantic waits, actual-surface captures, manifests, external-process witnesses, and contact sheets without copying its Minecraft-specific local-control topology. | — |
| P5-U2 | Determine how Poche is grounded in desktop and web experiences and whether snapshots/puppets let an agent experience it through image tooling. | Record the verified baseline and close the durable web-capture, live native-client, and shared puppet gaps. | — |
| P5-U3 | A Poche CLI should interact with a live desktop game from outside so computer players can behave exactly as players. | Preserved historically, but “interact with the window” is corrected to “join the same room as another certified device and produce observable shared actions.” | P5-U14 |
| P5-U4 | Use `G:\Programming\Repos\teamy-rust-cli` and `G:\Programming\Repos\facet\figue` to make the CLI and do communication work. | Use Teamy/Figue for reflected CLI/configuration, output, logging, cancellation, fuzzing, and module layout. Communication uses Poche's device/application transport rather than adding Vox IPC. | P5-U14 |
| P5-U5 | The same `poche.exe` should provide the desktop graphical experience and the CLI used to interact with graphical play. | Preserve one release executable, but CLI subcommands join/control the shared room through a device profile instead of discovering a resident window. | P5-U14 |
| P5-U6 | Decide whether same-device IPC is necessary when Poche already plans multiple devices per player whose actions are mutually perceivable. | The decision is no: omit Vox and local-instance discovery from this phase's foundation. | P5-U14 |
| P5-U7 | Connected devices retain agency; a player owns a set of device keys rather than moving one singleton authority between devices. | Every independently operating GUI, browser, CLI, or bot has a distinct certified device key, independent revocation, and no added player vote weight. | — |
| P5-U8 | Web and desktop experiences must be automatable and visually inspectable, not only reducer fixtures or preprogrammed state dumps. | Add real-surface browser and Bevy capture providers plus headless semantic evidence under one puppet scenario. | — |
| P5-U9 | A typed/text protocol is valuable for inspectability, multiplayer, and agent control even when richer rendering exists. | Keep observation/action/capture metadata renderer-neutral and machine-readable; renderers do not own game semantics or artifact persistence. | — |
| P5-U10 | Formal models and diagnostics should explain runtime logical state without claiming Alloy/NuSMV/Prolog execute in every browser. | Manifests bind runtime/projection/scene/transcript hashes and release-evidence versions; checked-now formal execution remains explicit and optional. | — |
| P5-U11 | Ordinary automation must not depend on public Veilid traffic, while guarded public acceptance remains meaningful. | Run the same device protocol over in-process/loopback transports in deterministic CI; retain public Veilid as an explicit opt-in integration gate. | — |
| P5-U12 | Use the resumable implementation plan discipline and preserve nuance through compaction. | Maintain stable guidance IDs, traceability, evidence boundaries, task contracts, risks, and literal three-pass intent audits. | — |
| P5-U13 | The project remains MPL-2.0 and must not absorb dirty sibling-repository state accidentally. | Reference sibling designs but use exact reproducible dependencies or reviewed attributed source; never path-depend on dirty Teamy/Facet worktrees for release. | — |
| P5-U14 | Agreed: remove Vox/local-instance control from the foundation and leverage Poche's existing/planned multi-device and Veilid architecture. | No Vox dependency, instance descriptor, named-pipe server, resident-window proxy, or local-control authority belongs in this phase. CLI/agents are certified room devices. | — |
| P5-U15 | A player's device should be able to request a graphical capture from another one of that player's devices; Bevy and browser capture and saving should follow one design. | Add signed, exact-target, same-player capture requests, advertised provider capabilities, a common capture payload/artifact pipeline, and bounded private transfer. | — |
| P5-U16 | Puppets should be one harness invocation that fans out and acts as devices for multiple players; Bevy is only an intermediary between game state and human interaction. | One harness owns explicit test player/device profiles, drives the shared device API, and asks rendering devices for evidence; Bevy remains a replaceable projection/input adapter. | — |
| P5-U17 | Puppet graphical capture should use a windowless draw target or equivalent so automation does not visibly pop windows onto the user's desktop. | Native puppet/capture workers render hidden or offscreen by default; showing a window is an explicit debugging option, and real people launching `poche desktop` still receive a visible interactive window. | — |

## Guidance traceability

| Guidance | Plan coverage | Evidence when complete |
| --- | --- | --- |
| P5-U1 | F6-F7; 4.1-4.4; 5.1 | SFM comparison, semantic puppet DSL, real capture manifests/contact sheet |
| P5-U2 | F1-F5; 4.2-4.4; 5.1-5.2 | Native/web artifacts directly readable through ordinary image tooling |
| P5-U3 | Superseded by P5-U14; C1-C5; 2.1-2.4; 3.3 | Device joins same room and produces UI-observable commands without window IPC |
| P5-U4 | C13; G1-G2; 1.1; 1.3 | Figue parser/help/fuzz evidence; no Vox/local IPC dependency |
| P5-U5 | G2; 1.3; 3.1; 5.3 | One executable passes graphical and CLI/device-agent acceptance |
| P5-U6 | P5-U14; F8; architecture table; out-of-scope boundary | Dependency tree and docs contain no required Vox/local-instance layer |
| P5-U7 | C2-C7; G3; 2.1-2.4; 3.1-3.2 | Unique device certificates, exact projections, revocation, one-player-vote evidence |
| P5-U8 | 4.1-4.4; 5.1-5.2 | Real DOM/browser and native captures plus structural assertions |
| P5-U9 | C1; C8-C11; 2.1-2.3; 4.1 | Shared typed device/capture/client abstractions and renderer-neutral scenarios |
| P5-U10 | 4.4; 5.2; 6.1 | Qualified evidence linkage without per-page checker claims |
| P5-U11 | C12; target matrix; 5.3 | Offline in-process/loopback CI and separately guarded public Veilid receipt |
| P5-U12 | Entire plan; audit profile; 6.2 | Repository guidance audit and final three-pass audit |
| P5-U13 | C14; G1; 1.1; 6.2 | Lockfile/license audit and sibling-independent build |
| P5-U14 | F8; C1-C5; architecture table; all phases | Device-native CLI/agent path; explicit absence of Vox and local discovery |
| P5-U15 | C8-C11; G4-G6; 2.2-2.4; 4.2-4.4; 5.1-5.3 | Cross-device capture request, common artifact bytes/manifest/transfer tests |
| P5-U16 | C1; C15; 4.1-4.4; 5.1-5.2 | One invocation drives distinct player devices and all three evidence surfaces |
| P5-U17 | C17; 3.1; 4.2; 5.1-5.2 | Native puppet acceptance completes with no visible automation window and a real render-target artifact |

## Intent audit evidence

- **Pass 1 — extraction:** Reread the available original SFM research,
  computer-player, visual evidence, `teamy-rust-cli`, Figue, one-executable,
  multi-device, Veilid, planning, and latest capture/harness instructions.
  Retained prior guidance as P5-U1 through P5-U13, marked corrected IPC
  expectations as superseded, and added P5-U14 through P5-U17 for the explicit
  architecture correction, cross-device capture, common artifact pipeline,
  single fan-out harness, Bevy boundary, and hidden/offscreen puppet rendering.
- **Pass 2 — traceability:** Mapped all seventeen guidance rows to constraints,
  design gates, tasks, validation, and release evidence. Removed every required
  Vox dependency, descriptor, named-pipe, resident-window proxy, and local-
  instance selection task. Replaced them with device enrollment/discovery,
  signed cooperation messages, common artifacts, and transport-matrix tests.
- **Pass 3 — adversarial omission:** Confirmed the revised plan says to
  “remove Vox and local-instance control from the foundation”; preserves that
  “capture is a signed device-to-device cooperation request”; requires that a
  “single harness acts as distinct devices for multiple players”; states that
  “Bevy is an intermediary between typed game state and human interaction”;
  and still accounts for “multi-device player-vote equivocation.” It also
  preserves browser capture consent/capability limits, large-payload transfer,
  exact-recipient privacy, offline CI, one executable, Figue, MPL-2.0, and
  release-evidence qualifications. It additionally requires that automation
  must not visibly pop native windows unless an explicit debug option asks it
  to do so.
- **Known source limitation:** None for the current phase request. Completed
  phase-one through phase-four intent remains authoritative through `PLAN.md`
  and `PLAN-2` through `PLAN-4`; those plans are referenced rather than copied
  incompletely into this ledger.

## Established foundation

### F1. Typed game state and exact-recipient projections precede rendering

Session/runtime reducers remain authoritative. `poche-ui` derives
`LiveClientPresentation` and renderer-visible `TypedUiControl` values from an
exact-recipient projection. Renderers expose labels and opaque controls while
the adapter submits typed `CommandPayload` values through ordinary
authorization.

References: `crates/poche-ui/src/live.rs`, `docs/viewer-projections.md`, and
`docs/semantic-html-tabletop.md`.

### F2. The real web player path lacks a durable multi-viewport image corpus

The phase-four web path supports arbitrary tab-scoped identities, opaque room
codes, two-tab play, exact-recipient SSE, chat, governance, reconnect, and
renderer-driven controls. It retains a JSON browser acceptance receipt but not
a repeatable captioned screenshot/contact-sheet set.

References: `PLAN-4-PLAYER-WEB-EXPERIENCE.md`,
`crates/poche-web-spike/src/game.rs`, and
`docs/evidence/semantic-html-acceptance.json`.

### F3. Bevy can capture a real rendered scene but is not yet a live room device

`poche-native-ui` renders an exact-recipient `SpatialScene`, supports camera
and debug controls, and writes a real screenshot and acceptance receipt. Its
current scene/controller is a deterministic fixture. This phase connects Bevy
to the shared device client and capture-provider contracts without granting
its ECS or pixels semantic authority.

References: `crates/poche-native-ui/src/lib.rs`, `docs/native-spatial-ui.md`,
`docs/assets/native-spatial-acceptance.png`, and
`docs/evidence/native-spatial-acceptance.json`.

### F4. The `poche` grammar is typed but mostly non-executing

`poche-cli` already builds the `poche` binary and declares room, game, chat,
governance, spectator, transcript, and identity commands. Its manual parser
has redaction and round-trip checks. Only transcript and governance currently
invoke behavior; the remaining groups return parsed receipts.

Reference: `crates/poche-cli/src/lib.rs` and `crates/poche-cli/src/cli/`.

### F5. Poche already separates player, device, and transport identity

Host-authoritative commands use stable application signatures and never treat
Veilid routes/node IDs as callers. The separately versioned replicated track
has player roots, certified devices, exact capabilities, revocation epochs,
player-deduplicated votes, and accountable equivocation evidence. Multiple
devices add agency, not player vote weight.

References: `docs/application-identity.md`,
`docs/veilid-command-transport.md`,
`docs/decisions/0007-player-device-and-replicated-log.md`, and
`crates/poche-protocol/src/replication.rs`.

### F6. SFM demonstrates semantic puppets and durable native evidence

SFM puppets use tick-driven actions and semantic waits, drive real UI/action
paths, capture native render targets, append captions outside the viewport,
and record geometry/hash/screen metadata. `sfm-propagate-changes puppet`
provides list/show/run/artifacts/matrix and HTML contact sheets. These artifact
and orchestration lessons remain applicable even though Poche rejects SFM's
local-instance control topology for this phase.

References:

- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\src\gametest\java\ca\teamdman\sfm\gametest\puppet\`
- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\cli\sfm-propagate-changes\src\cli\puppet.rs`
- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\build\sfm-toolchain\artifacts\game-test-preview\preview-manifest.json`

### F7. SFM's external CLI witness remains a useful acceptance shape

One SFM puppet launches the real external CLI asynchronously and observes the
result through the game. Poche should retain that witness shape, but the Poche
CLI process will be another certified room device and the visible GUI will
observe the resulting committed action through the room protocol.

References:

- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\src\gametest\java\ca\teamdman\sfm\gametest\puppet\definition\TitleScreenExternalCliSizeDisplayGamePuppet.java`
- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\src\gametest\java\ca\teamdman\sfm\gametest\puppet\action\InvokeExternalCliSizeDisplayPuppetAction.java`

### F8. Figue parses configuration; Poche's device protocol communicates

The Teamy template supplies Figue/Facet commands, structured output, logging,
cancellation, Windows resources, and Arbitrary round trips. Figue is not an
IPC or network transport. Poche already has in-process, browser gateway, and
native Veilid adapters around its application protocol. Phase five extends
that transport-neutral device layer for observation, actions, cooperation,
and bounded artifact transfer instead of adding Vox.

At plan creation `teamy-rust-cli` and `facet` contain unrelated uncommitted
changes, so they are design references rather than Poche path dependencies.

References: `G:\Programming\Repos\teamy-rust-cli\`,
`G:\Programming\Repos\facet\figue\`, `crates/poche-veilid/`, and
`crates/poche-web-spike/src/gateway.rs`.

## Confirmed constraints

- **C1 — one semantic player port:** GUI controls, CLI devices, policies, and
  puppets consume exact-recipient observations and advertised actions, then
  submit ordinary commands. Renderer coordinates never become game commands.
- **C2 — one key per independently operating device:** Native GUI, browser,
  CLI, and bot processes use distinct certified device keys. No process copies
  another device's private key merely to appear as the same player.
- **C3 — player continuity:** Devices may belong to the same player root and
  see actions through shared committed history, but each remains independently
  attributable and revocable.
- **C4 — no extra vote weight:** Multiple devices for one player contribute at
  most one player vote. Replicated devices share/synchronize a durable player
  vote lock or refuse unsafe concurrent voting.
- **C5 — clients are not the authority:** A device authentically proposes or
  signs within its capabilities. Host-authoritative reduction or replicated
  consensus still accepts and orders effects.
- **C6 — exact-recipient privacy:** A device sees only its authorized
  projection. A capture shows what the target rendering device could see at a
  named revision; it never asks the game authority for an omniscient view.
- **C7 — same-player capture authorization:** A capture requester and target
  must present valid, non-revoked certificates under the same player root and
  room/session binding, plus request/provide capabilities. Cross-player capture
  requires a future explicit consent/grant design and is out of scope.
- **C8 — capture is cooperation, not game history:** Capture request, consent,
  progress, denial, and artifact transfer are signed exact-target device
  messages. They do not change the canonical room revision or consensus log.
  Public diagnostics may record redacted local audit IDs only.
- **C9 — one common capture pipeline:** Renderers implement a narrow provider
  that yields raw image bytes and structural metadata. Shared code owns IDs,
  media validation, captions, hashes, manifests, chunking, persistence,
  cleanup, and publication. Bevy and browser code do not independently invent
  file layouts or evidence schemas.
- **C10 — explicit capture capability and consent:** A device advertises
  `unavailable`, `automatic-test`, `user-confirmed`, or another versioned
  policy. A normal browser page cannot silently screenshot itself; browser
  capture requires a harness/browser provider or explicit user-approved
  browser capability. Denial is a valid stable result.
- **C11 — bounded private artifacts:** Screenshot bytes are normally larger
  than Veilid's safe AppCall payload. Transfer uses a separately bounded,
  encrypted, hash-verified stream/chunk or blob capability selected by G5; raw
  images never enter ordinary command/reply frames.
- **C12 — offline default:** Headless, browser, native, capture, and puppet CI
  run the same protocol over in-process/loopback transports. Public Veilid
  remains an explicit opt-in integration proof.
- **C13 — preserve CLI semantics:** Figue migration retains secret-redacted
  errors, text/JSON/NDJSON, redirected stdout behavior, cancellation, logging,
  and current command vocabulary unless a versioned change is documented.
- **C14 — reproducible dependency boundary:** Do not path-depend on dirty
  sibling repositories. Use exact published versions or immutable commits,
  update `Cargo.lock`, and retain MPL-2.0 plus dependency-license evidence.
- **C15 — one release executable and one harness invocation:** Installed
  graphical, CLI/device-agent, and puppet entry points use `poche.exe`. One
  puppet invocation may own many explicit test roots/devices but must retain
  distinct player/device IDs and exact-recipient projections.
- **C16 — Bevy is a leaf adapter:** Bevy maps typed projections to visuals and
  human input back to advertised actions. Its ECS, camera, screenshot API, and
  animation state are not canonical game or device authority.
- **C17 — non-interactive native automation:** Puppet and capture-worker native
  surfaces are hidden or offscreen by default. Only an explicit debug/show
  option may open them; ordinary human `poche desktop` launches remain visible.

## Device and capture architecture

| Actor | Identity and connection | Game actions | Capture behavior |
| --- | --- | --- | --- |
| Native graphical client | Its own certified device using configured Poche transport | Exact projection → advertised action → signed command | Bevy provider returns render-target bytes, scene/camera/viewport metadata to common pipeline |
| Browser client | Browser-local or disclosed gateway-custodied device | Existing HTTP/SSE or future supported direct adapter | Advertises unavailable, explicit-consent capture, or harness/CDP provider; returns browser screenshot plus DOM/a11y/layout metadata |
| CLI invocation | Loads its own protected device profile and room membership | Same device API as GUI; other devices observe committed result | May request capture from a same-player target and receive/save through common artifact pipeline |
| Persistent computer player | Its own certified device and policy | Same exact observation/action API; no renderer dependency | Usually requester-only or unavailable provider |
| Puppet harness | Owns explicit test roots and multiple distinct devices | Fans out player devices and drives only advertised actions | Requests captures from enrolled rendering devices, verifies replies, builds one run manifest/contact sheet; native workers stay hidden/offscreen unless explicitly shown |
| Headless simulation | Explicit in-process test devices | Same reducers and typed device client without network/rendering | Emits semantic snapshot only; no fake pixels |

The graphical process is not being puppeted as a privileged local object. It is
one room device that renders its authorized state and optionally provides a
capture capability. A CLI request such as:

```text
poche device capture request --room <ROOM> --target <DEVICE> --label bidding
```

is signed by the CLI device, delivered through the same configured device
transport, authorized against both device certificates and capture policy, and
answered with a hash-bound artifact descriptor plus bounded private content.
The requesting process saves the result through shared artifact code.

## Scope

### In scope

- One `poche.exe` graphical, CLI, agent, capture, and puppet dispatch boundary
  using Teamy/Figue conventions.
- A reusable transport-neutral device client for observe/actions/invoke/wait,
  usable over in-process, loopback/gateway, and native Veilid adapters.
- Protected player/device profiles, certification, membership, selection,
  revocation, exact-recipient observations, and replicated vote-lock safety.
- Signed same-player device capability advertisement and capture requests.
- Common capture payload, artifact transfer, validation, caption, manifest,
  persistence, cleanup, and contact-sheet code.
- Bevy and real-browser capture providers under that common contract.
- One puppet harness invocation that orchestrates multiple player devices and
  headless/web/native surfaces.
- Full-round UI/CLI/bot parity and capture/privacy/failure acceptance.
- Documentation, decision records, Pages/CI artifacts, audit, commit, and push.

### Out of scope

- Vox, named pipes, local-instance descriptors/discovery, or a privileged
  resident-window remote-control protocol.
- Training/evaluating a Burn reinforcement-learning policy in this phase.
- A new consensus algorithm or stronger Byzantine-fault-tolerance claim.
- Making Veilid 0.5.7 browser-only networking work through an unrelated
  upstream patch.
- Cross-player capture without an explicit future grant/consent protocol.
- Silent screenshots from ordinary production browser pages.
- A generic tabletop engine, physics simulation, or renderer redesign.
- Exporting/copying player-root or device private keys for convenience.
- Pixel-driven gameplay bots.
- Treating screenshots as formal proof or running every formal backend for
  every page/capture request.

## Design gates that must close before downstream implementation

| Gate | Status | Required decision | Acceptance consequence |
| --- | --- | --- | --- |
| G1 | Closed | **Figue/dependency alignment:** Exact `figue = 5.0.0-rc.5` shares Facet `0.50.0-rc.5`, compiles with Rust 1.96 and the existing Phon/Weavy pins, and introduces no Vox or sibling path dependency. | Lockfile, offline workspace build, reflected parser suite, duplicate tree, and metadata license/path audit passed. |
| G2 | Closed | **One executable:** No arguments and `desktop` launch the GUI; explicit CLI, `agent`, `device capture`, and `puppet` commands remain console-subsystem terminal modes with preserved structured output and diagnostics. | ADR 0010; parser/contract evidence exists, while graphical dispatch acceptance remains Task 1.3. |
| G3 | Closed | **Common device client and profile:** Profiles bind one root-certified device; observe/actions/invoke/wait/cooperate cross transport adapters; vote capability/lock remains one-player authority. | ADR 0010; implementation and cross-adapter evidence remain phases 2-3. |
| G4 | Closed | **Capture authorization/schema:** Requests are signed, exact-target, same-player, room/epoch/revision/expiry/privacy/capability bound; providers retain consent and may sign a denial. | ADR 0010 plus protocol/session positive and fail-closed authorization tests. |
| G5 | Closed | **Artifact transfer:** Use content-addressed resumable private chunks, maximum 24 KiB each, 64 MiB per artifact, never a whole PNG in an ordinary command/AppCall reply. | ADR 0010 and descriptor bounds; interruption/resume/cleanup and public-Veilid qualification remain Task 2.4. |
| G6 | Closed | **Common artifact contract:** Providers return raw representations; `poche-capture` alone validates, captions, hashes, manifests, persists under ignored `target/poche-puppets`, cleans, and publishes via CI/Pages. | ADR 0010; shared pipeline/provider acceptance remains phases 2 and 4. |
| G7 | Closed | **Browser provider and puppet process topology:** Use a hidden CDP/Edge-or-Chrome harness with a fresh temporary profile and isolated participant contexts; only its separately certified BrowserLocal provider advertises `HarnessOnly` capture, while ordinary production pages advertise no silent provider. Provider keys remain in process memory rather than arguments/logs. | Complete ordinary-control game, signed same-player request/response, encrypted transfer, console/network/DOM/a11y/layout evidence, responsive collision audit, and temporary-profile cleanup passed. |

## Source and implementation references

### Poche

- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`
- `crates/poche-cli/`
- `crates/poche-protocol/`
- `crates/poche-session/`
- `crates/poche-runtime/`
- `crates/poche-ui/src/live.rs`
- `crates/poche-native-ui/src/lib.rs`
- `crates/poche-web-spike/src/game.rs`
- `crates/poche-web-spike/src/gateway.rs`
- `crates/poche-veilid/`
- `docs/decisions/0007-player-device-and-replicated-log.md`
- `docs/veilid-command-transport.md`
- `docs/native-spatial-ui.md`
- `docs/semantic-html-tabletop.md`

### Teamy/Facet references

- `G:\Programming\Repos\teamy-rust-cli\AGENTS.md`: every subcommand has a
  directory module and discoverable `*_cli.rs` implementation.
- `G:\Programming\Repos\teamy-rust-cli\src\cli\`: Figue commands,
  structured output, and module layout.
- `G:\Programming\Repos\teamy-rust-cli\tests\`: round-trip fuzz shape.
- `G:\Programming\Repos\facet\figue\`: reflected CLI/configuration only.

### SFM references

- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\cli\sfm-propagate-changes\src\cli\puppet.rs`
- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\src\gametest\java\ca\teamdman\sfm\gametest\puppet\`
- `D:\Repos\Minecraft\SFM\repos2\1.19.2\platform\minecraft\build\sfm-toolchain\artifacts\game-test-preview\preview-manifest.json`

## Target acceptance matrix

| Target/mode | Support status | Required validation | Evidence |
| --- | --- | --- | --- |
| Windows native `poche.exe` device | Required primary | Real live room, human controls, exact projection, cross-device capture provider, clean shutdown | Pending |
| CLI/agent device | Required primary | Protected profile, room membership, observe/actions/invoke/wait, capture request/save, revocation | Pending |
| Headless in-process devices | Required | Deterministic full round, multiple players/devices, transcript/projection/action hashes | Qualified: revision 159, 159 actions, eight witnesses, 138 public events across typed and NDJSON loopback |
| Semantic HTML two-tab devices | Required | Real browser round, SSE/POST, responsive viewports, capability-aware capture, DOM/a11y/layout evidence | Qualified harness: three isolated contexts, complete UI game, six four-representation captures; shared external authority remains Phase 5 |
| Host-authoritative Veilid v1 | Existing supported mode | Offline codec/reducer checks and guarded public full lifecycle including device cooperation qualification | Pending requalification |
| Experimental replicated devices | Existing experimental mode | Certificate/revocation, player vote deduplication, vote-lock refusal/evidence, no stronger BFT claim | Pending |
| Linux/macOS CLI and device protocol | Intended after Windows slice; not advertised before proof | Compile and transport-adapter tests where CI is available | Pending gate |
| Browser-only Veilid 0.5.7 | Explicitly unsupported | Existing feasibility documentation remains accurate; gateway/harness path is honestly labeled | Existing boundary |

## Execution order

```text
device/capture/artifact design gates
    → one-executable Figue dispatch
    → common device client and profiles
    → signed cooperation and artifact transfer
    → live Bevy device and CLI/agent devices
    → single fan-out puppet harness
    → Bevy and browser capture providers
    → cross-surface full-round/capture acceptance
    → documentation, Pages artifacts, audit, commit, push
```

## Phase 1 — freeze dependency, executable, and authority contracts

### [x] 1.1 Prove the Figue/Facet dependency set and absence of Vox

**Completion notes:** Completed 2026-08-12. Added exact published
`figue = 5.0.0-rc.5`, sharing Facet `0.50.0-rc.5`, with no sibling path or Vox
dependency. The reflected Poche schema and all CLI contract tests pass; the
locked workspace compiles offline on Rust 1.96. `cargo tree -p figue -d`
records its expected `smallvec`, `supports-color`, and `syn` version pairs.
`cargo-deny` is unavailable, so the repository metadata audit was used and
reported 1,024 packages, zero missing licenses, and zero external local paths;
Figue is MIT OR Apache-2.0. The source/dependency search reported zero Vox,
named-pipe, or local-instance matches under Cargo/crate sources.

**Work:**

- Test exact published/immutable Facet and Figue versions against Rust 1.96 and
  Poche's existing exact Phon/Weavy pins.
- Record Teamy CLI command, output, logging, cancellation, Windows-resource,
  and Arbitrary patterns to adopt; preserve Poche NDJSON.
- Confirm the planned device/capture protocol builds on existing Poche
  application transports and does not need Vox or another local IPC crate.
- Inspect duplicate dependencies and licenses. Never add a path dependency to
  either dirty sibling worktree.

**Validation:**

```powershell
cargo tree -d
cargo check --workspace --locked --offline
cargo test --locked -p poche-cli --offline
cargo deny check licenses
```

If `cargo-deny` is unavailable, record that absence and run the repository's
actual license audit instead of claiming the illustrative command passed.

**Completion criteria:** One exact dependency graph compiles offline, CLI
prototypes pass, licensing is recorded, and neither Vox nor a sibling path is
introduced.

### [x] 1.2 Freeze device cooperation and capture in an ADR

**Completion notes:** Completed 2026-08-12. ADR 0010 closes G2-G6 and names
the identity, key, certificate, capability, signature, transport, revision,
privacy, consent, transfer, pipeline, retention, and evidence owner at every
boundary. The implementation adds separate capture request/response/cancel
domains, exact-target descriptors, `RequestCapture`/`ProvideCapture`, and pure
same-player session authorization. Full protocol/session suites pass including
cross-player, stale, revoked, malformed, oversize, and substitution-sensitive
cases. Existing PNGs measured about 58-596 KiB, validating the separate
24-KiB-chunk transfer decision against the 30,000-byte command-frame bound.

**Work:**

- Add `docs/decisions/0010-device-orchestration-and-capture.md` recording the
  architecture table, same-player authorization, capture non-authority,
  renderer/provider boundary, common artifact ownership, and rejected local
  IPC/window-control alternative.
- Close G2-G6, including certificate/capability semantics, player vote lock,
  browser consent, transfer, persistence, and evidence qualification.
- State how CLI actions become visible to graphical devices through shared
  room history rather than process control.

**Validation:**

```powershell
rg -n "device cooperation|same player|capture|artifact|vote lock|Vox" docs\decisions\0010-device-orchestration-and-capture.md
cargo test --locked -p poche-protocol -p poche-session --offline replicated
```

**Completion criteria:** A fresh implementer can identify the key, certificate,
transport, authorization, state revision, privacy scope, and persistence owner
for every game and capture message.

### [x] 1.3 Migrate the public `poche.exe` command surface to Figue

**Completion notes:** Completed 2026-08-12. Existing room/game/chat/governance/
spectator/transcript/identity commands now parse through Facet/Figue with
generated ANSI-clean help and completions, redacted failures, unchanged
text/JSON/NDJSON behavior, logging, cancellation, and transcript execution.
No arguments maps to `desktop`; reflected `agent`, `device capture`, and
`puppet` groups exist in Teamy-style modules. The CLI contract and 10,000-case
arbitrary token corpus pass. `poche.exe desktop` now calls the Bevy library
through typed options rather than reparsing or launching a child, while the
standalone development binary remains compatible. A real unified-executable
run produced a 53,941-byte PNG and acceptance JSON with 52 cards, 124 sampled
frames, exact scene hash, private-face counts, and clean exit. Visual inspection
confirmed the requested debug-overlay surface. The run also exposed a Vulkan
validation-layer warning in this host configuration; it did not prevent the
artifact, report, or clean exit and remains renderer qualification evidence,
not a hidden success claim.

**Work:**

- Refactor `poche-cli` to Teamy-style Facet/Figue command types. Give every
  subcommand its own directory and discoverable `*_cli.rs` file.
- Preserve current room/game/chat/governance/spectator/transcript/identity
  spelling, redacted failures, cancellation, logging, and text/JSON/NDJSON.
- Add `desktop`, `agent`, `device`, `device capture`, and `puppet` groups.
- Make one `poche` binary dispatch Bevy graphical and terminal modes. Keep
  renderer crates as libraries/test fixtures, not separately installed
  authoritative products.
- Add help/schema snapshots and Arbitrary parse/render round trips.

**Validation:**

```powershell
cargo test --locked -p poche-cli --offline
cargo clippy --locked -p poche-cli --all-targets --offline -- -D warnings
cargo run --locked -p poche-cli --offline -- --help
```

**Completion criteria:** Existing CLI behavior is preserved through Figue and
one built `poche.exe` exposes graphical, device-agent, capture, and puppet
entry points with correct Windows/redirected-output behavior.

## Phase 2 — implement the shared device and capture protocols

### [x] 2.1 Extract a reusable exact-recipient device client

**Completion notes:** Completed 2026-08-12. Added `poche-player-client` with validated
public profile/certificate bindings, opaque protected-key handles and signing
port, exact-recipient observations, sorted advertised actions, revision and
projection-hash-bound invocation, wait progress checks, cooperation requests,
stable redacted errors, and no renderer dependency. Tests prove an
unadvertised action cannot reach the adapter and that a non-progressing wait
fails closed. Added a cloneable canonical-NDJSON loopback adapter over the
actual `InProcessAuthority` reducer: observations have stable semantic hashes
despite changing delivery envelopes, advertised invocations enter the real
command path, and queued recipient frames are drained. The in-process
transport now permits multiple independently routed devices for one player,
fans projections out to all of them, and emits a semantic disconnect only when
the player's last device disconnects. Focused tests prove a desktop-shaped and
CLI-shaped device for the same player observe the same committed revision.
ADR 0010 and protocol/session types carry the cooperation half. Gateway and
native Veilid adapter connections remain explicitly assigned to Tasks 3.1,
3.2, and 5.1; the plan moved those process integrations after the capture wire
contract rather than letting each adapter invent incompatible cooperation.
The external action half is now explicit as well: a canonical
`DeviceActionWire` binds the root-signed device certificate, propose
capability, room/session, command ID, expected revision/projection hash,
advertised action ID, and exact typed payload under a distinct device
signature. `DeviceActionRequest::sign` crosses only the opaque protected-key
port, and the runtime verifier checks both root and device Ed25519 signatures
before recovering the transport-neutral request. Mutation and wrong-root tests
fail closed. Exact-recipient reads and waits now have an independent signed
request domain as well: snapshot/wait mode, after-revision, unique request ID,
room/session, player/device certificate, and private-projection capability are
bound before a projection can be requested. Runtime verification checks both
signature layers and rejects a mutated wait cursor. Gateway/Veilid carriage
remains Phase 3/5 work. The first process-external carrier now exists as an
optional synchronous HTTP adapter: it permits HTTPS generally and plaintext
HTTP only on explicit loopback origins, signs every observation/wait/action,
bounds responses, and maps wire failures to the shared redacted error set. A
reducer-backed `CertifiedDeviceRoom` verifies both certificate and device
signatures, enrolls exact devices, and retains bounded exact-result caches so
retries cannot be reinterpreted against newer private state. Pending rooms use
epoch zero only for signed snapshot/create bootstrap and `RoomCreated`
atomically advances the authoritative session and first membership to the
one-based epoch required by device certificates. The prior capture-local epoch
offset was removed, the reducer rejects zero/future member epochs, and the
golden transcript was deliberately regenerated.

**Work:**

- Add a reusable `poche-player-client` that owns a protected device profile,
  membership locator, exact-recipient observation, advertised action set,
  expected revision, idempotency/retry state, and transport adapter.
- Support the in-process/loopback foundation without renderer dependencies;
  use this client from the gateway/native Veilid process integrations in
  Tasks 3.1, 3.2, and 5.1.
- Define observe/actions/invoke/wait semantics and stable typed errors.
- Keep device key material non-cloneable, non-serializable outside protected
  storage, redacted, and absent from diagnostics.

**Validation:**

```powershell
cargo test --locked -p poche-player-client -p poche-protocol -p poche-session --offline
cargo clippy --locked -p poche-player-client --all-targets --offline -- -D warnings
```

**Completion criteria:** One client API performs real reducer-backed behavior
over the canonical non-rendering loopback adapter, exposes only exact-recipient
state, and is the mandatory API for later gateway/native Veilid adapters.

### [x] 2.2 Define signed device cooperation and capture schemas

**Completion notes:** Completed 2026-08-12. The contract now has separately
domain-signed provider advertisements, exact-target requests, consent
decisions, monotonic progress, artifact offers, terminal completion receipts,
and cancellations. Requests bind a replay nonce, expiry, exact provider kind,
privacy scope, requested representations, room/membership/current revision,
player root, and both certified device IDs. Pure authorization enforces same-
player active/non-revoked request/provide capabilities, advertisement bounds,
artifact kind/revision/size integrity, and completion receipts matching the
offered transfer hashes without changing `SessionState`. A bounded replay
window rejects exact duplicates, same-ID conflicts, and overflow. Distinct
canonical domains, a fixed Ed25519 request hash/signature golden vector, the
explicit replay corpus, cross-player/stale/revoked negatives, lifecycle tests,
and Clippy all pass.

**Work:**

- Add versioned capability advertisement, `CaptureRequest`, progress/consent,
  denial, artifact offer, completion, and cancellation messages.
- Bind every message to request ID, player root, source/target device, device
  certificates, room/session, requested and captured authority revision,
  expiry/replay nonce, surface kind, requested semantic attachments, and
  canonical signing/encryption domains.
- Enforce same-player, active/non-revoked certificates and exact request/provide
  capabilities without changing room revision/history.
- Define stable denial categories including unavailable provider, consent
  required/denied, stale target, wrong room/player, revoked device, expiry,
  unsupported format, and busy.

**Validation:**

```powershell
cargo test --locked -p poche-protocol --offline capture
cargo test --locked -p poche-session --offline device_cooperation
```

**Completion criteria:** Golden vectors and positive/negative tests prove
signed exact-target same-player capture authorization and replay resistance.

### [x] 2.3 Implement the common capture artifact pipeline

**Completion notes:** Completed 2026-08-12. Added renderer-neutral
`poche-capture`. Providers return raw image/HTML/JSON bytes plus exact surface,
framebuffer/scale, optional integer camera, revision, projection/scene hashes,
and evidence qualification; they never choose persistence paths. The one
pipeline validates safe stable IDs/media/types/bounds, checks optional source
hashes and aggregate size, normalizes JSON and PNG, adds a deterministic
48-pixel caption band outside (without changing) the source viewport, records
source/output geometry and content hashes, scans caller-supplied private
markers before and after normalization, and atomically renames a complete
figure directory containing artifacts and `manifest.json`. The manifest is
also the renderer-neutral contact-sheet input. Synthetic native Bevy and
browser payloads yield identical normalized entry semantics. Tests cover
invalid images, traversal-shaped IDs, hash mismatch, duplicate IDs/publication,
caption geometry, private markers, cancellation, atomic cleanup, and manifest
reread; crate tests and Clippy pass offline.

**Work:**

- Add `poche-capture` with a renderer-neutral provider result containing raw
  image bytes, media type, surface metadata, viewport/framebuffer/DPI, optional
  camera, semantic projection/scene hash, DOM/a11y/layout attachments, and
  qualification.
- Centralize stable capture/figure IDs, safe paths, normalized encoding,
  captions outside the source viewport, dimensions, hashes, manifest entries,
  private-data scan, persistence, cleanup, and contact-sheet inputs.
- Make providers return data, never choose final filenames or independently
  save evidence.
- Add invalid image, unsafe path, hash mismatch, duplicate ID, caption geometry,
  privacy, cancellation, and atomic-write tests.

**Validation:**

```powershell
cargo test --locked -p poche-capture --offline
cargo clippy --locked -p poche-capture --all-targets --offline -- -D warnings
```

**Completion criteria:** Synthetic Bevy/browser provider payloads pass through
one validator/persistence path and yield identical manifest semantics.

### [x] 2.4 Implement bounded private artifact transfer

**Completion notes:** Completed 2026-08-12 for the transport-neutral contract
and deterministic loopback evidence. `poche-capture` derives a bounded
descriptor (measured 24-KiB plaintext chunks, 64-MiB artifact ceiling), then
encrypts each chunk with XChaCha20-Poly1305 under a protected zero-on-drop
per-transfer key. AEAD associated data binds the signed request hash (which
contains exact player/source/target device IDs) plus transfer ID, complete
content hash/length, chunk geometry, and index; deterministic per-transfer/
index nonces are safe under the required unique transfer key. Sender credit
requires acknowledgements and receiver state enforces order, encrypted-chunk
deduplication/conflict rejection, expiry, cancellation, bounded resume, and
verification of complete length/hash before returning publishable bytes.
Cancellation erases buffered content and keys/partial bytes never enter files.
A 3+ MiB encrypted transfer test proves backpressure, deduplication, resume,
premature-finish refusal, exact recovery, and publication through the common
pipeline only after verification. Wrong keys, tampering, expiry, cancellation,
and partial cleanup fail closed. Gateway/native Veilid transport qualification
remains explicitly owned by Tasks 5.1 and 5.3 rather than being inferred from
loopback evidence.

**Work:**

- Close G5 using measured screenshot sizes and existing transport limits.
- Transfer a signed/hash-bound descriptor and encrypted content without placing
  PNG bytes in ordinary command/reply frames.
- Enforce total/chunk/message limits, credit/backpressure, expiry, deduplication,
  cancellation, resume or explicit restart semantics, verification-before-
  publish, and cleanup of partial artifacts.
- Implement in-process/loopback evidence first; qualify gateway/native Veilid
  adapters honestly and retain explicit public-network gating.

**Validation:**

```powershell
cargo test --locked -p poche-capture -p poche-veilid --offline transfer
cargo test --locked -p poche-protocol --offline capture_transfer
```

**Completion criteria:** A multi-megabyte synthetic capture transfers between
certified devices and survives/rejects interruption, reordering, duplication,
oversize, expiry, tampering, and cancellation as specified.

## Phase 3 — connect graphical, CLI, and policy devices

### [~] 3.1 Turn Bevy into a live Poche device and capture provider

**Completion notes:** In progress. `poche-native-ui` now converts an exact
`DeviceObservation` into the shared presentation/spatial scene, admits only
advertised game actions, and maps a spatial click/drag result back to the
opaque advertised action ID. `NativeCaptureProvider` implements nonblocking
consent/queue/cancel/result state and returns real Bevy render-target bytes,
camera/viewport/revision/projection/scene metadata to `poche-capture`; it never
selects artifact paths. `poche desktop --capture-artifact-root ...` exercises
the provider and shared persistence pipeline through the unified executable.
Automation capture surfaces are truly windowless by default: Bevy creates no
primary window, disables Winit, renders the camera into a GPU image target, and
reads the PNG back from that image. Headless workers compile render pipelines
synchronously, wait for rendered application frames rather than assuming a
wall-clock delay proves readiness, and reject effectively uniform readbacks as
integrity failures. Normal `poche desktop` launch remains windowed. Unit,
Clippy, and GPU-gated windowless artifact acceptance pass.
The native update loop now has a bounded, nonblocking `NativeLiveDevice` bridge:
its worker owns an ordinary configured device transport, spatial input resolves
against the exact rendered observation, and accepted input returns through the
normal authority result and refreshed observation. A full room-lifecycle test
proves this path commits a native drag/play through the real reducer and that an
independently certified sibling device for the same player observes the next
revision and public-history entry. The deterministic action source now also
models player turns plus seeded chance/environment turns without exposing
private hands to the environment device. The reducer-backed loopback adapter
now also has an exact-target non-authoritative provider registry: it verifies
real Ed25519 root-signed device certificates plus request/advertisement/response
signatures, same-root capabilities, one-based cooperation membership epoch,
room/revision/expiry/provider bounds, and bounded replay before invoking a
handler that has no reducer argument. A focused accepted-response test proves
duplicate replay never re-enters the provider and capture cooperation changes
neither room revision nor public history. The native puppet now composes that
bridge with the real windowless Bevy provider: Alice's root-signed policy
device requests its root-signed native sibling, which returns a signed
hash-bound descriptor while the provider bytes cross acknowledged encrypted
24-KiB chunks and only the requester-side common pipeline persists them.
Remaining before completion: supply and manually exercise a persistent/live
transport profile and exercise human input in that profile. Visual inspection
also retains an existing
native-render gap: current/earlier unified screenshots show Slug and debug
geometry but omit solid PBR meshes, so capture structure is accepted but visual
completeness is not yet claimed.

**Work:**

- Replace the immutable-only Bevy entry path with `poche-player-client` while
  retaining deterministic fixture mode.
- Render exact-recipient projections and resolve human click/drag/keyboard
  input into the same advertised actions as other clients.
- Implement the common capture-provider trait using the real render target and
  typed scene/camera/viewport metadata; return payloads to `poche-capture`
  rather than saving independently.
- Process signed capture requests according to device policy without blocking
  the Bevy update/render loop; publish capture replies through the configured
  device transport.

**Validation:**

```powershell
cargo test --locked -p poche-native-ui -p poche-player-client -p poche-capture --offline
cargo clippy --locked -p poche-native-ui --all-targets --offline -- -D warnings
cargo run --locked -p poche-cli --offline -- desktop --automation-profile target\poche-puppets\profile.json
```

The final command is a manual live prerequisite; update its exact Figue syntax
and never mark it passed without observing the window and device enrollment.

**Completion criteria:** A real native client joins a room as its own device,
human actions are visible elsewhere, and a certified sibling device obtains a
validated captioned capture without Bevy owning file layout or game semantics.

### [ ] 3.2 Connect CLI commands and deterministic agents as real devices

**Completion notes:** In progress. The shared player-device client now resolves
typed convenience payloads only by finding exactly one matching action in the
current advertised action set; unavailable actions fail and duplicate semantic
matches are a protocol violation. It also owns reusable `first-legal` and
seeded-random policies with an explicit player-game-only scope that excludes
pause, chat, leave, close, and other human controls. The full certified
eight-device puppet now uses the seeded policy seam and still completes 159
committed revisions with every device converged. The one-executable Figue
schema validates `first-legal` and `seeded-random:<seed>` and now exposes a
typed `game bid` convenience action alongside typed card play. The optional
HTTP device adapter and the Axum `/device/v1/observe` plus
`/device/v1/invoke` endpoints now exercise a real process/socket boundary. A
real ephemeral-server test signs a pending observation, invokes advertised
room creation, crosses into epoch one, observes the lobby, performs a
strictly-later wait, and proves exact retry behavior. Remaining: connect this
transport to the public Figue execution surface; implement
chat/governance/spectator and capture commands; and run the persistent agent
loop against graphical peers. Protected persistence is now concrete:
`poche-player-client` stores root/device Ed25519 secrets only in the platform
credential vault (Windows Credential Manager on the acceptance platform),
persists authenticated root/certificate metadata separately, and implements
the opaque `DeviceSigner` port without key export. Root creation and device
enrollment are collision-safe, public files use atomic no-clobber publication,
and mismatched/corrupt secrets or certificates fail closed. `poche identity
create|show` and `poche device create|list|show` execute through Figue and emit
only redacted public summaries. Memory-vault tests cover collision,
authentication, signing, traversal-shaped labels, secret mismatch, and public
file scanning; a Windows-only vault probe stores, reads, and deletes its test
credential.
The public Figue layer now also executes certified HTTP behavior instead of
returning parse-only receipts: global `--endpoint` and `--profile` select a
transport/profile, `room host|show|ready|unready|countdown|abort|pause|resume|
leave|close`, and `game observe|actions|bid|play-card` resolve current
advertised controls and submit signed actions. `room host` now requires the
explicit room ID. Joining uses a device-signed discovery request that binds the
exact bearer invite; an unknown principal without that proof receives no join
action, invites are invalid on waits/pending bootstrap, and the transport
clears the proof after committed redemption. HTTP request IDs now carry a
random per-transport namespace so a restarted process cannot collide with an
earlier exact-retry cache. A real Axum test covers hidden-without-proof,
invite-bound discovery, committed join, and the joined exact-recipient lobby.
Persistent agent execution, chat/governance/spectator/capture CLI commands,
and external graphical convergence remain.

**Work:**

- Implement room/game/chat/governance/spectator observe/actions/invoke/wait via
  `poche-player-client`; keep transcript tools offline-capable.
- Add protected profile create/list/show, root-certified device enrollment,
  membership selection, revocation, and custody disclosure without printing
  secrets.
- Add `first-legal` and seeded-random policies; no Burn/learned policy yet.
- Add `device capture providers|request|status|receive` commands that use the
  same cooperation and artifact pipeline as puppets.
- Resolve convenience commands against current advertised actions; never
  invent a parallel payload.

**Validation:**

```powershell
cargo test --locked -p poche-cli -p poche-player-client -p poche-capture --offline
target\debug\poche.exe game observe --output json
target\debug\poche.exe game actions --output json
target\debug\poche.exe device capture providers --output json
```

Update argument ordering to the final Figue schema before completion.

**Completion criteria:** CLI/agent devices act independently, other graphical
devices observe the results, and a CLI device requests/receives/saves a sibling
device capture through shared code.

### [ ] 3.3 Prove GUI, CLI, and policy parity plus vote-lock safety

**Completion notes:** In progress. `PlayerDeviceClient::prepare` is now the
pure canonical request seam beneath invocation, and `prepare_payload` first
resolves a typed convenience payload to exactly one advertised opaque action.
A parity test proves action-ID and convenience entry points produce identical
structured requests and serialized bytes for the same command ID; another
proves duplicate semantic controls fail closed instead of choosing one by
order. Remaining: exercise the actual GUI and persistent policy adapters
against this seam, compare reducer dispositions/successor hashes and stable
denials, and complete the replicated per-player vote-lock cases.

**Work:**

- Run fixtures where GUI controls, CLI convenience resolution, direct action
  references, and deterministic policies select the same action from the same
  projection/revision.
- Compare canonical command bytes, disposition, events, successor state,
  projection/action hashes, and stable denial behavior.
- Cover stale/unavailable actions, disconnect/reconnect, wrong/revoked device,
  duplicate command, and attempted private observation.
- Synchronize the replicated player vote lock across devices or reject unsafe
  concurrent voting; retain controlled signed equivocation evidence.

**Validation:**

```powershell
cargo test --locked -p poche-player-client -p poche-ui -p poche-runtime --offline parity
cargo test --locked -p poche-session --offline replicated
```

**Completion criteria:** All legitimate entry points converge at one player
port, multiple devices add no vote weight, and no renderer/automation path has
privileged mutation authority.

## Phase 4 — build the single fan-out puppet and capture providers

### [x] 4.1 Add one multi-player, multi-device puppet harness

**Completion notes:** Completed 2026-08-12. Added `poche-puppet` and wired its
static `list`, `show`, `run`, and `artifacts path` catalog into the one Figue
`poche.exe`. The `two-player-full-round` scenario creates three player roots
(Alice, Bob, and spectator), seven distinct certified devices (two sibling
devices for each player plus spectator, authority-clock, and game-environment
devices), actual memberships/seats/readiness, and drives room creation through
terminal scoring exclusively by selecting opaque advertised actions from each
exact-recipient `PlayerDeviceClient` observation. Every completed semantic step
retains its pending observation revision/hash, action identity, committed
revision, and a same-revision witness from all eight devices. Bounded action and
whole-run deadlines, maximum semantic steps, caller cancellation, temporary
artifact cleanup, deterministic seeds, and explicit typed/canonical-NDJSON
loopback transports are implemented without sleeps. Atomic ignored evidence
contains `run.json`, `steps.ndjson`, and a hash/length-qualified manifest with a
clear headless-only evidence boundary. Tests prove cancellation publishes no
partial run, manifests match their bytes, and typed/NDJSON transports produce
identical deterministic actions, scores, history hash, and terminal revision.
A real unified-executable NDJSON run reached `post_game` at revision 159 after
159 actions across eight devices, with 138 public-history events and inspectable
artifacts. This completes the headless harness, not native/browser capture or
external transport qualification assigned to Tasks 4.2-5.3.

**Work:**

- Add `poche-puppet` with declarative scenarios and pending/complete semantic
  actions, per-action deadlines, whole-run watchdog, cancellation, cleanup,
  deterministic seeds, and transport selection.
- One invocation creates explicit test player roots, certified devices,
  memberships, and viewer roles; every device retains a distinct ID,
  projection, action stream, and local diagnostics.
- Drive only `poche-player-client` actions and signed cooperation messages.
  Keep presentation requests separate from game commands.
- Add Figue `puppet list|show|run|artifacts`; static catalog commands never
  launch a renderer.

**Validation:**

```powershell
cargo test --locked -p poche-puppet --offline
target\debug\poche.exe --output json puppet list
target\debug\poche.exe --output json puppet run two-player-full-round --surface headless --transport loopback-ndjson --seed 1
```

**Completion criteria:** One deterministic invocation completes a full round
with multiple players/devices using only exact observations and advertised
actions, without sleeps or a second semantic engine.

### [ ] 4.2 Drive native evidence through cross-device capture requests

**Completion notes:** Cross-device native evidence now spans six semantic
checkpoints. `poche.exe puppet run ... --surface native` plays the same
159-action certified full game while Alice's agent signs revision-bound
requests to Alice's separately certified native sibling at bidding, card
selection, trick in progress, resolved trick, scoring, and terminal states.
The runtime verifies root/device certificates, provider advertisement,
request, response, membership/revision, expiry, capability, and replay policy
before/after invoking a handler with no reducer access. Bevy creates no OS
window or Winit event loop, renders each exact revision to its 1280x800 GPU
image, synchronously readies render pipelines, waits for rendered frames, and
fails closed on an effectively uniform readback. Each response exposes only
hash-bound descriptors; plaintext stays provider-owned until the requester
acknowledges an encrypted bounded transfer. The requester alone reconstructs
bytes and calls `poche-capture`, which writes each captioned PNG/manifest inside
the atomic puppet run. The top-level manifest hashes all semantic and
graphical files with portable paths. A GPU-gated integration test passes at
revision 159 with seven converged devices, six ordered windowless captures,
and no visible automation window. `--show-window` is an explicit native-only
debug escape hatch. Remaining: decide and exercise main-menu/lobby and
disconnect/reconnect capture boundaries; the native game adapter currently
requires a seated game projection and therefore cannot honestly claim those
states.

**Work:**

- Launch/enroll a Bevy rendering device and one or more CLI/harness devices
  through secure temporary automation profiles rather than local discovery.
- Render native puppet workers to a hidden/offscreen surface by default so no
  automation window appears on the user's desktop. Retain an explicit
  `--show-window`-style debugging option; do not change ordinary interactive
  `poche desktop` visibility.
- Have harness devices play ordinary actions and wait until the Bevy device's
  advertised observation revision matches the expected committed state.
- Send signed capture requests to the Bevy device at main menu/lobby where
  applicable, bidding, card selection, trick resolution, score sheet,
  reconnect, and terminal state.
- Receive all bytes through common transfer/persistence and close naturally.

**Validation:**

```powershell
target\debug\poche.exe --output json puppet run two-player-full-round --surface native --transport loopback-ndjson --seed 1
cargo test --locked -p poche-puppet -p poche-native-ui -p poche-capture --offline
$env:WGPU_BACKEND='dx12'; cargo test --locked -p poche-puppet --offline --test native_full_game -- --ignored --nocapture
```

**Completion criteria:** Captioned native images and structural metadata are
requested by another certified device and saved only by the shared artifact
pipeline, with revision/projection/scene bindings.

### [x] 4.3 Implement the browser provider under the same capture contract

**Completion notes:** Complete. `poche-web-spike` is an embeddable
caller-owned-listener library, and `poche-puppet::browser` launches an installed
Edge/Chrome process headlessly with a private temporary profile and three
isolated browser contexts. The external qualification drives ordinary
create/join/seat/ready/countdown/bid/play/chat/exit/resume/reconnect controls
through a complete 124-choice game (terminal revision 159; 138 public events)
with zero console/network errors. Six semantic checkpoints each contain PNG,
semantic HTML, accessibility-tree JSON, and wide/narrow layout JSON; the audit
found and fixed a real phone-width deck/clock collision and records the
intentional viewer-hand fan as an explicit overlap group.

`PuppetSurface::Web` and `poche.exe puppet ... --surface web` enroll a separate
BrowserLocal same-player provider, advertise only `HarnessOnly` consent, sign
exact-target requests/responses, transfer all four representations through
bounded encrypted chunks, and persist only at the requester. The public command
completed 159 certified-device steps, eight device witnesses, six captures,
and 138 public events. The unified installed-browser test verifies four manifest
entries and distinct requester/provider IDs for every checkpoint. Ordinary
production pages advertise no silent capture provider. The evidence boundary
states that the browser UI and certified-device run are currently deterministic
parallel authorities matched by revision; proving one external shared authority
remains Phase 5 work and is not overclaimed here.

**Work:**

- Close G7 and run two independent browser devices/contexts with separate
  profiles and exact-recipient sessions.
- Drive ordinary main menu, room code, seat, ready, bid, play, chat,
  disconnect/rejoin, and exit controls—not reducer fixture methods.
- Register a harness/CDP browser capture provider that answers the same signed
  target request as Bevy and returns screenshot, DOM, accessible roles/names,
  console/network diagnostics, viewport metrics, and bounding boxes to the
  common pipeline.
- Represent ordinary production browser capture as unavailable or explicitly
  user-confirmed; never pretend web content can silently screenshot itself.
- Assert required controls do not intersect at declared narrow/wide viewports
  except where scrolling/overlap is explicitly designed.

**Validation:**

```powershell
target\debug\poche.exe --output json puppet run two-player-full-round --surface web --transport loopback-ndjson --seed 1
cargo test --locked -p poche-puppet -p poche-web-spike -p poche-capture --offline
cargo test --locked -p poche-puppet --offline --test browser_full_game -- --ignored --nocapture
```

**Completion criteria:** Real browsers play a full round and answer the common
capture contract with screenshot/DOM/a11y/console/network/layout evidence;
unsupported production capture fails explicitly.

### [x] 4.4 Emit one verified manifest and contact sheet across surfaces

**Completion notes:** Complete. Every run now publishes an atomically staged
v3 manifest, `run.json`, `steps.ndjson`, and a responsive `index.html`. The
manifest records build revision/worktree state, scenario/surface/status/final
revision/history hash/evidence boundary, every capture's signed requester and
target, requested/captured revision, projection/scene hash, provider,
representations/qualification, viewport/framebuffer/scale/camera presence,
windowless state, transfer bytes/chunks, preview/structural paths, and every
file's media type, size, and BLAKE3 hash. Safe relative paths, uniqueness,
listed-file hashes, and capture attachments are rechecked before a run can enter
the root catalog.

The Figue CLI accepts a unique comma-separated surface set. One real
`headless,web,native` NDJSON invocation (seed 45) completed all three surfaces
at revision 159 with the same final scores/history hash, generated twelve
graphical captures, and regenerated `target/poche-puppets/catalog.json` plus a
cross-surface `index.html`. Headless Edge visual QA confirmed the contact sheet
clearly distinguishes surface qualifications and links PNG, capture manifests,
HTML, accessibility, and layout evidence. `puppet artifacts path` locates the
root and `puppet artifacts open` opens the verified sheet (or root before one
exists). Older manifest schemas are skipped rather than defaulted into current
evidence. Formal checker evidence is explicitly release/developer evidence
linked by repository revision, not claimed as checked-now per capture.

**Work:**

- Record run/scenario/capture IDs, executable revision, surface/provider,
  player/device/custody, request/target, room/session, requested/captured
  revision, transcript head, projection/action/scene hashes, viewport/DPI,
  camera, crop/caption geometry, media/hash/size, transfer evidence, privacy
  scan, structural attachments, and qualification.
- Verify safe relative paths, unique figures, dimensions, hashes, caption
  geometry, projection linkage, authorization, and absence of unauthorized
  private material before publication.
- Build one HTML contact sheet and `puppet artifacts path|open` commands. Keep
  generated runs ignored; publish via CI/Pages unless G6 deliberately selects
  a small curated baseline.
- Link formal release evidence by version/hash without claiming each capture
  ran every checker. Label optional checked-now evidence explicitly.

**Validation:**

```powershell
target\debug\poche.exe puppet run two-player-full-round --surface headless,web,native --seed 1
target\debug\poche.exe puppet artifacts path
target\debug\poche.exe puppet artifacts open
cargo test --locked -p poche-capture -p poche-puppet --offline manifest
```

**Completion criteria:** An agent can locate the manifest, open every image,
compare target-device views, and understand what each artifact proves and omits.

## Phase 5 — prove complete behavior and failure boundaries

### [ ] 5.1 Run the complete external-device vertical slice

**Completion notes:** Preparatory external-boundary evidence is complete, but
the task remains unchecked until the multi-player graphical acceptance run.
The real Axum listener and HTTP device adapter now prove signed
observe/invoke/wait plus idempotent retry over a process-shaped socket
boundary, using the same advertised action resolver and authoritative reducer
as loopback devices. This currently covers single-device room creation only;
it does not yet claim native/browser peers, protected persistent keys, a full
game, or cross-device capture over the external carrier.

**Work:**

- Launch native/browser rendering devices and CLI/policy devices for at least
  two players through one harness invocation.
- Seat, ready, bid, play every trick, score, disconnect/rejoin, and reach the
  next-round or terminal boundary through ordinary device commands.
- Demonstrate that actions from one device are perceived by sibling and
  opponent graphical devices through committed room history.
- Request captures from each rendering device at named semantic checkpoints
  and verify requester/target/revision/privacy bindings.
- Let a human-compatible graphical device take over from a policy without
  rebuilding state or moving a private key.

**Validation:**

```powershell
target\debug\poche.exe puppet run external-devices-full-game --surface web,native --seed 1
target\debug\poche.exe puppet artifacts path
```

**Completion criteria:** One run manifest proves multiple real devices played
and rendered a complete path, shared actions naturally, and exchanged captures
without local window control.

### [ ] 5.2 Compare headless, web, and native semantic evidence

**Completion notes:** Not started.

**Work:**

- Replay the same seeded semantic scenario across all surfaces/transports.
- Compare transcript heads, authority revisions, exact-recipient projection
  hashes, action-space hashes, spatial-scene hashes, and selected visible
  labels/card identities.
- Preserve renderer-specific differences; never require HTML/Bevy pixel
  equality or make pixels authority.
- Associate relevant Rust/Alloy/NuSMV/Prolog release evidence and run concrete
  checks only where the executable scope supports them.

**Validation:**

```powershell
target\debug\poche.exe puppet run two-player-full-round --surface headless,web,native --seed 1
cargo run --locked -p poche-xtask --offline -- oracle report
cargo test --workspace --locked --offline
```

**Completion criteria:** Semantic observation/action evidence agrees across
surfaces, visual differences are qualified, and screenshots are not mislabeled
as formal proof.

### [ ] 5.3 Exercise device, capture, lifecycle, and transport failures

**Completion notes:** Not started.

**Work:**

- Cover wrong player/device/room/session/revision, revoked/expired certificate,
  missing capture capability, consent denial, busy target, target disconnect,
  renderer shutdown, cancellation, timeout, and duplicate/replayed request.
- Cover oversize, interrupted/reordered/duplicate chunks, hash mismatch,
  partial cleanup, retry/resume, disk error, unsafe path, and privacy-scan
  failure.
- Cover stale actions, reconnect/readmission, independent-device takeover,
  concurrent same-player devices, player vote deduplication, and controlled
  replicated equivocation.
- Confirm ordinary tests are offline; run guarded public Veilid lifecycle and
  capture-transfer qualification only with explicit network acknowledgement.
- Complete one-executable Windows acceptance from Explorer, PowerShell,
  redirected stdout, and concurrent device profiles.

**Validation:**

```powershell
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo run --locked -p poche-xtask --offline -- multiplayer smoke --transport veilid-local
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST='I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run --locked -p poche-xtask --offline -- multiplayer smoke --transport veilid-public
```

The final two lines are a separate public-network gate and must not be run or
recorded as ordinary offline CI.

**Completion criteria:** Failures are bounded, attributable, private, and
non-authoritative; partial artifacts are safe; target support claims have
evidence rather than inference.

## Phase 6 — document, publish, and audit

### [ ] 6.1 Document device, capture, agent, and puppet workflows

**Completion notes:** Not started.

**Work:**

- Update `README.md` with one-executable desktop launch, protected device
  profiles, shared-room CLI/agent behavior, sibling-device capture requests,
  puppet runs, and artifact viewing.
- Document cooperation security/privacy, browser consent/unavailable behavior,
  large artifact transfer, bot API, puppet authoring, evidence qualifications,
  and supported transports/surfaces.
- Publish contact sheets and manifests through GitHub Pages/CI without
  committing high-churn generated images or PDFs.
- Clearly distinguish host-authoritative, replicated experimental, in-process,
  gateway, public Veilid, capture cooperation, and presentation evidence.

**Validation:**

```powershell
cargo run --locked -p poche-xtask --offline -- pages build
git diff --check
```

**Completion criteria:** A new user can launch, enroll devices, play through
CLI/GUI/agent, request a sibling capture, run puppets, and inspect evidence
without confusing a device request with game authority.

### [ ] 6.2 Perform release validation and the final intent audit

**Completion notes:** Not started.

**Work:**

- Run focused, workspace, formal, artifact, privacy, license, and supported-
  target gates; record outcomes beside affected tasks.
- Re-run literal extraction, traceability, and adversarial omission passes
  against P5-U1 through P5-U16 and all later corrections; repair and rerun all
  three if any gap appears.
- Verify no Vox/local-instance dependency or stale documentation remains,
  generated artifacts are ignored/curated intentionally, secrets do not enter
  Git, MPL-2.0 remains correct, and dirty sibling state is absent.
- Commit intentionally, push `model-checking`, and require local `HEAD` to equal
  `origin/model-checking` with a clean worktree.

**Validation:**

```powershell
cargo run --locked -p poche-xtask --offline -- guidance audit PLAN-5-LIVE-CONTROL-PUPPETS.md
git diff --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
git status --short --branch
git rev-parse HEAD
git rev-parse origin/model-checking
```

**Completion criteria:** Every task is `[x]` with adjacent evidence, all active
and superseded guidance has a disposition, the three-pass audit passes, the
worktree is clean, and local/remote commits match.

## Overall completion criteria

- [ ] One installed `poche.exe` supplies graphical, CLI/device-agent, capture,
  and puppet workflows with preserved text/JSON/NDJSON behavior.
- [ ] No Vox, local-instance discovery, named-pipe authority, resident-window
  proxy, or copied device key is required for gameplay, agents, or puppets.
- [ ] GUI, browser, CLI, and policy processes use distinct certified devices,
  exact-recipient observations, ordinary actions, independent revocation, and
  at most one vote per player.
- [ ] Actions from one device are naturally perceived by other room devices
  through committed history and no renderer/automation path has privileged
  mutation authority.
- [ ] Same-player devices exchange signed, target-specific, replay-resistant,
  policy/consent-aware capture requests without changing room history.
- [ ] Bevy and browser providers return raw pixels/structure through one common
  capture/artifact/transfer/persistence contract; unsupported browser capture
  fails explicitly.
- [ ] One puppet invocation fans out as distinct devices for multiple players
  and drives headless, real-browser, and Bevy surfaces through shared APIs.
- [ ] Captioned images, structural attachments, hashes, privacy scans,
  manifests, and contact sheets are durable and directly inspectable.
- [ ] Runtime, visual, release-formal, and optional checked-now evidence are
  accurately distinguished; screenshots are never treated as formal proof.
- [ ] Deterministic CI is network-free; guarded public Veilid evidence remains
  explicit, and supported surfaces/transports have measured acceptance.
- [ ] Documentation, Pages artifacts, dependency/license/privacy checks,
  Windows one-executable UX, commit/push, and the literal final three-pass
  intent audit all have evidence.

## Risk register

| Risk | Consequence | Mitigation / evidence gate |
| --- | --- | --- |
| Capture messages become game commands | Presentation cooperation changes room revision or gains consensus authority | C8; separate signed device domain; reducer/log non-mutation assertions |
| “Same player” is inferred from a display name | One player requests another player's private hand image | C7/G4; root-certified devices, room/session binding, cross-player negative tests |
| CLI copies the GUI key | Devices cannot be independently attributed/revoked | C2-C3; protected unique profiles; key export/copy absent; revocation tests |
| Multiple devices increase vote weight | One player controls consensus by adding bots | C4; player-deduplicated certificates and quorum tests |
| Sibling devices equivocate | Same player signs conflicting replicated votes through two honest-looking processes | C4/G3; durable shared vote lock or refusal of concurrent voting; retained evidence |
| Browser is claimed to silently screenshot itself | Production capability is impossible, surprising, or violates user consent | C10/G7; advertised unavailable/user-confirmed/harness provider policies |
| Renderer providers save files independently | Bevy/web evidence schemas, naming, privacy, and cleanup drift | C9/G6; providers return payloads; `poche-capture` alone persists/publishes |
| Large PNG enters AppCall command reply | Transport limit failures, memory pressure, or command-path coupling | C11/G5; bounded separate artifact stream/blob with measured limits |
| Capture leaks private hand/join/device secrets | CI/Pages artifacts become privacy or credential incidents | Exact target/view, privacy scans, capability policy, publication gate |
| One harness collapses all identities | Tests pass by using omniscient shared state unavailable to real devices | C15; distinct roots/devices/projections and transport-bound action/capture APIs |
| Secure profile bootstrap leaks via process list | External browser/native puppet exposes keys in command arguments/logs | G7; owner-restricted temporary profiles/handles, redaction, cleanup tests |
| Pixels become game truth | Renderer differences rewrite state or tests mistake screenshots for proof | C1/C16; semantic hashes primary; visual evidence explicitly qualified |
| Bevy grows a second engine | ECS/camera/capture state diverges from typed room/device state | C16; leaf adapter and parity tests; no Bevy types in player/device crates |
| Figue migration drops CLI nuance | Existing scripts lose NDJSON, cancellation, redaction, or spelling | C13; old/new corpus, help snapshots, Arbitrary round trips, integration tests |
| Dirty sibling repos enter the build | Release is non-reproducible with unclear provenance | C14/G1; immutable pins, lockfile/license audit, no path dependencies |
| Public Veilid is normalized into CI | Tests become flaky, disclose traffic, or misstate offline support | C12; explicit environment acknowledgement and separate receipt |
| Formal status is overstated in captures | A visual artifact is advertised as proof of all reachable states | P5-U10; explicit runtime/release/checked-now qualifications |
| Plan compaction restores the rejected Vox design | Work duplicates transport and loses multi-device agency | P5-U14-P5-U16, supersession rows, adversarial audit, release grep |
