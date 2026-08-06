<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# ADR 0007: player/device identity and accountable replicated log

- Status: Accepted for the experimental `poche-replicated-v1` track
- Date: 2026-08-05 (America/Toronto)
- Scope: phase 3 gates G5 and G6
- Supersedes: nothing; host-authoritative protocol v1 remains supported

## Decision summary

The experimental replicated room uses player roots, separately certified
device keys, one consensus vote per player, a hash-linked event log, and a
strict player-majority prevote/precommit certificate. It is an accountable
crash-fault protocol, not a claim of unconditional Byzantine consensus.
Player-device equivocation is signed evidence and halts finalization at the
last common head. It is never resolved by silently choosing whichever event a
host or transport delivered first.

The design borrows the explicit height/round/propose/prevote/precommit and lock
vocabulary from the published Tendermint protocol, whose safety argument
depends on quorum intersection and validators not violating their locking
rules. Poche deliberately uses a simple strict-majority player quorum rather
than advertising Tendermint's less-than-one-third Byzantine guarantee. The
implementation is a small, separately versioned experiment that must earn its
own finite and executable evidence. Relevant primary references are the
[Tendermint consensus specification](https://docs.tendermint.com/master/spec/consensus/consensus),
[validator signing rules](https://docs.tendermint.com/master/spec/consensus/signing.html),
the [HotStuff paper](https://arxiv.org/abs/1803.05069), and
[RFC 8032 Ed25519](https://www.rfc-editor.org/rfc/rfc8032.html).

## Identity hierarchy

`PrincipalId` remains the player identifier in v1. For network-created
identities it is the lowercase hexadecimal Ed25519 player-root public key, as
it already is in `ApplicationPublicIdentity`. This avoids silently changing
the meaning of existing memberships and grants.

A player root signs versioned device certificates. A certificate binds:

- the player and root key;
- a device ID equal to its Ed25519 public key;
- a monotonically increasing certificate sequence;
- its first and optional final membership epochs;
- an exact sorted capability set;
- a disclosed custody class; and
- a root signature over a separate canonical domain.

Every proposal and consensus vote identifies both player and device and is
signed in its own device domain. Devices do not add voting weight: two phones
and a desktop belonging to Alice still contribute at most Alice's one vote to
one height/round/phase/value. Honest devices must share or synchronize a
durable player vote-lock record. Conflicting signed votes from two devices of
the same player are player equivocation, even if neither physical device signed
twice.

Adding a device requires a root-signed certificate. Losing or compromising a
device requires a root-signed revocation effective in a later membership
epoch; already committed history remains attributable. V1 has no magical root
recovery: loss of the player root prevents adding/revoking devices. The user
must retain a protected root backup or explicitly start a new player identity
through room governance. Threshold/social root recovery is deferred because
inventing it here would enlarge the cryptographic protocol materially.

### Legacy migration

An existing 64-hex `PrincipalId` migrates without changing bytes: that key
becomes the player root and receives a domain-separated `LegacySelf` device
certificate for the same public key. The legacy device can authorize a new
device, after which the root can revoke the self-device in a new membership
epoch. Arbitrary human-readable test principals have no cryptographic
migration and stay fixtures only.

Veilid node IDs, DHT owner keys, private routes, gateway URLs, TLS sessions, and
browser connection IDs are transport locators. None is a player or device
identity and none contributes a consensus vote.

## Custody and browser agency

The default is a device-local key. `NativeLocal` and `BrowserLocal` describe
where a device key is expected to live; they are disclosures, not proof that an
operating system or browser protects it. WebCrypto permits non-extractable
`CryptoKey` objects and persistence through stores such as IndexedDB, but the
[WebCrypto specification](https://www.w3.org/TR/webcrypto-2/) does not itself
provide storage or guarantee a particular browser's hardware protection.

`GatewayCustodied` is an explicit degraded certificate class. The gateway can
impersonate that device and receives everything authorized to it, but it does
not receive the player root or another device's key. Export uses
export-and-rotate: create a new local device key/certificate, verify it from
another authorized/root channel, then revoke the gateway device. Copying one
private device key into two places does not create two independent devices and
is never presented as additive agency. The UI must show custody, certificate
sequence/epoch, active devices, and a revoke/export-and-rotate action.

## Replicated ordering contract

The log is a sequence of committed batches. A candidate binds the room,
membership epoch, parent event/hash, height, round, deterministic proposer,
ordered command references, and candidate hash. Event/proposal IDs derive from
canonical semantic bytes, never arrival order. A committed event adds a
successor state hash and a certificate of unique player precommits for exactly
that candidate.

At height `H` and round `R`:

1. Every authorized device may gossip signed semantic command proposals.
2. A deterministic rotating proposer player is derived from the previous head,
   height, round, and sorted active player IDs. Any certified device of that
   player may publish the batch; duplicate proposer-device candidates are
   resolved only through the voting rules, not transport arrival.
3. Players prevote once for a valid candidate or nil. A player precommits only
   after a strict-majority prevote certificate and locks that value.
4. A strict-majority precommit certificate commits the candidate. Locks may
   change only with a higher-round strict-majority proof, which is included in
   signed evidence.
5. Logical round changes are ordered consensus inputs. Wall clocks only cause
   devices to propose a round-change/nil vote; reducers never read time.

For `N` active players, quorum is `floor(N/2)+1`. A certificate deduplicates by
player, validates every active device certificate at the event's membership
epoch, and rejects mixed room/epoch/height/round/value signatures.

This gives commit safety under the named assumption that every honest player
obeys one-vote/lock rules. A strict-majority intersection always contains a
player; without player equivocation that player cannot certify two conflicting
values. It does not tolerate arbitrary Byzantine player behavior. Signed
double votes are accountable evidence, but accountability is not prevention.

## Conflicts, membership, snapshots, and recovery

Two distinct commit certificates for the same parent/height are a fork proof.
A replica records both certificates, stops applying shared effects after the
last common head, and enters governable recovery. It does not pick the smaller
hash after users may have acted on a purported commit. Players may explicitly
create a new session anchored to one disclosed branch; that is a fork/new-room
choice, not retroactive finality in the old room.

Membership changes use joint certification: the final event in epoch `E` must
have a quorum of the old active player set and a quorum of the proposed new
set. Its successor begins epoch `E+1`. Device revocations and player kicks are
therefore deterministic epoch changes. Network disconnect alone changes no
membership or voting weight; otherwise a partition could make both halves
believe they removed the other.

Snapshots bind schema hash, state hash, membership epoch, height, head event and
head hash, plus the commit-certificate hash at that head. A snapshot is a
replay optimization, not independent authority. Recovery requires a matching
certified head and replays the canonical tail.

All devices derive and may propose deterministic follow-up commands. Duplicate
semantic transition keys collapse before batching, and one committed command
can produce the effect only once. A scorekeeper role may be granted a narrow
proposal/capability function but is not a permanent hidden sequencer.

## Liveness and the two-player limit

Progress requires a connected strict majority, eventual delivery among that
majority, an eventually responsive proposer round, and local runtimes that
continue proposing round changes. A paused room, unavailable quorum, or
permanent partition may remain live-but-uncommitted forever. Safety is not
weakened to claim termination.

In particular, a two-player room has quorum two. If one player disappears, the
remaining player cannot both commit on behalf of the shared room and prove that
the absent player is not concurrently committing another history. This is an
information-theoretic partition problem, not a Veilid limitation. The remaining
player retains local agency to inspect, propose, export evidence, or explicitly
fork into a new session, but the old shared log does not claim a commit. Three
players can tolerate one crash only in the crash/non-equivocation model; a
Byzantine-tolerant profile would require a separately reviewed supermajority
protocol and generally at least four players to tolerate one faulty validator.

## Rejected alternatives

- **One host orders everything:** remains the supported compatibility mode but
  does not satisfy distributed agency.
- **One vote per device:** lets a player mint voting power by adding devices.
- **Connected-device quorum:** partitions can independently redefine who is
  connected and commit split histories.
- **Smallest hash wins after commit:** converges only by silently rolling back
  effects users were told were final.
- **Transport identity as authority:** route rotation and gateway mediation
  would change player membership.
- **Gateway stores every key:** convenient but recreates a trusted host and
  defeats the requested multi-device agency.
- **Custom threshold root recovery now:** materially enlarges the cryptographic
  threat model before the base device protocol has evidence.

## Consequences and required evidence

`poche-replicated-v1` stays separate from host-authoritative protocol v1. Phase
5.1 fixes canonical types, signing domains, IDs, quorum, migration, certificate
and event vectors. Phase 5.2 implements delivery-independent convergence,
partitions, reconnect, snapshots, and fork evidence. Phase 5.3 independently
models safety and conditional liveness in Rust, Alloy, NuSMV, and Prolog.
Nothing in this ADR is a production security audit or a promise of availability
without quorum.
