// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Four independently gated evidence tracks for replicated consensus.

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
use poche_protocol::replicated_quorum;
use poche_runtime::{REPLICATED_MICRO_SCOPE, run_replicated_micro_check};

const COMPARISON_SCOPE: &str = "consensus-micro";
const ALLOY_SCOPE: &str = "consensus-alloy-3players-5devices-2epochs-int5";
const NUSMV_SCOPE: &str = "consensus-nusmv-10schedulers-5steps-4replicas";
const PROLOG_SCOPE: &str = "consensus-prolog-34-explanation-rows";

const ALLOY_MODEL: &str = "models/alloy/consensus.als";
const NUSMV_MODEL: &str = "models/nusmv/consensus.smv";
const PROLOG_MODEL: &str = "models/prolog/consensus.pl";

pub(crate) const CONSENSUS_OBLIGATION_IDS: [&str; 10] = [
    "P3-C-DEVICE-WEIGHT",
    "P3-C-QUORUM",
    "P3-C-STALE-REVOKED",
    "P3-C-CANONICAL-BATCH",
    "P3-C-FORK-SAFETY",
    "P3-C-RECOVERY",
    "P3-C-CONDITIONAL-LIVENESS",
    "P3-C-SNAPSHOT-TAIL",
    "P3-C-LOCAL-PROPOSAL",
    "P3-C-TWO-PLAYER-LIMIT",
];

/// Complete neutral comparison after every native source gate succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusComparisonReport {
    pub agreement: SessionAgreementWire,
    pub tracks: Vec<SessionTrackEvidenceWire>,
    pub source_gates: usize,
    pub alloy_commands: usize,
    pub nusmv_properties: usize,
    pub prolog_rows: usize,
}

/// Native, Rust, or neutral replicated-consensus evidence failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusComparisonError(String);

impl fmt::Display for ConsensusComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConsensusComparisonError {}

/// Run the Rust, Alloy, `NuSMV`, and Scryer gates before comparing only their
/// genuinely shared, explicitly qualified claims.
///
/// # Errors
///
/// Fails closed on runtime receipt drift, changed native polarity/count,
/// malformed neutral evidence, or any shared-claim disagreement.
pub fn compare_consensus_models(
    root: &Path,
) -> Result<ConsensusComparisonReport, ConsensusComparisonError> {
    check_rust_consensus()?;
    let alloy_commands = check_alloy_consensus(root)?;
    let nusmv_properties = check_nusmv_consensus(root)?;
    let prolog_rows = check_prolog_consensus(root)?;
    let tracks = consensus_track_evidence();
    let agreement = compare_session_tracks(&tracks)
        .map_err(|error| problem(format!("neutral consensus evidence is invalid: {error}")))?;
    if !agreement.disagreements.is_empty() {
        return Err(problem(format!(
            "consensus sources disagree: {:?}",
            agreement.disagreements
        )));
    }
    Ok(ConsensusComparisonReport {
        agreement,
        tracks,
        source_gates: 4,
        alloy_commands,
        nusmv_properties,
        prolog_rows,
    })
}

fn check_rust_consensus() -> Result<(), ConsensusComparisonError> {
    let report = run_replicated_micro_check().map_err(|error| problem(error.to_string()))?;
    if report.scope != REPLICATED_MICRO_SCOPE
        || report.replicas != 4
        || report.committed_events != 4
        || report.proposal_attempts != 9
        || report.unique_proposals != 5
        || report.duplicate_deliveries != 1
        || report.buffered_reorders != 2
        || report.stale_device_denials != 1
        || report.revoked_device_denials != 1
        || report.minority_no_quorum_denials != 1
        || report.snapshot_installs != 1
        || report.snapshot_tail_events != 1
        || report.quorum_before_kick != 2
        || report.quorum_after_kick != 2
        || report.final_membership_epoch != 2
        || !report.convergence
        || report.retained_counterexamples.len() != 1
    {
        return Err(problem(format!(
            "Rust consensus receipt drifted: {report:?}"
        )));
    }
    check_bounded_quorum_intersection()
}

