// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Explicit cryptographic-abort and governance recovery composition.
//!
//! The unanimous-share profile in ADR 0008 cannot recover an abandoned hand.
//! This module makes every registered failure an immediate shared phase and
//! routes remedies through the existing typed governance reducer. There is no
//! current-actor requirement, background tick, or fabricated secret recovery.

use core::fmt;

use poche_crypto_prototype::RoundContext;
use poche_protocol::{
    CommandId, GovernanceCommandV1, GovernanceCommandWire, GovernedActionWire, PrincipalId,
    RecoveryActionWire, VoteChoiceWire,
};
use poche_session::{
    DEFAULT_VOTE_DURATION_TICKS, GovernanceError, GovernanceInvocation, GovernanceReceipt,
    GovernanceState,
};

/// Honest lifecycle stages before an abort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TrustlessStage {
    KeySetup,
    Shuffling,
    PrivateDeal,
    Holding,
    PublicPlay,
    FinalDisclosure,
    Complete,
}

impl TrustlessStage {
    const fn next(self) -> Option<Self> {
        match self {
            Self::KeySetup => Some(Self::Shuffling),
            Self::Shuffling => Some(Self::PrivateDeal),
            Self::PrivateDeal => Some(Self::Holding),
            Self::Holding => Some(Self::PublicPlay),
            Self::PublicPlay => Some(Self::FinalDisclosure),
            Self::FinalDisclosure => Some(Self::Complete),
            Self::Complete => None,
        }
    }

    const fn code(self) -> u8 {
        self as u8
    }
}

/// Every dropout point registered by plan task 6.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DropoutPoint {
    BeforeShuffleContribution,
    AfterShuffleContribution,
    DuringPrivateDeal,
    WhileHoldingCards,
    BeforeFinalDisclosure,
}

impl DropoutPoint {
    /// Exact lifecycle stage in which this evidence can be recorded.
    #[must_use]
    pub const fn stage(self) -> TrustlessStage {
        match self {
            Self::BeforeShuffleContribution | Self::AfterShuffleContribution => {
                TrustlessStage::Shuffling
            }
            Self::DuringPrivateDeal => TrustlessStage::PrivateDeal,
            Self::WhileHoldingCards => TrustlessStage::Holding,
            Self::BeforeFinalDisclosure => TrustlessStage::FinalDisclosure,
        }
    }

    const fn expected(self) -> ExpectedContribution {
        match self {
            Self::BeforeShuffleContribution => ExpectedContribution::Shuffle,
            Self::AfterShuffleContribution | Self::DuringPrivateDeal => {
                ExpectedContribution::PrivateRevealShare
            }
            Self::WhileHoldingCards => ExpectedContribution::PublicRevealShare,
            Self::BeforeFinalDisclosure => ExpectedContribution::FinalDisclosureShare,
        }
    }

    const fn code(self) -> u8 {
        self as u8
    }
}

/// Exact contribution whose absence or invalidity stopped the round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExpectedContribution {
    Shuffle,
    PrivateRevealShare,
    PublicRevealShare,
    FinalDisclosureShare,
}

impl ExpectedContribution {
    const fn code(self) -> u8 {
        self as u8
    }
}

/// Evidence classification. Rule cheating remains a separate retrospective audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CryptographicFailureCause {
    Disconnected,
    UnavailableShare,
    InvalidProof,
}

impl CryptographicFailureCause {
    const fn code(self) -> u8 {
        self as u8
    }
}

/// Stable evidence explaining why one cryptographic hand was abandoned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptographicAbort {
    pub abort_id: [u8; 32],
    pub context_digest: [u8; 32],
    pub transcript_head: [u8; 32],
    pub point: DropoutPoint,
    pub stage: TrustlessStage,
    pub subject: PrincipalId,
    pub expected: ExpectedContribution,
    pub cause: CryptographicFailureCause,
    pub logical_tick: u64,
}

