// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Four independent evidence gates for the finite governance micro-scope.

use std::{collections::BTreeSet, error::Error, fmt, path::Path};

use poche_interchange::{
    BackendKindWire, ConfidenceKindWire, SessionAgreementWire, SessionClaimWire,
    SessionTrackEvidenceWire, compare_session_tracks,
};
use poche_native_tools::{
    AlloyCommandExpectation, AlloyCommandKind, AlloyOutcome, NativeDisposition,
    NuSmvPropertyExpectation, NuSmvPropertyKind, run_alloy_suite, run_nusmv_suite,
    run_prolog_model_fixture,
};
use poche_protocol::{
    CommandId, GovernanceCapabilityWire, GovernanceCommandV1, GovernanceCommandWire,
    GovernedActionWire, PrincipalId, ProposalId, RecoveryActionWire, RightsChangeWire,
    VoteChoiceWire, decode_governance_command,
};
use poche_session::{
    DEFAULT_VOTE_DURATION_TICKS, GovernanceInvocation, GovernanceState, ProposalRejection,
    ProposalStatus,
};

const COMPARISON_SCOPE: &str = "governance-micro";
const ALLOY_SCOPE: &str = "governance-alloy-3members-3cards-3proposals-int5";
const NUSMV_SCOPE: &str = "governance-nusmv-6modes-5steps-52cards";
const PROLOG_SCOPE: &str = "governance-prolog-5proposals-10votes";

const ALLOY_MODEL: &str = "models/alloy/governance.als";
const NUSMV_MODEL: &str = "models/nusmv/governance.smv";
const PROLOG_MODEL: &str = "models/prolog/governance.pl";

/// Complete neutral comparison after all four source gates pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceComparisonReport {
    /// Neutral agreement record; no implementation is the privileged oracle.
    pub agreement: SessionAgreementWire,
    /// Exact independently qualified evidence entering comparison.
    pub tracks: Vec<SessionTrackEvidenceWire>,
    /// Number of source implementations executed.
    pub source_gates: usize,
    /// Total native Alloy commands.
    pub alloy_commands: usize,
    /// Total native `NuSMV` properties.
    pub nusmv_properties: usize,
    /// Total normalized Scryer rows.
    pub prolog_rows: usize,
}

/// Native, Rust, or neutral evidence failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceComparisonError(String);

impl fmt::Display for GovernanceComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for GovernanceComparisonError {}

/// Execute the Rust, Alloy, `NuSMV`, and Scryer governance gates and compare
/// only claims that at least two independent tracks share.
///
/// # Errors
///
/// Fails closed on any changed fixture, native polarity/count, malformed
/// evidence, or source disagreement.
pub fn compare_governance_models(
    root: &Path,
) -> Result<GovernanceComparisonReport, GovernanceComparisonError> {
    check_rust_governance()?;
    let alloy_commands = check_alloy_governance(root)?;
    let nusmv_properties = check_nusmv_governance(root)?;
    let prolog_rows = check_prolog_governance(root)?;
    let tracks = track_evidence();
    let agreement = compare_session_tracks(&tracks)
        .map_err(|error| problem(format!("neutral governance evidence is invalid: {error}")))?;
    if !agreement.disagreements.is_empty() {
        return Err(problem(format!(
            "governance sources disagree: {:?}",
            agreement.disagreements
        )));
    }
    Ok(GovernanceComparisonReport {
        agreement,
        tracks,
        source_gates: 4,
        alloy_commands,
        nusmv_properties,
        prolog_rows,
    })
}

