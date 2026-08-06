// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic proposal, vote, capability, amendment, and recovery state.

use core::fmt;

use facet::Facet;
use poche_protocol::{
    CommandId, EventId, FindingId, GovernanceCapabilityWire, GovernanceCommandV1,
    GovernanceCommandWire, GovernedActionWire, PrincipalId, ProposalId, RecoveryActionWire,
    RightsChangeWire, RuleLayerWire, SemanticHash, VoteChoiceWire, encode_governance_command,
};

use crate::RetrospectiveAudit;

/// Default logical duration of a proposal vote.
pub const DEFAULT_VOTE_DURATION_TICKS: u64 = 10;

/// One active/inactive player in the governance roster.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GovernanceMember {
    pub principal_id: PrincipalId,
    pub connected: bool,
    pub kicked: bool,
}

/// One score ledger entry, independent of spatial score glyphs.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GovernedScore {
    pub principal_id: PrincipalId,
    pub points: i64,
}

/// Exact unilateral capability grant.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct CapabilityGrant {
    pub principal_id: PrincipalId,
    pub capability: GovernanceCapabilityWire,
}

/// Confirmed accused member and the finding that established that status.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct AccusedMember {
    pub principal_id: PrincipalId,
    pub finding_id: FindingId,
}

/// Why a snapshotted member cannot contribute to this proposal's tally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum EligibilityExclusion {
    DisconnectedAtSnapshot,
    KickedAtSnapshot,
    TargetOfKick,
    ConfirmedAccusedSubject,
}

/// Immutable proposal-time eligibility evidence for one roster member.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct EligibilityRecord {
    pub principal_id: PrincipalId,
    pub eligible: bool,
    pub exclusion: Option<EligibilityExclusion>,
}

/// One visible vote. Excluded voters may author a record, but `counted=false`
/// prevents it from affecting the tally.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GovernanceVoteRecord {
    pub command_id: CommandId,
    pub voter: PrincipalId,
    pub choice: VoteChoiceWire,
    pub counted: bool,
    pub exclusion: Option<EligibilityExclusion>,
}

/// Exact tally over only snapshotted eligible voters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub struct ProposalTally {
    pub eligible: u16,
    pub approvals: u16,
    pub rejections: u16,
    pub abstentions: u16,
}

impl ProposalTally {
    #[must_use]
    pub const fn has_approval_majority(self) -> bool {
        self.approvals > self.eligible / 2
    }

    #[must_use]
    pub const fn has_rejection_majority(self) -> bool {
        self.rejections > self.eligible / 2
    }

    #[must_use]
    pub const fn every_eligible_voted(self) -> bool {
        self.approvals + self.rejections + self.abstentions == self.eligible
    }
}

/// Stable rejection cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ProposalRejection {
    MajorityRejected,
    AllVotesNoMajority,
    DeadlineNoMajority,
}

/// Current immutable-result status of one proposal.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ProposalStatus {
    Pending,
    Approved {
        tally: ProposalTally,
        effect_id: EventId,
    },
    Rejected {
        tally: ProposalTally,
        reason: ProposalRejection,
    },
}

/// Complete proposal record.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GovernanceProposal {
    pub proposal_id: ProposalId,
    pub command_id: CommandId,
    pub proposer: PrincipalId,
    pub opened_tick: u64,
    pub deadline_tick: u64,
    pub action: GovernedActionWire,
    pub eligibility: Vec<EligibilityRecord>,
    pub votes: Vec<GovernanceVoteRecord>,
    pub status: ProposalStatus,
}

/// Authority that caused one shared effect.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum EffectAuthority {
    ApprovedVote {
        proposal_id: ProposalId,
    },
    UnilateralCapability {
        principal_id: PrincipalId,
        capability: GovernanceCapabilityWire,
    },
}

/// One exact governable mutation.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GovernanceEffect {
    pub effect_id: EventId,
    pub action: GovernedActionWire,
    pub authority: EffectAuthority,
    pub logical_tick: u64,
}

/// Input context not supplied by slash text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceInvocation {
    pub command_id: CommandId,
    pub issuer: PrincipalId,
    pub logical_tick: u64,
    pub command: GovernanceCommandV1,
}

/// Stable result of one idempotent governance command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceReceipt {
    pub command_id: CommandId,
    pub proposal_id: Option<ProposalId>,
    pub effect_id: Option<EventId>,
    pub vote_counted: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProcessedGovernanceCommand {
    command_id: CommandId,
    command_hash: SemanticHash,
    receipt: GovernanceReceipt,
}

