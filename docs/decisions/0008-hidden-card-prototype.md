<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# ADR 0008: bounded verifiable hidden-card prototype

- Status: Accepted only for the research-only `poche-mental-poker-bg12-v0` track
- Date: 2026-08-05 (America/Toronto)
- Scope: phase 3 gate G7 and tasks 6.1-6.3
- Supersedes: nothing; host-authoritative dealing remains supported

## Decision summary

The first trustless hidden-card experiment uses a full 52-card ElGamal deck,
sequential verifiable re-encryption shuffles, card-specific verifiable reveal
shares, and a public round-end transcript. The implementation pins
[`ziffle` 0.1.0](https://crates.io/crates/ziffle/0.1.0), whose shuffle proof is
based on Bayer and Groth's published efficient zero-knowledge shuffle argument.
The exact upstream tag peels to commit
`2bdd74408ac536583b03058ef6529035f39f57bc`.

This is a bounded research prototype, not audited production cryptography.
Upstream explicitly describes the crate as experimental and unaudited. Its
16 unit tests and 15 doctests pass from the exact tag, including shuffle,
reveal, serialization, tamper, wrong-context, wrong-key, and wrong-card cases.
Those tests establish reproducibility and API behavior; they are not a security
review.

The selected construction requires a contribution from every enrolled player
to reveal a card. It therefore does **not** let the same cryptographic hand
continue after an enrolled player withholds a required share. Such a failure
enters a typed cryptographic-abort state from which normal out-of-turn
governance may kick, redeal with a new roster, end the game, or apply another
authorized remedy. It never waits on the removed player's ordinary turn and
never claims that a missing secret can be recovered. This limitation is why the
track remains research-only.

## Published basis and exact dependency

The construction is grounded in these primary or implementation sources:

- Bayer and Groth, [*Efficient Zero-Knowledge Argument for Correctness of a
  Shuffle*](https://www0.cs.ucl.ac.uk/staff/J.Groth/MinimalShuffle.pdf),
  DOI `10.1007/978-3-642-29011-4_17`;
- Wei and Wang, [*A Fast Mental Poker
  Protocol*](https://eprint.iacr.org/2009/439.pdf), for an actively secure
  full-deck mental-poker construction and its availability boundary;
- Golle, [*Dealing Cards in Poker
  Games*](https://crypto.stanford.edu/~pgolle/papers/poker.html), for the
  alternative on-demand family;
- Castella-Roca and coauthors, [a dropout-tolerant mental-poker
  protocol](https://crises-deim.urv.cat/web/docs/publications/lncs/435.pdf),
  DOI `10.1007/11537878_4`, as a reference for same-game dropout recovery not
  implemented here;
- the maintained [`ziffle` source](https://github.com/v26-solutions/ziffle),
  licensed MIT OR Apache-2.0; and
- [LibTMCG](https://www.nongnu.org/libtmcg/), a GPL C++ implementation and
  architectural reference whose authenticated-broadcast and robustness
  assumptions are not silently inherited by Poche.

Poche pins `ziffle = "=0.1.0"`. Poche-owned wrapper and transcript code remains
MPL-2.0. No local dirty checkout or unpublished cryptographic primitive enters
the dependency graph.

## Frozen protocol profile

`poche-mental-poker-bg12-v0` has this flow:

1. Every player generates an independent secret key and proves ownership of
   the corresponding public key in the exact round context.
2. The ordered roster keys form the aggregate encryption key.
3. Starting from the canonical 52-card deck, every enrolled player in roster
   order re-encrypts and permutes the entire deck and publishes a verifiable
   shuffle proof. At least one honest shuffle is required to hide the final
   permutation.
4. Consensus assigns opaque encrypted deck positions uniquely to players. A
   player does not choose a plaintext card and cannot substitute a different
   position without changing the accepted transcript.
5. For a private deal, every non-holder creates a card-specific reveal token
   and proof. Those tokens travel to the holder over an authenticated,
   confidential recipient channel. The holder adds their own share, verifies
   the set, and learns the card. The public log stores bounded commitments and
   receipts, not the private reveal tokens.
6. To play or publicly reveal a card, all reveal tokens and proofs become
   public. Every replica verifies the revealed identity against the accepted
   encrypted position before the typed Poche action can use it.
7. At round end all dealt cards and the necessary reveal evidence are
   disclosed. Any client may audit deck conservation, assignment uniqueness,
   every public reveal, and the completed-round Poche history.
8. A new round uses a new context and freshly randomized handles. A stable
   spatial card-object trajectory never crosses the hidden shuffle boundary.

The wrapper rejects wrong roster order or count, duplicate players or deck
positions, invalid proofs, out-of-bounds positions, wrong-round material,
incomplete reveal sets, noncanonical encodings, and messages above their exact
bounds. Production execution must use an operating-system CSPRNG. Seeded RNGs
exist only in labelled deterministic test vectors.

The domain is length-framed and begins with
`POCHE\0MENTAL-POKER\0V0`. It binds the room, session, membership epoch, round,
ordered roster, and deck-schema hash. Room strings or transport routes are not
used as unframed cryptographic contexts.

## Security claims and assumptions

The adversary is a computationally bounded active coalition of at most `n-1`
players. Privacy of an honest player's unrevealed card requires that player's
secret key to remain secret, at least one shuffle contributor to choose an
unpredictable private permutation/randomizer, confidential delivery of private
reveal shares, discrete-log/DDH hardness for the selected group, and sound
ownership/shuffle/equality-of-discrete-log proofs in their specified model.
The system cannot prevent players from voluntarily sharing their own hands.

Correctness and accountability additionally require authenticated player/device
signatures and a consistent, hash-linked broadcast transcript. ADR 0007's
replicated mode offers conditional, accountable consistency: a valid fork
proof halts rather than choosing a branch invisibly. This ADR does not elevate
that protocol to Byzantine consensus or Sybil resistance.

The profile claims, within its bounded implementation and assumptions:

- no plaintext card is assigned twice in an accepted complete deck transcript;
- an accepted shuffle is a re-encryption and permutation of its input deck;
- a holder can privately identify only positions for which they receive all
  required valid shares, while public observers see redacted commitments;
- a public reveal is bound to the encrypted position and round; and
- completed rounds can be audited after all hands become public.

It does **not** claim guaranteed termination, bias-free completion in the face
of adaptive aborts, same-hand recovery after a missing share, protection after
all players collude, traffic-analysis privacy, side-channel resistance,
post-compromise secrecy, anonymous participation, or production fitness.
Every player can abort progress by withholding a shuffle or reveal share.
Governance makes that failure explicit and recoverable at the game-session
level; it does not repair the cryptography of the abandoned hand.

## Secret Santa analogy and its limit

The user's [Secret Santa video](https://www.youtube.com/watch?v=wqOb5n3BIn0)
and [no-trusted-party construction](https://math.stackexchange.com/a/2896914)
capture the useful shape: participants privately receive mutually exclusive
choices from one public pool without trusting a single allocator. Poche also
needs malicious-participant proof verification, conservation of an entire
deck, collusion assumptions, authenticated broadcast, confidential directed
shares, valid public reveal, transcript replay, adaptive abort handling, and
post-round audit. The analogy motivates the problem; it is not a sufficient
security protocol.

## Dropout and lifecycle policy

Before aggregate-key finalization, governance may remove an unavailable player
and restart setup with a new membership epoch. After a player's key or shuffle
is accepted, all of these are explicit abort points:

- missing or invalid shuffle contribution;
- missing private-deal share;
- missing share while an occupied position is held;
- missing public-play reveal share; and
- missing final-disclosure evidence.

The reducer records the exact phase, player, expected contribution, transcript
head, and whether the evidence was absent or invalid, then enters
`CryptographicAbort`. Kick/redeal/end/score remedies operate through the normal
typed governance path. Redeal creates a fresh round context, roster, keys,
shuffle, positions, and handles. No event from the abandoned transcript is
replayed into the new deal. The host-authoritative trusted dealer remains the
honest fallback and is labelled as learning every hand.

Threshold decryption or the published dropout-tolerant remasking construction
could improve availability, but it changes the privacy/collusion tradeoff and
requires a reviewed DKG, verifiable directed decryption, complaint protocol,
and composition proof absent from `ziffle`. That is a future protocol version,
not an undocumented extension to v0.

## Alternatives considered

- **Golle-style on-demand cards:** attractive when a full shuffled deck is too
  expensive, but no maintained compatible Rust implementation or Poche-specific
  composition was found. Deferred rather than reimplemented from a paper.
- **Wei/Wang or a bespoke full protocol:** useful security reference, but
  reimplementing cryptographic primitives would violate this phase's review
  gate.
- **LibTMCG:** valuable and more mature, but it is C++/GPL and brings different
  integration and threat assumptions. It remains read-only reference material.
- **Threshold/VSS recovery:** improves some dropout cases but deliberately lets
  a threshold reconstruct what unanimous shares currently protect. It requires
  a separate adversary analysis and audited implementation.
- **Castella-Roca-style dropout recovery:** directly relevant, but no maintained
  audited Rust implementation was identified and its remasking/veto composition
  is too large to invent here.
- **Commit/reveal randomness only:** detects some bias after reveal but neither
  hides assigned card identities nor proves a full deck permutation.
- **Trusted dealer:** retained as the supported availability fallback with its
  trust and visibility stated plainly.

## Mandatory security-review gate

No documentation or UI may call this track secure, production-ready, or
dropout-tolerant until an independent qualified cryptographic review covers the
exact dependency checksum and Poche composition, including:

- the Bayer-Groth transcript and Fiat-Shamir transformation;
- curve/subgroup and adversarial input validation;
- random-number generation, domain separation, and context binding;
- canonical serialization, malleability, replay, and message bounds;
- recipient-channel encryption and metadata leakage;
- zeroization, side channels, and dependency/supply-chain integrity;
- active/adaptive collusion and abort bias;
- complaint, transcript, governance, and dropout state machines; and
- independent vectors, fuzzing, and negative/cross-implementation tests.

Upstream's unaudited warning must remain visible in developer and player-facing
research-mode surfaces. Passing tests and formal finite models cannot substitute
for this review.

## Consequences and required evidence

Phase 6.2 adds a small wrapper crate with exact domain/context types, bounded
transcripts, deterministic public vectors, private/public projections, and
tamper/replay/duplicate/wrong-card checks. Phase 6.3 composes every registered
dropout point with the existing governance reducer and demonstrates either a
named recovery threshold or the explicit abort/redeal path. No spatial,
renderer, transport, or Slug type participates in cryptographic truth.
