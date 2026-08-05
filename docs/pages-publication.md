# Rulebook publication

The public “pretty view” is <https://teamdman.github.io/Poche/>. GitHub Pages
serves a small landing page, a PDF compiled from `docs/main.typ`, and the static
exact-projection egui/WASM replay; no generated artifact is committed to Git.

## Format decision

| Format | Disposition | Evidence |
| --- | --- | --- |
| PDF | Published; canonical pretty view | Typst 0.15.1 compiles the complete `charged-ieee` rulebook. CI installs TeX Gyre Termes and Cursor before compiling. |
| Landing HTML | Published | Hand-authored, accessible navigation to the generated PDF, Typst source, and formal-evidence matrix. CI stamps it with the exact source commit and event time. |
| Static egui/WASM replay | Published | Rust 1.96.0 and `wasm-bindgen` 0.2.126 build the checked exact-recipient fixture into `replay/`. It contains no authority, room transport, or live Veilid node. |
| Live multiplayer | Not hosted by Pages | Direct HTTPS/WSS Veilid failed the G26 gate. The host-colocated Datastar authority is a separately run server; the current deterministic demo is not authenticated production multiplayer. |
| Native Typst HTML | Evaluated and rejected for this milestone | `typst compile --features html --format html` warns that HTML is experimental and drops the template's page setup, two-column layout, title placement, vertical/horizontal spacing, and explicit alignment. Publishing it would not be a faithful pretty view. |

This is the G13 fidelity gate, not a permanent rejection of Typst HTML. Revisit
it when the template and Typst HTML target can preserve the document's intended
structure. Typst's own documentation currently describes HTML export as
experimental and warns that templates may not render properly:
<https://typst.app/docs/web-app/export-and-preview/#html-preview-and-export>.

## Production contract

- `.github/workflows/pages.yml` runs only for the `model-checking` project head
  (or a manual dispatch of that ref) and deploys through the protected
  `github-pages` environment.
- The workflow downloads the official Typst 0.15.1 Linux archive and verifies
  its GitHub release SHA-256 digest before execution.
- Compilation, font checks, and artifact checks precede upload. A failure skips
  deployment; the previously published page continues to identify the exact
  commit that produced it rather than silently presenting itself as a newer
  build.
- The Pages artifact contains the landing page, `poche-rules.pdf`, MPL-2.0
  license, and generated `replay/` HTML/JS/WASM. It contains no live backend,
  Veilid bundle, invitation, room state, transcript, or private user data. The
  `site/` build directory and `docs/main.pdf` are ignored.

## Local reproduction

Set `TYPST_BIN` to Typst 0.15.1, install TeX Gyre Termes and TeX Gyre Cursor,
then run:

```pwsh
& $env:TYPST_BIN compile --root . docs/main.typ "$env:TEMP\poche-rules.pdf"
```

The workflow follows GitHub's custom Pages workflow shape: configure Pages,
upload one Pages artifact, and deploy it from a job with `pages: write` and
`id-token: write` permissions:
<https://docs.github.com/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages>.

Local replay reproduction requires Rust 1.96.0, the
`wasm32-unknown-unknown` target, and exact `wasm-bindgen-cli` 0.2.126:

```pwsh
pwsh crates/poche-ui/web/build.ps1 -OutputDirectory site/replay
```

The deployment and trust distinctions are kept in
[`deployment-modes.md`](deployment-modes.md).

## Static replay publication evidence

GitHub Actions run
[`31026114728`](https://github.com/TeamDman/Poche/actions/runs/31026114728)
built and deployed commit `16f213252e3c447216582b462a498fbf27800379` on
2026-08-05. The build and deploy jobs both passed. Independent HTTPS checks
returned 200 for `/Poche/`, `/Poche/replay/`, and
`/Poche/replay/pkg/poche_ui_bg.wasm`; the WASM response was
`application/wasm`. The landing page named the exact commit and stated that
Pages has no live multiplayer backend. The in-app browser followed the public
replay link and reached the active `Poche projection replay` application.

The generated replay consisted of a 1,763-byte page, 73,769-byte JavaScript
module, and 3,544,207-byte WASM module in this build. `git status` remained free
of generated assets because `site/` is ignored.