fn check_rust_governance() -> Result<(), GovernanceComparisonError> {
    let mut state = GovernanceState::new(
        [principal("alice")?, principal("bob")?, principal("john")?],
        DEFAULT_VOTE_DURATION_TICKS,
    )
    .map_err(|error| problem(error.to_string()))?;
    let kick = submit(
        &mut state,
        "start-kick",
        "alice",
        1,
        GovernanceCommandWire::StartVote {
            action: GovernedActionWire::Recover {
                recovery: RecoveryActionWire::Kick {
                    target: principal("john")?,
                },
            },
        },
    )?
    .proposal_id
    .ok_or_else(|| problem("Rust kick vote omitted its proposal ID"))?;
    let excluded = submit(
        &mut state,
        "john-no",
        "john",
        2,
        vote(kick.clone(), VoteChoiceWire::Reject),
    )?;
    if excluded.vote_counted != Some(false) {
        return Err(problem("Rust counted the kick target's visible vote"));
    }
    submit(
        &mut state,
        "alice-yes",
        "alice",
        3,
        vote(kick.clone(), VoteChoiceWire::Approve),
    )?;
    submit(
        &mut state,
        "bob-yes",
        "bob",
        3,
        vote(kick, VoteChoiceWire::Approve),
    )?;
    if !state.is_kicked(&principal("john")?) || state.effects().len() != 1 {
        return Err(problem("Rust did not apply approved out-of-turn kick"));
    }

    state
        .set_connected(&principal("bob")?, false)
        .map_err(|error| problem(error.to_string()))?;
    let redeal = submit(
        &mut state,
        "start-redeal",
        "alice",
        4,
        GovernanceCommandWire::StartVote {
            action: GovernedActionWire::Recover {
                recovery: RecoveryActionWire::Redeal,
            },
        },
    )?
    .proposal_id
    .ok_or_else(|| problem("Rust redeal vote omitted its proposal ID"))?;
    submit(
        &mut state,
        "alice-redeal",
        "alice",
        5,
        vote(redeal, VoteChoiceWire::Approve),
    )?;
    if state.redeal_epoch() != 1 {
        return Err(problem("Rust let unavailable players block redeal"));
    }

    check_rust_timeout_and_capability()?;
    let invalid = br#"{"schema_version":1,"command":{"kind":"execute","data":{"action":{"kind":"create_card","data":{"card":52}}}}}"#;
    if decode_governance_command(invalid).is_ok() {
        return Err(problem(
            "Rust admitted structural card mutation into the AST",
        ));
    }
    Ok(())
}

fn check_rust_timeout_and_capability() -> Result<(), GovernanceComparisonError> {
    let mut timeout = GovernanceState::new(
        [principal("alice")?, principal("bob")?],
        DEFAULT_VOTE_DURATION_TICKS,
    )
    .map_err(|error| problem(error.to_string()))?;
    let proposal = submit(
        &mut timeout,
        "start-timeout",
        "alice",
        1,
        GovernanceCommandWire::StartVote {
            action: GovernedActionWire::AdjustScore {
                target: principal("alice")?,
                delta: 10,
            },
        },
    )?
    .proposal_id
    .ok_or_else(|| problem("Rust timeout fixture omitted its proposal ID"))?;
    submit(
        &mut timeout,
        "one-yes",
        "alice",
        2,
        vote(proposal.clone(), VoteChoiceWire::Approve),
    )?;
    timeout
        .advance_to(11)
        .map_err(|error| problem(error.to_string()))?;
    let record = timeout
        .proposals()
        .iter()
        .find(|candidate| candidate.proposal_id == proposal)
        .ok_or_else(|| problem("Rust lost timeout proposal"))?;
    if !matches!(
        record.status,
        ProposalStatus::Rejected {
            reason: ProposalRejection::DeadlineNoMajority,
            ..
        }
    ) {
        return Err(problem("Rust timeout did not reject no-majority proposal"));
    }

    let mut direct = GovernanceState::new(
        [principal("alice")?, principal("bob")?],
        DEFAULT_VOTE_DURATION_TICKS,
    )
    .map_err(|error| problem(error.to_string()))?;
    direct
        .add_bootstrap_capability(principal("alice")?, GovernanceCapabilityWire::ChangeRights)
        .map_err(|error| problem(error.to_string()))?;
    submit(
        &mut direct,
        "grant-score",
        "alice",
        1,
        GovernanceCommandWire::Execute {
            action: GovernedActionWire::ChangeRights {
                target: principal("alice")?,
                capability: GovernanceCapabilityWire::AdjustScore,
                change: RightsChangeWire::Grant,
            },
        },
    )?;
    submit(
        &mut direct,
        "direct-score",
        "alice",
        2,
        GovernanceCommandWire::Execute {
            action: GovernedActionWire::AdjustScore {
                target: principal("alice")?,
                delta: 100,
            },
        },
    )?;
    if direct.score(&principal("alice")?) != Some(100) {
        return Err(problem(
            "Rust exact capability did not authorize score effect",
        ));
    }
    Ok(())
}

