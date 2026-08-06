# Replicated runtime micro-scenario

`poche-replicated-v1` has a deterministic, transport-independent runtime
scenario registered as
`replicated-micro-3players-5devices-4events-majority-partition-snapshot-tail`.
It exercises the log/reducer contract frozen in ADR 0007 without pretending
that an in-process scheduler is a production network or a cryptographic proof.

Run it with:

```pwsh
cargo test -p poche-runtime replicated --offline
cargo run -p poche-xtask --offline -- consensus check --scope micro
```

The pinned receipt has four replicas, three players, five devices, four
committed events, nine proposal attempts, five unique semantic proposals, one
duplicate delivery, and two reorder operations. All replicas finish at epoch
two and state hash
`1b51070836855771b6e51e8a0aae8cffe4784af2c478674e13f579ab5713c168`.

## Causal sequence

1. Alice and Bob certify the first scoring event while Carol is partitioned.
2. Alice and Bob submit two valid proposals in opposite arrival order. Sorting
   by semantic transition key produces one identical candidate batch.
3. Carol receives height two before height one, buffers it, drains it after the
   gap arrives, and treats a later exact retry as a duplicate.
4. A browser replica installs the already-certified height-one snapshot and
   consumes the height-two tail.
5. Alice and Bob jointly certify a membership change that kicks Carol even
   though Carol is the modeled current actor. No game-turn tick is consulted.
6. Carol's epoch-one proposal and Alice's explicitly revoked-device proposal
   are denied. Three devices independently propose the same automatic advance;
   their common transition key and command hash collapse to one event.
7. Bob alone cannot certify another event in the two-player epoch.

The old and new memberships both require quorum two. The example therefore
shows progress with a connected majority before the kick and full agreement
afterward; it does not claim that one survivor of a two-player split can safely
continue.

## Evidence boundaries

The runtime object called `CertifiedRuntimeEvent` is deliberately abstract.
The protocol crate pins real Ed25519 signing bytes and the session crate checks
device certificates, candidate signatures, vote signatures, player-deduplicated
quorums, exact candidate/certificate round binding, joint membership,
revocation epochs, and snapshots through an explicit verifier port.
`ReplicatedRuntimeLog::from_snapshot` consequently accepts an
already-verified snapshot descriptor; a transport adapter must pass through
the session/cryptographic boundary before installing it.

Future events may be delivered out of order, but only the single certified
event extending the local hash-linked head can apply. A second fully validated
certificate for one parent and height is retained as fork evidence and halts
mutation at the common prefix. Merely presenting conflicting-looking bytes is
not enough: the session boundary revalidates the historical epoch roster,
deterministic proposer, device authority, candidate signature, and vote
certificate first. This prevents unauthenticated fork-shaped traffic from
becoming a log-halting denial of service.

The convergence claim is conditional on deterministic reduction, eventual
delivery among connected honest devices, an available strict player majority,
and non-equivocation. The retained negative control removes non-equivocation:
two distinct two-of-three certificates can intersect only at Bob, so if Bob
signs both values conflicting commits become possible. This is accountable
crash-fault evidence, not a Byzantine-fault-tolerance claim.

The scenario does not model socket timing, unbounded message queues, durable
storage failure, hostile cryptographic implementations, arbitrary player
counts, or real Veilid/SSE delivery. Those remain separate adapter and system
tests. Phase 5.3 adds independent finite Alloy, NuSMV, Prolog, and Rust evidence
for the shared consensus claims.
