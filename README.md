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
- [Static exact-projection replay](https://teamdman.github.io/Poche/replay/) —
  browser egui/WASM fixture; no room authority or network
- [Deployment-mode matrix](docs/deployment-modes.md) — native/static,
  browser-only Veilid, and self-hosted Datastar status without overclaiming
- [Native Veilid acceptance](docs/veilid-native-acceptance.md) — isolated-local
  limitation and opt-in public DHT/private-route lifecycle evidence
- [Burn PPO learner and evaluation](docs/burn-learning.md) — CPU/WGPU backend,
  frozen self-play, reproducibility, and score-first empirical results
- [Deterministic live-client evidence](docs/live-client.md)
- [Publication and format decision](docs/pages-publication.md)

The completed first milestone covers the four models, named finite-scope
checking, cross-model conformance, and published documentation. The next-phase
plan composes that game core with a text-first multiplayer session protocol,
Veilid rooms, minimal rendering, and reinforcement learning without making
network or UI state part of the formal game state. Automatic target-language
generation and legacy-v2 comparison remain deferred.

## Client and deployment status

GitHub Pages publishes only the rulebook and static projection replay. Direct
browser-only Veilid multiplayer is not supported from its HTTPS origin: the
tested public WSS bootstrap path failed and Veilid 0.5.7 lacks the required
outbound-relay HTTPS topology. No companion application is implied.

The repository includes a native egui replay and a host-colocated
Axum/Datastar deterministic authority demo. The latter is inspectable and
container-buildable, but its named viewer routes, static development invite
codes, and in-memory state are not authenticated production multiplayer. The
native Veilid protocol has passed guarded two-node DHT/private-route lifecycle
acceptance, but no packaged end-user client is released. See [the deployment-
mode matrix](docs/deployment-modes.md) before exposing any process outside
loopback.

```pwsh
cargo run --release -p poche-web-spike
```

## Native tools

Install Alloy 6.2.0, NuSMV 2.7.1, Scryer Prolog, and Typst, then place them on
`PATH` or provide `ALLOY_BIN`, `NUSMV_BIN`, `SCRYER_PROLOG_BIN`, and `TYPST_BIN`.
Machine-specific executable paths are never committed.

```pwsh
cargo run -p poche-xtask -- doctor
```

## License

Poche is licensed under the [Mozilla Public License 2.0](LICENSE).
