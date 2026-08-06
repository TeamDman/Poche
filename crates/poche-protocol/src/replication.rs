// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned player/device identity and replicated-log wire contract.

#![allow(
    clippy::missing_errors_doc,
    reason = "every public validator/codec returns the documented stable ReplicationWireError categories"
)]

use core::fmt;
use std::collections::BTreeSet;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::{
    CertificateId, CommandId, DeviceId, PrincipalId, ProposalId, RoomId, SemanticHash,
    SignatureAlgorithm, SignatureBytes, SignatureIntent, SignatureMetadata, SnapshotId,
};

/// Initial experimental replicated-log schema.
pub const REPLICATION_SCHEMA_VERSION_V1: u16 = 1;
/// Independent canonical signature-domain version for replicated objects.
pub const REPLICATION_SIGNATURE_DOMAIN_V1: u16 = 1;
/// Defensive certificate/device bound per player in one membership epoch.
pub const MAX_DEVICES_PER_PLAYER: usize = 8;
/// Defensive command-reference bound in one replicated candidate.
pub const MAX_REPLICATED_COMMANDS: usize = 64;

const CERTIFICATE_DOMAIN: &[u8] = b"POCHE\0DEVICE-CERTIFICATE\0V1";
const REVOCATION_DOMAIN: &[u8] = b"POCHE\0DEVICE-REVOCATION\0V1";
const CANDIDATE_BODY_DOMAIN: &[u8] = b"POCHE\0REPLICATED-CANDIDATE-BODY\0V1";
const CANDIDATE_SIGNATURE_DOMAIN: &[u8] = b"POCHE\0REPLICATED-CANDIDATE\0V1";
const VOTE_DOMAIN: &[u8] = b"POCHE\0REPLICATED-VOTE\0V1";
const COMMIT_DOMAIN: &[u8] = b"POCHE\0REPLICATED-COMMIT\0V1";
const SNAPSHOT_DOMAIN: &[u8] = b"POCHE\0REPLICATED-SNAPSHOT\0V1";

/// Stable public player root. `player_id` retains the existing `PrincipalId`
/// bytes and is bound to the Ed25519 public key.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRootWire {
    pub schema_version: u16,
    pub player_id: PrincipalId,
    pub signing_public_key: String,
}

impl PlayerRootWire {
    /// Validate version, lowercase key shape, and player/key binding.
    ///
    /// # Errors
    ///
    /// Rejects an unknown version, malformed key, or changed player ID.
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1 {
            return Err(ReplicationWireError::UnknownVersion);
        }
        if !is_public_key(&self.signing_public_key)
            || self.player_id.as_str() != self.signing_public_key
        {
            return Err(ReplicationWireError::IdentityBinding);
        }
        Ok(())
    }
}

/// Disclosed location/ownership of a device secret. This is not proof of
/// hardware protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCustodyWire {
    NativeLocal,
    BrowserLocal,
    GatewayCustodied,
    LegacySelf,
}

/// Exact capability delegated to one device key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCapabilityWire {
    Propose,
    Vote,
    ReceivePrivateProjection,
    RequestDeviceChange,
}

/// Signature intent whose key is a device rather than a player root.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSignatureIntentWire {
    pub domain_version: u16,
    pub algorithm: SignatureAlgorithm,
    pub key_id: DeviceId,
}

/// Attached device signature.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSignatureMetadataWire {
    pub domain_version: u16,
    pub algorithm: SignatureAlgorithm,
    pub key_id: DeviceId,
    pub signature: SignatureBytes,
}

impl DeviceSignatureMetadataWire {
    #[must_use]
    pub fn intent(&self) -> DeviceSignatureIntentWire {
        DeviceSignatureIntentWire {
            domain_version: self.domain_version,
            algorithm: self.algorithm,
            key_id: self.key_id.clone(),
        }
    }
}

/// Root-signable device authorization before the signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedDeviceCertificateWire {
    pub schema_version: u16,
    pub certificate_id: CertificateId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub device_signing_public_key: String,
    pub sequence: u64,
    pub valid_from_membership_epoch: u64,
    pub valid_through_membership_epoch: Option<u64>,
    pub capabilities: Vec<DeviceCapabilityWire>,
    pub custody: DeviceCustodyWire,
    pub signature_intent: SignatureIntent,
}

