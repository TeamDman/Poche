// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic delivery/log simulator for `poche-replicated-v1`.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use poche_protocol::{DeviceId, PrincipalId, SemanticHash, replicated_quorum};
use serde::{Deserialize, Serialize};

/// Registered finite runtime scope.
pub const REPLICATED_MICRO_SCOPE: &str =
    "replicated-micro-3players-5devices-4events-majority-partition-snapshot-tail";

/// Device authorization relevant to runtime proposal admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDeviceAuthority {
    pub device_id: DeviceId,
    pub player_id: PrincipalId,
    pub valid_from_epoch: u64,
    pub revoked_from_epoch: Option<u64>,
}

/// Immutable active-player view for one committed membership epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMembership {
    pub epoch: u64,
    pub active_players: Vec<PrincipalId>,
}

impl RuntimeMembership {
    fn validate(&self) -> Result<(), ReplicatedRuntimeError> {
        if self.epoch == 0
            || self.active_players.len() < 2
            || !self.active_players.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(ReplicatedRuntimeError::InvalidMembership);
        }
        Ok(())
    }

    #[must_use]
    pub fn quorum(&self) -> usize {
        replicated_quorum(self.active_players.len())
    }

    fn contains(&self, player: &PrincipalId) -> bool {
        self.active_players.binary_search(player).is_ok()
    }
}

/// Device-authored semantic proposal. `transition_key` is the idempotency key
/// for an effect, so many devices can propose the same deterministic advance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSemanticProposal {
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub membership_epoch: u64,
    pub transition_key: String,
    pub command_hash: SemanticHash,
}

/// Canonically ordered proposal batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCandidateBatch {
    pub membership_epoch: u64,
    pub proposals: Vec<RuntimeSemanticProposal>,
    pub batch_hash: SemanticHash,
}

/// Strict-majority-certified abstract event used by the delivery simulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertifiedRuntimeEvent {
    pub height: u64,
    pub membership_epoch: u64,
    pub parent_event_hash: SemanticHash,
    pub batch: RuntimeCandidateBatch,
    pub signer_players: Vec<PrincipalId>,
    pub next_membership: Option<RuntimeMembership>,
    pub successor_state_hash: SemanticHash,
    pub event_hash: SemanticHash,
}

/// Certified snapshot plus a canonical event tail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCertifiedSnapshot {
    pub height: u64,
    pub head_event_hash: SemanticHash,
    pub state_hash: SemanticHash,
    pub membership: RuntimeMembership,
    pub certificate_hash: SemanticHash,
}

/// One replica's deterministic committed prefix and future-event buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicatedRuntimeLog {
    membership: RuntimeMembership,
    height: u64,
    head_event_hash: SemanticHash,
    state_hash: SemanticHash,
    committed: Vec<CertifiedRuntimeEvent>,
    buffered: BTreeMap<u64, Vec<CertifiedRuntimeEvent>>,
    forked: bool,
}

/// Observable result of one delivery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeDeliveryReceipt {
    pub applied: usize,
    pub buffered: usize,
    pub duplicates: usize,
}

impl ReplicatedRuntimeLog {
    /// Construct an empty log from the certified genesis membership.
    ///
    /// # Errors
    ///
    /// Rejects an invalid membership view.
    pub fn new(membership: RuntimeMembership) -> Result<Self, ReplicatedRuntimeError> {
        membership.validate()?;
        Ok(Self {
            membership,
            height: 0,
            head_event_hash: SemanticHash([0; 32]),
            state_hash: SemanticHash([0x42; 32]),
            committed: Vec::new(),
            buffered: BTreeMap::new(),
            forked: false,
        })
    }