/// Pure governance state. It owns only governable overlays, never card truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceState {
    logical_tick: u64,
    vote_duration_ticks: u64,
    members: Vec<GovernanceMember>,
    scores: Vec<GovernedScore>,
    capabilities: Vec<CapabilityGrant>,
    accused: Vec<AccusedMember>,
    proposals: Vec<GovernanceProposal>,
    effects: Vec<GovernanceEffect>,
    processed: Vec<ProcessedGovernanceCommand>,
    redeal_epoch: u64,
    game_ended: bool,
}

impl GovernanceState {
    /// Construct a governance overlay from a unique player roster.
    ///
    /// # Errors
    ///
    /// Rejects fewer than two members, invalid/duplicate principals, or a zero
    /// vote duration.
    pub fn new(
        principals: impl IntoIterator<Item = PrincipalId>,
        vote_duration_ticks: u64,
    ) -> Result<Self, GovernanceError> {
        let members = principals
            .into_iter()
            .map(|principal_id| GovernanceMember {
                principal_id,
                connected: true,
                kicked: false,
            })
            .collect::<Vec<_>>();
        if vote_duration_ticks == 0
            || members.len() < 2
            || members.iter().enumerate().any(|(index, member)| {
                !member.principal_id.validate()
                    || members[index + 1..]
                        .iter()
                        .any(|other| other.principal_id == member.principal_id)
            })
        {
            return Err(GovernanceError::InvalidRoster);
        }
        let scores = members
            .iter()
            .map(|member| GovernedScore {
                principal_id: member.principal_id.clone(),
                points: 0,
            })
            .collect();
        Ok(Self {
            logical_tick: 0,
            vote_duration_ticks,
            members,
            scores,
            capabilities: Vec::new(),
            accused: Vec::new(),
            proposals: Vec::new(),
            effects: Vec::new(),
            processed: Vec::new(),
            redeal_epoch: 0,
            game_ended: false,
        })
    }

    /// Add an initial room capability before commands begin.
    ///
    /// # Errors
    ///
    /// Rejects unknown/kicked principals or any call after command processing.
    pub fn add_bootstrap_capability(
        &mut self,
        principal_id: PrincipalId,
        capability: GovernanceCapabilityWire,
    ) -> Result<(), GovernanceError> {
        if !self.processed.is_empty() || !self.is_active_member(&principal_id) {
            return Err(GovernanceError::MissingCapability);
        }
        self.grant_capability(principal_id, capability);
        Ok(())
    }

    /// Mark connection state for later proposal snapshots.
    ///
    /// # Errors
    ///
    /// Rejects unknown or kicked principals.
    pub fn set_connected(
        &mut self,
        principal_id: &PrincipalId,
        connected: bool,
    ) -> Result<(), GovernanceError> {
        let member = self
            .members
            .iter_mut()
            .find(|member| &member.principal_id == principal_id && !member.kicked)
            .ok_or(GovernanceError::UnknownPrincipal)?;
        member.connected = connected;
        Ok(())
    }

    /// Register accused status only from an already confirmed audit finding.
    ///
    /// # Errors
    ///
    /// Rejects unknown findings/actions or non-members.
    pub fn register_confirmed_finding(
        &mut self,
        audit: &RetrospectiveAudit,
        finding_id: &FindingId,
    ) -> Result<PrincipalId, GovernanceError> {
        let finding = audit
            .findings()
            .iter()
            .find(|finding| &finding.finding_id == finding_id)
            .ok_or(GovernanceError::UnconfirmedFinding)?;
        let actor = audit
            .actions()
            .iter()
            .find(|action| action.event_id == finding.offending_action_id)
            .map(|action| action.actor.clone())
            .ok_or(GovernanceError::UnconfirmedFinding)?;
        if !self.is_active_member(&actor) {
            return Err(GovernanceError::UnknownPrincipal);
        }
        if !self
            .accused
            .iter()
            .any(|accused| accused.finding_id == *finding_id)
        {
            self.accused.push(AccusedMember {
                principal_id: actor.clone(),
                finding_id: finding_id.clone(),
            });
        }
        Ok(actor)
    }

