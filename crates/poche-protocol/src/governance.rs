// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned typed command and legality-policy contract.
//!
//! Text parsers and GUI controls may construct these shapes, but only a
//! validated typed value can enter signing, authorization, voting, or replay.

use core::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::{EventId, GameActionWire, PrincipalId, ProposalId};

/// Stable governance-command schema version.
pub const GOVERNANCE_COMMAND_VERSION_V1: u16 = 1;
/// Maximum absolute score amendment admitted by the wire boundary.
pub const MAX_SCORE_AMENDMENT: i32 = 1_000_000;
/// Maximum canonical JSON size for an isolated typed command.
pub const MAX_GOVERNANCE_COMMAND_BYTES: usize = 4_096;

/// Stable descriptor for hashing and cross-client schema negotiation.
pub const GOVERNANCE_COMMAND_SCHEMA_DESCRIPTOR: &str = concat!(
    "poche.governance-command.v1\n",
    "command=execute|start_vote|vote\n",
    "action=game|adjust_score|change_rights|accuse|recover\n",
    "recovery=redeal|kick|end_game\n",
    "capability=adjust_score|change_rights|resolve_recovery\n",
    "constraints=typed-only;score-delta-nonzero-abs<=1000000;no-structural-card-mutation\n",
);

/// One command whose schema version is explicit at every boundary.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceCommandV1 {
    pub schema_version: u16,
    pub command: GovernanceCommandWire,
}

impl GovernanceCommandV1 {
    /// Construct and validate one v1 command.
    ///
    /// # Errors
    ///
    /// Returns a stable semantic validation error for an invalid payload.
    pub fn new(command: GovernanceCommandWire) -> Result<Self, GovernanceCommandError> {
        let result = Self {
            schema_version: GOVERNANCE_COMMAND_VERSION_V1,
            command,
        };
        result.validate()?;
        Ok(result)
    }

    /// Check the exact v1 refinements after decoding.
    ///
    /// # Errors
    ///
    /// Returns a stable semantic validation error for an invalid payload.
    pub fn validate(&self) -> Result<(), GovernanceCommandError> {
        if self.schema_version != GOVERNANCE_COMMAND_VERSION_V1 {
            return Err(GovernanceCommandError::UnknownVersion);
        }
        self.command.validate()
    }
}

/// Player-facing command vocabulary. Vote wrapping contains an action, never
/// another raw command string or recursively nested vote.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GovernanceCommandWire {
    /// Request direct execution under the action's authority requirement.
    Execute { action: GovernedActionWire },
    /// Propose that eligible members approve one typed action.
    StartVote { action: GovernedActionWire },
    /// Cast one visible choice against a stable proposal.
    Vote {
        proposal_id: ProposalId,
        choice: VoteChoiceWire,
    },
}

impl GovernanceCommandWire {
    /// Return the authority needed before this command can have a shared effect.
    #[must_use]
    pub const fn authority_requirement(&self) -> AuthorityRequirementWire {
        match self {
            Self::Execute { action } => action.authority_requirement(),
            Self::StartVote { .. } | Self::Vote { .. } => AuthorityRequirementWire::ActivePlayer,
        }
    }

    fn validate(&self) -> Result<(), GovernanceCommandError> {
        match self {
            Self::Execute { action } | Self::StartVote { action } => action.validate(),
            Self::Vote { proposal_id, .. } => validate_proposal(proposal_id),
        }
    }
}

/// Actions whose effects may be direct or vote-authorized.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GovernedActionWire {
    /// Ordinary Poche bid/play intent.
    Game { action: GameActionWire },
    /// Add a signed amount to a typed player's score ledger.
    AdjustScore { target: PrincipalId, delta: i32 },
    /// Grant or revoke an exact governable capability.
    ChangeRights {
        target: PrincipalId,
        capability: GovernanceCapabilityWire,
        change: RightsChangeWire,
    },
    /// Point to an action/event that should be evaluated as a rule violation.
    Accuse { offending_event_id: EventId },
    /// Recover from an interrupted or disputed game.
    Recover { recovery: RecoveryActionWire },
}

impl GovernedActionWire {
    /// Structural invariants have no command variant; every representable
    /// action is either ordinary game legality or an explicitly governable
    /// effect.
    #[must_use]
    pub const fn rule_layer(&self) -> RuleLayerWire {
        match self {
            Self::Game { .. } => RuleLayerWire::GameLegality,
            Self::AdjustScore { .. }
            | Self::ChangeRights { .. }
            | Self::Accuse { .. }
            | Self::Recover { .. } => RuleLayerWire::Governable,
        }
    }