    /// Ingest one event in arbitrary delivery order, buffering future heights
    /// and automatically draining a now-contiguous tail.
    ///
    /// # Errors
    ///
    /// Rejects malformed/quorum-invalid events, gaps with ambiguous competing
    /// values, or any input after a fork is detected.
    pub fn ingest(
        &mut self,
        event: CertifiedRuntimeEvent,
    ) -> Result<RuntimeDeliveryReceipt, ReplicatedRuntimeError> {
        if self.forked {
            return Err(ReplicatedRuntimeError::Forked);
        }
        if let Some(committed) = self
            .committed
            .iter()
            .find(|committed| committed.height == event.height)
        {
            return if committed == &event {
                Ok(RuntimeDeliveryReceipt {
                    duplicates: 1,
                    ..RuntimeDeliveryReceipt::default()
                })
            } else {
                self.forked = true;
                Err(ReplicatedRuntimeError::CertifiedFork)
            };
        }
        if event.height > self.height.saturating_add(1) {
            let at_height = self.buffered.entry(event.height).or_default();
            if at_height.iter().any(|queued| queued == &event) {
                return Ok(RuntimeDeliveryReceipt {
                    duplicates: 1,
                    ..RuntimeDeliveryReceipt::default()
                });
            }
            at_height.push(event);
            return Ok(RuntimeDeliveryReceipt {
                buffered: 1,
                ..RuntimeDeliveryReceipt::default()
            });
        }
        let mut receipt = RuntimeDeliveryReceipt::default();
        self.apply_next(event)?;
        receipt.applied += 1;
        loop {
            let next_height = self.height.saturating_add(1);
            let Some(mut candidates) = self.buffered.remove(&next_height) else {
                break;
            };
            candidates.sort_by_key(|event| event.event_hash.0);
            candidates.dedup();
            if candidates.len() != 1 {
                self.forked = true;
                return Err(ReplicatedRuntimeError::CertifiedFork);
            }
            self.apply_next(candidates.remove(0))?;
            receipt.applied += 1;
        }
        Ok(receipt)
    }

    /// Create a certified snapshot of the current committed head.
    ///
    /// # Errors
    ///
    /// Rejects an empty log because genesis alone has no commit certificate.
    pub fn snapshot(&self) -> Result<RuntimeCertifiedSnapshot, ReplicatedRuntimeError> {
        let last = self
            .committed
            .last()
            .ok_or(ReplicatedRuntimeError::InvalidSnapshot)?;
        Ok(RuntimeCertifiedSnapshot {
            height: self.height,
            head_event_hash: self.head_event_hash,
            state_hash: self.state_hash,
            membership: self.membership.clone(),
            certificate_hash: certificate_hash(last),
        })
    }

    /// Recover a new replica from a certified snapshot. The caller still
    /// delivers every later event as a canonical tail.
    ///
    /// # Errors
    ///
    /// Rejects a zero-height/hash or invalid membership snapshot.
    pub fn from_snapshot(
        snapshot: RuntimeCertifiedSnapshot,
    ) -> Result<Self, ReplicatedRuntimeError> {
        snapshot.membership.validate()?;
        if snapshot.height == 0
            || snapshot.head_event_hash == SemanticHash([0; 32])
            || snapshot.certificate_hash == SemanticHash([0; 32])
        {
            return Err(ReplicatedRuntimeError::InvalidSnapshot);
        }
        Ok(Self {
            membership: snapshot.membership,
            height: snapshot.height,
            head_event_hash: snapshot.head_event_hash,
            state_hash: snapshot.state_hash,
            committed: Vec::new(),
            buffered: BTreeMap::new(),
            forked: false,
        })
    }

    #[must_use]
    pub const fn height(&self) -> u64 {
        self.height
    }

    #[must_use]
    pub const fn membership_epoch(&self) -> u64 {
        self.membership.epoch
    }

    #[must_use]
    pub fn quorum(&self) -> usize {
        self.membership.quorum()
    }

    #[must_use]
    pub const fn state_hash(&self) -> SemanticHash {
        self.state_hash
    }