impl UnsignedDeviceCertificateWire {
    /// Validate the exact v1 device refinement.
    ///
    /// # Errors
    ///
    /// Rejects bad identity binding, epochs, capabilities, or root intent.
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1 {
            return Err(ReplicationWireError::UnknownVersion);
        }
        if !self.certificate_id.validate()
            || !is_public_key(&self.device_signing_public_key)
            || self.device_id.as_str() != self.device_signing_public_key
            || self.sequence == 0
            || self.valid_from_membership_epoch == 0
            || self
                .valid_through_membership_epoch
                .is_some_and(|through| through < self.valid_from_membership_epoch)
        {
            return Err(ReplicationWireError::InvalidCertificate);
        }
        validate_capabilities(&self.capabilities)?;
        validate_root_intent(&self.signature_intent, &self.player_id)
    }

    /// Attach the root signature without changing signed fields.
    ///
    /// # Errors
    ///
    /// Rejects an invalid unsigned certificate or signature shape.
    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<DeviceCertificateWire, ReplicationWireError> {
        self.validate()?;
        let signed = DeviceCertificateWire {
            schema_version: self.schema_version,
            certificate_id: self.certificate_id,
            player_id: self.player_id,
            device_id: self.device_id,
            device_signing_public_key: self.device_signing_public_key,
            sequence: self.sequence,
            valid_from_membership_epoch: self.valid_from_membership_epoch,
            valid_through_membership_epoch: self.valid_through_membership_epoch,
            capabilities: self.capabilities,
            custody: self.custody,
            signature: SignatureMetadata {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        };
        signed.validate()?;
        Ok(signed)
    }
}

/// Root-signed device authorization.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCertificateWire {
    pub schema_version: u16,
    pub certificate_id: CertificateId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub device_signing_public_key: String,
    pub sequence: u64,
    pub valid_from_membership_epoch: u64,
    pub valid_through_membership_epoch: Option<u64>,
    pub capabilities: Vec<DeviceCapabilityWire>,
    pub custody: DeviceCustodyWire,
    pub signature: SignatureMetadata,
}

impl DeviceCertificateWire {
    /// Validate structure and reconstruct the unsigned root-signing view.
    ///
    /// # Errors
    ///
    /// Rejects any invalid signed field or root signature intent.
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.unsigned().validate()
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedDeviceCertificateWire {
        UnsignedDeviceCertificateWire {
            schema_version: self.schema_version,
            certificate_id: self.certificate_id.clone(),
            player_id: self.player_id.clone(),
            device_id: self.device_id.clone(),
            device_signing_public_key: self.device_signing_public_key.clone(),
            sequence: self.sequence,
            valid_from_membership_epoch: self.valid_from_membership_epoch,
            valid_through_membership_epoch: self.valid_through_membership_epoch,
            capabilities: self.capabilities.clone(),
            custody: self.custody,
            signature_intent: self.signature.intent(),
        }
    }

    #[must_use]
    pub fn is_valid_at(&self, membership_epoch: u64) -> bool {
        membership_epoch >= self.valid_from_membership_epoch
            && self
                .valid_through_membership_epoch
                .is_none_or(|through| membership_epoch <= through)
    }

    #[must_use]
    pub fn has_capability(&self, capability: DeviceCapabilityWire) -> bool {
        self.capabilities.binary_search(&capability).is_ok()
    }
}

/// Root-signable device revocation.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedDeviceRevocationWire {
    pub schema_version: u16,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub certificate_sequence: u64,
    pub effective_membership_epoch: u64,
    pub signature_intent: SignatureIntent,
}

impl UnsignedDeviceRevocationWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1 {
            return Err(ReplicationWireError::UnknownVersion);
        }
        if !self.device_id.validate()
            || self.certificate_sequence == 0
            || self.effective_membership_epoch == 0
        {
            return Err(ReplicationWireError::InvalidRevocation);
        }
        validate_root_intent(&self.signature_intent, &self.player_id)
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<DeviceRevocationWire, ReplicationWireError> {
        self.validate()?;
        Ok(DeviceRevocationWire {
            schema_version: self.schema_version,
            player_id: self.player_id,
            device_id: self.device_id,
            certificate_sequence: self.certificate_sequence,
            effective_membership_epoch: self.effective_membership_epoch,
            signature: SignatureMetadata {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Root-signed device revocation effective at an epoch boundary.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceRevocationWire {
    pub schema_version: u16,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub certificate_sequence: u64,
    pub effective_membership_epoch: u64,
    pub signature: SignatureMetadata,
}

impl DeviceRevocationWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.unsigned().validate()
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedDeviceRevocationWire {
        UnsignedDeviceRevocationWire {
            schema_version: self.schema_version,
            player_id: self.player_id.clone(),
            device_id: self.device_id.clone(),
            certificate_sequence: self.certificate_sequence,
            effective_membership_epoch: self.effective_membership_epoch,
            signature_intent: self.signature.intent(),
        }
    }
}

/// One immutable semantic command reference in a candidate batch.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicatedCommandRefWire {
    pub command_id: CommandId,
    pub semantic_hash: SemanticHash,
}

/// Membership result jointly certified by old and new player sets.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipTransitionWire {
    pub next_membership_epoch: u64,
    pub active_players: Vec<PrincipalId>,
    pub revoked_devices: Vec<DeviceId>,
}

impl MembershipTransitionWire {
    pub fn validate(&self, current_epoch: u64) -> Result<(), ReplicationWireError> {
        if self.next_membership_epoch != current_epoch.saturating_add(1)
            || self.active_players.len() < 2
            || !strict_sorted_unique(&self.active_players)
            || !strict_sorted_unique(&self.revoked_devices)
        {
            return Err(ReplicationWireError::InvalidMembershipTransition);
        }
        Ok(())
    }
}

/// Candidate content whose hash defines the proposal value.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusCandidateBodyWire {
    pub schema_version: u16,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub parent_event_id: Option<crate::EventId>,
    pub parent_event_hash: SemanticHash,
    pub height: u64,
    pub round: u32,
    pub proposer_player_id: PrincipalId,
    pub proposer_device_id: DeviceId,
    pub ordered_commands: Vec<ReplicatedCommandRefWire>,
    pub membership_transition: Option<MembershipTransitionWire>,
}

impl ConsensusCandidateBodyWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1
            || !self.room_id.validate()
            || self.membership_epoch == 0
            || self.height == 0
            || self.ordered_commands.is_empty()
            || self.ordered_commands.len() > MAX_REPLICATED_COMMANDS
            || !commands_are_canonical(&self.ordered_commands)
        {
            return Err(ReplicationWireError::InvalidCandidate);
        }
        if self.height == 1 && self.parent_event_id.is_some()
            || self.height > 1 && self.parent_event_id.is_none()
        {
            return Err(ReplicationWireError::InvalidCandidate);
        }
        if let Some(transition) = &self.membership_transition {
            transition.validate(self.membership_epoch)?;
        }
        Ok(())
    }
}

