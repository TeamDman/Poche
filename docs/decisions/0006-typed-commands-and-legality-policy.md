# Typed commands and legality policy

- Status: Accepted; G10/G11 closed
- Date: 2026-08-05 (America/Toronto)
- Scope: `poche-phase-3`, `poche-governance-command-v1`
- Supersedes: The untyped CLI placeholder `game act <string>`; ADRs 0001-0005 otherwise remain in force

## Decision

CLI text, HTML controls, native gestures, protocol messages, votes, and replay
all converge on `GovernanceCommandV1`. The original string is discarded after
parsing. Only the validated Facet-reflected AST can later be embedded in signed
protocol data, authorized, proposed, reduced, or replayed.

```text
slash text / GUI / classified drag
  -> bounded parser or typed constructor
  -> GovernanceCommandV1
  -> semantic validation
  -> canonical typed bytes
  -> signing and authorization (phase 4.3)
  -> immutable decision / proposal / event
```

No command variant evaluates a raw string, invokes a shell, selects a function
name dynamically, or contains recursively nested vote text. `/startvote` parses
its quoted argument immediately and stores only the enclosed typed action.
Proposal identity will derive from the accepted start-vote command/event in
4.3; a voter refers to that stable `ProposalId`.

The isolated v1 command codec has a 4,096-byte ceiling, rejects unknown fields
and variants, validates stable IDs and value bounds, and accepts only its unique
canonical JSON. The schema descriptor is `poche.governance-command.v1`. It is
kept separate from the already released `poche.protocol.v1` envelope until 4.3
adds an explicitly versioned payload instead of silently changing v1's schema
hash.

## G10: AST and parser choice

### Typed vocabulary

| Surface | Typed meaning | Direct authority requirement |
| --- | --- | --- |
| `/play-card jack-spades` | `Execute(Game(Play(card=48)))` | Current game actor |
| `/bid 3` | `Execute(Game(Bid(3)))` | Current game actor |
| `/score add player1 100` | `Execute(AdjustScore(player1,+100))` | Approved vote or `AdjustScore` capability |
| `/rights remove player1 adjust-score` | `Execute(ChangeRights(player1,AdjustScore,Revoke))` | Approved vote or `ChangeRights` capability |
| `/accuse event-2` | `Execute(Accuse(event-2))` | Any active player; confirmation remains an audit rule |
| `/recovery redeal`, `kick`, or `end-game` | `Execute(Recover(...))` | Approved vote or `ResolveRecovery` capability |
| `/startvote "/score add player1 100"` | `StartVote(AdjustScore(...))` | Any active player; no shared effect yet |
| `/vote proposal-1 approve` | `Vote(proposal-1,Approve)` | Eligible active player; tally semantics are frozen in 4.3 |

There is deliberately no create-card, delete-card, change-card-identity,
arbitrary state-patch, or raw-execute variant. Score amendments are nonzero and
bounded to an absolute value of 1,000,000 per command. The cap is a codec/abuse
bound, not a Poche scoring rule.

The existing `game act <raw-string>` placeholder is removed. `game play-card`
remains a typed convenience surface; the new `command parse <SLASH-COMMAND>`
proves the complete vocabulary reaches the same AST. Help and Bash/Zsh/Fish
completion strings come from the same fixed command catalog as the parser.

### Figue disposition

The inspected Figue source at plan-recorded revision `adac882811c4` declares
`figue 5.0.0-rc.5` and Facet `0.50.0-rc.5`, exactly matching this workspace's
Facet version. Its subcommand, help, completion, and round-trip APIs fit the
desired design. The exact offline dependency probe nevertheless failed because
Cargo could resolve cached `figue 5.0.0-rc.5` but not its required
`figue-attrs ^5.0.0-rc.5` (the local registry index offered only older
versions). A dirty sibling path dependency would violate C11 and make CI depend
on this machine.

The accepted implementation therefore uses a small first-party deterministic
parser/catalog feeding the Facet-reflected protocol AST. Figue is
API-compatible but not an admitted build dependency. It may replace this leaf
adapter later when the exact published pair passes a fresh locked/offline probe;
that must not change the protocol AST, canonical bytes, or command meanings.

## G11: invariant and legality layers

Every attempted action is considered in this order:

1. **Structural validation:** codec bounds, identities, signatures, revisions,
   the finite card universe, unique objects/cards, and event integrity. Failure
   is always denied and cannot be voted around.
2. **Authorization:** TPBAC-shaped principal/capability policy decides whether
   the caller may request this typed operation. Unknowns are default-denied;
   explicit enforce-deny overrides all allows.
3. **Poche legality:** a room uses `Prevent` or `AllowAttempt`. Prevention keeps
   an illegal move out of accepted semantic history. Allow-attempt records an
   attempted action/evidence without constructing an invalid typed Poche state.
4. **Finding publication:** independently, a detected violation is either
   published automatically or requires a manual accusation. Detection does not
   itself choose a recovery effect.
5. **Governable effect:** score, rights, and recovery changes require either a
   successful proposal or the exact unilateral capability. No such authority
   can cross the structural boundary.

This yields four shared room modes without conflation:

| Illegal action | Finding publication | Result |
| --- | --- | --- |
| Prevent | Automatic | Reject now; publish the immediate finding when evidence suffices |
| Prevent | Accusation required | Reject now; retain evidence for an explicit accusation |
| Allow attempt | Automatic | Record attempted intent; publish now or when later evidence proves it |
| Allow attempt | Accusation required | Record attempted intent/evidence; a player chooses whether to accuse |

`FindingAutomationWire` is device-local. `ObserveOnly` does nothing;
`Propose(action)` may submit a normal signed proposal when a client observes a
finding. Ten clients detecting the same problem may propose duplicate intents,
but none gains authority merely by running a detector. Stable finding/proposal
IDs and deduplication are implemented in 4.2-4.3.

## TPBAC consequences

The current session policy keeps these rules:

- no matching enforce-allow means deny;
- any matching enforce-deny wins even when a lower-priority allow exists;
- audit-only allow/deny results are immutable evidence but have no authority;
- higher integer priority is evaluated first, with canonical `PolicyId` as the
  deterministic tie-breaker;
- the baseline result has the lowest priority and custom policy results retain
  their exact priority in `PolicyDecision` evidence;
- duplicate policy IDs violate session structural validation.

Priority orders evaluation and selects the reported winning allow/deny. It does
not turn an allow into permission to ignore a matching explicit deny.

## Consequences and deferred work

- Task 4.2 can attach stable event knowledge and retrospective findings without
  redesigning command meaning.
- Task 4.3 must embed this AST in a new signed payload version, implement
  proposals/votes/capability state, and derive stable proposal IDs. It must not
  authorize the isolated codec output directly.
- `AllowAttempt` does not mean corrupting `Game`; loose/spatial attempts remain
  evidence alongside the last valid typed state until an allowed transition or
  recovery event commits.
- Renderer, browser, and distributed replicas consume the same AST. They may
  offer more convenient widgets but cannot invent privileged commands.