    fn apply_next(&mut self, event: CertifiedRuntimeEvent) -> Result<(), ReplicatedRuntimeError> {
        validate_certified_event(&event, &self.membership)?;
        if event.height != self.height.saturating_add(1)
            || event.parent_event_hash != self.head_event_hash
        {
            return Err(ReplicatedRuntimeError::WrongParent);
        }
        if let Some(next) = &event.next_membership {
            validate_joint_quorum(&event.signer_players, &self.membership, next)?;
        }
        if event.successor_state_hash
            != successor_hash(
                self.state_hash,
                &event.batch,
                event.next_membership.as_ref(),
            )
        {
            return Err(ReplicatedRuntimeError::InvalidEvent);
        }
        self.height = event.height;
        self.head_event_hash = event.event_hash;
        self.state_hash = event.successor_state_hash;
        if let Some(next) = &event.next_membership {
            self.membership = next.clone();
        }
        self.committed.push(event);
        Ok(())
    }
}

/// Reproducible result of the registered runtime micro-scenario.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicatedMicroReport {
    pub scope: String,
    pub replicas: usize,
    pub committed_events: usize,
    pub proposal_attempts: usize,
    pub unique_proposals: usize,
    pub duplicate_deliveries: usize,
    pub buffered_reorders: usize,
    pub stale_device_denials: usize,
    pub revoked_device_denials: usize,
    pub minority_no_quorum_denials: usize,
    pub snapshot_installs: usize,
    pub snapshot_tail_events: usize,
    pub quorum_before_kick: usize,
    pub quorum_after_kick: usize,
    pub final_membership_epoch: u64,
    pub final_state_hash: String,
    pub convergence: bool,
    pub retained_counterexamples: Vec<String>,
}

/// Stable runtime-simulation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicatedRuntimeError {
    InvalidMembership,
    UnknownDevice,
    StaleEpoch,
    RevokedDevice,
    InactivePlayer,
    InvalidProposal,
    ConflictingProposal,
    NoQuorum,
    NoJointQuorum,
    WrongParent,
    InvalidEvent,
    InvalidSnapshot,
    CertifiedFork,
    Forked,
}

impl fmt::Display for ReplicatedRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMembership => "replicated runtime membership is invalid",
            Self::UnknownDevice => "proposal device is unknown",
            Self::StaleEpoch => "proposal uses a stale membership epoch",
            Self::RevokedDevice => "proposal device is revoked in this epoch",
            Self::InactivePlayer => "proposal player is not active in this epoch",
            Self::InvalidProposal => "semantic proposal is invalid",
            Self::ConflictingProposal => "one transition key has conflicting semantic hashes",
            Self::NoQuorum => "event lacks a strict player majority",
            Self::NoJointQuorum => "membership event lacks joint old/new majority",
            Self::WrongParent => "event does not extend the local committed head",
            Self::InvalidEvent => "certified runtime event is invalid",
            Self::InvalidSnapshot => "runtime snapshot is invalid or uncertified",
            Self::CertifiedFork => "two certified events conflict at one height",
            Self::Forked => "replica halted after certified fork evidence",
        })
    }
}

impl std::error::Error for ReplicatedRuntimeError {}