impl CryptographicAbort {
    /// Canonical evidence bytes used by the smoke receipt.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"poche-cryptographic-abort-v0");
        bytes.extend_from_slice(&self.context_digest);
        bytes.extend_from_slice(&self.transcript_head);
        bytes.push(self.point.code());
        bytes.push(self.stage.code());
        push_bytes(&mut bytes, self.subject.as_str().as_bytes());
        bytes.push(self.expected.code());
        bytes.push(self.cause.code());
        bytes.extend_from_slice(&self.logical_tick.to_be_bytes());
        bytes
    }
}

/// Shared trustless lifecycle phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustlessPhase {
    Active {
        stage: TrustlessStage,
        transcript_head: [u8; 32],
    },
    RecoveryPending {
        abort: CryptographicAbort,
    },
    RedealRequired {
        abort: CryptographicAbort,
        redeal_epoch: u64,
    },
    Ended {
        abort: CryptographicAbort,
    },
}

/// Observable recovery effect applied by typed governance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustlessLifecycleEvent {
    AbortRecorded(CryptographicAbort),
    PlayerKicked {
        abort_id: [u8; 32],
        player: PrincipalId,
    },
    RedealRequired {
        abort_id: [u8; 32],
        redeal_epoch: u64,
    },
    GameEnded {
        abort_id: [u8; 32],
    },
}

/// Failures at the composition boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustlessRuntimeError {
    InvalidPlayerIdentity,
    InvalidStage,
    UnknownPlayer,
    ConflictingAbort,
    Governance(GovernanceError),
    InvalidCommand,
}

impl fmt::Display for TrustlessRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlayerIdentity => {
                formatter.write_str("cryptographic player is not a protocol principal")
            }
            Self::InvalidStage => formatter.write_str("invalid trustless lifecycle transition"),
            Self::UnknownPlayer => formatter.write_str("abort subject is not in the roster"),
            Self::ConflictingAbort => formatter.write_str("a different abort is already active"),
            Self::Governance(error) => write!(formatter, "governance: {error}"),
            Self::InvalidCommand => formatter.write_str("invalid generated governance command"),
        }
    }
}

impl std::error::Error for TrustlessRuntimeError {}

impl From<GovernanceError> for TrustlessRuntimeError {
    fn from(value: GovernanceError) -> Self {
        Self::Governance(value)
    }
}

/// Pure lifecycle plus the existing typed governance overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustlessRoundState {
    context_digest: [u8; 32],
    roster: Vec<PrincipalId>,
    phase: TrustlessPhase,
    governance: GovernanceState,
    reconciled_effects: usize,
    events: Vec<TrustlessLifecycleEvent>,
}

