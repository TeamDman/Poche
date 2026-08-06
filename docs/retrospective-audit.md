# Retrospective action audit

Poche can retain an illegal tabletop move as evidence without constructing an
invalid strict `Game`. `RetrospectiveAudit` is an append-only companion to the
strict reducer. It does not own score, turn, cards, or room authority.

## History and knowledge

Every game action receives a stable `EventId`, logical sequence, round ID,
actor, typed `GameActionWire`, led suit, and the exact cards that one detector
could prove the actor held immediately before that action. The disposition is
either:

- `Accepted`, meaning the strict game transition committed; or
- `AttemptedStructurallyValid`, meaning the game-legality layer did not commit
  it, but hard identity, ownership, and card-conservation checks succeeded.

That distinction matters. A raw claim such as “I played the ace of clubs” is not
evidence that the claimant owns that card. It cannot enter this history until
the structural/hidden-card layer verifies identity and ownership. The audit
module checks card/ID/order shape; the authority or future mental-poker layer
supplies that ownership fact.

An empty `known_held_cards_before` means “this detector had no applicable
private knowledge,” not “the hand was empty.” A complete remaining-hand
disclosure is a separately identified logical event at round end. All history
records use strictly increasing logical sequence numbers and reject duplicate
event IDs.

## Follow-suit proof

The initial registered retrospective rule is `R-TRICK-005`. An off-suit play is
a confirmed violation only when the engine can exhibit a card of the led suit
that the actor still held at the offending action. It chooses one of three
evidence classes:

| Confidence | Evidence |
| --- | --- |
| `ImmediateHeldCard` | The detector's action-time knowledge already contains a card of the led suit. |
| `DelayedPublicPlay` | The same actor reveals/plays a card of the led suit later in the same round. |
| `RoundEndDisclosure` | The same actor's complete remaining hand reveals a card of the led suit. |

The delayed inference relies on Poche's invariant that a player acquires no new
cards during a round and on the structural uniqueness/ownership gate. It never
infers across `round_id`. If neither immediate nor later evidence exists, the
engine records no confirmed finding—even if an off-suit action looks
suspicious.

A finding links the stable rule, offending action and its accepted/attempted
disposition, revealing action/disclosure, concrete revealed card, detector, and
confidence. Its canonical `FindingId` is derived from the rule plus offending
action, not from detector or publication mode. Re-running the audit is
idempotent; separately implemented clients can therefore refer to the same
semantic finding while retaining who detected/submitted it.

## Manual accusations

`ManualAccusation` contains only an accusation ID, detector, and offending
action ID. The accuser cannot supply a rule, revealed card, confidence, or a
“confirmed” bit. The same declarative audit derives one of:

- `Confirmed { finding_id }`;
- `Unfounded::UnknownAction`;
- `Unfounded::ActionNotViolation`; or
- `Unfounded::InsufficientEvidence`.

An accusation ID is immutable and idempotent. If it was evaluated before new
evidence and found insufficient, replay retains that outcome; a later
accusation uses a new ID and can cite the newly confirmable canonical finding.
Reusing an accusation ID with different content fails closed.

## Boundaries

This is deterministic rule evidence, not proof that a device is honest, that a
signature is valid, or that a future cryptographic ownership proof is sound.
Room policy still decides whether findings publish automatically or require an
accusation. Client-local automation may propose a recovery action, but a
finding alone never kicks a player, changes rights, redeals, ends a game, or
changes score. Those shared effects remain the proposal/capability work in
task 4.3.