/// Execute the deterministic partition/reorder/recovery micro-scenario.
///
/// # Errors
///
/// Returns the first changed invariant, convergence failure, or missing
/// negative-control witness.
#[allow(
    clippy::too_many_lines,
    reason = "the registered delivery scenario keeps its causal event sequence auditable in one place"
)]
pub fn run_replicated_micro_check() -> Result<ReplicatedMicroReport, ReplicatedRuntimeError> {
    let alice = principal("01")?;
    let bob = principal("02")?;
    let carol = principal("03")?;
    let alice_native = device("11")?;
    let alice_browser = device("12")?;
    let alice_revoked = device("13")?;
    let bob_device = device("21")?;
    let carol_device = device("31")?;
    let membership = RuntimeMembership {
        epoch: 1,
        active_players: vec![alice.clone(), bob.clone(), carol.clone()],
    };
    membership.validate()?;
    let directory = vec![
        RuntimeDeviceAuthority {
            device_id: alice_native.clone(),
            player_id: alice.clone(),
            valid_from_epoch: 1,
            revoked_from_epoch: None,
        },
        RuntimeDeviceAuthority {
            device_id: alice_browser.clone(),
            player_id: alice.clone(),
            valid_from_epoch: 1,
            revoked_from_epoch: None,
        },
        RuntimeDeviceAuthority {
            device_id: bob_device.clone(),
            player_id: bob.clone(),
            valid_from_epoch: 1,
            revoked_from_epoch: None,
        },
        RuntimeDeviceAuthority {
            device_id: alice_revoked.clone(),
            player_id: alice.clone(),
            valid_from_epoch: 1,
            revoked_from_epoch: Some(2),
        },
        RuntimeDeviceAuthority {
            device_id: carol_device.clone(),
            player_id: carol.clone(),
            valid_from_epoch: 1,
            revoked_from_epoch: Some(2),
        },
    ];

    let mut canonical = ReplicatedRuntimeLog::new(membership.clone())?;
    let mut bob_log = canonical.clone();
    let mut carol_log = canonical.clone();

    let mut attempts = 0;
    let mut unique = 0;
    let mut pool = BTreeMap::new();
    attempts += 1;
    admit_proposal(
        &mut pool,
        proposal(&alice, &alice_native, 1, "score.round.1", 1),
        &membership,
        &directory,
    )?;
    unique += pool.len();
    let event1 = certify_event(
        &canonical,
        candidate_from_pool(1, &pool)?,
        vec![alice.clone(), bob.clone()],
        None,
    )?;
    canonical.ingest(event1.clone())?;
    bob_log.ingest(event1.clone())?;

    let mut concurrent_left = BTreeMap::new();
    let mut concurrent_right = BTreeMap::new();
    let proposals = [
        proposal(&bob, &bob_device, 1, "chat.public.1", 2),
        proposal(&alice, &alice_browser, 1, "score.adjust.1", 3),
    ];
    attempts += proposals.len();
    for proposal in &proposals {
        admit_proposal(
            &mut concurrent_left,
            proposal.clone(),
            &membership,
            &directory,
        )?;
    }
    for proposal in proposals.iter().rev() {
        admit_proposal(
            &mut concurrent_right,
            proposal.clone(),
            &membership,
            &directory,
        )?;
    }
    let batch_left = candidate_from_pool(1, &concurrent_left)?;
    let batch_right = candidate_from_pool(1, &concurrent_right)?;
    if batch_left != batch_right {
        return Err(ReplicatedRuntimeError::ConflictingProposal);
    }
    unique += batch_left.proposals.len();
    let event2 = certify_event(
        &canonical,
        batch_left,
        vec![alice.clone(), bob.clone()],
        None,
    )?;
    canonical.ingest(event2.clone())?;
    bob_log.ingest(event2.clone())?;

    let reorder = carol_log.ingest(event2.clone())?;
    let drained = carol_log.ingest(event1.clone())?;
    let duplicate = carol_log.ingest(event2.clone())?;
    let snapshot = bob_log.committed[0].clone();
    let snapshot_source = ReplicatedRuntimeLog {
        membership: membership.clone(),
        height: 1,
        head_event_hash: snapshot.event_hash,
        state_hash: snapshot.successor_state_hash,
        committed: vec![snapshot],
        buffered: BTreeMap::new(),
        forked: false,
    };
    let mut browser_log = ReplicatedRuntimeLog::from_snapshot(snapshot_source.snapshot()?)?;
    browser_log.ingest(event2.clone())?;

    let next_membership = RuntimeMembership {
        epoch: 2,
        active_players: vec![alice.clone(), bob.clone()],
    };
    let mut recovery_pool = BTreeMap::new();
    attempts += 1;
    admit_proposal(
        &mut recovery_pool,
        proposal(
            &alice,
            &alice_native,
            1,
            "recover.kick.current-actor.carol",
            4,
        ),
        &membership,
        &directory,
    )?;
    unique += recovery_pool.len();
    let event3 = certify_event(
        &canonical,
        candidate_from_pool(1, &recovery_pool)?,
        vec![alice.clone(), bob.clone()],
        Some(next_membership.clone()),
    )?;
    for log in [
        &mut canonical,
        &mut bob_log,
        &mut carol_log,
        &mut browser_log,
    ] {
        log.ingest(event3.clone())?;
    }

    let stale = proposal(&carol, &carol_device, 1, "game.advance.stale", 5);
    attempts += 1;
    let stale_result = admit_proposal(&mut BTreeMap::new(), stale, &next_membership, &directory);
    let revoked = proposal(&alice, &alice_revoked, 2, "game.advance.revoked", 6);
    attempts += 1;
    let revoked_result =
        admit_proposal(&mut BTreeMap::new(), revoked, &next_membership, &directory);

    let mut automatic_pool = BTreeMap::new();
    let auto_proposals = [
        proposal(&alice, &alice_native, 2, "automatic.state.advance.4", 7),
        proposal(&alice, &alice_browser, 2, "automatic.state.advance.4", 7),
        proposal(&bob, &bob_device, 2, "automatic.state.advance.4", 7),
    ];
    attempts += auto_proposals.len();
    for proposal in auto_proposals {
        admit_proposal(&mut automatic_pool, proposal, &next_membership, &directory)?;
    }
    unique += automatic_pool.len();
    let event4 = certify_event(
        &canonical,
        candidate_from_pool(2, &automatic_pool)?,
        vec![alice.clone(), bob.clone()],
        None,
    )?;
    for log in [
        &mut canonical,
        &mut bob_log,
        &mut carol_log,
        &mut browser_log,
    ] {
        log.ingest(event4.clone())?;
    }

    let minority = certify_event(
        &canonical,
        RuntimeCandidateBatch {
            membership_epoch: 2,
            proposals: vec![proposal(
                &bob,
                &bob_device,
                2,
                "minority.must.not.commit",
                8,
            )],
            batch_hash: SemanticHash([8; 32]),
        },
        vec![bob.clone()],
        None,
    );

    let counterexample =
        equivocation_counterexample(&membership, &[alice.clone(), bob.clone()], &[bob, carol])?;
    let hashes = [
        canonical.state_hash(),
        bob_log.state_hash(),
        carol_log.state_hash(),
        browser_log.state_hash(),
    ];
    let convergence = hashes.iter().all(|hash| *hash == hashes[0]);
    if !convergence
        || stale_result != Err(ReplicatedRuntimeError::StaleEpoch)
        || revoked_result != Err(ReplicatedRuntimeError::RevokedDevice)
        || minority != Err(ReplicatedRuntimeError::NoQuorum)
    {
        return Err(ReplicatedRuntimeError::InvalidEvent);
    }
    Ok(ReplicatedMicroReport {
        scope: REPLICATED_MICRO_SCOPE.to_owned(),
        replicas: 4,
        committed_events: usize::try_from(canonical.height())
            .map_err(|_| ReplicatedRuntimeError::InvalidEvent)?,
        proposal_attempts: attempts,
        unique_proposals: unique,
        duplicate_deliveries: duplicate.duplicates,
        buffered_reorders: reorder.buffered + drained.applied.saturating_sub(1),
        stale_device_denials: usize::from(stale_result.is_err()),
        revoked_device_denials: usize::from(revoked_result.is_err()),
        minority_no_quorum_denials: usize::from(minority.is_err()),
        snapshot_installs: 1,
        snapshot_tail_events: 1,
        quorum_before_kick: membership.quorum(),
        quorum_after_kick: next_membership.quorum(),
        final_membership_epoch: canonical.membership_epoch(),
        final_state_hash: hash_hex(hashes[0]),
        convergence,
        retained_counterexamples: vec![counterexample],
    })
}