    /// Return the exact direct-execution authority. An approved proposal is an
    /// alternative only for `VoteOrCapability` actions.
    #[must_use]
    pub const fn authority_requirement(&self) -> AuthorityRequirementWire {
        match self {
            Self::Game { .. } => AuthorityRequirementWire::CurrentGameActor,
            Self::Accuse { .. } => AuthorityRequirementWire::ActivePlayer,
            Self::AdjustScore { .. } => {
                AuthorityRequirementWire::VoteOrCapability(GovernanceCapabilityWire::AdjustScore)
            }
            Self::ChangeRights { .. } => {
                AuthorityRequirementWire::VoteOrCapability(GovernanceCapabilityWire::ChangeRights)
            }
            Self::Recover { .. } => AuthorityRequirementWire::VoteOrCapability(
                GovernanceCapabilityWire::ResolveRecovery,
            ),
        }
    }

    fn validate(&self) -> Result<(), GovernanceCommandError> {
        match self {
            Self::Game {
                action: GameActionWire::Bid { tricks },
            } if *tricks > 7 => Err(GovernanceCommandError::InvalidGameAction),
            Self::Game {
                action: GameActionWire::Play { card },
            } if *card >= 52 => Err(GovernanceCommandError::InvalidGameAction),
            Self::AdjustScore { target, delta }
                if !target.validate()
                    || *delta == 0
                    || delta
                        .checked_abs()
                        .is_none_or(|value| value > MAX_SCORE_AMENDMENT) =>
            {
                Err(GovernanceCommandError::InvalidScoreAmendment)
            }
            Self::ChangeRights { target, .. } if !target.validate() => {
                Err(GovernanceCommandError::InvalidPrincipal)
            }
            Self::Accuse { offending_event_id } if !offending_event_id.validate() => {
                Err(GovernanceCommandError::InvalidEvent)
            }
            Self::Recover {
                recovery: RecoveryActionWire::Kick { target },
            } if !target.validate() => Err(GovernanceCommandError::InvalidPrincipal),
            _ => Ok(()),
        }
    }
}

/// The three invariant layers kept separate by every policy decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum RuleLayerWire {
    /// Codec, identity, uniqueness, and finite-universe invariants. No user
    /// command can target this layer.
    Structural,
    /// Poche rules that can be prevented or accepted as auditable attempts.
    GameLegality,
    /// Score, rights, and recovery effects subject to authority/governance.
    Governable,
}

/// Capabilities that can substitute for a successful vote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceCapabilityWire {
    AdjustScore,
    ChangeRights,
    ResolveRecovery,
}

/// Exact authority consequence attached to a typed command/action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "capability",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AuthorityRequirementWire {
    CurrentGameActor,
    ActivePlayer,
    VoteOrCapability(GovernanceCapabilityWire),
}

/// Grant or revoke an exact capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum RightsChangeWire {
    Grant,
    Revoke,
}

/// Supported recovery effects. There is deliberately no arbitrary state edit.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RecoveryActionWire {
    Redeal,
    Kick { target: PrincipalId },
    EndGame,
}

/// Visible vote choice. Eligibility/counting is recorded separately in 4.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoiceWire {
    Approve,
    Reject,
    Abstain,
}

/// Shared room handling for an action that violates Poche rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum IllegalActionDispositionWire {
    /// Reject before the semantic action enters accepted history.
    Prevent,
    /// Record the attempt and let audit/recovery policy handle it.
    AllowAttempt,
}

/// When a detected finding becomes a shared finding record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum FindingPublicationWire {
    Automatic,
    AccusationRequired,
}

/// Shared legality policy. Structural validation always precedes this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegalityPolicyWire {
    pub illegal_action: IllegalActionDispositionWire,
    pub finding_publication: FindingPublicationWire,
}

/// Device-local response to an observed finding. It may create a proposal but
/// can never commit a shared effect by itself.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FindingAutomationWire {
    ObserveOnly,
    Propose { action: GovernedActionWire },
}

/// Stable typed-command validation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GovernanceCommandError {
    UnknownVersion,
    Oversize,
    InvalidJson,
    NonCanonical,
    InvalidProposal,
    InvalidPrincipal,
    InvalidEvent,
    InvalidGameAction,
    InvalidScoreAmendment,
}

