// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pure validation/application boundary for `poche-replicated-v1`.

use core::fmt;
use std::collections::BTreeSet;

use poche_protocol::{
    CommitCertificateWire, ConsensusCandidateWire, ConsensusVoteWire, DeviceCapabilityWire,
    DeviceCertificateWire, DeviceId, DeviceRevocationWire, PlayerRootWire, PrincipalId,
    ReplicatedEventWire, ReplicatedSnapshotWire, RoomId, SemanticHash, commit_certificate_hash,
    consensus_candidate_hash, replicated_quorum,
};

/// Cryptographic verification port. Pure replicated semantics never treats a
/// transport peer or structurally valid signature string as authorization.
pub trait ReplicationSignatureVerifier {
    fn verify_device_certificate(
        &self,
        root: &PlayerRootWire,
        certificate: &DeviceCertificateWire,
    ) -> bool;

    fn verify_device_revocation(
        &self,
        root: &PlayerRootWire,
        revocation: &DeviceRevocationWire,
    ) -> bool;

    fn verify_candidate(
        &self,
        certificate: &DeviceCertificateWire,
        candidate: &ConsensusCandidateWire,
    ) -> bool;

    fn verify_vote(&self, certificate: &DeviceCertificateWire, vote: &ConsensusVoteWire) -> bool;
}

/// One player root and every known root-certified device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicatedPlayerState {
    pub root: PlayerRootWire,
    pub active: bool,
    pub devices: Vec<DeviceCertificateWire>,
}

/// Root-signed revocation waiting for its jointly committed epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedDeviceRevocation {
    pub revocation: DeviceRevocationWire,
}

/// Two conflicting certificates for one parent/height. Shared mutation stops
/// at the last common head; this record is evidence, not a winner selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicatedForkProof {
    pub height: u64,
    pub first_event: poche_protocol::EventId,
    pub second_event: poche_protocol::EventId,
    pub first_candidate_hash: SemanticHash,
    pub second_candidate_hash: SemanticHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReplicatedEpochRoster {
    membership_epoch: u64,
    active_players: Vec<PrincipalId>,
}

/// Result of applying one already signed/certificate-bearing event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicatedApplyResult {
    Applied,
    Duplicate,
}

/// Pure replicated membership/log head. Event payload reduction and transport
/// live outside this structure; callers supply the deterministic successor
/// hash already present in the event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicatedSessionState {
    room_id: RoomId,
    membership_epoch: u64,
    height: u64,
    head_event_id: Option<poche_protocol::EventId>,
    head_event_hash: SemanticHash,
    players: Vec<ReplicatedPlayerState>,
    epoch_rosters: Vec<ReplicatedEpochRoster>,
    staged_revocations: Vec<StagedDeviceRevocation>,
    last_event: Option<ReplicatedEventWire>,
    forks: Vec<ReplicatedForkProof>,
}