fn admit_proposal(
    pool: &mut BTreeMap<String, RuntimeSemanticProposal>,
    proposal: RuntimeSemanticProposal,
    membership: &RuntimeMembership,
    directory: &[RuntimeDeviceAuthority],
) -> Result<(), ReplicatedRuntimeError> {
    if proposal.transition_key.is_empty() || proposal.transition_key.len() > 128 {
        return Err(ReplicatedRuntimeError::InvalidProposal);
    }
    if proposal.membership_epoch != membership.epoch {
        return Err(ReplicatedRuntimeError::StaleEpoch);
    }
    let authority = directory
        .iter()
        .find(|authority| authority.device_id == proposal.device_id)
        .ok_or(ReplicatedRuntimeError::UnknownDevice)?;
    if authority.player_id != proposal.player_id
        || proposal.membership_epoch < authority.valid_from_epoch
    {
        return Err(ReplicatedRuntimeError::UnknownDevice);
    }
    if !membership.contains(&proposal.player_id) {
        return Err(ReplicatedRuntimeError::InactivePlayer);
    }
    if authority
        .revoked_from_epoch
        .is_some_and(|epoch| proposal.membership_epoch >= epoch)
    {
        return Err(ReplicatedRuntimeError::RevokedDevice);
    }
    match pool.get(&proposal.transition_key) {
        Some(existing) if existing.command_hash != proposal.command_hash => {
            Err(ReplicatedRuntimeError::ConflictingProposal)
        }
        Some(_) => Ok(()),
        None => {
            pool.insert(proposal.transition_key.clone(), proposal);
            Ok(())
        }
    }
}

