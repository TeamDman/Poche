# Native tool policy

The authoritative version pins are in `tools/versions.toml`. Executable paths
are machine-specific and must not be committed.

`poche-xtask doctor` discovers each binary on `PATH`, with these optional
overrides:

| Tool | Override |
| --- | --- |
| Alloy | `ALLOY_BIN` |
| NuSMV | `NUSMV_BIN` |
| Scryer Prolog | `SCRYER_PROLOG_BIN` |
| Typst | `TYPST_BIN` |

On Windows, a local pinned Typst binary can be placed beneath the ignored
`target/tools/` directory. CI must download the same official release and verify
its version before compiling `docs/main.typ`. The rulebook currently imports
`@preview/charged-ieee:0.1.4`; CI should install TeX Gyre fonts so its selected
typefaces are present instead of relying on fallback fonts.

The doctor treats a missing tool as `unavailable` and a configured binary that
cannot execute as `failed`. NuSMV 2.7.1 is a documented special case for probing:
its `-h` command prints the version banner but exits with status 2, so the probe
accepts that status only when the output contains a NuSMV banner.
