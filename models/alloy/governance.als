// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module governance

// Independent bounded governance oracle. It is handwritten from the
// governance contract, not generated from Rust. The micro-scope has two
// ordinary voters and one accused/kick subject whose vote remains visible.

abstract sig Member {}
one sig Alice, Bob, Subject extends Member {}

abstract sig Outcome {}
one sig Pending, Approved, Rejected extends Outcome {}

abstract sig Action {}
one sig Redeal, Kick, EndGame, AdjustScore, ChangeRights extends Action {}

abstract sig Authority {}
one sig VoteAuthority, CapabilityAuthority extends Authority {}

sig Card {}

sig Effect {
  action: one Action,
  authority: one Authority
}

sig Proposal {
  action: one Action,
  eligible: set Member,
  visibleVotes: set Member,
  approvals: set Member,
  rejections: set Member,
  abstentions: set Member,
  outcome: one Outcome,
  effect: lone Effect
}

sig Snapshot {
  cards: set Card,
  scores: Member -> one Int,
  rights: set Member
}

pred validProposal[p: Proposal] {
  p.eligible = Alice + Bob
  p.approvals + p.rejections + p.abstentions = p.visibleVotes
  no p.approvals & p.rejections
  no p.approvals & p.abstentions
  no p.rejections & p.abstentions

  (p.outcome = Approved) iff
    #((p.approvals) & p.eligible) > div[#p.eligible, 2]
  p.outcome = Approved iff one p.effect
  some p.effect implies p.effect.authority = VoteAuthority
  some p.effect implies p.effect.action = p.action
}

pred validSnapshot[s: Snapshot] {
  s.cards = Card
}

pred permittedAmendment[pre, post: Snapshot] {
  validSnapshot[pre]
  validSnapshot[post]
  post.cards = pre.cards
}

assert StrictMajorityControlsApproval {
  all p: Proposal | validProposal[p] implies
    (p.outcome = Approved iff
      #((p.approvals) & p.eligible) > div[#p.eligible, 2])
}

assert ExcludedVoteVisibleButNotCounted {
  all p: Proposal | validProposal[p] and Subject in p.visibleVotes implies
    Subject not in p.eligible and
    Subject not in (p.approvals + p.rejections + p.abstentions) & p.eligible
}

assert VoteEffectRequiresApproval {
  all p: Proposal | validProposal[p] and some p.effect implies
    p.outcome = Approved and p.effect.authority = VoteAuthority
}

assert NoMajorityHasNoEffect {
  all p: Proposal | validProposal[p] and
    #((p.approvals) & p.eligible) <= div[#p.eligible, 2] implies no p.effect
}

assert PermittedAmendmentPreservesCardUniverse {
  all pre, post: Snapshot |
    permittedAmendment[pre, post] implies post.cards = pre.cards
}

pred CanonicalGovernanceWitness {
  some approved, timeout: Proposal |
    approved != timeout and
    validProposal[approved] and validProposal[timeout] and
    approved.action = Redeal and
    approved.visibleVotes = Alice + Bob + Subject and
    approved.approvals = Alice + Bob and
    approved.rejections = Subject and
    approved.outcome = Approved and
    timeout.action = AdjustScore and
    timeout.visibleVotes = Alice and
    timeout.approvals = Alice and
    timeout.outcome = Rejected
}

pred OutOfTurnRecoveryWitness {
  some p: Proposal |
    validProposal[p] and
    p.action = Kick and
    p.visibleVotes = Alice + Bob + Subject and
    p.approvals = Alice + Bob and
    p.rejections = Subject and
    p.outcome = Approved
}

// Controlled defects live outside validProposal/permittedAmendment.
pred CountedExcludedVoteDefect {
  some p: Proposal |
    Subject in p.eligible and Subject in p.approvals
}

pred CardUniverseMutationDefect {
  some pre, post: Snapshot |
    validSnapshot[pre] and Card not in post.cards
}

run CanonicalGovernanceWitness for 6 but exactly 3 Member, exactly 3 Card,
  exactly 2 Proposal, exactly 2 Effect, exactly 2 Snapshot, 5 Int
run OutOfTurnRecoveryWitness for 6 but exactly 3 Member, exactly 3 Card,
  exactly 1 Proposal, exactly 1 Effect, exactly 2 Snapshot, 5 Int
check StrictMajorityControlsApproval for 6 but exactly 3 Member, exactly 3 Card,
  exactly 3 Proposal, exactly 3 Effect, exactly 2 Snapshot, 5 Int
check ExcludedVoteVisibleButNotCounted for 6 but exactly 3 Member, exactly 3 Card,
  exactly 3 Proposal, exactly 3 Effect, exactly 2 Snapshot, 5 Int
check VoteEffectRequiresApproval for 6 but exactly 3 Member, exactly 3 Card,
  exactly 3 Proposal, exactly 3 Effect, exactly 2 Snapshot, 5 Int
check NoMajorityHasNoEffect for 6 but exactly 3 Member, exactly 3 Card,
  exactly 3 Proposal, exactly 3 Effect, exactly 2 Snapshot, 5 Int
check PermittedAmendmentPreservesCardUniverse for 6 but exactly 3 Member,
  exactly 3 Card, exactly 2 Proposal, exactly 2 Effect, exactly 2 Snapshot, 5 Int
run CountedExcludedVoteDefect for 6 but exactly 3 Member, exactly 3 Card,
  exactly 1 Proposal, exactly 1 Effect, exactly 2 Snapshot, 5 Int
run CardUniverseMutationDefect for 6 but exactly 3 Member, exactly 3 Card,
  exactly 1 Proposal, exactly 1 Effect, exactly 2 Snapshot, 5 Int