fn candidate_from_pool(
    epoch: u64,
    pool: &BTreeMap<String, RuntimeSemanticProposal>,
) -> Result<RuntimeCandidateBatch, ReplicatedRuntimeError> {
    if pool.is_empty() {
        return Err(ReplicatedRuntimeError::InvalidProposal);
    }
    let proposals = pool.values().cloned().collect::<Vec<_>>();
    let batch_hash = proposal_batch_hash(epoch, &proposals);
    Ok(RuntimeCandidateBatch {
        membership_epoch: epoch,
        proposals,
        batch_hash,
    })
}

fn certify_event(
    log: &ReplicatedRuntimeLog,
    batch: RuntimeCandidateBatch,
    mut signers: Vec<PrincipalId>,
    next_membership: Option<RuntimeMembership>,
) -> Result<CertifiedRuntimeEvent, ReplicatedRuntimeError> {
    signers.sort();
    if !signers.windows(2).all(|pair| pair[0] < pair[1])
        || signers
            .iter()
            .any(|player| !log.membership.contains(player))
        || signers.len() < log.quorum()
    {
        return Err(ReplicatedRuntimeError::NoQuorum);
    }
    if batch.membership_epoch != log.membership.epoch {
        return Err(ReplicatedRuntimeError::StaleEpoch);
    }
    if let Some(next) = &next_membership {
        validate_joint_quorum(&signers, &log.membership, next)?;
    }
    let height = log.height.saturating_add(1);
    let successor_state_hash = successor_hash(log.state_hash, &batch, next_membership.as_ref());
    let event_hash = runtime_event_hash(
        height,
        log.head_event_hash,
        batch.batch_hash,
        &signers,
        next_membership.as_ref(),
        successor_state_hash,
    );
    Ok(CertifiedRuntimeEvent {
        height,
        membership_epoch: log.membership.epoch,
        parent_event_hash: log.head_event_hash,
        batch,
        signer_players: signers,
        next_membership,
        successor_state_hash,
        event_hash,
    })
}

fn validate_certified_event(
    event: &CertifiedRuntimeEvent,
    membership: &RuntimeMembership,
) -> Result<(), ReplicatedRuntimeError> {
    if event.membership_epoch != membership.epoch
        || event.batch.membership_epoch != membership.epoch
        || event.batch.proposals.is_empty()
        || !event
            .batch
            .proposals
            .windows(2)
            .all(|pair| pair[0].transition_key < pair[1].transition_key)
        || proposal_batch_hash(membership.epoch, &event.batch.proposals) != event.batch.batch_hash
        || event
            .signer_players
            .iter()
            .any(|player| !membership.contains(player))
        || !event
            .signer_players
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || event.signer_players.len() < membership.quorum()
    {
        return Err(ReplicatedRuntimeError::InvalidEvent);
    }
    let expected = runtime_event_hash(
        event.height,
        event.parent_event_hash,
        event.batch.batch_hash,
        &event.signer_players,
        event.next_membership.as_ref(),
        event.successor_state_hash,
    );
    if expected != event.event_hash {
        return Err(ReplicatedRuntimeError::InvalidEvent);
    }
    Ok(())
}