fn check_bounded_quorum_intersection() -> Result<(), ConsensusComparisonError> {
    for players in 2_u32..=8 {
        let player_count = usize::try_from(players).map_err(|_| problem("player bound"))?;
        let quorum = replicated_quorum(player_count);
        let limit = 1_u32 << players;
        for left in 0..limit {
            if usize::try_from(left.count_ones()).unwrap_or(0) < quorum {
                continue;
            }
            for right in 0..limit {
                if usize::try_from(right.count_ones()).unwrap_or(0) >= quorum && left & right == 0 {
                    return Err(problem(format!(
                        "disjoint strict majorities at player_count={players}: {left:b}/{right:b}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn check_alloy_consensus(root: &Path) -> Result<usize, ConsensusComparisonError> {
    use AlloyCommandKind::{Assertion, Witness};
    use AlloyOutcome::{Sat, Unsat};

    let expectations = [
        ("OutOfTurnKickWitness", Witness, Sat),
        ("AutomaticProposalCollapseWitness", Witness, Sat),
        (
            "DeviceMultiplicityDoesNotIncreaseVotingWeight",
            Assertion,
            Unsat,
        ),
        ("StrictMajorityControlsCertification", Assertion, Unsat),
        ("StaleOrRevokedDeviceCannotCertify", Assertion, Unsat),
        ("CanonicalBatchHasOneCommandPerKey", Assertion, Unsat),
        ("MajorityCertificatesIntersect", Assertion, Unsat),
        ("TwoPlayerEpochRequiresBothPlayers", Assertion, Unsat),
        ("EquivocationForkWitness", Witness, Sat),
    ]
    .map(|(name, kind, outcome)| AlloyCommandExpectation {
        name: name.to_owned(),
        kind,
        outcome,
    });
    let report = run_alloy_suite(
        root,
        "consensus-alloy-micro",
        Path::new(ALLOY_MODEL),
        &expectations,
    );
    if report.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "Alloy consensus gate was {:?}: {} ({})",
            report.disposition,
            report.diagnostic,
            report.evidence_directory.display()
        )));
    }
    Ok(report.results.len())
}

fn check_nusmv_consensus(root: &Path) -> Result<usize, ConsensusComparisonError> {
    use NuSmvPropertyKind::{Invariant, Specification};

    let expectations = [
        (
            "device_multiplicity_does_not_add_player_votes",
            Invariant,
            true,
        ),
        ("minority_cannot_commit", Invariant, true),
        ("stale_device_is_denied", Invariant, true),
        ("revoked_device_is_denied", Invariant, true),
        (
            "automatic_proposals_collapse_to_one_effect",
            Invariant,
            true,
        ),
        ("fork_safety_under_non_equivocation", Invariant, true),
        ("commit_requires_player_quorum", Invariant, true),
        ("out_of_turn_kick_recovers", Specification, true),
        (
            "conditional_eventual_delivery_converges",
            Specification,
            true,
        ),
        ("consensus_transition_deadlock_free", Specification, true),
        ("equivocation_fork_defect_witness", Invariant, false),
        (
            "unfair_delivery_liveness_defect_witness",
            Specification,
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
        "consensus-nusmv-micro",
        Path::new(NUSMV_MODEL),
        &expectations,
    );
    if report.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "NuSMV consensus gate was {:?}: {} ({})",
            report.disposition,
            report.diagnostic,
            report.evidence_directory.display()
        )));
    }
    let fsm = report
        .fsm
        .ok_or_else(|| problem("NuSMV consensus gate omitted FSM diagnostics"))?;
    if !fsm.transition_total || !fsm.deadlock_free || report.counterexamples.len() != 2 {
        return Err(problem(format!(
            "NuSMV consensus FSM/controls changed: {fsm:?}, counterexamples={}",
            report.counterexamples.len()
        )));
    }
    Ok(report.results.len())
}