/// Device-signable candidate with its derived stable proposal ID.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedConsensusCandidateWire {
    pub body: ConsensusCandidateBodyWire,
    pub proposal_id: ProposalId,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedConsensusCandidateWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.body.validate()?;
        if self.proposal_id != derive_replicated_proposal_id(&self.body)?
            || self.signature_intent.domain_version != REPLICATION_SIGNATURE_DOMAIN_V1
            || self.signature_intent.algorithm != SignatureAlgorithm::Ed25519
            || self.signature_intent.key_id != self.body.proposer_device_id
        {
            return Err(ReplicationWireError::InvalidCandidate);
        }
        Ok(())
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<ConsensusCandidateWire, ReplicationWireError> {
        self.validate()?;
        Ok(ConsensusCandidateWire {
            body: self.body,
            proposal_id: self.proposal_id,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Signed candidate proposed by one certified device.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusCandidateWire {
    pub body: ConsensusCandidateBodyWire,
    pub proposal_id: ProposalId,
    pub signature: DeviceSignatureMetadataWire,
}

impl ConsensusCandidateWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.unsigned().validate()
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedConsensusCandidateWire {
        UnsignedConsensusCandidateWire {
            body: self.body.clone(),
            proposal_id: self.proposal_id.clone(),
            signature_intent: self.signature.intent(),
        }
    }
}

/// Consensus vote phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusVotePhaseWire {
    Prevote,
    Precommit,
}

/// Player vote value. Nil advances/retries a round without choosing a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "kind", content = "hash", rename_all = "snake_case")]
pub enum ConsensusValueWire {
    Nil,
    Candidate(SemanticHash),
}

/// Device-signable consensus vote. Voting power is later deduplicated by
/// `voter_player_id`, never by device.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedConsensusVoteWire {
    pub schema_version: u16,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub height: u64,
    pub round: u32,
    pub phase: ConsensusVotePhaseWire,
    pub value: ConsensusValueWire,
    pub voter_player_id: PrincipalId,
    pub voter_device_id: DeviceId,
    pub lock_round: Option<u32>,
    pub lock_proof_hash: Option<SemanticHash>,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedConsensusVoteWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1
            || self.membership_epoch == 0
            || self.height == 0
            || self.signature_intent.domain_version != REPLICATION_SIGNATURE_DOMAIN_V1
            || self.signature_intent.algorithm != SignatureAlgorithm::Ed25519
            || self.signature_intent.key_id != self.voter_device_id
            || self.lock_round.is_some() != self.lock_proof_hash.is_some()
            || self.lock_round.is_some_and(|locked| locked >= self.round)
        {
            return Err(ReplicationWireError::InvalidVote);
        }
        Ok(())
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<ConsensusVoteWire, ReplicationWireError> {
        self.validate()?;
        Ok(ConsensusVoteWire {
            schema_version: self.schema_version,
            room_id: self.room_id,
            membership_epoch: self.membership_epoch,
            height: self.height,
            round: self.round,
            phase: self.phase,
            value: self.value,
            voter_player_id: self.voter_player_id,
            voter_device_id: self.voter_device_id,
            lock_round: self.lock_round,
            lock_proof_hash: self.lock_proof_hash,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Signed prevote or precommit.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusVoteWire {
    pub schema_version: u16,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub height: u64,
    pub round: u32,
    pub phase: ConsensusVotePhaseWire,
    pub value: ConsensusValueWire,
    pub voter_player_id: PrincipalId,
    pub voter_device_id: DeviceId,
    pub lock_round: Option<u32>,
    pub lock_proof_hash: Option<SemanticHash>,
    pub signature: DeviceSignatureMetadataWire,
}

impl ConsensusVoteWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.unsigned().validate()
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedConsensusVoteWire {
        UnsignedConsensusVoteWire {
            schema_version: self.schema_version,
            room_id: self.room_id.clone(),
            membership_epoch: self.membership_epoch,
            height: self.height,
            round: self.round,
            phase: self.phase,
            value: self.value,
            voter_player_id: self.voter_player_id.clone(),
            voter_device_id: self.voter_device_id.clone(),
            lock_round: self.lock_round,
            lock_proof_hash: self.lock_proof_hash,
            signature_intent: self.signature.intent(),
        }
    }
}

/// Strict-majority precommit evidence for one candidate.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitCertificateWire {
    pub schema_version: u16,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub height: u64,
    pub round: u32,
    pub candidate_hash: SemanticHash,
    pub precommits: Vec<ConsensusVoteWire>,
}

impl CommitCertificateWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1
            || self.membership_epoch == 0
            || self.height == 0
            || self.precommits.is_empty()
        {
            return Err(ReplicationWireError::InvalidCommitCertificate);
        }
        let mut players = BTreeSet::new();
        for vote in &self.precommits {
            vote.validate()?;
            if vote.room_id != self.room_id
                || vote.membership_epoch != self.membership_epoch
                || vote.height != self.height
                || vote.round != self.round
                || vote.phase != ConsensusVotePhaseWire::Precommit
                || vote.value != ConsensusValueWire::Candidate(self.candidate_hash)
                || !players.insert(vote.voter_player_id.clone())
            {
                return Err(ReplicationWireError::InvalidCommitCertificate);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn unique_player_votes(&self) -> usize {
        self.precommits
            .iter()
            .map(|vote| &vote.voter_player_id)
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// One committed hash-linked replicated event.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicatedEventWire {
    pub schema_version: u16,
    pub event_id: crate::EventId,
    pub candidate: ConsensusCandidateWire,
    pub certificate: CommitCertificateWire,
    pub successor_state_hash: SemanticHash,
}

impl ReplicatedEventWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        self.candidate.validate()?;
        self.certificate.validate()?;
        let candidate_hash = consensus_candidate_hash(&self.candidate.body)?;
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1
            || self.event_id != derive_replicated_event_id(candidate_hash)?
            || self.certificate.candidate_hash != candidate_hash
            || self.certificate.room_id != self.candidate.body.room_id
            || self.certificate.membership_epoch != self.candidate.body.membership_epoch
            || self.certificate.height != self.candidate.body.height
        {
            return Err(ReplicationWireError::InvalidReplicatedEvent);
        }
        Ok(())
    }
}

/// Certified-head snapshot descriptor. Snapshot payload bytes remain outside
/// this public metadata and are checked against `state_hash`.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicatedSnapshotWire {
    pub schema_version: u16,
    pub snapshot_id: SnapshotId,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub height: u64,
    pub head_event_id: crate::EventId,
    pub head_event_hash: SemanticHash,
    pub state_schema_hash: SemanticHash,
    pub state_hash: SemanticHash,
    pub commit_certificate_hash: SemanticHash,
}

impl ReplicatedSnapshotWire {
    pub fn validate(&self) -> Result<(), ReplicationWireError> {
        if self.schema_version != REPLICATION_SCHEMA_VERSION_V1
            || !self.snapshot_id.validate()
            || self.membership_epoch == 0
            || self.height == 0
        {
            return Err(ReplicationWireError::InvalidSnapshot);
        }
        Ok(())
    }
}

/// Stable protocol refinement failure without rejected bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationWireError {
    UnknownVersion,
    IdentityBinding,
    InvalidCertificate,
    InvalidRevocation,
    InvalidCapabilities,
    InvalidRootSignatureIntent,
    InvalidMembershipTransition,
    InvalidCandidate,
    InvalidVote,
    InvalidCommitCertificate,
    InvalidReplicatedEvent,
    InvalidSnapshot,
    Encoding,
}