    /// Submit one typed command with deterministic logical-time context.
    ///
    /// # Errors
    ///
    /// Rejects stale time, conflicting IDs, unavailable principals,
    /// unsupported direct actions, missing capability, invalid proposal/vote,
    /// or arithmetic overflow without partial mutation.
    pub fn submit(
        &mut self,
        invocation: GovernanceInvocation,
    ) -> Result<GovernanceReceipt, GovernanceError> {
        invocation
            .command
            .validate()
            .map_err(|_| GovernanceError::InvalidCommand)?;
        let command_hash = invocation_hash(&invocation)?;
        if let Some(processed) = self
            .processed
            .iter()
            .find(|processed| processed.command_id == invocation.command_id)
        {
            return if processed.command_hash == command_hash {
                Ok(processed.receipt.clone())
            } else {
                Err(GovernanceError::ConflictingCommandId)
            };
        }
        if invocation.logical_tick < self.logical_tick {
            return Err(GovernanceError::StaleTick);
        }
        let before = self.clone();
        let result = self.submit_unchecked(&invocation);
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(error) => {
                *self = before;
                return Err(error);
            }
        };
        self.processed.push(ProcessedGovernanceCommand {
            command_id: invocation.command_id,
            command_hash,
            receipt: receipt.clone(),
        });
        Ok(receipt)
    }

    /// Deliver a logical deadline and finalize every due pending proposal.
    ///
    /// # Errors
    ///
    /// Rejects decreasing logical time or an effect that cannot be applied.
    pub fn advance_to(&mut self, logical_tick: u64) -> Result<Vec<ProposalId>, GovernanceError> {
        if logical_tick < self.logical_tick {
            return Err(GovernanceError::StaleTick);
        }
        let before = self.clone();
        self.logical_tick = logical_tick;
        let mut finalized = Vec::new();
        for index in 0..self.proposals.len() {
            if matches!(self.proposals[index].status, ProposalStatus::Pending)
                && self.proposals[index].deadline_tick <= logical_tick
            {
                let tally = proposal_tally(&self.proposals[index]);
                self.proposals[index].status = ProposalStatus::Rejected {
                    tally,
                    reason: ProposalRejection::DeadlineNoMajority,
                };
                finalized.push(self.proposals[index].proposal_id.clone());
            }
        }
        if let Err(error) = self.validate() {
            *self = before;
            return Err(error);
        }
        Ok(finalized)
    }

    /// Borrow proposal records.
    #[must_use]
    pub fn proposals(&self) -> &[GovernanceProposal] {
        &self.proposals
    }

    /// Borrow applied effects.
    #[must_use]
    pub fn effects(&self) -> &[GovernanceEffect] {
        &self.effects
    }

    /// Return one player's governed score.
    #[must_use]
    pub fn score(&self, principal_id: &PrincipalId) -> Option<i64> {
        self.scores
            .iter()
            .find(|score| &score.principal_id == principal_id)
            .map(|score| score.points)
    }

    /// Whether a player currently has an exact unilateral capability.
    #[must_use]
    pub fn has_capability(
        &self,
        principal_id: &PrincipalId,
        capability: GovernanceCapabilityWire,
    ) -> bool {
        self.capabilities
            .iter()
            .any(|grant| &grant.principal_id == principal_id && grant.capability == capability)
    }

    #[must_use]
    pub const fn redeal_epoch(&self) -> u64 {
        self.redeal_epoch
    }

    #[must_use]
    pub const fn game_ended(&self) -> bool {
        self.game_ended
    }

    #[must_use]
    pub fn is_kicked(&self, principal_id: &PrincipalId) -> bool {
        self.members
            .iter()
            .any(|member| &member.principal_id == principal_id && member.kicked)
    }

    fn submit_unchecked(
        &mut self,
        invocation: &GovernanceInvocation,
    ) -> Result<GovernanceReceipt, GovernanceError> {
        self.advance_to(invocation.logical_tick)?;
        self.require_connected_member(&invocation.issuer)?;
        match &invocation.command.command {
            GovernanceCommandWire::Execute { action } => {
                let capability = required_capability(action)?;
                if !self.has_capability(&invocation.issuer, capability) {
                    return Err(GovernanceError::MissingCapability);
                }
                let effect_id = event_id("direct", invocation.command_id.as_str())?;
                self.apply_effect(
                    effect_id.clone(),
                    action.clone(),
                    EffectAuthority::UnilateralCapability {
                        principal_id: invocation.issuer.clone(),
                        capability,
                    },
                )?;
                Ok(GovernanceReceipt {
                    command_id: invocation.command_id.clone(),
                    proposal_id: None,
                    effect_id: Some(effect_id),
                    vote_counted: None,
                })
            }
            GovernanceCommandWire::StartVote { action } => {
                required_capability(action)?;
                let proposal_id = proposal_id(&invocation.command_id)?;
                let eligibility = self.eligibility_for(action);
                if !eligibility.iter().any(|record| record.eligible) {
                    return Err(GovernanceError::NoEligibleVoters);
                }
                let deadline_tick = invocation
                    .logical_tick
                    .checked_add(self.vote_duration_ticks)
                    .ok_or(GovernanceError::ArithmeticOverflow)?;
                self.proposals.push(GovernanceProposal {
                    proposal_id: proposal_id.clone(),
                    command_id: invocation.command_id.clone(),
                    proposer: invocation.issuer.clone(),
                    opened_tick: invocation.logical_tick,
                    deadline_tick,
                    action: action.clone(),
                    eligibility,
                    votes: Vec::new(),
                    status: ProposalStatus::Pending,
                });
                Ok(GovernanceReceipt {
                    command_id: invocation.command_id.clone(),
                    proposal_id: Some(proposal_id),
                    effect_id: None,
                    vote_counted: None,
                })
            }
            GovernanceCommandWire::Vote {
                proposal_id,
                choice,
            } => self.cast_vote(invocation, proposal_id, *choice),
        }
    }

    fn cast_vote(
        &mut self,
        invocation: &GovernanceInvocation,
        proposal_id: &ProposalId,
        choice: VoteChoiceWire,
    ) -> Result<GovernanceReceipt, GovernanceError> {
        let index = self
            .proposals
            .iter()
            .position(|proposal| &proposal.proposal_id == proposal_id)
            .ok_or(GovernanceError::UnknownProposal)?;
        if !matches!(self.proposals[index].status, ProposalStatus::Pending) {
            return Err(GovernanceError::ProposalClosed);
        }
        if self.proposals[index]
            .votes
            .iter()
            .any(|vote| vote.voter == invocation.issuer)
        {
            return Err(GovernanceError::AlreadyVoted);
        }
        let eligibility = self.proposals[index]
            .eligibility
            .iter()
            .find(|record| record.principal_id == invocation.issuer)
            .ok_or(GovernanceError::UnknownPrincipal)?
            .clone();
        let counted = eligibility.eligible;
        self.proposals[index].votes.push(GovernanceVoteRecord {
            command_id: invocation.command_id.clone(),
            voter: invocation.issuer.clone(),
            choice,
            counted,
            exclusion: eligibility.exclusion,
        });
        let effect_id = self.maybe_finalize_from_votes(index)?;
        Ok(GovernanceReceipt {
            command_id: invocation.command_id.clone(),
            proposal_id: Some(proposal_id.clone()),
            effect_id,
            vote_counted: Some(counted),
        })
    }

    fn maybe_finalize_from_votes(
        &mut self,
        proposal_index: usize,
    ) -> Result<Option<EventId>, GovernanceError> {
        let tally = proposal_tally(&self.proposals[proposal_index]);
        if tally.has_approval_majority() {
            let proposal_id = self.proposals[proposal_index].proposal_id.clone();
            let action = self.proposals[proposal_index].action.clone();
            let effect_id = event_id("proposal", proposal_id.as_str())?;
            self.apply_effect(
                effect_id.clone(),
                action,
                EffectAuthority::ApprovedVote {
                    proposal_id: proposal_id.clone(),
                },
            )?;
            self.proposals[proposal_index].status = ProposalStatus::Approved {
                tally,
                effect_id: effect_id.clone(),
            };
            Ok(Some(effect_id))
        } else if tally.has_rejection_majority() || tally.every_eligible_voted() {
            self.proposals[proposal_index].status = ProposalStatus::Rejected {
                tally,
                reason: if tally.has_rejection_majority() {
                    ProposalRejection::MajorityRejected
                } else {
                    ProposalRejection::AllVotesNoMajority
                },
            };
            Ok(None)
        } else {
            Ok(None)
        }
    }

    fn apply_effect(
        &mut self,
        effect_id: EventId,
        action: GovernedActionWire,
        authority: EffectAuthority,
    ) -> Result<(), GovernanceError> {
        match &action {
            GovernedActionWire::AdjustScore { target, delta } => {
                let score = self
                    .scores
                    .iter_mut()
                    .find(|score| &score.principal_id == target)
                    .ok_or(GovernanceError::UnknownPrincipal)?;
                score.points = score
                    .points
                    .checked_add(i64::from(*delta))
                    .ok_or(GovernanceError::ArithmeticOverflow)?;
            }
            GovernedActionWire::ChangeRights {
                target,
                capability,
                change,
            } => {
                if !self.is_active_member(target) {
                    return Err(GovernanceError::UnknownPrincipal);
                }
                match change {
                    RightsChangeWire::Grant => self.grant_capability(target.clone(), *capability),
                    RightsChangeWire::Revoke => self.capabilities.retain(|grant| {
                        grant.principal_id != *target || grant.capability != *capability
                    }),
                }
            }
            GovernedActionWire::Recover { recovery } => match recovery {
                RecoveryActionWire::Redeal => {
                    self.redeal_epoch = self
                        .redeal_epoch
                        .checked_add(1)
                        .ok_or(GovernanceError::ArithmeticOverflow)?;
                }
                RecoveryActionWire::Kick { target } => {
                    let member = self
                        .members
                        .iter_mut()
                        .find(|member| &member.principal_id == target && !member.kicked)
                        .ok_or(GovernanceError::UnknownPrincipal)?;
                    member.kicked = true;
                    member.connected = false;
                    self.capabilities
                        .retain(|grant| grant.principal_id != *target);
                }
                RecoveryActionWire::EndGame => self.game_ended = true,
            },
            GovernedActionWire::Game { .. } | GovernedActionWire::Accuse { .. } => {
                return Err(GovernanceError::DelegatedAction);
            }
        }
        self.effects.push(GovernanceEffect {
            effect_id,
            action,
            authority,
            logical_tick: self.logical_tick,
        });
        Ok(())
    }

    fn eligibility_for(&self, action: &GovernedActionWire) -> Vec<EligibilityRecord> {
        let subject = action_subject(action);
        let kick_target = matches!(
            action,
            GovernedActionWire::Recover {
                recovery: RecoveryActionWire::Kick { .. }
            }
        );
        self.members
            .iter()
            .map(|member| {
                let exclusion = if member.kicked {
                    Some(EligibilityExclusion::KickedAtSnapshot)
                } else if !member.connected {
                    Some(EligibilityExclusion::DisconnectedAtSnapshot)
                } else if subject == Some(&member.principal_id) && kick_target {
                    Some(EligibilityExclusion::TargetOfKick)
                } else if subject == Some(&member.principal_id)
                    && self
                        .accused
                        .iter()
                        .any(|accused| accused.principal_id == member.principal_id)
                {
                    Some(EligibilityExclusion::ConfirmedAccusedSubject)
                } else {
                    None
                };
                EligibilityRecord {
                    principal_id: member.principal_id.clone(),
                    eligible: exclusion.is_none(),
                    exclusion,
                }
            })
            .collect()
    }

    fn require_connected_member(&self, principal_id: &PrincipalId) -> Result<(), GovernanceError> {
        match self
            .members
            .iter()
            .find(|member| &member.principal_id == principal_id)
        {
            Some(member) if member.connected && !member.kicked => Ok(()),
            Some(_) => Err(GovernanceError::NotConnected),
            None => Err(GovernanceError::UnknownPrincipal),
        }
    }

    fn is_active_member(&self, principal_id: &PrincipalId) -> bool {
        self.members
            .iter()
            .any(|member| &member.principal_id == principal_id && !member.kicked)
    }

    fn grant_capability(
        &mut self,
        principal_id: PrincipalId,
        capability: GovernanceCapabilityWire,
    ) {
        if !self.has_capability(&principal_id, capability) {
            self.capabilities.push(CapabilityGrant {
                principal_id,
                capability,
            });
        }
    }

    fn validate(&self) -> Result<(), GovernanceError> {
        if self.proposals.iter().enumerate().any(|(index, proposal)| {
            self.proposals[index + 1..]
                .iter()
                .any(|other| other.proposal_id == proposal.proposal_id)
        }) || self.effects.iter().enumerate().any(|(index, effect)| {
            self.effects[index + 1..]
                .iter()
                .any(|other| other.effect_id == effect.effect_id)
        }) {
            return Err(GovernanceError::Invariant);
        }
        Ok(())
    }
}