impl TrustlessRoundState {
    /// Start key setup for one exact ADR-0008 context.
    ///
    /// # Errors
    ///
    /// Rejects a cryptographic player name that is not a valid protocol
    /// principal or an invalid governance roster.
    pub fn new(context: &RoundContext) -> Result<Self, TrustlessRuntimeError> {
        let roster = context
            .roster()
            .iter()
            .map(|player| {
                PrincipalId::new(player.as_str())
                    .map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let governance = GovernanceState::new(roster.clone(), DEFAULT_VOTE_DURATION_TICKS)?;
        Ok(Self {
            context_digest: context.digest(),
            roster,
            phase: TrustlessPhase::Active {
                stage: TrustlessStage::KeySetup,
                transcript_head: context.digest(),
            },
            governance,
            reconciled_effects: 0,
            events: Vec::new(),
        })
    }

    /// Advance to the same or immediately following honest stage.
    ///
    /// Repeating a stage records a new accepted transcript head, which permits
    /// multiple roster-ordered shuffle contributions without inventing substages.
    ///
    /// # Errors
    ///
    /// Rejects skips, backwards motion, or progress after recovery/termination.
    pub fn advance(
        &mut self,
        next: TrustlessStage,
        transcript_head: [u8; 32],
    ) -> Result<(), TrustlessRuntimeError> {
        let TrustlessPhase::Active { stage, .. } = &mut self.phase else {
            return Err(TrustlessRuntimeError::InvalidStage);
        };
        if next != *stage && stage.next() != Some(next) {
            return Err(TrustlessRuntimeError::InvalidStage);
        }
        *stage = next;
        self.phase = TrustlessPhase::Active {
            stage: next,
            transcript_head,
        };
        Ok(())
    }

    /// Immediately leave ordinary play for a stable governable abort.
    ///
    /// This command is out-of-turn. Calling it twice with the same evidence is
    /// idempotent; a conflicting second abort is rejected.
    ///
    /// # Errors
    ///
    /// Rejects a point outside the current stage, an unknown subject, or a
    /// conflicting already-recorded abort.
    pub fn record_failure(
        &mut self,
        point: DropoutPoint,
        subject: &PrincipalId,
        cause: CryptographicFailureCause,
        logical_tick: u64,
    ) -> Result<[u8; 32], TrustlessRuntimeError> {
        if !self.roster.contains(subject) {
            return Err(TrustlessRuntimeError::UnknownPlayer);
        }
        let (stage, transcript_head) = match &self.phase {
            TrustlessPhase::Active {
                stage,
                transcript_head,
            } if *stage == point.stage() => (*stage, *transcript_head),
            TrustlessPhase::RecoveryPending { abort }
                if abort.point == point
                    && abort.subject == *subject
                    && abort.cause == cause
                    && abort.logical_tick == logical_tick =>
            {
                return Ok(abort.abort_id);
            }
            TrustlessPhase::RecoveryPending { .. }
            | TrustlessPhase::RedealRequired { .. }
            | TrustlessPhase::Ended { .. } => {
                return Err(TrustlessRuntimeError::ConflictingAbort);
            }
            TrustlessPhase::Active { .. } => return Err(TrustlessRuntimeError::InvalidStage),
        };
        let expected = point.expected();
        let mut evidence = CryptographicAbort {
            abort_id: [0; 32],
            context_digest: self.context_digest,
            transcript_head,
            point,
            stage,
            subject: subject.clone(),
            expected,
            cause,
            logical_tick,
        };
        evidence.abort_id = *blake3::hash(&evidence.canonical_bytes()).as_bytes();
        if cause == CryptographicFailureCause::Disconnected {
            self.governance.set_connected(subject, false)?;
        }
        self.events
            .push(TrustlessLifecycleEvent::AbortRecorded(evidence.clone()));
        let abort_id = evidence.abort_id;
        self.phase = TrustlessPhase::RecoveryPending { abort: evidence };
        Ok(abort_id)
    }

    /// Submit a typed proposal/vote/direct recovery command and immediately
    /// reconcile any accepted effects into the trustless phase.
    ///
    /// # Errors
    ///
    /// Propagates typed governance validation/authorization failures or rejects
    /// a recovery effect outside an active abort.
    pub fn submit_governance(
        &mut self,
        invocation: GovernanceInvocation,
    ) -> Result<GovernanceReceipt, TrustlessRuntimeError> {
        let receipt = self.governance.submit(invocation)?;
        self.reconcile_effects()?;
        Ok(receipt)
    }

    /// Current explicit phase.
    #[must_use]
    pub const fn phase(&self) -> &TrustlessPhase {
        &self.phase
    }

    /// Existing typed governance state, exposed read-only for UI/audit.
    #[must_use]
    pub const fn governance(&self) -> &GovernanceState {
        &self.governance
    }

    /// Append-only lifecycle evidence.
    #[must_use]
    pub fn events(&self) -> &[TrustlessLifecycleEvent] {
        &self.events
    }

    fn reconcile_effects(&mut self) -> Result<(), TrustlessRuntimeError> {
        let effects = self.governance.effects()[self.reconciled_effects..].to_vec();
        self.reconciled_effects = self.governance.effects().len();
        for effect in effects {
            let abort = match &self.phase {
                TrustlessPhase::RecoveryPending { abort } => abort.clone(),
                TrustlessPhase::RedealRequired { .. } | TrustlessPhase::Ended { .. } => {
                    return Err(TrustlessRuntimeError::InvalidStage);
                }
                TrustlessPhase::Active { .. } => continue,
            };
            let GovernedActionWire::Recover { recovery } = effect.action else {
                continue;
            };
            match recovery {
                RecoveryActionWire::Kick { target } => {
                    self.events.push(TrustlessLifecycleEvent::PlayerKicked {
                        abort_id: abort.abort_id,
                        player: target,
                    });
                    // Kicking changes the next roster but cannot reconstruct this hand.
                }
                RecoveryActionWire::Redeal => {
                    let redeal_epoch = self.governance.redeal_epoch();
                    self.events.push(TrustlessLifecycleEvent::RedealRequired {
                        abort_id: abort.abort_id,
                        redeal_epoch,
                    });
                    self.phase = TrustlessPhase::RedealRequired {
                        abort,
                        redeal_epoch,
                    };
                }
                RecoveryActionWire::EndGame => {
                    self.events.push(TrustlessLifecycleEvent::GameEnded {
                        abort_id: abort.abort_id,
                    });
                    self.phase = TrustlessPhase::Ended { abort };
                }
            }
        }
        Ok(())
    }
}

/// Deterministic dropout matrix receipt printed by `poche-xtask`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustlessDropoutReport {
    pub scope: &'static str,
    pub scenarios: usize,
    pub aborts: usize,
    pub kicks: usize,
    pub redeals: usize,
    pub ended_games: usize,
    pub no_hangs: bool,
    pub retrospective_cheat_separate: bool,
    pub corpus_digest: String,
}

/// Execute all five registered dropout points through vote-authorized remedies.
///
/// # Errors
///
/// Returns the first context, lifecycle, command, or governance error.
pub fn run_trustless_dropout_smoke() -> Result<TrustlessDropoutReport, TrustlessRuntimeError> {
    let points = [
        DropoutPoint::BeforeShuffleContribution,
        DropoutPoint::AfterShuffleContribution,
        DropoutPoint::DuringPrivateDeal,
        DropoutPoint::WhileHoldingCards,
        DropoutPoint::BeforeFinalDisclosure,
    ];
    let causes = [
        CryptographicFailureCause::Disconnected,
        CryptographicFailureCause::Disconnected,
        CryptographicFailureCause::UnavailableShare,
        CryptographicFailureCause::Disconnected,
        CryptographicFailureCause::InvalidProof,
    ];
    let mut corpus = Vec::new();
    let mut aborts = 0;
    let mut kicks = 0;
    let mut redeals = 0;
    let mut ended_games = 0;
    let mut no_hangs = true;

    for (scenario, (point, cause)) in points.into_iter().zip(causes).enumerate() {
        let context = smoke_context(scenario)?;
        let mut state = TrustlessRoundState::new(&context)?;
        advance_to(&mut state, point.stage(), scenario)?;
        let subject =
            PrincipalId::new("carol").map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)?;
        state.record_failure(point, &subject, cause, 10)?;
        aborts += 1;

        if scenario == 0 || scenario == 3 {
            approve_recovery(
                &mut state,
                scenario,
                20,
                RecoveryActionWire::Kick {
                    target: subject.clone(),
                },
            )?;
            kicks += 1;
        }

        if scenario == 2 {
            approve_recovery(&mut state, scenario, 30, RecoveryActionWire::EndGame)?;
            ended_games += 1;
        } else {
            approve_recovery(&mut state, scenario, 30, RecoveryActionWire::Redeal)?;
            redeals += 1;
        }
        no_hangs &= matches!(
            state.phase(),
            TrustlessPhase::RedealRequired { .. } | TrustlessPhase::Ended { .. }
        );
        for event in state.events() {
            encode_event(&mut corpus, event);
        }
    }