impl fmt::Display for ReplicationWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownVersion => "unknown replication schema version",
            Self::IdentityBinding => "player/device public-key binding is invalid",
            Self::InvalidCertificate => "device certificate is invalid",
            Self::InvalidRevocation => "device revocation is invalid",
            Self::InvalidCapabilities => "device capabilities are empty, duplicate, or unsorted",
            Self::InvalidRootSignatureIntent => "player-root signature intent is invalid",
            Self::InvalidMembershipTransition => "membership transition is invalid",
            Self::InvalidCandidate => "replicated candidate is invalid",
            Self::InvalidVote => "consensus vote is invalid",
            Self::InvalidCommitCertificate => "commit certificate is invalid",
            Self::InvalidReplicatedEvent => "replicated event is invalid",
            Self::InvalidSnapshot => "replicated snapshot is invalid",
            Self::Encoding => "replication canonical encoding failed",
        })
    }
}

impl std::error::Error for ReplicationWireError {}

/// Strict player-majority threshold. Devices never increase this value.
#[must_use]
pub const fn replicated_quorum(player_count: usize) -> usize {
    player_count / 2 + 1
}

/// Root-signing bytes for one device certificate.
pub fn canonical_device_certificate_bytes(
    certificate: &UnsignedDeviceCertificateWire,
) -> Result<Vec<u8>, ReplicationWireError> {
    certificate.validate()?;
    encode_json_domain(CERTIFICATE_DOMAIN, certificate)
}

/// Root-signing bytes for one device revocation.
pub fn canonical_device_revocation_bytes(
    revocation: &UnsignedDeviceRevocationWire,
) -> Result<Vec<u8>, ReplicationWireError> {
    revocation.validate()?;
    encode_json_domain(REVOCATION_DOMAIN, revocation)
}

/// Candidate semantic value independent of proposer signature bytes.
pub fn consensus_candidate_hash(
    body: &ConsensusCandidateBodyWire,
) -> Result<SemanticHash, ReplicationWireError> {
    body.validate()?;
    Ok(hash_bytes(&encode_json_domain(
        CANDIDATE_BODY_DOMAIN,
        body,
    )?))
}

/// Derived proposal ID for one candidate body.
pub fn derive_replicated_proposal_id(
    body: &ConsensusCandidateBodyWire,
) -> Result<ProposalId, ReplicationWireError> {
    derived_id("rp", consensus_candidate_hash(body)?.0, ProposalId::new)
}

/// Device-signing bytes for one candidate.
pub fn canonical_consensus_candidate_bytes(
    candidate: &UnsignedConsensusCandidateWire,
) -> Result<Vec<u8>, ReplicationWireError> {
    candidate.validate()?;
    encode_json_domain(CANDIDATE_SIGNATURE_DOMAIN, candidate)
}

/// Device-signing bytes for one prevote/precommit.
pub fn canonical_consensus_vote_bytes(
    vote: &UnsignedConsensusVoteWire,
) -> Result<Vec<u8>, ReplicationWireError> {
    vote.validate()?;
    encode_json_domain(VOTE_DOMAIN, vote)
}

/// Hash a structurally valid commit certificate including its signatures.
pub fn commit_certificate_hash(
    certificate: &CommitCertificateWire,
) -> Result<SemanticHash, ReplicationWireError> {
    certificate.validate()?;
    Ok(hash_bytes(&encode_json_domain(COMMIT_DOMAIN, certificate)?))
}

/// Derive the stable event ID from the committed candidate value.
pub fn derive_replicated_event_id(
    candidate_hash: SemanticHash,
) -> Result<crate::EventId, ReplicationWireError> {
    derived_id("re", candidate_hash.0, crate::EventId::new)
}

/// Hash a valid snapshot descriptor.
pub fn replicated_snapshot_hash(
    snapshot: &ReplicatedSnapshotWire,
) -> Result<SemanticHash, ReplicationWireError> {
    snapshot.validate()?;
    Ok(hash_bytes(&encode_json_domain(SNAPSHOT_DOMAIN, snapshot)?))
}

