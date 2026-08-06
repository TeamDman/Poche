<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Trustless-round abort and recovery

ADR 0008's research profile requires every enrolled player's reveal share. A
disconnect, withheld share, or invalid proof can therefore stop the current
cryptographic hand. `poche-runtime::TrustlessRoundState` makes that limitation
an explicit shared transition instead of a timeout loop or a fictional secret
recovery.

## Registered points

| Point | Stage | Missing/invalid contribution |
| --- | --- | --- |
| Before shuffle contribution | Shuffling | Full-deck shuffle/proof |
| After shuffle contribution | Shuffling | Participant remains required for later private reveal |
| During private deal | Private deal | Holder-directed reveal share |
| While holding cards | Holding | Future public-play reveal share |
| Before final disclosure | Final disclosure | Round-end disclosure share |

Each accepted failure records the context digest, current transcript head,
point, stage, subject, expected contribution, cause, and logical tick in a
stable `CryptographicAbort`. The transition to `RecoveryPending` is immediate
and out-of-turn. Repeating identical evidence is idempotent; conflicting
evidence or continued ordinary progress fails closed.

`Disconnected`, `UnavailableShare`, and `InvalidProof` are distinct causes.
They are also distinct from a retrospective Poche-rule cheat: the latter keeps
using the action/finding evidence in `poche-runtime::RetrospectiveAudit`. A
client may detect either kind locally, but a shared abort or finding still
enters the accepted replicated history through normal authority/consensus.

## Governance composition

Recovery uses the existing typed `GovernanceState`, proposal eligibility,
visible votes, strict majority, capability alternative, and exact effects:

- `Kick` removes a player from future governance/rosters but deliberately
  leaves this hand in `RecoveryPending`; it cannot recreate a missing share.
- `Redeal` enters `RedealRequired` with the new governance redeal epoch. The
  next hand must create a fresh membership-bound context, keys, shuffle,
  assignments, and spatial handles.
- `EndGame` enters `Ended` while retaining the abort transcript.
- Score amendments and other remedies remain separately typed governance
  actions and do not mutate cryptographic artifacts.

The disconnected/target player is never awaited as the current game actor or
by cleanup work. Logical ticks are explicit command inputs used for proposal
records; reducers never inspect wall time. If players choose neither redeal nor
end, `RecoveryPending` honestly remains pending rather than claiming liveness.

## Executable receipt

`cargo run -p poche-xtask --offline -- trustless smoke --scenario dropout`
exercises a three-player matrix covering all five points. It produces five
aborts, two vote-kicks, four vote-redeals, one vote-end, no unresolved terminal
scenario, and corpus digest
`blake3:1134633ac68490a908103d8e2751ec423e9970f1c813a4dbb15f6e31b52d8624`.
Focused tests also prove idempotence, wrong-stage/conflicting-abort rejection,
and that kick alone does not advance or increase the redeal epoch.

This is session-level recovery, not same-hand threshold recovery. The hidden
card mode remains experimental and research-only.
