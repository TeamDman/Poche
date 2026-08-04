# Rulebook publication

The public “pretty view” is <https://teamdman.github.io/Poche/>. GitHub Pages
serves a small landing page and a PDF compiled from `docs/main.typ`; neither
generated file is committed to Git.

## Format decision

| Format | Disposition | Evidence |
| --- | --- | --- |
| PDF | Published; canonical pretty view | Typst 0.15.1 compiles the complete `charged-ieee` rulebook. CI installs TeX Gyre Termes and Cursor before compiling. |
| Landing HTML | Published | Hand-authored, accessible navigation to the generated PDF, Typst source, and formal-evidence matrix. CI stamps it with the exact source commit and event time. |
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
- The Pages artifact contains only `index.html`, `poche-rules.pdf`, and the
  MPL-2.0 license. The `site/` build directory and `docs/main.pdf` are ignored.

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