fn required_capability(
    action: &GovernedActionWire,
) -> Result<GovernanceCapabilityWire, GovernanceError> {
    if action.rule_layer() == RuleLayerWire::Structural {
        return Err(GovernanceError::StructuralInvariant);
    }
    match action {
        GovernedActionWire::AdjustScore { .. } => Ok(GovernanceCapabilityWire::AdjustScore),
        GovernedActionWire::ChangeRights { .. } => Ok(GovernanceCapabilityWire::ChangeRights),
        GovernedActionWire::Recover { .. } => Ok(GovernanceCapabilityWire::ResolveRecovery),
        GovernedActionWire::Game { .. } | GovernedActionWire::Accuse { .. } => {
            Err(GovernanceError::DelegatedAction)
        }
    }
}

fn action_subject(action: &GovernedActionWire) -> Option<&PrincipalId> {
    match action {
        GovernedActionWire::AdjustScore { target, .. }
        | GovernedActionWire::ChangeRights { target, .. }
        | GovernedActionWire::Recover {
            recovery: RecoveryActionWire::Kick { target },
        } => Some(target),
        GovernedActionWire::Game { .. }
        | GovernedActionWire::Accuse { .. }
        | GovernedActionWire::Recover {
            recovery: RecoveryActionWire::Redeal | RecoveryActionWire::EndGame,
        } => None,
    }
}