/// Construct the domain-separated legacy self-device certificate request.
pub fn legacy_self_device_certificate(
    root: &PlayerRootWire,
) -> Result<UnsignedDeviceCertificateWire, ReplicationWireError> {
    root.validate()?;
    let certificate = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new("legacy-self-1")
            .map_err(|_| ReplicationWireError::InvalidCertificate)?,
        player_id: root.player_id.clone(),
        device_id: DeviceId::new(root.signing_public_key.clone())
            .map_err(|_| ReplicationWireError::IdentityBinding)?,
        device_signing_public_key: root.signing_public_key.clone(),
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities: vec![
            DeviceCapabilityWire::Propose,
            DeviceCapabilityWire::Vote,
            DeviceCapabilityWire::ReceivePrivateProjection,
            DeviceCapabilityWire::RequestDeviceChange,
        ],
        custody: DeviceCustodyWire::LegacySelf,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: root.player_id.clone(),
        },
    };
    certificate.validate()?;
    Ok(certificate)
}

fn validate_root_intent(
    intent: &SignatureIntent,
    player: &PrincipalId,
) -> Result<(), ReplicationWireError> {
    if intent.domain_version == REPLICATION_SIGNATURE_DOMAIN_V1
        && intent.algorithm == SignatureAlgorithm::Ed25519
        && &intent.key_id == player
    {
        Ok(())
    } else {
        Err(ReplicationWireError::InvalidRootSignatureIntent)
    }
}

fn validate_capabilities(
    capabilities: &[DeviceCapabilityWire],
) -> Result<(), ReplicationWireError> {
    if capabilities.is_empty()
        || capabilities.len() > 4
        || !capabilities.windows(2).all(|pair| pair[0] < pair[1])
    {
        Err(ReplicationWireError::InvalidCapabilities)
    } else {
        Ok(())
    }
}

fn commands_are_canonical(commands: &[ReplicatedCommandRefWire]) -> bool {
    commands.iter().all(|command| command.command_id.validate())
        && commands.windows(2).all(|pair| {
            (pair[0].semantic_hash.0, pair[0].command_id.as_str())
                < (pair[1].semantic_hash.0, pair[1].command_id.as_str())
        })
}

fn strict_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn is_public_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_json_domain<T: Serialize>(
    domain: &[u8],
    value: &T,
) -> Result<Vec<u8>, ReplicationWireError> {
    let json = serde_json::to_vec(value).map_err(|_| ReplicationWireError::Encoding)?;
    let length = u64::try_from(json.len()).map_err(|_| ReplicationWireError::Encoding)?;
    let mut bytes = Vec::with_capacity(domain.len() + 8 + json.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&json);
    Ok(bytes)
}

fn hash_bytes(bytes: &[u8]) -> SemanticHash {
    SemanticHash(*blake3::hash(bytes).as_bytes())
}

