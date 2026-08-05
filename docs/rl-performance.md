# RL rollout performance

This is a diagnostic, not a stable performance promise. It was measured on
2026-08-05 with Rust 1.96.0 in `--release` on Windows/AMD64 Family 25 Model 33
(32 logical processors). Every path ran the same typed full-rule game and fixed
first-legal policy for at least one million decisions. No path loaded protocol,
serialization, rendering, Veilid, or any external network dependency.

| Path | Batch | Decisions/s | Hot reallocations | Observation + mask storage |
| --- | ---: | ---: | ---: | ---: |
| direct typed scalar | 1 | 57,904.667 | 0 | 1,288 B |
| naive allocating batch | 256 | 64,423.005 | 3,907 | 329,728 B |
| preallocated batch | 256 | 69,594.485 | 0 | 329,728 B |
| four thread partitions | 256 | 201,006.447 | 0 | 329,728 B |

The preallocated structure-of-arrays path is retained because measurement shows
an approximately 8% throughput improvement over the naive batch while removing
one action-vector allocation per batch iteration. Four independent thread
partitions produce the larger improvement without duplicating the game rules.
Exact per-seed sequential and parallel rollout signatures are tested equal.

The Phase 9 release gate repeated the same million-decision command three times
after the Burn integration. Median decisions/s were 61,052.586 direct,
64,747.282 naive batch, 68,285.275 preallocated batch, and 195,637.909 across
four thread partitions. The respective observed ranges were
58,728.000–77,539.180, 55,664.831–74,827.612,
58,347.717–73,380.393, and 183,157.847–220,646.540. Preallocation was slower
than naive batching in one noisy sample but approximately 5.5% faster at the
median, while its zero-hot-reallocation result held in every sample. This
supports retaining the allocation discipline without claiming stable timing
ordering from any single run.

The hot-path buffers include observations, legal masks, actions, rewards,
terminals, seats, episode indices, and seeds. A second fixed-capacity assembler
pairs each seat action with that same seat's next observation or terminal,
accumulating intervening round reward and decision-time distance. The checked
one-game fixture yields 124 such transitions, including two terminal rows.

No unsafe alternate environment, double buffering, or inference overlap was
introduced: the current measurement says thread partitioning is already useful,
while further structure-of-arrays or device-transfer work should be measured
with the Burn learner present. `hot_reallocations` for the naive path counts the
deliberate per-iteration action-vector allocation; it is not a process-wide
allocator trace.

Reproduce:

```powershell
cargo run --release -p poche-xtask -- rl benchmark --steps 1000000 --batch 256
```