fn proposal_tally(proposal: &GovernanceProposal) -> ProposalTally {
    let mut tally = ProposalTally {
        eligible: u16::try_from(
            proposal
                .eligibility
                .iter()
                .filter(|record| record.eligible)
                .count(),
        )
        .unwrap_or(u16::MAX),
        ..ProposalTally::default()
    };
    for vote in proposal.votes.iter().filter(|vote| vote.counted) {
        match vote.choice {
            VoteChoiceWire::Approve => tally.approvals += 1,
            VoteChoiceWire::Reject => tally.rejections += 1,
            VoteChoiceWire::Abstain => tally.abstentions += 1,
        }
    }
    tally
}

fn invocation_hash(invocation: &GovernanceInvocation) -> Result<SemanticHash, GovernanceError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-governance-invocation-v1\0");
    hash_field(&mut hasher, invocation.command_id.as_str().as_bytes());
    hash_field(&mut hasher, invocation.issuer.as_str().as_bytes());
    hash_field(&mut hasher, &invocation.logical_tick.to_be_bytes());
    hash_field(
        &mut hasher,
        &encode_governance_command(&invocation.command)
            .map_err(|_| GovernanceError::InvalidCommand)?,
    );
    Ok(SemanticHash(*hasher.finalize().as_bytes()))
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn proposal_id(command_id: &CommandId) -> Result<ProposalId, GovernanceError> {
    derived_id("proposal", command_id.as_str(), ProposalId::new)
}