fn validate_joint_quorum(
    signers: &[PrincipalId],
    old: &RuntimeMembership,
    next: &RuntimeMembership,
) -> Result<(), ReplicatedRuntimeError> {
    next.validate()?;
    if next.epoch != old.epoch.saturating_add(1)
        || signers.iter().filter(|player| old.contains(player)).count() < old.quorum()
        || signers
            .iter()
            .filter(|player| next.contains(player))
            .count()
            < next.quorum()
    {
        return Err(ReplicatedRuntimeError::NoJointQuorum);
    }
    Ok(())
}

fn proposal(
    player_id: &PrincipalId,
    device_id: &DeviceId,
    membership_epoch: u64,
    transition_key: &str,
    hash_byte: u8,
) -> RuntimeSemanticProposal {
    RuntimeSemanticProposal {
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        membership_epoch,
        transition_key: transition_key.to_owned(),
        command_hash: SemanticHash([hash_byte; 32]),
    }
}

fn proposal_batch_hash(epoch: u64, proposals: &[RuntimeSemanticProposal]) -> SemanticHash {
    let mut hasher = blake3::Hasher::new();
    hash_part(&mut hasher, b"POCHE\0RUNTIME-BATCH\0V1");
    hash_part(&mut hasher, &epoch.to_be_bytes());
    for proposal in proposals {
        hash_part(&mut hasher, proposal.transition_key.as_bytes());
        hash_part(&mut hasher, &proposal.command_hash.0);
    }
    SemanticHash(*hasher.finalize().as_bytes())
}

fn successor_hash(
    prior: SemanticHash,
    batch: &RuntimeCandidateBatch,
    next: Option<&RuntimeMembership>,
) -> SemanticHash {
    let mut hasher = blake3::Hasher::new();
    hash_part(&mut hasher, b"POCHE\0RUNTIME-STATE\0V1");
    hash_part(&mut hasher, &prior.0);
    hash_part(&mut hasher, &batch.batch_hash.0);
    if let Some(next) = next {
        hash_part(&mut hasher, &next.epoch.to_be_bytes());
        for player in &next.active_players {
            hash_part(&mut hasher, player.as_str().as_bytes());
        }
    }
    SemanticHash(*hasher.finalize().as_bytes())
}

fn runtime_event_hash(
    height: u64,
    parent: SemanticHash,
    batch: SemanticHash,
    signers: &[PrincipalId],
    next: Option<&RuntimeMembership>,
    successor: SemanticHash,
) -> SemanticHash {
    let mut hasher = blake3::Hasher::new();
    hash_part(&mut hasher, b"POCHE\0RUNTIME-EVENT\0V1");
    hash_part(&mut hasher, &height.to_be_bytes());
    hash_part(&mut hasher, &parent.0);
    hash_part(&mut hasher, &batch.0);
    for signer in signers {
        hash_part(&mut hasher, signer.as_str().as_bytes());
    }
    if let Some(next) = next {
        hash_part(&mut hasher, &next.epoch.to_be_bytes());
        for player in &next.active_players {
            hash_part(&mut hasher, player.as_str().as_bytes());
        }
    }
    hash_part(&mut hasher, &successor.0);
    SemanticHash(*hasher.finalize().as_bytes())
}

fn certificate_hash(event: &CertifiedRuntimeEvent) -> SemanticHash {
    let mut hasher = blake3::Hasher::new();
    hash_part(&mut hasher, b"POCHE\0RUNTIME-CERTIFICATE\0V1");
    hash_part(&mut hasher, &event.event_hash.0);
    for signer in &event.signer_players {
        hash_part(&mut hasher, signer.as_str().as_bytes());
    }
    SemanticHash(*hasher.finalize().as_bytes())
}