    Ok(TrustlessDropoutReport {
        scope: "dropout-3p-5points-unanimous-reveal-v0",
        scenarios: points.len(),
        aborts,
        kicks,
        redeals,
        ended_games,
        no_hangs,
        retrospective_cheat_separate: true,
        corpus_digest: hex(*blake3::hash(&corpus).as_bytes()),
    })
}

fn smoke_context(scenario: usize) -> Result<RoundContext, TrustlessRuntimeError> {
    let players = ["alice", "bob", "carol"]
        .into_iter()
        .map(poche_crypto_prototype::PlayerId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)?;
    RoundContext::new(
        format!("dropout-{scenario}"),
        [u8::try_from(scenario + 1).map_err(|_| TrustlessRuntimeError::InvalidStage)?; 32],
        4,
        9,
        players,
    )
    .map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)
}

fn advance_to(
    state: &mut TrustlessRoundState,
    target: TrustlessStage,
    scenario: usize,
) -> Result<(), TrustlessRuntimeError> {
    loop {
        let TrustlessPhase::Active { stage, .. } = state.phase() else {
            return Err(TrustlessRuntimeError::InvalidStage);
        };
        if *stage == target {
            return Ok(());
        }
        let next = stage.next().ok_or(TrustlessRuntimeError::InvalidStage)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"poche-dropout-smoke-progress");
        bytes.extend_from_slice(&scenario.to_be_bytes());
        bytes.push(next.code());
        state.advance(next, *blake3::hash(&bytes).as_bytes())?;
    }
}

