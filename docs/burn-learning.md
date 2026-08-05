# Burn PPO learner and empirical evaluation

Status: Phase 8 complete (2026-08-05).

`poche-burn` sits above `poche-rl`; the dependency never points back. Game
rules, legal actions, viewer observations, chance, settlement, and raw round
rewards remain in the typed framework-neutral environment. Burn 0.21.0 supplies
tensors, autodiff, Flex/WGPU backends, Adam, gradient clipping, and records.

## Algorithm boundary

The first learner is the ADR 0003 actor-critic PPO/GAE design: shared 307→64→
64 features, 60 policy logits, one value, legal-logit masking, `gamma=1`,
`lambda=0.95`, ratio clip 0.2, value coefficient 0.5, entropy coefficient 0.01,
and gradient norm cap 0.5. Same-seat transitions accumulate the untouched
`round-score-v1` reward until that seat's next observation or terminal, and GAE
applies `gamma` once per recorded decision-time distance. There is no terminal
win bonus, clipping, money reward, score-differential shaping, hidden-state
feature, or learner-side game rule.

The deterministic controlled task has two observable contexts, two legal
actions, and a known exhaustive optimum. The real clipped PPO update changes
greedy behavior to the correct action in both contexts. CPU Flex and default
WGPU run the same full-rule forward/backward/mask/checkpoint code:

```powershell
cargo run -p poche-burn --bin poche-burn-backend-smoke --offline -- cpu
cargo run -p poche-burn --bin poche-burn-backend-smoke --offline -- gpu
```

Both report exactly zero illegal probability, 51,994 model bytes, 103,766
optimizer bytes, and exact pre/post-restore logits. Their output digests differ
because backend floating-point execution is not promised bitwise identical.

## Frozen self-play and training

```powershell
cargo run -p poche-xtask --offline -- rl train --manifest rl/manifests/poche-ppo-v1.json
```

The manifest fixes all semantics, seeds, shapes, hyperparameters, corpus, and
artifact location. Four updates each collect one current mirror game plus three
games against the latest immutable historical snapshot with the current seat
derived from the episode seed. The append-only pool holds exactly initial plus
four update records and rejects duplicate identities or capacity overflow.
Evaluation never uses this moving pool as its baseline.

The short run completed 16 games and 1,240 training rows. Mean total losses by
update were 112.599, 80.789, 55.877, and 42.578. Large model/optimizer/replay
artifacts remain beneath ignored `artifacts/rl/poche-ppo-v1-short`; Git stores
only source, the manifest, summarized evidence, and digests.

Burn record containers include random internal parameter IDs. Consequently a
regenerated `model.bin` has a different file digest even when every parameter
and output matches. Poche therefore records both the digest of the concrete
artifact and a semantic probe over fixed viewer-only policy/value inputs. Two
clean reruns reproduced every loss and semantic probe
`1aa3597e69074098bdeecc43b7ff3f52f0d25d9e5f0de1cb31d4a1cf21140a08`.
Checkpoint load first verifies the concrete model/optimizer digests and all
spec/reward/model semantics, then verifies the loaded semantic probe.

## Score-first evaluation and replay

```powershell
cargo run -p poche-xtask --offline -- rl evaluate --manifest rl/manifests/poche-ppo-v1.json
```

The fixed corpus contains 16 disclosed seeds. It runs each baseline matchup
with seat swaps (64 games total). The learned seat's mean score advantages were
+2.625 and +24.125 against legal-random, and +18.563 and +15.438 against the
heuristic. Only the learned-as-seat-1 legal-random interval excluded zero;
three intervals crossed zero. These are empirical estimates from a deliberately
small first run, not formal claims or a declaration that training is solved.
All 64 games had 124 decisions and zero illegal actions.

Evaluation records mean raw scores, score differentials and 95% intervals, all
13 round-score means, exact bids, wins/ties, length, and illegal actions. Each
matchup's best/median/worst episode is written as both inspectable NDJSON and
strict JSON. No failure episode occurred, so `failure_episode` is explicitly
null.

Selected learned episodes use the same inspectable CLI command as baseline
episodes. The learned-manifest path validates the current concrete checkpoint,
evaluation summary, selected row, and strict episode hash before printing
NDJSON:

```powershell
cargo run -p poche-xtask --offline -- rl replay --manifest rl/manifests/poche-ppo-v1.json --matchup learned-vs-heuristic --seed 3778019106
```

To inspect one selected policy episode in the hostable semantic web renderer:

```powershell
$env:POCHE_RL_EPISODE_PATH='artifacts/rl/poche-ppo-v1-short/replays/learned-vs-heuristic-median-3778019106.json'
cargo run -p poche-web-spike --offline
```

Open <http://127.0.0.1:4174/rl/replay>. The loader rejects spec/reward drift or
any illegal-action transcript. The format contains observation/action hashes,
public score diagnostics, and viewer-policy behavior—not private hands or deck
order.

Machine-readable committed evidence is in
`evidence/burn-poche-ppo-v1.json`.
