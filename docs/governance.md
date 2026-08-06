<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Governance, recovery, and permitted amendments

Poche governance is a pure overlay around typed game state. It can amend
scores and capabilities, remove a participant, request a redeal, or end a
game. It cannot create, remove, replace, or rename cards. The finite card
universe, stable identities, ownership checks, codec bounds, and event
integrity remain structural preconditions rather than matters for a vote.

The executable comparison is:

```pwsh
cargo run -p poche-xtask --offline -- session compare all --scope governance-micro
```

It runs all four source gates before comparing their neutral Boolean evidence.
The registered result is four tracks, seven shared claims, 27 observations,
zero disagreements, nine Alloy commands, nine NuSMV properties, and 42 Scryer
Prolog rows. No backend is treated as the oracle for another backend.

## Rust semantics

`GovernanceState` records a monotonic logical tick, roster, score ledger,
capability grants, confirmed accused members, proposals, visible votes,
effects, and processed command receipts. Every mutation is transactional and
validated. Retrying the same command ID and meaning returns its original
receipt; reusing an ID for a different meaning fails.

Proposal eligibility is frozen when the proposal opens. Disconnected and
kicked members are excluded. A kick target is excluded from its kick tally;
a confirmed accused subject is excluded from a proposal directly changing
that subject's rights. Excluded members may still cast a visible vote record,
but `counted` is false and the exclusion is explicit.

A proposal succeeds as soon as approvals are strictly greater than half of
the snapshotted eligible voters. It is rejected when rejections have that
majority, when every eligible member has voted without an approval majority
(including ties and abstentions), or when its logical deadline arrives without
an approval majority. Wall-clock time and transport delivery do not enter the
reducer. The authority/log layer is responsible for agreeing on logical tick
events in the later replicated design.

Score, rights, and recovery actions may be applied by an approved proposal or
by an issuer holding the exact unilateral capability. Direct execution does
not imply a broad gamemaster flag. `AdjustScore`, `ChangeRights`, and
`ResolveRecovery` are distinct grants. Ordinary game actions and accusations
remain delegated to the game and retrospective-audit reducers.

Recovery never waits for the current game actor. A kick marks the target
kicked and disconnected and removes its capabilities. A redeal increments a
deterministic epoch for the game reducer to consume. End-game records a stable
terminal overlay. This boundary makes AFK recovery possible without making
governance responsible for card dealing.

## Independent evidence

The handwritten Alloy model uses three members, three abstract cards, up to
three proposals, two snapshots, and 5-bit integers. It proves strict-majority,
excluded-vote, approved-effect, no-majority, and card-universe-preservation
assertions within that bound. SAT witnesses demonstrate canonical governance,
out-of-turn recovery, a deliberately counted excluded vote, and a deliberately
mutated card universe. Alloy does not prove unbounded membership or temporal
delivery.

The handwritten NuSMV model symbolically checks six deterministic environment
modes over five logical steps and a fixed 52-card count. Its true properties
cover visible/non-counting exclusion, majority and capability authority,
structural immutability, timeout rejection, AFK recovery, and deadlock freedom.
Two isolated defect modes retain counterexamples for counting an excluded vote
and changing the card count. This is a finite scheduler abstraction, not a
Byzantine-consensus or network-liveness proof.

The handwritten Scryer Prolog model answers exact finite relations for five
proposals and ten visible votes. Its 42 normalized rows cover eligibility,
tallies, approval/rejection/timeout decisions, vote- and capability-authorized
effects, reverse effect-to-vote queries, and structural denials. These are
ground query results, not exhaustive reasoning over arbitrary rosters.

The Rust gate executes typed kick, disconnected-player redeal, logical timeout,
rights grant, direct score adjustment, and structural-codec rejection paths.
Focused unit tests additionally cover majority rejection, abstention/tie
behavior, concurrent proposals, score removal, rights revocation, duplicate
commands, and atomic failure.

## Deliberate boundary for phase 5

This task defines what a proposal means after an ordered command reaches the
pure reducer. It does not decide how multiple devices order competing events,
certify device membership, resolve forks, or advance logical deadlines during
a partition. Those consensus and identity decisions belong to phase 5 and
must preserve these proposal records rather than silently reinterpret them.
