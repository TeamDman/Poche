# Native tool runners and fixture adapters

## Policy and discovery

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

## Typed runners

`poche-native-tools` is the execution boundary around the three handwritten
non-Rust oracles. It does not generate Alloy, NuSMV, or Prolog source. Given the
repository root and a backend, it resolves the installed binary, records the
version and exact invocation, executes the native source, and parses only a
small accepted output grammar.

## Fail-closed result model

Every invocation returns `NativeReport` with one of three dispositions:

- `Success`: the process exited successfully, every expected native result was
  recognized, all results had their passing polarity, and every fixture selector
  for the backend resolved;
- `Failure`: the executable could not run, exited unsuccessfully, or returned a
  recognized counterexample/failed query; or
- `Unknown`: the process succeeded but a command/property/test was missing,
  duplicated, unexpected, or expressed in an output form the parser does not
  recognize.

Unknown output is never treated as success. Parser tests cover missing and
unexpected Alloy commands, unknown NuSMV truth values, missing Prolog test
lines, duplicate names, and malformed Alloy receipts.

Each ignored `target/<backend>-oracle/` evidence directory contains:

- `command.txt`, `version.txt`, and `exit-code.txt`;
- byte-for-byte `stdout.log` and `stderr.log` from the native process; and
- `normalized-results.txt` for stable inspection.

Alloy additionally owns `receipt.json`. The normalizer extracts each exact
`run`/`check` source from that receipt, retaining the integer and atom scope
beside SAT/UNSAT and instance counts. NuSMV results retain property kind,
normalized expression, and truth value. Scryer emits one stable named PASS line
per `oracle_test/1` query plus a checked aggregate count.

Named Alloy conformance suites use the same raw/normalized evidence boundary
but declare expected polarity per command. This is necessary because a malformed
fixture expressed as `run InvalidThing` passes when it is UNSAT, while a
controlled weakened-rule witness passes when it is SAT. The generic parser
still requires an exact command inventory and receipt-derived scope for every
result. Canonical repository paths are used for containment checks; Windows
verbatim `\\?\` prefixes are removed only from paths passed to Alloy's Java
launcher.

## Common fixture conversion

`fixture_adapters()` is an explicit registry from
`fixtures/oracle-inventory.toml` IDs to native-language selectors. Conversion
therefore means selecting the native relation/command/property that represents
the common intent; it does not imply that the languages share a state syntax.

| Backend | Adapter pairs | Native selector | Preserved boundary |
| --- | ---: | --- | --- |
| Alloy | 16 | named `run`/`check` commands | exact bounded command source from the receipt |
| NuSMV | 18 | invariant/CTL/LTL output expressions | fixed two-player symbolic abstraction |
| Scryer Prolog | 19 | named finite `oracle_test/1` modes | grounded/productive relational constraints |

The registry covers all 15 common scenarios in all three native backends, plus
only the property/query tracks assigned to that backend by the inventory. A
single shared fixture may intentionally be a conjunction of several native
selectors—for example, deck conservation uses both Alloy deck/partition checks,
three NuSMV count invariants, and Prolog deck/round-restoration queries.

## Reproduction

```pwsh
cargo test -p poche-native-tools
cargo run -p poche-xtask -- oracle check all
```

The aggregate command first executes the conventional Rust oracle, then Alloy,
NuSMV, and Scryer Prolog. On the installed versions it normalizes 15 Alloy
commands, 46 NuSMV properties, and 16 Prolog queries; all 53 native
fixture/backend adapters resolve and pass.