fn event_id(kind: &str, basis: &str) -> Result<EventId, GovernanceError> {
    derived_id(kind, basis, EventId::new)
}

fn derived_id<T>(
    kind: &str,
    basis: &str,
    constructor: impl FnOnce(String) -> Result<T, poche_protocol::IdentifierError>,
) -> Result<T, GovernanceError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-governance-id-v1\0");
    hash_field(&mut hasher, kind.as_bytes());
    hash_field(&mut hasher, basis.as_bytes());
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(24);
    for byte in &digest.as_bytes()[..12] {
        use core::fmt::Write;
        write!(&mut suffix, "{byte:02x}").map_err(|_| GovernanceError::InvalidIdentifier)?;
    }
    constructor(format!("{kind}-{suffix}")).map_err(|_| GovernanceError::InvalidIdentifier)
}

/// Stable governance failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GovernanceError {
    InvalidRoster,
    InvalidCommand,
    InvalidIdentifier,
    UnknownPrincipal,
    NotConnected,
    MissingCapability,
    DelegatedAction,
    StructuralInvariant,
    UnknownProposal,
    ProposalClosed,
    AlreadyVoted,
    NoEligibleVoters,
    UnconfirmedFinding,
    StaleTick,
    ConflictingCommandId,
    ArithmeticOverflow,
    Invariant,
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoster => "governance roster or vote duration is invalid",
            Self::InvalidCommand => "typed governance command is invalid",
            Self::InvalidIdentifier => "derived governance identifier is invalid",
            Self::UnknownPrincipal => "governance principal is unknown",
            Self::NotConnected => "governance principal is not active and connected",
            Self::MissingCapability => "unilateral governance capability is missing",
            Self::DelegatedAction => "game action or accusation belongs to another reducer",
            Self::StructuralInvariant => "structural invariants cannot be governed",
            Self::UnknownProposal => "governance proposal is unknown",
            Self::ProposalClosed => "governance proposal is already final",
            Self::AlreadyVoted => "governance voter already has a visible vote",
            Self::NoEligibleVoters => "governance proposal has no eligible voters",
            Self::UnconfirmedFinding => "accused status requires a confirmed finding",
            Self::StaleTick => "governance logical time cannot move backward",
            Self::ConflictingCommandId => "governance command ID was reused with new content",
            Self::ArithmeticOverflow => "governance arithmetic exceeded its bounds",
            Self::Invariant => "governance state invariant failed",
        })
    }
}