fn approve_recovery(
    state: &mut TrustlessRoundState,
    scenario: usize,
    tick: u64,
    recovery: RecoveryActionWire,
) -> Result<(), TrustlessRuntimeError> {
    let alice =
        PrincipalId::new("alice").map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)?;
    let bob = PrincipalId::new("bob").map_err(|_| TrustlessRuntimeError::InvalidPlayerIdentity)?;
    let action = GovernedActionWire::Recover { recovery };
    let start = governance_command(GovernanceCommandWire::StartVote { action })?;
    let receipt = state.submit_governance(GovernanceInvocation {
        command_id: command_id(scenario, tick, "start")?,
        issuer: alice.clone(),
        logical_tick: tick,
        command: start,
    })?;
    let proposal_id = receipt
        .proposal_id
        .ok_or(TrustlessRuntimeError::InvalidCommand)?;
    for (offset, voter) in [(1_u64, alice), (2, bob)] {
        let vote = governance_command(GovernanceCommandWire::Vote {
            proposal_id: proposal_id.clone(),
            choice: VoteChoiceWire::Approve,
        })?;
        state.submit_governance(GovernanceInvocation {
            command_id: command_id(scenario, tick + offset, "vote")?,
            issuer: voter,
            logical_tick: tick + offset,
            command: vote,
        })?;
    }
    Ok(())
}

fn governance_command(
    command: GovernanceCommandWire,
) -> Result<GovernanceCommandV1, TrustlessRuntimeError> {
    GovernanceCommandV1::new(command).map_err(|_| TrustlessRuntimeError::InvalidCommand)
}

fn command_id(scenario: usize, tick: u64, kind: &str) -> Result<CommandId, TrustlessRuntimeError> {
    CommandId::new(format!("trustless-{scenario}-{tick}-{kind}"))
        .map_err(|_| TrustlessRuntimeError::InvalidCommand)
}

fn encode_event(output: &mut Vec<u8>, event: &TrustlessLifecycleEvent) {
    match event {
        TrustlessLifecycleEvent::AbortRecorded(abort) => {
            output.push(0);
            push_bytes(output, &abort.canonical_bytes());
        }
        TrustlessLifecycleEvent::PlayerKicked { abort_id, player } => {
            output.push(1);
            output.extend_from_slice(abort_id);
            push_bytes(output, player.as_str().as_bytes());
        }
        TrustlessLifecycleEvent::RedealRequired {
            abort_id,
            redeal_epoch,
        } => {
            output.push(2);
            output.extend_from_slice(abort_id);
            output.extend_from_slice(&redeal_epoch.to_be_bytes());
        }
        TrustlessLifecycleEvent::GameEnded { abort_id } => {
            output.push(3);
            output.extend_from_slice(abort_id);
        }
    }
}

