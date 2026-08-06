// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module consensus

// Independent bounded oracle for the replicated-consensus micro-scope.
// Voting weight belongs to players; devices only provide signed agency.

abstract sig Player {}
one sig Alice, Bob, Carol extends Player {}

abstract sig Epoch { active: set Player }
one sig E1, E2 extends Epoch {}

abstract sig Device {
  owner: one Player,
  valid: set Epoch
}
one sig AliceNative, AliceBrowser, AliceRevoked,
  BobDevice, CarolDevice extends Device {}

abstract sig Value {}
one sig ValueA, ValueB extends Value {}

sig Certificate {
  epoch: one Epoch,
  value: one Value,
  signers: set Device
}

abstract sig TransitionKey {}
one sig AutoAdvance, Chat, Score extends TransitionKey {}
abstract sig CommandHash {}
one sig AutoHash, ChatHash, ScoreHash extends CommandHash {}

sig Proposal {
  author: one Device,
  epoch: one Epoch,
  key: one TransitionKey,
  command: one CommandHash
}

sig Batch { proposals: some Proposal }

fact MembershipAndDeviceAuthority {
  E1.active = Alice + Bob + Carol
  E2.active = Alice + Bob
  AliceNative.owner = Alice
  AliceBrowser.owner = Alice
  AliceRevoked.owner = Alice
  BobDevice.owner = Bob
  CarolDevice.owner = Carol
  AliceNative.valid = E1 + E2
  AliceBrowser.valid = E1 + E2
  AliceRevoked.valid = E1
  BobDevice.valid = E1 + E2
  CarolDevice.valid = E1
}

fun voters[c: Certificate]: set Player { c.signers.owner }

pred validCertificate[c: Certificate] {
  all d: c.signers | d.owner in c.epoch.active and c.epoch in d.valid
  #voters[c] > div[#c.epoch.active, 2]
}

pred canonicalBatch[b: Batch] {
  all disj p, q: b.proposals | p.key = q.key implies p.command = q.command
}

assert DeviceMultiplicityDoesNotIncreaseVotingWeight {
  all c: Certificate |
    c.epoch = E1 and c.signers in AliceNative + AliceBrowser + AliceRevoked
      implies not validCertificate[c]
}

assert StrictMajorityControlsCertification {
  all c: Certificate | validCertificate[c] implies
    #voters[c] > div[#c.epoch.active, 2]
}

assert StaleOrRevokedDeviceCannotCertify {
  all c: Certificate | validCertificate[c] implies
    all d: c.signers | d.owner in c.epoch.active and c.epoch in d.valid
}

assert CanonicalBatchHasOneCommandPerKey {
  all b: Batch | canonicalBatch[b] implies
    all disj p, q: b.proposals | p.key = q.key implies p.command = q.command
}

assert MajorityCertificatesIntersect {
  all disj left, right: Certificate |
    left.epoch = right.epoch and validCertificate[left] and
    validCertificate[right] implies some voters[left] & voters[right]
}

assert TwoPlayerEpochRequiresBothPlayers {
  all c: Certificate | c.epoch = E2 and validCertificate[c] implies
    voters[c] = Alice + Bob
}

pred OutOfTurnKickWitness {
  some c: Certificate |
    c.epoch = E1 and validCertificate[c] and
    voters[c] = Alice + Bob and Carol not in E2.active
}

pred AutomaticProposalCollapseWitness {
  some b: Batch |
    canonicalBatch[b] and #b.proposals = 3 and
    all p: b.proposals | p.epoch = E2 and
      p.key = AutoAdvance and p.command = AutoHash and
    b.proposals.author.owner = Alice + Bob
}

// Controlled negative witness: Bob equivocates, so the two majorities can
// certify distinct values. This is outside the non-equivocation assumption.
pred EquivocationForkWitness {
  some disj left, right: Certificate |
    left.epoch = E1 and right.epoch = E1 and
    left.value = ValueA and right.value = ValueB and
    voters[left] = Alice + Bob and voters[right] = Bob + Carol and
    validCertificate[left] and validCertificate[right]
}

run OutOfTurnKickWitness for 8 but exactly 3 Player, exactly 2 Epoch,
  exactly 5 Device, exactly 2 Value, exactly 1 Certificate, exactly 3 Proposal,
  exactly 1 Batch, exactly 3 TransitionKey, exactly 3 CommandHash, 5 Int
run AutomaticProposalCollapseWitness for 8 but exactly 3 Player, exactly 2 Epoch,
  exactly 5 Device, exactly 2 Value, exactly 1 Certificate, exactly 3 Proposal,
  exactly 1 Batch, exactly 3 TransitionKey, exactly 3 CommandHash, 5 Int
check DeviceMultiplicityDoesNotIncreaseVotingWeight for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
check StrictMajorityControlsCertification for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
check StaleOrRevokedDeviceCannotCertify for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
check CanonicalBatchHasOneCommandPerKey for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
check MajorityCertificatesIntersect for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
check TwoPlayerEpochRequiresBothPlayers for 8 but exactly 3 Player,
  exactly 2 Epoch, exactly 5 Device, exactly 2 Value, exactly 3 Certificate,
  exactly 3 Proposal, exactly 2 Batch, exactly 3 TransitionKey,
  exactly 3 CommandHash, 5 Int
run EquivocationForkWitness for 8 but exactly 3 Player, exactly 2 Epoch,
  exactly 5 Device, exactly 2 Value, exactly 2 Certificate, exactly 3 Proposal,
  exactly 1 Batch, exactly 3 TransitionKey, exactly 3 CommandHash, 5 Int