impl std::error::Error for GovernanceError {}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        AccusationId, FindingPublicationWire, GameActionWire, IllegalActionDispositionWire,
        LegalityPolicyWire,
    };

    use super::*;
    use crate::{ActionKnowledge, AuditedGameAction, HistoryActionDisposition, ManualAccusation};

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).unwrap()
    }

    fn command_id(value: &str) -> CommandId {
        CommandId::new(value).unwrap()
    }

    fn command(command: GovernanceCommandWire) -> GovernanceCommandV1 {
        GovernanceCommandV1::new(command).unwrap()
    }

    fn invocation(
        id: &str,
        issuer: &str,
        tick: u64,
        command: GovernanceCommandWire,
    ) -> GovernanceInvocation {
        GovernanceInvocation {
            command_id: command_id(id),
            issuer: principal(issuer),
            logical_tick: tick,
            command: self::command(command),
        }
    }

    fn state(names: &[&str]) -> GovernanceState {
        GovernanceState::new(
            names.iter().copied().map(principal),
            DEFAULT_VOTE_DURATION_TICKS,
        )
        .unwrap()
    }

    fn start_vote(action: GovernedActionWire) -> GovernanceCommandWire {
        GovernanceCommandWire::StartVote { action }
    }

    fn state_with_confirmed_accused() -> GovernanceState {
        let mut audit = RetrospectiveAudit::default();
        audit
            .append_action(AuditedGameAction {
                event_id: EventId::new("john-off-suit").unwrap(),
                sequence: 1,
                round_id: 1,
                actor: principal("john"),
                disposition: HistoryActionDisposition::AttemptedStructurallyValid,
                action: GameActionWire::Play { card: 26 },
                led_suit: Some(0),
                knowledge_at_action: ActionKnowledge {
                    known_held_cards_before: vec![26, 4],
                },
            })
            .unwrap();
        let finding = audit.audit(&principal("alice-device")).unwrap()[0].clone();
        let mut state = state(&["alice", "bob", "john"]);
        state
            .register_confirmed_finding(&audit, &finding.finding_id)
            .unwrap();
        state
    }

    fn proposal_record<'a>(state: &'a GovernanceState, id: &ProposalId) -> &'a GovernanceProposal {
        state
            .proposals()
            .iter()
            .find(|candidate| &candidate.proposal_id == id)
            .unwrap()
    }

    fn vote(proposal_id: ProposalId, choice: VoteChoiceWire) -> GovernanceCommandWire {
        GovernanceCommandWire::Vote {
            proposal_id,
            choice,
        }
    }

    #[test]
    fn governance_approval_rejection_timeout_and_concurrency_are_deterministic() {
        let mut state = state(&["alice", "bob", "carol"]);
        let score_action = GovernedActionWire::AdjustScore {
            target: principal("alice"),
            delta: 100,
        };
        let first = state
            .submit(invocation(
                "start-score",
                "alice",
                1,
                start_vote(score_action),
            ))
            .unwrap();
        let first_id = first.proposal_id.unwrap();
        state
            .submit(invocation(
                "alice-yes",
                "alice",
                2,
                vote(first_id.clone(), VoteChoiceWire::Approve),
            ))
            .unwrap();
        let approved = state
            .submit(invocation(
                "bob-yes",
                "bob",
                3,
                vote(first_id.clone(), VoteChoiceWire::Approve),
            ))
            .unwrap();
        assert!(approved.effect_id.is_some());
        assert_eq!(state.score(&principal("alice")), Some(100));

        let reject = state
            .submit(invocation(
                "start-reject",
                "alice",
                4,
                start_vote(GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::EndGame,
                }),
            ))
            .unwrap()
            .proposal_id
            .unwrap();
        let concurrent = state
            .submit(invocation(
                "start-timeout",
                "bob",
                4,
                start_vote(GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::Redeal,
                }),
            ))
            .unwrap()
            .proposal_id
            .unwrap();
        for (id, voter) in [("reject-a", "alice"), ("reject-b", "bob")] {
            state
                .submit(invocation(
                    id,
                    voter,
                    5,
                    vote(reject.clone(), VoteChoiceWire::Reject),
                ))
                .unwrap();
        }
        assert!(matches!(
            state
                .proposals()
                .iter()
                .find(|proposal| proposal.proposal_id == reject)
                .unwrap()
                .status,
            ProposalStatus::Rejected {
                reason: ProposalRejection::MajorityRejected,
                ..
            }
        ));
        assert_eq!(state.advance_to(14).unwrap(), vec![concurrent.clone()]);
        assert!(matches!(
            state
                .proposals()
                .iter()
                .find(|proposal| proposal.proposal_id == concurrent)
                .unwrap()
                .status,
            ProposalStatus::Rejected {
                reason: ProposalRejection::DeadlineNoMajority,
                ..
            }
        ));
        assert!(!state.game_ended());
        assert_eq!(state.redeal_epoch(), 0);
    }

    #[test]
    fn governance_accused_vote_is_visible_not_counted_and_afk_actor_cannot_block_recovery() {
        let mut state = state_with_confirmed_accused();
        let proposal = state
            .submit(invocation(
                "remove-john-right",
                "alice",
                1,
                start_vote(GovernedActionWire::ChangeRights {
                    target: principal("john"),
                    capability: GovernanceCapabilityWire::AdjustScore,
                    change: RightsChangeWire::Revoke,
                }),
            ))
            .unwrap()
            .proposal_id
            .unwrap();
        let excluded = state
            .submit(invocation(
                "john-visible-no",
                "john",
                2,
                vote(proposal.clone(), VoteChoiceWire::Reject),
            ))
            .unwrap();
        assert_eq!(excluded.vote_counted, Some(false));
        state
            .submit(invocation(
                "alice-yes-remove",
                "alice",
                3,
                vote(proposal.clone(), VoteChoiceWire::Approve),
            ))
            .unwrap();
        state
            .submit(invocation(
                "bob-yes-remove",
                "bob",
                3,
                vote(proposal.clone(), VoteChoiceWire::Approve),
            ))
            .unwrap();
        let record = proposal_record(&state, &proposal);
        assert!(matches!(record.status, ProposalStatus::Approved { .. }));
        assert_eq!(record.votes.len(), 3);
        assert!(!record.votes[0].counted);

        let kick = state
            .submit(invocation(
                "kick-afk-john",
                "alice",
                4,
                start_vote(GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::Kick {
                        target: principal("john"),
                    },
                }),
            ))
            .unwrap()
            .proposal_id
            .unwrap();
        state
            .submit(invocation(
                "alice-kick-yes",
                "alice",
                5,
                vote(kick.clone(), VoteChoiceWire::Approve),
            ))
            .unwrap();
        state
            .submit(invocation(
                "bob-kick-yes",
                "bob",
                5,
                vote(kick, VoteChoiceWire::Approve),
            ))
            .unwrap();
        assert!(state.is_kicked(&principal("john")));

        state.set_connected(&principal("bob"), false).unwrap();
        let redeal = state
            .submit(invocation(
                "redeal-without-afk",
                "alice",
                6,
                start_vote(GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::Redeal,
                }),
            ))
            .unwrap()
            .proposal_id
            .unwrap();
        state
            .submit(invocation(
                "alice-redeal-yes",
                "alice",
                7,
                vote(redeal, VoteChoiceWire::Approve),
            ))
            .unwrap();
        assert_eq!(state.redeal_epoch(), 1);
    }

    #[test]
    fn governance_capability_score_add_remove_rights_and_structural_denial_are_atomic() {
        let mut state = state(&["alice", "bob"]);
        state
            .add_bootstrap_capability(principal("alice"), GovernanceCapabilityWire::ChangeRights)
            .unwrap();
        state
            .submit(invocation(
                "grant-score",
                "alice",
                1,
                GovernanceCommandWire::Execute {
                    action: GovernedActionWire::ChangeRights {
                        target: principal("alice"),
                        capability: GovernanceCapabilityWire::AdjustScore,
                        change: RightsChangeWire::Grant,
                    },
                },
            ))
            .unwrap();
        for (id, tick, delta) in [("score-add", 2, 100), ("score-remove", 3, -40)] {
            state
                .submit(invocation(
                    id,
                    "alice",
                    tick,
                    GovernanceCommandWire::Execute {
                        action: GovernedActionWire::AdjustScore {
                            target: principal("bob"),
                            delta,
                        },
                    },
                ))
                .unwrap();
        }
        assert_eq!(state.score(&principal("bob")), Some(60));
        state
            .submit(invocation(
                "remove-score-right",
                "alice",
                4,
                GovernanceCommandWire::Execute {
                    action: GovernedActionWire::ChangeRights {
                        target: principal("alice"),
                        capability: GovernanceCapabilityWire::AdjustScore,
                        change: RightsChangeWire::Revoke,
                    },
                },
            ))
            .unwrap();
        let before = state.clone();
        assert_eq!(
            state.submit(invocation(
                "score-denied",
                "alice",
                5,
                GovernanceCommandWire::Execute {
                    action: GovernedActionWire::AdjustScore {
                        target: principal("bob"),
                        delta: 1,
                    },
                },
            )),
            Err(GovernanceError::MissingCapability)
        );
        assert_eq!(state, before);

        let illegal = br#"{"schema_version":1,"command":{"kind":"execute","data":{"action":{"kind":"create_card","data":{"card":52}}}}}"#;
        assert!(poche_protocol::decode_governance_command(illegal).is_err());
        assert_eq!(state, before);

        let modes = [
            LegalityPolicyWire {
                illegal_action: IllegalActionDispositionWire::Prevent,
                finding_publication: FindingPublicationWire::Automatic,
            },
            LegalityPolicyWire {
                illegal_action: IllegalActionDispositionWire::AllowAttempt,
                finding_publication: FindingPublicationWire::AccusationRequired,
            },
        ];
        assert_ne!(modes[0], modes[1]);

        let accusation = ManualAccusation {
            accusation_id: AccusationId::new("unrelated-accusation").unwrap(),
            detector: principal("alice"),
            offending_action_id: EventId::new("not-present").unwrap(),
        };
        assert!(matches!(
            RetrospectiveAudit::default()
                .accuse(accusation)
                .unwrap()
                .outcome,
            crate::AccusationOutcome::Unfounded { .. }
        ));
    }
}
