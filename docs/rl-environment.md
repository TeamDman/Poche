# Reinforcement-learning environment

The first immutable contract is `poche-2p-v1`, with reward projection
`round-score-v1`. Its checked user-facing manifest is
[`rl/specs/poche-2p-v1.json`](../rl/specs/poche-2p-v1.json), whose canonical
semantic hash is
`8852f8568ead1e40aad7bb4ca5b7725340cc01422e077ffddcd5d4f5665bf0bf`.

`poche-rl` depends directly on the typed, policy-neutral `GameEnvironment`. It
does not depend on the session, protocol, renderer, Veilid, or Burn. Chance and
settlement auto-advance from a replayable episode seed. Policy actions are eight
bid slots followed by 52 card-identity slots; a legal mask is mandatory and an
illegal selection is rejected rather than silently changed.

The observation is seat-relative and contains only the viewer hand plus public
state. Public played-card memory is owned by the episode wrapper, because it is
information a human can remember rather than hidden reducer state. Tests swap
opponent/stock-only cards and require the viewer encoding to remain identical.

Reward is zero between round boundaries. At settlement each seat receives the
raw nonnegative rulebook points written for that round. Cumulative score, score
differential, exact bids, game length, pot, winners, and illegal-action count
are metrics; no reward clipping, material proxy, or undocumented shaping is
used.

The pre-learner baseline corpus is
[`rl/manifests/baseline-v1.json`](../rl/manifests/baseline-v1.json), hash
`1ab1c865758b33fda0ab90870b8f2fb74bd76c554ac577db4e3de85770d3aea4`.
It fixes 64 seeds, both seat assignments for legal-random versus the explainable
high-card/low-play heuristic, and two random controls. Episode transcripts are
generated only for selected evaluation episodes and contain observation/action/
reward hashes, not hidden hands or deck order.

Reproduce the contracts and a network-free performance diagnostic with:

```powershell
cargo run -p poche-xtask -- rl spec
cargo run -p poche-xtask -- rl evaluate --manifest rl/manifests/baseline-v1.json
cargo run -p poche-xtask -- rl episode --policy heuristic --seed 24301
cargo run -p poche-xtask -- rl benchmark --steps 1000000 --batch 256
```

Training quality is empirical. None of these measurements is formal evidence
that a policy is optimal, and the RL path never opens a socket.

The static native/browser egui replay includes the corpus-selected best
seat-zero episode for `legal-random-vs-heuristic` (seed `3025338370`). Its CLI
and web semantic hash is
`7ab24388ab33c34e4763e065e493b2529c6562c8d70b59e909f70920442fc5a9`.
The viewer exposes only action/observation/reward hashes, round scores, and
terminal metrics. Generate the identical inspectable NDJSON with:

```powershell
cargo run -p poche-xtask -- rl replay --manifest rl/manifests/baseline-v1.json --matchup legal-random-vs-heuristic --seed 3025338370
```

See [`docs/rl-performance.md`](rl-performance.md) for the measured scalar,
naive-batch, preallocated, and parallel paths.