fn push_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(
        &u32::try_from(bytes.len())
            .expect("bounded trustless records fit u32")
            .to_be_bytes(),
    );
    output.extend_from_slice(bytes);
}

fn hex(bytes: [u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trustless_dropout_matrix_reaches_only_explicit_terminal_recovery() {
        let report = run_trustless_dropout_smoke().unwrap();
        assert_eq!(report.scope, "dropout-3p-5points-unanimous-reveal-v0");
        assert_eq!(report.scenarios, 5);
        assert_eq!(report.aborts, 5);
        assert_eq!(report.kicks, 2);
        assert_eq!(report.redeals, 4);
        assert_eq!(report.ended_games, 1);
        assert!(report.no_hangs);
        assert!(report.retrospective_cheat_separate);
        assert_eq!(
            report.corpus_digest,
            "1134633ac68490a908103d8e2751ec423e9970f1c813a4dbb15f6e31b52d8624"
        );
    }

    #[test]
    fn trustless_dropout_is_out_of_turn_idempotent_and_kick_does_not_restore_secrets() {
        let context = smoke_context(8).unwrap();
        let mut state = TrustlessRoundState::new(&context).unwrap();
        advance_to(&mut state, TrustlessStage::Holding, 8).unwrap();
        let carol = PrincipalId::new("carol").unwrap();
        let abort_id = state
            .record_failure(
                DropoutPoint::WhileHoldingCards,
                &carol,
                CryptographicFailureCause::Disconnected,
                10,
            )
            .unwrap();
        assert_eq!(
            state
                .record_failure(
                    DropoutPoint::WhileHoldingCards,
                    &carol,
                    CryptographicFailureCause::Disconnected,
                    10,
                )
                .unwrap(),
            abort_id
        );
        approve_recovery(
            &mut state,
            8,
            20,
            RecoveryActionWire::Kick { target: carol },
        )
        .unwrap();
        assert!(matches!(
            state.phase(),
            TrustlessPhase::RecoveryPending { .. }
        ));
        assert!(
            state
                .governance()
                .is_kicked(&PrincipalId::new("carol").unwrap())
        );
        assert_eq!(state.governance().redeal_epoch(), 0);

        approve_recovery(&mut state, 8, 30, RecoveryActionWire::Redeal).unwrap();
        assert!(matches!(
            state.phase(),
            TrustlessPhase::RedealRequired {
                redeal_epoch: 1,
                ..
            }
        ));
    }

    #[test]
    fn trustless_dropout_wrong_stage_and_conflicting_abort_fail_closed() {
        let context = smoke_context(9).unwrap();
        let mut state = TrustlessRoundState::new(&context).unwrap();
        let carol = PrincipalId::new("carol").unwrap();
        assert_eq!(
            state.record_failure(
                DropoutPoint::DuringPrivateDeal,
                &carol,
                CryptographicFailureCause::UnavailableShare,
                10,
            ),
            Err(TrustlessRuntimeError::InvalidStage)
        );
        advance_to(&mut state, TrustlessStage::Shuffling, 9).unwrap();
        state
            .record_failure(
                DropoutPoint::BeforeShuffleContribution,
                &carol,
                CryptographicFailureCause::Disconnected,
                10,
            )
            .unwrap();
        assert_eq!(
            state.record_failure(
                DropoutPoint::AfterShuffleContribution,
                &carol,
                CryptographicFailureCause::Disconnected,
                10,
            ),
            Err(TrustlessRuntimeError::ConflictingAbort)
        );
        assert_eq!(
            state.advance(TrustlessStage::PrivateDeal, [4; 32]),
            Err(TrustlessRuntimeError::InvalidStage)
        );
    }
}