fn check_prolog_consensus(root: &Path) -> Result<usize, ConsensusComparisonError> {
    const FIXTURES: [(&str, usize); 7] = [
        ("devices", 5),
        ("proposals", 8),
        ("votes", 6),
        ("certificates", 4),
        ("recovery", 3),
        ("predecessors", 5),
        ("assumptions", 3),
    ];
    let mut rows = 0;
    let mut all = BTreeSet::new();
    for (goal, expected) in FIXTURES {
        let report = run_prolog_model_fixture(
            root,
            &format!("consensus-{}", goal.replace('_', "-")),
            Path::new(PROLOG_MODEL),
            "poche_consensus",
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
        "proposal(carol_stale,carol_device,denied(stale_epoch),because(epoch1_is_not_epoch2))",
        "vote(event1,alice_browser,alice,duplicate,because(player_vote_already_counted))",
        "certificate(two_player_partition,epoch2,stalled,because(both_players_are_required))",
        "recovery(current_actor_carol,propose(kick_carol),allowed,because(governance_is_out_of_turn))",
        "assumption(non_equivocation,removed,counterexample(two_majorities_intersect_at_equivocating_bob))",
    ] {
        if !all.contains(required) {
            return Err(problem(format!("Prolog omitted required row {required}")));
        }
    }
    Ok(rows)
}

pub(crate) fn consensus_track_evidence() -> Vec<SessionTrackEvidenceWire> {
    use BackendKindWire::{Alloy, NuSmv, RustExplicit, ScryerProlog};
    use ConfidenceKindWire::{Bounded, Queried, Sampled, Symbolic};

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
    let all = CONSENSUS_OBLIGATION_IDS;
    vec![
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: RustExplicit,
            confidence: Sampled,
            qualification: format!(
                "{REPLICATED_MICRO_SCOPE}; exhaustive strict-majority intersection for 2-8 players"
            ),
            claims: claims(&all),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: Alloy,
            confidence: Bounded,
            qualification: ALLOY_SCOPE.to_owned(),
            claims: claims(&[
                all[0], all[1], all[2], all[3], all[4], all[5], all[8], all[9],
            ]),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: NuSmv,
            confidence: Symbolic,
            qualification: NUSMV_SCOPE.to_owned(),
            claims: claims(&[
                all[0], all[1], all[2], all[3], all[4], all[5], all[6], all[8], all[9],
            ]),
        },
        SessionTrackEvidenceWire {
            scope_id: COMPARISON_SCOPE.to_owned(),
            backend: ScryerProlog,
            confidence: Queried,
            qualification: PROLOG_SCOPE.to_owned(),
            claims: claims(&[
                all[0], all[1], all[2], all[3], all[4], all[5], all[6], all[8], all[9],
            ]),
        },
    ]
}

fn claim_rules(claim: &str) -> &'static [&'static str] {
    match claim {
        "P3-C-DEVICE-WEIGHT" | "P3-C-STALE-REVOKED" => &["P3-U2", "P3-U34"],
        "P3-C-QUORUM" | "P3-C-FORK-SAFETY" | "P3-C-TWO-PLAYER-LIMIT" => &["P3-U3", "P3-U14"],
        "P3-C-RECOVERY" => &["P3-U7", "P3-U10"],
        "P3-C-CONDITIONAL-LIVENESS" | "P3-C-SNAPSHOT-TAIL" => &["P3-U7"],
        "P3-C-CANONICAL-BATCH" | "P3-C-LOCAL-PROPOSAL" => &["P3-U11", "P3-U14"],
        _ => &["P3-U14"],
    }
}

fn problem(message: impl Into<String>) -> ConsensusComparisonError {
    ConsensusComparisonError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_consensus_sources_gate_before_neutral_comparison() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = compare_consensus_models(&root).expect("consensus sources should agree");
        assert_eq!(report.source_gates, 4);
        assert_eq!(report.alloy_commands, 9);
        assert_eq!(report.nusmv_properties, 12);
        assert_eq!(report.prolog_rows, 34);
        assert_eq!(report.agreement.tracks, 4);
        assert_eq!(report.agreement.compared_claims, 9);
        assert_eq!(report.agreement.observations, 35);
        assert!(report.agreement.disagreements.is_empty());
    }
}
