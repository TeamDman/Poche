# Poche

Poche is a rule-traceable card-game model implemented independently in Rust,
Alloy, NuSMV, and Scryer Prolog. The project uses each backend for the questions
it answers naturally, then compares their observable behavior through shared
fixtures and evidence.

- [Completed formal-modeling plan](PLAN.md)
- [Next-phase multiplayer, rendering, and RL plan](PLAN-2-MULTIPLAYER-RL-RENDERING.md)
- [Contributor and evidence guide](CONTRIBUTING.md)
- [Typst rule source](docs/main.typ)
- [Pretty rules](https://teamdman.github.io/Poche/) — generated from the
  `model-checking` rulebook source by GitHub Pages
- [Direct rulebook PDF](https://teamdman.github.io/Poche/poche-rules.pdf)
- [Publication and format decision](docs/pages-publication.md)

The completed first milestone covers the four models, named finite-scope
checking, cross-model conformance, and published documentation. The next-phase
plan composes that game core with a text-first multiplayer session protocol,
Veilid rooms, minimal rendering, and reinforcement learning without making
network or UI state part of the formal game state. Automatic target-language
generation and legacy-v2 comparison remain deferred.

## Native tools

Install Alloy 6.2.0, NuSMV 2.7.1, Scryer Prolog, and Typst, then place them on
`PATH` or provide `ALLOY_BIN`, `NUSMV_BIN`, `SCRYER_PROLOG_BIN`, and `TYPST_BIN`.
Machine-specific executable paths are never committed.

```pwsh
cargo run -p poche-xtask -- doctor
```

## License

Poche is licensed under the [Mozilla Public License 2.0](LICENSE).