fn check_alloy_governance(root: &Path) -> Result<usize, GovernanceComparisonError> {
    use AlloyCommandKind::{Assertion, Witness};
    use AlloyOutcome::{Sat, Unsat};

    let expectations = [
        ("CanonicalGovernanceWitness", Witness, Sat),
        ("OutOfTurnRecoveryWitness", Witness, Sat),
        ("StrictMajorityControlsApproval", Assertion, Unsat),
        ("ExcludedVoteVisibleButNotCounted", Assertion, Unsat),
        ("VoteEffectRequiresApproval", Assertion, Unsat),
        ("NoMajorityHasNoEffect", Assertion, Unsat),
        ("PermittedAmendmentPreservesCardUniverse", Assertion, Unsat),
        ("CountedExcludedVoteDefect", Witness, Sat),
        ("CardUniverseMutationDefect", Witness, Sat),
    ]
    .map(|(name, kind, outcome)| AlloyCommandExpectation {
        name: name.to_owned(),
        kind,
        outcome,
    });
    let report = run_alloy_suite(
        root,
        "governance-alloy-micro",
        Path::new(ALLOY_MODEL),
        &expectations,
    );
    if report.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "Alloy governance gate was {:?}: {} ({})",
            report.disposition,
            report.diagnostic,
            report.evidence_directory.display()
        )));
    }
    Ok(report.results.len())
}

fn check_nusmv_governance(root: &Path) -> Result<usize, GovernanceComparisonError> {
    let expectations = [
        (
            "excluded_vote_visible_not_counted",
            NuSmvPropertyKind::Invariant,
            true,
        ),
        (
            "strict_majority_before_vote_effect",
            NuSmvPropertyKind::Invariant,
            true,
        ),
        (
            "effect_has_capability_or_approval",
            NuSmvPropertyKind::Invariant,
            true,
        ),
        (
            "finite_card_universe_is_immutable",
            NuSmvPropertyKind::Invariant,
            true,
        ),
        (
            "timeout_no_majority_rejects",
            NuSmvPropertyKind::Specification,
            true,
        ),
        (
            "afk_actor_cannot_block_recovery",
            NuSmvPropertyKind::Specification,
            true,
        ),
        (
            "governance_transition_deadlock_free",
            NuSmvPropertyKind::Specification,
            true,
        ),
        (
            "counted_excluded_vote_defect_witness",
            NuSmvPropertyKind::Invariant,
            false,
        ),
        (
            "structural_mutation_defect_witness",
            NuSmvPropertyKind::Invariant,
            false,
        ),
    ]
    .map(|(name, kind, holds)| NuSmvPropertyExpectation {
        name: name.to_owned(),
        kind,
        holds,
    });
    let report = run_nusmv_suite(
        root,
        "governance-nusmv-micro",
        Path::new(NUSMV_MODEL),
        &expectations,
    );
    if report.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "NuSMV governance gate was {:?}: {} ({})",
            report.disposition,
            report.diagnostic,
            report.evidence_directory.display()
        )));
    }
    let fsm = report
        .fsm
        .ok_or_else(|| problem("NuSMV governance gate omitted FSM diagnostics"))?;
    if !fsm.transition_total || !fsm.deadlock_free || report.counterexamples.len() != 2 {
        return Err(problem(format!(
            "NuSMV governance FSM/controls changed: {fsm:?}, counterexamples={}",
            report.counterexamples.len()
        )));
    }
    Ok(report.results.len())
}

fn check_prolog_governance(root: &Path) -> Result<usize, GovernanceComparisonError> {
    const FIXTURES: [(&str, usize); 7] = [
        ("eligibility", 10),
        ("visible_votes", 10),
        ("tallies", 5),
        ("decisions", 5),
        ("effects", 5),
        ("predecessors", 5),
        ("structural_denials", 2),
    ];
    let mut rows = 0;
    let mut all = BTreeSet::new();
    for (goal, expected) in FIXTURES {
        let report = run_prolog_model_fixture(
            root,
            &format!("governance-{}", goal.replace('_', "-")),
            Path::new(PROLOG_MODEL),
            "poche_governance",
            goal,
        );
        if report.disposition != NativeDisposition::Success || report.answers.len() != expected {
            return Err(problem(format!(
                "Prolog {goal} was {:?} with {} rows, expected {expected}: {} ({})",
                report.disposition,
                report.answers.len(),
                report.diagnostic,
                report.evidence_directory.display()
            )));
        }
        rows += report.answers.len();
        all.extend(report.answers);
    }
    for required in [
        "vote(kick_accused,john,reject,counted(no),exclusion(target_and_confirmed_accused))",
        "decision(score_timeout,rejected,logical_tick_10,because(deadline_without_majority))",
        "effect(redeal_afk,redeal,approved_vote,because(strict_majority))",
        "structural(create_card,deny(structural_invariant),because(finite_card_universe_not_governable))",
    ] {
        if !all.contains(required) {
            return Err(problem(format!("Prolog omitted required row {required}")));
        }
    }
    Ok(rows)
}