fn derived_id<T>(
    prefix: &str,
    hash: [u8; 32],
    constructor: impl FnOnce(String) -> Result<T, crate::IdentifierError>,
) -> Result<T, ReplicationWireError> {
    let mut value = String::with_capacity(59);
    value.push_str(prefix);
    value.push('.');
    for byte in &hash[..28] {
        use fmt::Write as _;
        write!(value, "{byte:02x}").map_err(|_| ReplicationWireError::Encoding)?;
    }
    constructor(value).map_err(|_| ReplicationWireError::Encoding)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Signer, SigningKey};

    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            use fmt::Write as _;
            write!(output, "{byte:02x}").unwrap();
            output
        })
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn root(seed: u8) -> PlayerRootWire {
        let public = hex(&key(seed).verifying_key().to_bytes());
        PlayerRootWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            player_id: PrincipalId::new(public.clone()).unwrap(),
            signing_public_key: public,
        }
    }

    fn signature(bytes: &[u8], signing: &SigningKey) -> SignatureBytes {
        SignatureBytes::new(hex(&signing.sign(bytes).to_bytes())).unwrap()
    }

    fn signature_value(value: &SignatureBytes) -> Signature {
        let mut bytes = [0_u8; 64];
        for (target, pair) in bytes
            .iter_mut()
            .zip(value.as_str().as_bytes().chunks_exact(2))
        {
            *target = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        Signature::from_bytes(&bytes)
    }

    fn signed_certificate(
        root: &PlayerRootWire,
        root_seed: u8,
        device_seed: u8,
    ) -> DeviceCertificateWire {
        let public = hex(&key(device_seed).verifying_key().to_bytes());
        let unsigned = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("certificate-{device_seed}")).unwrap(),
            player_id: root.player_id.clone(),
            device_id: DeviceId::new(public.clone()).unwrap(),
            device_signing_public_key: public,
            sequence: u64::from(device_seed),
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::Vote,
                DeviceCapabilityWire::ReceivePrivateProjection,
            ],
            custody: DeviceCustodyWire::BrowserLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: root.player_id.clone(),
            },
        };
        let bytes = canonical_device_certificate_bytes(&unsigned).unwrap();
        unsigned
            .attach_signature(signature(&bytes, &key(root_seed)))
            .unwrap()
    }

    #[test]
    fn device_certificate_vector_is_exact_and_domain_separated() {
        let player_root = root(1);
        let certificate = signed_certificate(&player_root, 1, 11);
        let bytes = canonical_device_certificate_bytes(&certificate.unsigned()).unwrap();
        assert_eq!(
            hex(blake3::hash(&bytes).as_bytes()),
            "8b7b7aa2d2759c0c2745d919be1491dc089c142453950c4aa47f89ed47abd8c3"
        );
        assert_eq!(
            certificate.signature.signature.as_str(),
            "a5976b56c202450e64584b7d93fdc38c105a00b23b9c9b8af620121a6d0184b8a39e15e79168ba8f3abadb01a619d79eb2a3e3fc4cac6fd23fe2bc81db717d0d"
        );
        key(1)
            .verifying_key()
            .verify_strict(&bytes, &signature_value(&certificate.signature.signature))
            .unwrap();
        assert!(certificate.is_valid_at(1));
        assert!(certificate.has_capability(DeviceCapabilityWire::Vote));

        let mut wrong = certificate.unsigned();
        wrong.signature_intent.key_id = root(2).player_id;
        assert_eq!(
            canonical_device_certificate_bytes(&wrong),
            Err(ReplicationWireError::InvalidRootSignatureIntent)
        );
    }

    #[test]
    fn migration_preserves_principal_bytes_and_gateway_is_explicit() {
        let root = root(7);
        let legacy = legacy_self_device_certificate(&root).unwrap();
        assert_eq!(legacy.player_id.as_str(), root.player_id.as_str());
        assert_eq!(legacy.device_id.as_str(), root.player_id.as_str());
        assert_eq!(legacy.custody, DeviceCustodyWire::LegacySelf);

        let mut gateway = legacy;
        gateway.certificate_id = CertificateId::new("gateway-device-2").unwrap();
        gateway.sequence = 2;
        gateway.custody = DeviceCustodyWire::GatewayCustodied;
        gateway.device_signing_public_key = hex(&key(9).verifying_key().to_bytes());
        gateway.device_id = DeviceId::new(gateway.device_signing_public_key.clone()).unwrap();
        assert!(gateway.validate().is_ok());
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one end-to-end vector keeps candidate, votes, commit, event, and snapshot bytes visibly contiguous"
    )]
    fn device_candidate_vote_commit_event_and_snapshot_vectors_are_exact() {
        let alice = root(1);
        let bob = root(2);
        let alice_device = signed_certificate(&alice, 1, 11);
        let bob_device = signed_certificate(&bob, 2, 12);
        let body = ConsensusCandidateBodyWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            room_id: RoomId::new("room-vector").unwrap(),
            membership_epoch: 1,
            parent_event_id: None,
            parent_event_hash: SemanticHash([0; 32]),
            height: 1,
            round: 0,
            proposer_player_id: alice.player_id.clone(),
            proposer_device_id: alice_device.device_id.clone(),
            ordered_commands: vec![ReplicatedCommandRefWire {
                command_id: CommandId::new("command-1").unwrap(),
                semantic_hash: SemanticHash([3; 32]),
            }],
            membership_transition: None,
        };
        let candidate_hash = consensus_candidate_hash(&body).unwrap();
        let proposal_id = derive_replicated_proposal_id(&body).unwrap();
        let unsigned_candidate = UnsignedConsensusCandidateWire {
            body,
            proposal_id,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: alice_device.device_id.clone(),
            },
        };
        let candidate_bytes = canonical_consensus_candidate_bytes(&unsigned_candidate).unwrap();
        let candidate = unsigned_candidate
            .attach_signature(signature(&candidate_bytes, &key(11)))
            .unwrap();
        key(11)
            .verifying_key()
            .verify_strict(
                &candidate_bytes,
                &signature_value(&candidate.signature.signature),
            )
            .unwrap();
        let votes = [(&alice, &alice_device, 11_u8), (&bob, &bob_device, 12_u8)].map(
            |(player, device, seed)| {
                let unsigned = UnsignedConsensusVoteWire {
                    schema_version: REPLICATION_SCHEMA_VERSION_V1,
                    room_id: RoomId::new("room-vector").unwrap(),
                    membership_epoch: 1,
                    height: 1,
                    round: 0,
                    phase: ConsensusVotePhaseWire::Precommit,
                    value: ConsensusValueWire::Candidate(candidate_hash),
                    voter_player_id: player.player_id.clone(),
                    voter_device_id: device.device_id.clone(),
                    lock_round: None,
                    lock_proof_hash: None,
                    signature_intent: DeviceSignatureIntentWire {
                        domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                        algorithm: SignatureAlgorithm::Ed25519,
                        key_id: device.device_id.clone(),
                    },
                };
                let bytes = canonical_consensus_vote_bytes(&unsigned).unwrap();
                unsigned
                    .attach_signature(signature(&bytes, &key(seed)))
                    .unwrap()
            },
        );
        let certificate = CommitCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            room_id: RoomId::new("room-vector").unwrap(),
            membership_epoch: 1,
            height: 1,
            round: 0,
            candidate_hash,
            precommits: votes.to_vec(),
        };
        certificate.validate().unwrap();
        for (vote, seed) in certificate.precommits.iter().zip([11_u8, 12_u8]) {
            key(seed)
                .verifying_key()
                .verify_strict(
                    &canonical_consensus_vote_bytes(&vote.unsigned()).unwrap(),
                    &signature_value(&vote.signature.signature),
                )
                .unwrap();
        }
        assert_eq!(certificate.unique_player_votes(), 2);
        assert_eq!(replicated_quorum(2), 2);
        assert_eq!(replicated_quorum(3), 2);

        let event = ReplicatedEventWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            event_id: derive_replicated_event_id(candidate_hash).unwrap(),
            candidate,
            certificate: certificate.clone(),
            successor_state_hash: SemanticHash([4; 32]),
        };
        event.validate().unwrap();
        let certificate_hash = commit_certificate_hash(&certificate).unwrap();
        let snapshot = ReplicatedSnapshotWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            snapshot_id: SnapshotId::new("snapshot-vector").unwrap(),
            room_id: RoomId::new("room-vector").unwrap(),
            membership_epoch: 1,
            height: 1,
            head_event_id: event.event_id,
            head_event_hash: candidate_hash,
            state_schema_hash: SemanticHash([5; 32]),
            state_hash: event.successor_state_hash,
            commit_certificate_hash: certificate_hash,
        };
        assert_eq!(
            hex(candidate_hash.0.as_slice()),
            "183311602c793ffd3bbfd988e0303daa1dbcd2eb7e16aa2c48adb05366e75ace"
        );
        assert_eq!(
            hex(certificate_hash.0.as_slice()),
            "0d62a63e78dad9decbf196ae35895cd8e4509e511a88c45b7b21aad5e5ce09a5"
        );
        assert_eq!(
            hex(replicated_snapshot_hash(&snapshot).unwrap().0.as_slice()),
            "0a815ea3062fa3d728a51739599f93211f72a7058e91fb109a09d0f600c3e110"
        );
    }

    #[test]
    fn duplicate_devices_do_not_make_a_valid_player_deduplicated_certificate() {
        let alice = root(1);
        let first = signed_certificate(&alice, 1, 11);
        let second = signed_certificate(&alice, 1, 12);
        let hash = SemanticHash([8; 32]);
        let vote = |device: &DeviceCertificateWire, seed| {
            let unsigned = UnsignedConsensusVoteWire {
                schema_version: REPLICATION_SCHEMA_VERSION_V1,
                room_id: RoomId::new("room-duplicate").unwrap(),
                membership_epoch: 1,
                height: 1,
                round: 0,
                phase: ConsensusVotePhaseWire::Precommit,
                value: ConsensusValueWire::Candidate(hash),
                voter_player_id: alice.player_id.clone(),
                voter_device_id: device.device_id.clone(),
                lock_round: None,
                lock_proof_hash: None,
                signature_intent: DeviceSignatureIntentWire {
                    domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: device.device_id.clone(),
                },
            };
            let bytes = canonical_consensus_vote_bytes(&unsigned).unwrap();
            unsigned
                .attach_signature(signature(&bytes, &key(seed)))
                .unwrap()
        };
        let certificate = CommitCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            room_id: RoomId::new("room-duplicate").unwrap(),
            membership_epoch: 1,
            height: 1,
            round: 0,
            candidate_hash: hash,
            precommits: vec![vote(&first, 11), vote(&second, 12)],
        };
        assert_eq!(
            certificate.validate(),
            Err(ReplicationWireError::InvalidCommitCertificate)
        );
    }
}