fn equivocation_counterexample(
    membership: &RuntimeMembership,
    left_signers: &[PrincipalId],
    right_signers: &[PrincipalId],
) -> Result<String, ReplicatedRuntimeError> {
    let left = left_signers.iter().cloned().collect::<BTreeSet<_>>();
    let right = right_signers.iter().cloned().collect::<BTreeSet<_>>();
    let intersection = left.intersection(&right).cloned().collect::<Vec<_>>();
    if left.len() < membership.quorum()
        || right.len() < membership.quorum()
        || intersection.len() != 1
    {
        return Err(ReplicatedRuntimeError::InvalidEvent);
    }
    Ok(format!(
        "non-equivocation-removed: two {}-of-{} certificates intersect only at player {}; conflicting commits become possible",
        membership.quorum(),
        membership.active_players.len(),
        intersection[0].as_str()
    ))
}

fn principal(byte: &str) -> Result<PrincipalId, ReplicatedRuntimeError> {
    PrincipalId::new(byte.repeat(32)).map_err(|_| ReplicatedRuntimeError::InvalidMembership)
}

fn device(byte: &str) -> Result<DeviceId, ReplicatedRuntimeError> {
    DeviceId::new(byte.repeat(32)).map_err(|_| ReplicatedRuntimeError::InvalidMembership)
}

fn hash_part(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn hash_hex(hash: SemanticHash) -> String {
    hash.0.iter().fold(String::new(), |mut output, byte| {
        use fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replicated_micro_converges_and_retains_assumption_counterexample() {
        let report = run_replicated_micro_check().expect("registered assumptions should converge");
        assert_eq!(report.scope, REPLICATED_MICRO_SCOPE);
        assert_eq!(report.replicas, 4);
        assert_eq!(report.committed_events, 4);
        assert_eq!(report.proposal_attempts, 9);
        assert_eq!(report.unique_proposals, 5);
        assert_eq!(report.duplicate_deliveries, 1);
        assert_eq!(report.buffered_reorders, 2);
        assert_eq!(report.stale_device_denials, 1);
        assert_eq!(report.revoked_device_denials, 1);
        assert_eq!(report.minority_no_quorum_denials, 1);
        assert_eq!(report.snapshot_installs, 1);
        assert_eq!(report.snapshot_tail_events, 1);
        assert_eq!(report.quorum_before_kick, 2);
        assert_eq!(report.quorum_after_kick, 2);
        assert_eq!(report.final_membership_epoch, 2);
        assert!(report.convergence);
        assert_eq!(report.retained_counterexamples.len(), 1);
        assert_eq!(
            report.final_state_hash,
            "1b51070836855771b6e51e8a0aae8cffe4784af2c478674e13f579ab5713c168"
        );
    }

    #[test]
    fn replicated_future_delivery_is_buffered_without_mutation() {
        let membership = RuntimeMembership {
            epoch: 1,
            active_players: vec![principal("01").unwrap(), principal("02").unwrap()],
        };
        let mut log = ReplicatedRuntimeLog::new(membership).unwrap();
        let future = CertifiedRuntimeEvent {
            height: 2,
            membership_epoch: 1,
            parent_event_hash: SemanticHash([9; 32]),
            batch: RuntimeCandidateBatch {
                membership_epoch: 1,
                proposals: Vec::new(),
                batch_hash: SemanticHash([8; 32]),
            },
            signer_players: Vec::new(),
            next_membership: None,
            successor_state_hash: SemanticHash([7; 32]),
            event_hash: SemanticHash([6; 32]),
        };
        let receipt = log.ingest(future.clone()).unwrap();
        assert_eq!(receipt.buffered, 1);
        assert_eq!(log.height(), 0);
        assert_eq!(log.ingest(future).unwrap().duplicates, 1);
    }
}