fn track_evidence() -> Vec<SessionTrackEvidenceWire> {
    use BackendKindWire::{Alloy, NuSmv, RustExplicit, ScryerProlog};
    use ConfidenceKindWire::{Bounded, Exhaustive, Queried, Symbolic};

    let claims = |ids: &[&str]| {
        ids.iter()
            .map(|id| SessionClaimWire {
                claim_id: (*id).to_owned(),
                value: true,
                rule_ids: claim_rules(id)
                    .iter()
                    .map(|rule| (*rule).to_owned())
                    .collect(),
            })
            .collect()
    };
    let all = [
        "strict-majority-controls-effect",
        "excluded-vote-visible-not-counted",
        "effect-requires-vote-or-capability",
        "timeout-without-majority-rejects",
        "afk-actor-cannot-block-recovery",
        "structural-card-universe-not-governable",
        "exact-capability-allows-direct-effect",
    ];
    vec![
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: RustExplicit,
            confidence: Exhaustive,
            qualification: "deterministic typed fixtures plus invariant validation over every submitted transition".to_owned(),
            claims: claims(&all),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: Alloy,
            confidence: Bounded,
            qualification: ALLOY_SCOPE.to_owned(),
            claims: claims(&all[..6]),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: NuSmv,
            confidence: Symbolic,
            qualification: NUSMV_SCOPE.to_owned(),
            claims: claims(&all),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: ScryerProlog,
            confidence: Queried,
            qualification: PROLOG_SCOPE.to_owned(),
            claims: claims(&all),
        },
    ]
}

fn claim_rules(claim: &str) -> &'static [&'static str] {
    match claim {
        "strict-majority-controls-effect" | "excluded-vote-visible-not-counted" => &["P3-U10"],
        "effect-requires-vote-or-capability" | "exact-capability-allows-direct-effect" => {
            &["P3-U12"]
        }
        "timeout-without-majority-rejects" | "afk-actor-cannot-block-recovery" => &["P3-U7"],
        "structural-card-universe-not-governable" => &["P3-U13"],
        _ => &["P3-U14"],
    }
}

fn submit(
    state: &mut GovernanceState,
    id: &str,
    issuer: &str,
    logical_tick: u64,
    command: GovernanceCommandWire,
) -> Result<poche_session::GovernanceReceipt, GovernanceComparisonError> {
    state
        .submit(GovernanceInvocation {
            command_id: CommandId::new(id).map_err(|error| problem(error.to_string()))?,
            issuer: principal(issuer)?,
            logical_tick,
            command: GovernanceCommandV1::new(command)
                .map_err(|error| problem(error.to_string()))?,
        })
        .map_err(|error| problem(error.to_string()))
}

fn vote(proposal_id: ProposalId, choice: VoteChoiceWire) -> GovernanceCommandWire {
    GovernanceCommandWire::Vote {
        proposal_id,
        choice,
    }
}

fn principal(value: &str) -> Result<PrincipalId, GovernanceComparisonError> {
    PrincipalId::new(value).map_err(|error| problem(error.to_string()))
}

fn problem(message: impl Into<String>) -> GovernanceComparisonError {
    GovernanceComparisonError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_governance_sources_gate_before_neutral_comparison() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = compare_governance_models(&root).expect("governance sources should agree");
        assert_eq!(report.source_gates, 4);
        assert_eq!(report.alloy_commands, 9);
        assert_eq!(report.nusmv_properties, 9);
        assert_eq!(report.prolog_rows, 42);
        assert_eq!(report.agreement.tracks, 4);
        assert_eq!(report.agreement.compared_claims, 7);
        assert_eq!(report.agreement.observations, 27);
        assert!(report.agreement.disagreements.is_empty());
    }
}
