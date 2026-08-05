# Session cross-oracle agreement

The `lobby-micro` agreement gate runs all four independent source gates before
constructing neutral `poche-interchange` evidence:

```text
cargo run -p poche-xtask -- session compare all --scope lobby-micro
```

Rust is not the expected-answer source. Each applicable backend maps its own
already-checked assertion, temporal property, or complete query set to a stable
Boolean semantic claim. `compare_session_tracks` groups claims by ID and
reports contradictions without choosing a winner. Its unit test deliberately
feeds conflicting Alloy and Prolog values and verifies that the disagreement
is retained.

| Claim | Rust explicit | Alloy | NuSMV | Scryer Prolog | Rules |
| --- | --- | --- | --- | --- | --- |
| Start requires readiness | true | true | true | true | `S-ROOM-006`, `S-ROOM-008` |
| Any player may pause | true | true | true | true | `S-ROOM-011` |
| Any player may resume | true | true | true | true | `S-ROOM-012` |
| Paused game is immobile | true | true | true | true | `S-ROOM-013` |
| Outsider is default denied | true | true | abstracted | true | `S-AUTH-001` |
| Spectator needs exact grant | true | true | abstracted | true | `S-VIEW-006` |
| Revoke stops future delivery | true | true | abstracted | true | `S-VIEW-007` |
| Unconditional session termination | false | not a temporal claim | false | not a temporal claim | `S-TIME-007` |
| Conditional terminal reachability | true | not a temporal claim | true | not a temporal claim | `S-TIME-006` |
| Closed is absorbing | true | outside selected Alloy transitions | true | outside selected query corpus | `S-ROOM-022` |

The passing receipt has four tracks, ten shared claims, 31 applicable backend
observations, and zero disagreements. Confidence remains track-specific:

| Track | Confidence | Exact qualification |
| --- | --- | --- |
| Rust explicit | exhaustive | 800 reachable states and 38,400 action attempts in the named abstract scope |
| Alloy | bounded | Five principals, exactly two seats, and two/four snapshots with recorded 5-bit integer command scopes |
| NuSMV | symbolic | Finite two-player lifecycle with explicit `fair_all` and four omission scheduler modes |
| Scryer Prolog | queried | Seven complete bounded answer sets, 63 rows total, each pinned by count and BLAKE3 |

Agreement does not widen any result. Cryptography, arbitrary network delivery,
chat contents, full card semantics, malicious-host prevention, and unbounded
principals/history remain abstracted or deferred in `session-coverage.md`.