impl fmt::Display for GovernanceCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownVersion => "unknown governance command version",
            Self::Oversize => "governance command exceeds its byte bound",
            Self::InvalidJson => "governance command is not registered JSON",
            Self::NonCanonical => "governance command is not canonical JSON",
            Self::InvalidProposal => "governance proposal identifier is invalid",
            Self::InvalidPrincipal => "governance principal identifier is invalid",
            Self::InvalidEvent => "accused event identifier is invalid",
            Self::InvalidGameAction => "game action is outside the v1 Poche bounds",
            Self::InvalidScoreAmendment => "score amendment is zero or outside the v1 bound",
        })
    }
}

impl std::error::Error for GovernanceCommandError {}

/// Encode one validated typed command as canonical JSON. This isolated codec
/// will become a signed payload field; the source text is never returned.
///
/// # Errors
///
/// Rejects invalid semantics, serialization failure, or oversize output.
pub fn encode_governance_command(
    command: &GovernanceCommandV1,
) -> Result<Vec<u8>, GovernanceCommandError> {
    command.validate()?;
    let encoded = serde_json::to_vec(command).map_err(|_| GovernanceCommandError::InvalidJson)?;
    if encoded.len() > MAX_GOVERNANCE_COMMAND_BYTES {
        Err(GovernanceCommandError::Oversize)
    } else {
        Ok(encoded)
    }
}

/// Decode only canonical JSON into a validated typed command.
///
/// # Errors
///
/// Rejects oversized, malformed, noncanonical, or semantically invalid input.
pub fn decode_governance_command(
    bytes: &[u8],
) -> Result<GovernanceCommandV1, GovernanceCommandError> {
    if bytes.len() > MAX_GOVERNANCE_COMMAND_BYTES {
        return Err(GovernanceCommandError::Oversize);
    }
    let command: GovernanceCommandV1 =
        serde_json::from_slice(bytes).map_err(|_| GovernanceCommandError::InvalidJson)?;
    command.validate()?;
    if encode_governance_command(&command)? != bytes {
        return Err(GovernanceCommandError::NonCanonical);
    }
    Ok(command)
}

fn validate_proposal(proposal: &ProposalId) -> Result<(), GovernanceCommandError> {
    if proposal.validate() {
        Ok(())
    } else {
        Err(GovernanceCommandError::InvalidProposal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).unwrap()
    }

    #[test]
    fn command_codec_round_trips_all_required_meanings() {
        let commands = [
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::Game {
                    action: GameActionWire::Play { card: 48 },
                },
            },
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::AdjustScore {
                    target: principal("player1"),
                    delta: 100,
                },
            },
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::ChangeRights {
                    target: principal("player1"),
                    capability: GovernanceCapabilityWire::AdjustScore,
                    change: RightsChangeWire::Revoke,
                },
            },
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::Accuse {
                    offending_event_id: EventId::new("event-2").unwrap(),
                },
            },
            GovernanceCommandWire::StartVote {
                action: GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::Redeal,
                },
            },
            GovernanceCommandWire::Vote {
                proposal_id: ProposalId::new("proposal-1").unwrap(),
                choice: VoteChoiceWire::Approve,
            },
        ];
        for command in commands {
            let command = GovernanceCommandV1::new(command).unwrap();
            let encoded = encode_governance_command(&command).unwrap();
            assert_eq!(decode_governance_command(&encoded).unwrap(), command);
        }
    }

    #[test]
    fn structural_mutation_and_raw_execution_are_not_in_the_schema() {
        let unknown = br#"{"schema_version":1,"command":{"kind":"execute","data":{"action":{"kind":"create_card","data":{"card":52}}}}}"#;
        assert_eq!(
            decode_governance_command(unknown),
            Err(GovernanceCommandError::InvalidJson)
        );
        let zero_score = GovernanceCommandV1::new(GovernanceCommandWire::Execute {
            action: GovernedActionWire::AdjustScore {
                target: principal("player1"),
                delta: 0,
            },
        });
        assert_eq!(
            zero_score,
            Err(GovernanceCommandError::InvalidScoreAmendment)
        );
    }

    #[test]
    fn authority_and_legality_layers_are_explicit() {
        let score = GovernedActionWire::AdjustScore {
            target: principal("player1"),
            delta: 100,
        };
        assert_eq!(score.rule_layer(), RuleLayerWire::Governable);
        assert_eq!(
            score.authority_requirement(),
            AuthorityRequirementWire::VoteOrCapability(GovernanceCapabilityWire::AdjustScore)
        );
        let play = GovernedActionWire::Game {
            action: GameActionWire::Play { card: 48 },
        };
        assert_eq!(play.rule_layer(), RuleLayerWire::GameLegality);
        assert_eq!(
            play.authority_requirement(),
            AuthorityRequirementWire::CurrentGameActor
        );
    }
}