impl ReplicatedSessionState {
    /// Construct epoch one from sorted unique player roots and verified initial
    /// device certificates.
    ///
    /// # Errors
    ///
    /// Rejects an invalid roster/certificate, duplicate key, missing player,
    /// bad root signature, or more than the per-player device bound.
    pub fn new(
        room_id: RoomId,
        roots: Vec<PlayerRootWire>,
        certificates: Vec<DeviceCertificateWire>,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<Self, ReplicatedSessionError> {
        if roots.len() < 2
            || !roots
                .windows(2)
                .all(|pair| pair[0].player_id < pair[1].player_id)
            || roots.iter().any(|root| root.validate().is_err())
        {
            return Err(ReplicatedSessionError::InvalidRoster);
        }
        let mut players = roots
            .into_iter()
            .map(|root| ReplicatedPlayerState {
                root,
                active: true,
                devices: Vec::new(),
            })
            .collect::<Vec<_>>();
        for certificate in certificates {
            add_certificate_to_players(&mut players, certificate, verifier)?;
        }
        if players.iter().any(|player| {
            !player.devices.iter().any(|device| {
                device.is_valid_at(1)
                    && device.has_capability(DeviceCapabilityWire::Propose)
                    && device.has_capability(DeviceCapabilityWire::Vote)
            })
        }) {
            return Err(ReplicatedSessionError::MissingVotingDevice);
        }
        let epoch_rosters = vec![ReplicatedEpochRoster {
            membership_epoch: 1,
            active_players: players
                .iter()
                .map(|player| player.root.player_id.clone())
                .collect(),
        }];
        let state = Self {
            room_id,
            membership_epoch: 1,
            height: 0,
            head_event_id: None,
            head_event_hash: SemanticHash([0; 32]),
            players,
            epoch_rosters,
            staged_revocations: Vec::new(),
            last_event: None,
            forks: Vec::new(),
        };
        state.validate()?;
        Ok(state)
    }

    /// Add a root-certified device without changing player voting weight.
    ///
    /// # Errors
    ///
    /// Rejects unknown players, invalid/root-unverified certificates,
    /// non-monotonic sequences, duplicates, and the per-player device bound.
    pub fn register_device(
        &mut self,
        certificate: DeviceCertificateWire,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<(), ReplicatedSessionError> {
        let before = self.clone();
        if let Err(error) = add_certificate_to_players(&mut self.players, certificate, verifier)
            .and_then(|()| self.validate())
        {
            *self = before;
            return Err(error);
        }
        Ok(())
    }

    /// Stage a root-signed device revocation for exactly the next membership
    /// epoch. It has no effect before a jointly certified epoch transition.
    ///
    /// # Errors
    ///
    /// Rejects unknown/stale devices, invalid signatures, duplicate
    /// revocations, and an effective epoch other than the next epoch.
    pub fn stage_revocation(
        &mut self,
        revocation: DeviceRevocationWire,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<(), ReplicatedSessionError> {
        revocation
            .validate()
            .map_err(|_| ReplicatedSessionError::InvalidRevocation)?;
        let player = self
            .players
            .iter()
            .find(|player| player.root.player_id == revocation.player_id)
            .ok_or(ReplicatedSessionError::UnknownPlayer)?;
        let device = player
            .devices
            .iter()
            .find(|device| {
                device.device_id == revocation.device_id
                    && device.sequence == revocation.certificate_sequence
            })
            .ok_or(ReplicatedSessionError::UnknownDevice)?;
        if revocation.effective_membership_epoch != self.membership_epoch.saturating_add(1)
            || !verifier.verify_device_revocation(&player.root, &revocation)
            || self.staged_revocations.iter().any(|record| {
                record.revocation.device_id == revocation.device_id
                    && record.revocation.effective_membership_epoch
                        == revocation.effective_membership_epoch
            })
            || !device.is_valid_at(self.membership_epoch)
        {
            return Err(ReplicatedSessionError::InvalidRevocation);
        }
        self.staged_revocations
            .push(StagedDeviceRevocation { revocation });
        Ok(())
    }

    /// Validate and apply one quorum-certified event at the current head.
    ///
    /// A conflicting certificate records a fork proof and returns
    /// `ForkDetected`; the state does not choose or apply either conflicting
    /// successor beyond the already known common head.
    ///
    /// # Errors
    ///
    /// Rejects wrong heads/coordinators, invalid signatures or certificates,
    /// weak/joint quorums, revoked devices, forks, and invariant failures.
    pub fn apply_event(
        &mut self,
        event: ReplicatedEventWire,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<ReplicatedApplyResult, ReplicatedSessionError> {
        if let Some(last) = &self.last_event
            && event.candidate.body.height == last.candidate.body.height
        {
            if event == *last {
                return Ok(ReplicatedApplyResult::Duplicate);
            }
            if event.candidate.body.parent_event_hash == last.candidate.body.parent_event_hash
                && event.candidate.body.parent_event_id == last.candidate.body.parent_event_id
            {
                self.validate_historical_conflict(&event, verifier)?;
                let second_hash = consensus_candidate_hash(&event.candidate.body)
                    .map_err(|_| ReplicatedSessionError::InvalidEvent)?;
                let first_hash = consensus_candidate_hash(&last.candidate.body)
                    .map_err(|_| ReplicatedSessionError::InvalidEvent)?;
                self.forks.push(ReplicatedForkProof {
                    height: event.candidate.body.height,
                    first_event: last.event_id.clone(),
                    second_event: event.event_id,
                    first_candidate_hash: first_hash,
                    second_candidate_hash: second_hash,
                });
                return Err(ReplicatedSessionError::ForkDetected);
            }
        }
        if !self.forks.is_empty() {
            return Err(ReplicatedSessionError::Forked);
        }
        self.validate_event(&event, verifier)?;
        let before = self.clone();
        self.height = event.candidate.body.height;
        self.head_event_id = Some(event.event_id.clone());
        self.head_event_hash = consensus_candidate_hash(&event.candidate.body)
            .map_err(|_| ReplicatedSessionError::InvalidEvent)?;
        if let Some(transition) = &event.candidate.body.membership_transition {
            self.membership_epoch = transition.next_membership_epoch;
            for player in &mut self.players {
                player.active = transition
                    .active_players
                    .binary_search(&player.root.player_id)
                    .is_ok();
            }
            self.epoch_rosters.push(ReplicatedEpochRoster {
                membership_epoch: transition.next_membership_epoch,
                active_players: transition.active_players.clone(),
            });
            // Retain the signed revocation as permanent evidence. Mutating a
            // certificate's signed validity fields would invalidate its root
            // signature; `require_device` instead applies effective epochs.
        }
        self.last_event = Some(event);
        if let Err(error) = self.validate() {
            *self = before;
            return Err(error);
        }
        Ok(ReplicatedApplyResult::Applied)
    }

    /// Check that a snapshot describes exactly the current certified head.
    ///
    /// # Errors
    ///
    /// Rejects a malformed descriptor or any room, epoch, height, head,
    /// state, or certificate hash mismatch.
    pub fn validate_snapshot_at_head(
        &self,
        snapshot: &ReplicatedSnapshotWire,
    ) -> Result<(), ReplicatedSessionError> {
        snapshot
            .validate()
            .map_err(|_| ReplicatedSessionError::InvalidSnapshot)?;
        let event = self
            .last_event
            .as_ref()
            .ok_or(ReplicatedSessionError::InvalidSnapshot)?;
        let certificate_hash = commit_certificate_hash(&event.certificate)
            .map_err(|_| ReplicatedSessionError::InvalidSnapshot)?;
        if snapshot.room_id != self.room_id
            || snapshot.membership_epoch != self.membership_epoch
            || snapshot.height != self.height
            || Some(&snapshot.head_event_id) != self.head_event_id.as_ref()
            || snapshot.head_event_hash != self.head_event_hash
            || snapshot.state_hash != event.successor_state_hash
            || snapshot.commit_certificate_hash != certificate_hash
        {
            return Err(ReplicatedSessionError::InvalidSnapshot);
        }
        Ok(())
    }

    #[must_use]
    pub fn active_players(&self) -> Vec<&PrincipalId> {
        self.players
            .iter()
            .filter(|player| player.active)
            .map(|player| &player.root.player_id)
            .collect()
    }

    #[must_use]
    pub fn quorum(&self) -> usize {
        replicated_quorum(self.active_players().len())
    }

    #[must_use]
    pub const fn membership_epoch(&self) -> u64 {
        self.membership_epoch
    }

    #[must_use]
    pub const fn height(&self) -> u64 {
        self.height
    }

    #[must_use]
    pub fn forks(&self) -> &[ReplicatedForkProof] {
        &self.forks
    }

    /// Deterministic rotating proposer player for one height/round.
    ///
    /// # Errors
    ///
    /// Rejects a height other than the next height or an empty active roster.
    pub fn proposer_for(
        &self,
        height: u64,
        round: u32,
    ) -> Result<&PrincipalId, ReplicatedSessionError> {
        let active = self.active_players();
        if active.is_empty() || height != self.height.saturating_add(1) {
            return Err(ReplicatedSessionError::InvalidHeight);
        }
        let index = proposer_index(self.head_event_hash, height, round, active.len())?;
        Ok(active[index])
    }

    fn validate_event(
        &self,
        event: &ReplicatedEventWire,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<(), ReplicatedSessionError> {
        event
            .validate()
            .map_err(|_| ReplicatedSessionError::InvalidEvent)?;
        let body = &event.candidate.body;
        if body.room_id != self.room_id
            || body.membership_epoch != self.membership_epoch
            || body.height != self.height.saturating_add(1)
            || body.parent_event_id != self.head_event_id
            || body.parent_event_hash != self.head_event_hash
            || &body.proposer_player_id != self.proposer_for(body.height, body.round)?
        {
            return Err(ReplicatedSessionError::InvalidEvent);
        }
        let proposer_device = self.require_device(
            &body.proposer_player_id,
            &body.proposer_device_id,
            DeviceCapabilityWire::Propose,
            self.membership_epoch,
        )?;
        if !verifier.verify_candidate(proposer_device, &event.candidate) {
            return Err(ReplicatedSessionError::BadSignature);
        }
        let roster = self
            .roster(self.membership_epoch)
            .ok_or(ReplicatedSessionError::InvalidRoster)?;
        self.validate_commit_certificate(
            &event.certificate,
            body.membership_transition.as_ref(),
            roster,
            verifier,
        )
    }

    fn validate_historical_conflict(
        &self,
        event: &ReplicatedEventWire,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<(), ReplicatedSessionError> {
        event
            .validate()
            .map_err(|_| ReplicatedSessionError::InvalidEvent)?;
        let body = &event.candidate.body;
        let roster = self
            .roster(body.membership_epoch)
            .ok_or(ReplicatedSessionError::InvalidRoster)?;
        if body.room_id != self.room_id
            || body.height == 0
            || body.proposer_player_id
                != roster.active_players[proposer_index(
                    body.parent_event_hash,
                    body.height,
                    body.round,
                    roster.active_players.len(),
                )?]
        {
            return Err(ReplicatedSessionError::InvalidEvent);
        }
        let proposer_device = self.require_device_in_roster(
            &body.proposer_player_id,
            &body.proposer_device_id,
            DeviceCapabilityWire::Propose,
            body.membership_epoch,
            roster,
        )?;
        if !verifier.verify_candidate(proposer_device, &event.candidate) {
            return Err(ReplicatedSessionError::BadSignature);
        }
        self.validate_commit_certificate(
            &event.certificate,
            body.membership_transition.as_ref(),
            roster,
            verifier,
        )
    }

    fn validate_commit_certificate(
        &self,
        certificate: &CommitCertificateWire,
        transition: Option<&poche_protocol::MembershipTransitionWire>,
        roster: &ReplicatedEpochRoster,
        verifier: &impl ReplicationSignatureVerifier,
    ) -> Result<(), ReplicatedSessionError> {
        certificate
            .validate()
            .map_err(|_| ReplicatedSessionError::InvalidCertificate)?;
        let mut old_voters = BTreeSet::new();
        for vote in &certificate.precommits {
            let device = self.require_device_in_roster(
                &vote.voter_player_id,
                &vote.voter_device_id,
                DeviceCapabilityWire::Vote,
                certificate.membership_epoch,
                roster,
            )?;
            if !verifier.verify_vote(device, vote) {
                return Err(ReplicatedSessionError::BadSignature);
            }
            old_voters.insert(vote.voter_player_id.clone());
        }
        if old_voters.len() < replicated_quorum(roster.active_players.len()) {
            return Err(ReplicatedSessionError::NoQuorum);
        }
        if let Some(transition) = transition {
            if transition.active_players.iter().any(|player_id| {
                !self
                    .players
                    .iter()
                    .any(|player| &player.root.player_id == player_id)
            }) {
                return Err(ReplicatedSessionError::InvalidRoster);
            }
            let new_votes = old_voters
                .iter()
                .filter(|player| transition.active_players.binary_search(player).is_ok())
                .count();
            if new_votes < replicated_quorum(transition.active_players.len())
                || !transition.revoked_devices.iter().all(|device| {
                    self.staged_revocations.iter().any(|record| {
                        &record.revocation.device_id == device
                            && record.revocation.effective_membership_epoch
                                == transition.next_membership_epoch
                    })
                })
            {
                return Err(ReplicatedSessionError::NoJointQuorum);
            }
        }
        Ok(())
    }

    fn require_device(
        &self,
        player_id: &PrincipalId,
        device_id: &DeviceId,
        capability: DeviceCapabilityWire,
        epoch: u64,
    ) -> Result<&DeviceCertificateWire, ReplicatedSessionError> {
        let roster = self
            .roster(epoch)
            .ok_or(ReplicatedSessionError::InvalidRoster)?;
        self.require_device_in_roster(player_id, device_id, capability, epoch, roster)
    }

    fn require_device_in_roster(
        &self,
        player_id: &PrincipalId,
        device_id: &DeviceId,
        capability: DeviceCapabilityWire,
        epoch: u64,
        roster: &ReplicatedEpochRoster,
    ) -> Result<&DeviceCertificateWire, ReplicatedSessionError> {
        if roster.active_players.binary_search(player_id).is_err() {
            return Err(ReplicatedSessionError::UnknownPlayer);
        }
        let player = self
            .players
            .iter()
            .find(|player| &player.root.player_id == player_id)
            .ok_or(ReplicatedSessionError::UnknownPlayer)?;
        let device = player
            .devices
            .iter()
            .find(|device| &device.device_id == device_id)
            .ok_or(ReplicatedSessionError::UnknownDevice)?;
        let revoked = self.staged_revocations.iter().any(|record| {
            &record.revocation.device_id == device_id
                && record.revocation.effective_membership_epoch <= epoch
        });
        if !device.is_valid_at(epoch) || !device.has_capability(capability) || revoked {
            return Err(ReplicatedSessionError::DeviceNotAuthorized);
        }
        Ok(device)
    }

    fn roster(&self, epoch: u64) -> Option<&ReplicatedEpochRoster> {
        self.epoch_rosters
            .binary_search_by_key(&epoch, |roster| roster.membership_epoch)
            .ok()
            .map(|index| &self.epoch_rosters[index])
    }

    fn validate(&self) -> Result<(), ReplicatedSessionError> {
        if self.membership_epoch == 0
            || self.active_players().len() < 2
            || self.epoch_rosters.last().is_none_or(|roster| {
                roster.membership_epoch != self.membership_epoch
                    || roster.active_players
                        != self
                            .active_players()
                            .into_iter()
                            .cloned()
                            .collect::<Vec<_>>()
            })
            || self.epoch_rosters.windows(2).any(|pair| {
                pair[0].membership_epoch.saturating_add(1) != pair[1].membership_epoch
                    || !pair[0]
                        .active_players
                        .windows(2)
                        .all(|players| players[0] < players[1])
            })
            || self
                .players
                .windows(2)
                .any(|pair| pair[0].root.player_id >= pair[1].root.player_id)
            || self.players.iter().any(|player| {
                player.devices.len() > poche_protocol::MAX_DEVICES_PER_PLAYER
                    || player
                        .devices
                        .windows(2)
                        .any(|pair| pair[0].device_id >= pair[1].device_id)
            })
        {
            return Err(ReplicatedSessionError::Invariant);
        }
        Ok(())
    }
}

fn proposer_index(
    parent_event_hash: SemanticHash,
    height: u64,
    round: u32,
    active_player_count: usize,
) -> Result<usize, ReplicatedSessionError> {
    if active_player_count == 0 || height == 0 {
        return Err(ReplicatedSessionError::InvalidHeight);
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"POCHE\0REPLICATED-PROPOSER\0V1");
    hasher.update(&parent_event_hash.0);
    hasher.update(&height.to_be_bytes());
    hasher.update(&round.to_be_bytes());
    let digest = hasher.finalize();
    let index_bytes: [u8; 8] = digest.as_bytes()[..8]
        .try_into()
        .map_err(|_| ReplicatedSessionError::Invariant)?;
    Ok(
        usize::try_from(u64::from_be_bytes(index_bytes)).unwrap_or(usize::MAX)
            % active_player_count,
    )
}

/// Replicated identity/log validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicatedSessionError {
    InvalidRoster,
    UnknownPlayer,
    UnknownDevice,
    DuplicateDevice,
    DeviceLimit,
    MissingVotingDevice,
    InvalidRevocation,
    InvalidHeight,
    InvalidEvent,
    InvalidCertificate,
    InvalidSnapshot,
    DeviceNotAuthorized,
    BadSignature,
    NoQuorum,
    NoJointQuorum,
    ForkDetected,
    Forked,
    Invariant,
}

impl fmt::Display for ReplicatedSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoster => "replicated player roster is invalid",
            Self::UnknownPlayer => "replicated player is unknown or inactive",
            Self::UnknownDevice => "replicated device is unknown",
            Self::DuplicateDevice => "replicated device/certificate is duplicate or stale",
            Self::DeviceLimit => "replicated player device limit exceeded",
            Self::MissingVotingDevice => "active player lacks a propose/vote device",
            Self::InvalidRevocation => "device revocation is invalid or not next-epoch",
            Self::InvalidHeight => "replicated height is not the next height",
            Self::InvalidEvent => "replicated event does not extend the current head",
            Self::InvalidCertificate => "replicated commit certificate is invalid",
            Self::InvalidSnapshot => "snapshot does not bind the certified head",
            Self::DeviceNotAuthorized => "device is expired, revoked, or lacks exact capability",
            Self::BadSignature => "replication cryptographic verifier rejected a signature",
            Self::NoQuorum => "commit lacks a strict player majority",
            Self::NoJointQuorum => "membership transition lacks old/new joint quorum",
            Self::ForkDetected => "conflicting certified event recorded as fork evidence",
            Self::Forked => "replica is halted at a certified fork",
            Self::Invariant => "replicated state invariant failed",
        })
    }
}

impl std::error::Error for ReplicatedSessionError {}

fn add_certificate_to_players(
    players: &mut [ReplicatedPlayerState],
    certificate: DeviceCertificateWire,
    verifier: &impl ReplicationSignatureVerifier,
) -> Result<(), ReplicatedSessionError> {
    certificate
        .validate()
        .map_err(|_| ReplicatedSessionError::InvalidCertificate)?;
    let player = players
        .iter_mut()
        .find(|player| player.root.player_id == certificate.player_id)
        .ok_or(ReplicatedSessionError::UnknownPlayer)?;
    if !verifier.verify_device_certificate(&player.root, &certificate) {
        return Err(ReplicatedSessionError::BadSignature);
    }
    if player.devices.len() >= poche_protocol::MAX_DEVICES_PER_PLAYER {
        return Err(ReplicatedSessionError::DeviceLimit);
    }
    if player.devices.iter().any(|device| {
        device.device_id == certificate.device_id
            || device.sequence >= certificate.sequence
            || device.certificate_id == certificate.certificate_id
    }) {
        return Err(ReplicatedSessionError::DuplicateDevice);
    }
    player.devices.push(certificate);
    player
        .devices
        .sort_by(|left, right| left.device_id.cmp(&right.device_id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CertificateId, CommandId, ConsensusCandidateBodyWire, ConsensusValueWire,
        ConsensusVotePhaseWire, DeviceCustodyWire, DeviceSignatureIntentWire,
        MembershipTransitionWire, REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1,
        ReplicatedCommandRefWire, SignatureAlgorithm, SignatureBytes, SignatureIntent,
        UnsignedConsensusCandidateWire, UnsignedConsensusVoteWire, UnsignedDeviceCertificateWire,
        UnsignedDeviceRevocationWire, commit_certificate_hash, derive_replicated_event_id,
        derive_replicated_proposal_id,
    };

    use super::*;

    struct FixtureVerifier;

    impl ReplicationSignatureVerifier for FixtureVerifier {
        fn verify_device_certificate(
            &self,
            root: &PlayerRootWire,
            certificate: &DeviceCertificateWire,
        ) -> bool {
            certificate.signature.key_id == root.player_id
        }

        fn verify_device_revocation(
            &self,
            root: &PlayerRootWire,
            revocation: &DeviceRevocationWire,
        ) -> bool {
            revocation.signature.key_id == root.player_id
        }

        fn verify_candidate(
            &self,
            certificate: &DeviceCertificateWire,
            candidate: &ConsensusCandidateWire,
        ) -> bool {
            candidate.signature.key_id == certificate.device_id
        }

        fn verify_vote(
            &self,
            certificate: &DeviceCertificateWire,
            vote: &ConsensusVoteWire,
        ) -> bool {
            vote.signature.key_id == certificate.device_id
        }
    }

    fn public(seed: u8) -> String {
        format!("{seed:02x}").repeat(32)
    }

    fn player(seed: u8) -> PlayerRootWire {
        PlayerRootWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            player_id: PrincipalId::new(public(seed)).unwrap(),
            signing_public_key: public(seed),
        }
    }

    fn signature() -> SignatureBytes {
        SignatureBytes::new("00".repeat(64)).unwrap()
    }

    fn device(player: &PlayerRootWire, seed: u8, sequence: u64) -> DeviceCertificateWire {
        UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("certificate-{seed}")).unwrap(),
            player_id: player.player_id.clone(),
            device_id: DeviceId::new(public(seed)).unwrap(),
            device_signing_public_key: public(seed),
            sequence,
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
                key_id: player.player_id.clone(),
            },
        }
        .attach_signature(signature())
        .unwrap()
    }

    fn fixture_state() -> (
        ReplicatedSessionState,
        Vec<PlayerRootWire>,
        Vec<DeviceCertificateWire>,
    ) {
        let players = vec![player(1), player(2), player(3)];
        let devices = vec![
            device(&players[0], 11, 1),
            device(&players[1], 12, 1),
            device(&players[2], 13, 1),
        ];
        let state = ReplicatedSessionState::new(
            RoomId::new("replicated-room").unwrap(),
            players.clone(),
            devices.clone(),
            &FixtureVerifier,
        )
        .unwrap();
        (state, players, devices)
    }

    fn candidate(
        state: &ReplicatedSessionState,
        players: &[PlayerRootWire],
        devices: &[DeviceCertificateWire],
        round: u32,
        command_byte: u8,
        transition: Option<MembershipTransitionWire>,
    ) -> ConsensusCandidateWire {
        let proposer = state.proposer_for(state.height() + 1, round).unwrap();
        let index = players
            .iter()
            .position(|player| &player.player_id == proposer)
            .unwrap();
        let body = ConsensusCandidateBodyWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            room_id: RoomId::new("replicated-room").unwrap(),
            membership_epoch: state.membership_epoch(),
            parent_event_id: state.head_event_id.clone(),
            parent_event_hash: state.head_event_hash,
            height: state.height() + 1,
            round,
            proposer_player_id: proposer.clone(),
            proposer_device_id: devices[index].device_id.clone(),
            ordered_commands: vec![ReplicatedCommandRefWire {
                command_id: CommandId::new(format!("command-{command_byte}")).unwrap(),
                semantic_hash: SemanticHash([command_byte; 32]),
            }],
            membership_transition: transition,
        };
        UnsignedConsensusCandidateWire {
            proposal_id: derive_replicated_proposal_id(&body).unwrap(),
            signature_intent: DeviceSignatureIntentWire {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: devices[index].device_id.clone(),
            },
            body,
        }
        .attach_signature(signature())
        .unwrap()
    }

    fn event(
        candidate: ConsensusCandidateWire,
        voters: &[(&PlayerRootWire, &DeviceCertificateWire)],
        successor_byte: u8,
    ) -> ReplicatedEventWire {
        let hash = consensus_candidate_hash(&candidate.body).unwrap();
        let precommits = voters
            .iter()
            .map(|(player, device)| {
                UnsignedConsensusVoteWire {
                    schema_version: REPLICATION_SCHEMA_VERSION_V1,
                    room_id: candidate.body.room_id.clone(),
                    membership_epoch: candidate.body.membership_epoch,
                    height: candidate.body.height,
                    round: candidate.body.round,
                    phase: ConsensusVotePhaseWire::Precommit,
                    value: ConsensusValueWire::Candidate(hash),
                    voter_player_id: player.player_id.clone(),
                    voter_device_id: device.device_id.clone(),
                    lock_round: None,
                    lock_proof_hash: None,
                    signature_intent: DeviceSignatureIntentWire {
                        domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                        algorithm: SignatureAlgorithm::Ed25519,
                        key_id: device.device_id.clone(),
                    },
                }
                .attach_signature(signature())
                .unwrap()
            })
            .collect();
        ReplicatedEventWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            event_id: derive_replicated_event_id(hash).unwrap(),
            certificate: CommitCertificateWire {
                schema_version: REPLICATION_SCHEMA_VERSION_V1,
                room_id: candidate.body.room_id.clone(),
                membership_epoch: candidate.body.membership_epoch,
                height: candidate.body.height,
                round: candidate.body.round,
                candidate_hash: hash,
                precommits,
            },
            candidate,
            successor_state_hash: SemanticHash([successor_byte; 32]),
        }
    }

    #[test]
    fn replicated_devices_count_once_and_majority_commits() {
        let (mut state, players, mut devices) = fixture_state();
        assert_eq!(state.quorum(), 2);
        let alice_second = device(&players[0], 21, 2);
        state
            .register_device(alice_second.clone(), &FixtureVerifier)
            .unwrap();
        devices.push(alice_second.clone());

        let candidate = candidate(&state, &players, &devices[..3], 0, 1, None);
        let duplicate_player_event = event(
            candidate.clone(),
            &[(&players[0], &devices[0]), (&players[0], &alice_second)],
            7,
        );
        assert_eq!(
            state.apply_event(duplicate_player_event, &FixtureVerifier),
            Err(ReplicatedSessionError::InvalidEvent)
        );
        assert_eq!(state.height(), 0);

        let committed = event(
            candidate,
            &[(&players[0], &devices[0]), (&players[1], &devices[1])],
            7,
        );
        assert_eq!(
            state.apply_event(committed.clone(), &FixtureVerifier),
            Ok(ReplicatedApplyResult::Applied)
        );
        assert_eq!(
            state.apply_event(committed, &FixtureVerifier),
            Ok(ReplicatedApplyResult::Duplicate)
        );
        assert_eq!(state.height(), 1);
    }

    #[test]
    fn replicated_revocation_and_membership_change_require_joint_quorum() {
        let (mut state, players, devices) = fixture_state();
        let alice_second = device(&players[0], 21, 2);
        state
            .register_device(alice_second.clone(), &FixtureVerifier)
            .unwrap();
        let unsigned_revocation = UnsignedDeviceRevocationWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            player_id: players[2].player_id.clone(),
            device_id: devices[2].device_id.clone(),
            certificate_sequence: devices[2].sequence,
            effective_membership_epoch: 2,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: players[2].player_id.clone(),
            },
        };
        state
            .stage_revocation(
                unsigned_revocation.attach_signature(signature()).unwrap(),
                &FixtureVerifier,
            )
            .unwrap();
        let alice_revocation = UnsignedDeviceRevocationWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            player_id: players[0].player_id.clone(),
            device_id: alice_second.device_id.clone(),
            certificate_sequence: alice_second.sequence,
            effective_membership_epoch: 2,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: players[0].player_id.clone(),
            },
        };
        state
            .stage_revocation(
                alice_revocation.attach_signature(signature()).unwrap(),
                &FixtureVerifier,
            )
            .unwrap();
        let transition = MembershipTransitionWire {
            next_membership_epoch: 2,
            active_players: vec![players[0].player_id.clone(), players[1].player_id.clone()],
            revoked_devices: vec![devices[2].device_id.clone(), alice_second.device_id.clone()],
        };
        let candidate = candidate(&state, &players, &devices, 0, 2, Some(transition));
        let weak = event(candidate.clone(), &[(&players[0], &devices[0])], 8);
        assert_eq!(
            state.apply_event(weak, &FixtureVerifier),
            Err(ReplicatedSessionError::NoQuorum)
        );
        let joint = event(
            candidate,
            &[(&players[0], &devices[0]), (&players[1], &devices[1])],
            8,
        );
        assert_eq!(
            state.apply_event(joint, &FixtureVerifier),
            Ok(ReplicatedApplyResult::Applied)
        );
        assert_eq!(state.membership_epoch(), 2);
        assert_eq!(state.quorum(), 2);
        assert_eq!(state.active_players().len(), 2);
        assert_eq!(
            state.require_device(
                &players[0].player_id,
                &alice_second.device_id,
                DeviceCapabilityWire::Vote,
                2
            ),
            Err(ReplicatedSessionError::DeviceNotAuthorized)
        );
    }

    #[test]
    fn replicated_snapshot_binds_head_and_conflict_halts() {
        let (mut state, players, devices) = fixture_state();
        let first_candidate = candidate(&state, &players, &devices, 0, 3, None);
        let first = event(
            first_candidate,
            &[(&players[0], &devices[0]), (&players[1], &devices[1])],
            9,
        );
        state.apply_event(first.clone(), &FixtureVerifier).unwrap();
        let snapshot = ReplicatedSnapshotWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            snapshot_id: poche_protocol::SnapshotId::new("replicated-snapshot").unwrap(),
            room_id: RoomId::new("replicated-room").unwrap(),
            membership_epoch: 1,
            height: 1,
            head_event_id: first.event_id.clone(),
            head_event_hash: consensus_candidate_hash(&first.candidate.body).unwrap(),
            state_schema_hash: SemanticHash([6; 32]),
            state_hash: first.successor_state_hash,
            commit_certificate_hash: commit_certificate_hash(&first.certificate).unwrap(),
        };
        state.validate_snapshot_at_head(&snapshot).unwrap();

        let mut fork_state = fixture_state().0;
        let second_candidate = candidate(&fork_state, &players, &devices, 1, 4, None);
        let second = event(
            second_candidate,
            &[(&players[1], &devices[1]), (&players[2], &devices[2])],
            10,
        );
        fork_state.apply_event(first, &FixtureVerifier).unwrap();
        assert_eq!(
            fork_state.apply_event(second, &FixtureVerifier),
            Err(ReplicatedSessionError::ForkDetected)
        );
        assert_eq!(fork_state.forks().len(), 1);
        assert_eq!(fork_state.height(), 1);
    }

    #[test]
    fn replicated_uncertified_conflict_cannot_halt_the_log() {
        struct RejectCandidateVerifier;

        impl ReplicationSignatureVerifier for RejectCandidateVerifier {
            fn verify_device_certificate(
                &self,
                root: &PlayerRootWire,
                certificate: &DeviceCertificateWire,
            ) -> bool {
                FixtureVerifier.verify_device_certificate(root, certificate)
            }

            fn verify_device_revocation(
                &self,
                root: &PlayerRootWire,
                revocation: &DeviceRevocationWire,
            ) -> bool {
                FixtureVerifier.verify_device_revocation(root, revocation)
            }

            fn verify_candidate(
                &self,
                _certificate: &DeviceCertificateWire,
                _candidate: &ConsensusCandidateWire,
            ) -> bool {
                false
            }

            fn verify_vote(
                &self,
                certificate: &DeviceCertificateWire,
                vote: &ConsensusVoteWire,
            ) -> bool {
                FixtureVerifier.verify_vote(certificate, vote)
            }
        }

        let (mut state, players, devices) = fixture_state();
        let first_candidate = candidate(&state, &players, &devices, 0, 3, None);
        let first = event(
            first_candidate,
            &[(&players[0], &devices[0]), (&players[1], &devices[1])],
            9,
        );
        let conflicting_candidate = candidate(&state, &players, &devices, 1, 4, None);
        let conflicting = event(
            conflicting_candidate,
            &[(&players[1], &devices[1]), (&players[2], &devices[2])],
            10,
        );
        state.apply_event(first, &FixtureVerifier).unwrap();

        assert_eq!(
            state.apply_event(conflicting, &RejectCandidateVerifier),
            Err(ReplicatedSessionError::BadSignature)
        );
        assert!(state.forks().is_empty());
        assert_eq!(state.height(), 1);
    }

    #[test]
    fn replicated_two_player_room_requires_both_players() {
        let alice = player(1);
        let bob = player(2);
        let alice_device = device(&alice, 11, 1);
        let bob_device = device(&bob, 12, 1);
        let mut state = ReplicatedSessionState::new(
            RoomId::new("replicated-room").unwrap(),
            vec![alice.clone(), bob.clone()],
            vec![alice_device.clone(), bob_device.clone()],
            &FixtureVerifier,
        )
        .unwrap();
        assert_eq!(state.quorum(), 2);
        let candidate = candidate(
            &state,
            &[alice.clone(), bob.clone()],
            &[alice_device.clone(), bob_device.clone()],
            0,
            5,
            None,
        );
        let one_vote = event(candidate, &[(&alice, &alice_device)], 11);
        assert_eq!(
            state.apply_event(one_vote, &FixtureVerifier),
            Err(ReplicatedSessionError::NoQuorum)
        );
        assert_eq!(state.height(), 0);
    }
}
