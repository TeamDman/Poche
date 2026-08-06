// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Independent symbolic `NuSMV` evidence for spatial endpoint transitions.

use std::{error::Error, fmt, path::Path};

use poche_native_tools::{
    NativeDisposition, NuSmvCounterexample, NuSmvPropertyExpectation, NuSmvPropertyKind,
    NuSmvTraceState, run_nusmv_suite,
};

const SUITE_ID: &str = "spatial-nusmv-transition-micro";
const MODEL_PATH: &str = "models/nusmv/spatial.smv";
const SCOPE_ID: &str = "transition-micro-2cards-2viewers-endpoints-transit-pause-recovery";

const TRUE_INVARIANTS: [&str; 4] = [
    "safe_card0_owned_zone",
    "safe_card1_owned_zone",
    "safe_private_face_knowledge",
    "transit_refines_committed_play",
];

const TRUE_TEMPORAL: [&str; 6] = [
    "legal_play_moves_exactly_one_card",
    "paused_committed_state_is_immobile",
    "recovery_committed_state_is_immobile",
    "spatial_transition_deadlock_free",
    "fair_endpoint_convergence",
    "fair_ltl_endpoint_convergence",
];

const FALSE_PROPERTIES: [(&str, NuSmvPropertyKind); 3] = [
    (
        "stuck_animation_refutes_endpoint_convergence",
        NuSmvPropertyKind::Specification,
    ),
    ("cross_owner_defect_witness", NuSmvPropertyKind::Invariant),
    ("privacy_leak_defect_witness", NuSmvPropertyKind::Invariant),
];

/// Successful normalized symbolic receipt for `transition-micro`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialNuSmvReport {
    /// Stable symbolic scope identifier.
    pub scope: &'static str,
    /// Total named native properties.
    pub property_count: usize,
    /// Safety invariants holding in every safe scheduler mode.
    pub safety_invariants: usize,
    /// True CTL/LTL transition and liveness properties.
    pub temporal_properties: usize,
    /// Deliberately false properties with retained counterexamples.
    pub negative_controls: usize,
    /// States in the stuck-animation lasso.
    pub stuck_trace_states: usize,
    /// States in the cross-owner finite counterexample.
    pub cross_owner_trace_states: usize,
    /// States in the private-face leak finite counterexample.
    pub privacy_leak_trace_states: usize,
    /// Ignored evidence directory containing native and normalized evidence.
    pub evidence_directory: String,
}

/// Failure to obtain complete, discriminating `NuSMV` spatial evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialNuSmvError(String);

impl fmt::Display for SpatialNuSmvError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialNuSmvError {}

/// Run the handwritten `NuSMV` transition model and validate its positive
/// properties, named scheduler assumption, and controlled counterexamples.
///
/// # Errors
///
/// Fails closed on missing native output, changed truth values, incomplete FSM
/// diagnostics, or a counterexample that no longer exhibits its named defect.
pub fn check_spatial_nusmv_transition_micro(
    root: &Path,
) -> Result<SpatialNuSmvReport, SpatialNuSmvError> {
    let expectations = expectations();
    let native = run_nusmv_suite(root, SUITE_ID, Path::new(MODEL_PATH), &expectations);
    if native.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "native NuSMV suite was {:?}: {} ({})",
            native.disposition,
            native.diagnostic,
            native.evidence_directory.display()
        )));
    }
    let fsm = native
        .fsm
        .as_ref()
        .ok_or_else(|| problem("NuSMV omitted check_fsm diagnostics"))?;
    if !fsm.transition_total || !fsm.deadlock_free || fsm.deadlock_state.is_some() {
        return Err(problem(format!(
            "transition-micro is not total and deadlock-free: {fsm:?}"
        )));
    }

    let stuck = trace(&native.counterexamples, FALSE_PROPERTIES[0].0)?;
    if stuck.loop_start.is_none()
        || !stuck
            .states
            .iter()
            .any(|state| value(state, "present0") == Some("transit0"))
    {
        return Err(problem("stuck-animation control omitted its transit lasso"));
    }
    let cross_owner = trace(&native.counterexamples, FALSE_PROPERTIES[1].0)?;
    let cross_last = cross_owner
        .states
        .last()
        .ok_or_else(|| problem("cross-owner trace is empty"))?;
    require(cross_last, "mode", "cross_owner_defect")?;
    require(cross_last, "committed1", "play0_zone")?;

    let privacy = trace(&native.counterexamples, FALSE_PROPERTIES[2].0)?;
    let privacy_last = privacy
        .states
        .last()
        .ok_or_else(|| problem("privacy-leak trace is empty"))?;
    require(privacy_last, "mode", "privacy_leak_defect")?;
    require(privacy_last, "committed1", "hand1")?;
    require(privacy_last, "viewer0_knows1", "TRUE")?;

    Ok(SpatialNuSmvReport {
        scope: SCOPE_ID,
        property_count: expectations.len(),
        safety_invariants: TRUE_INVARIANTS.len(),
        temporal_properties: TRUE_TEMPORAL.len(),
        negative_controls: FALSE_PROPERTIES.len(),
        stuck_trace_states: stuck.states.len(),
        cross_owner_trace_states: cross_owner.states.len(),
        privacy_leak_trace_states: privacy.states.len(),
        evidence_directory: native.evidence_directory.display().to_string(),
    })
}

fn expectations() -> Vec<NuSmvPropertyExpectation> {
    let mut expectations = TRUE_INVARIANTS
        .map(|name| NuSmvPropertyExpectation {
            name: name.to_owned(),
            kind: NuSmvPropertyKind::Invariant,
            holds: true,
        })
        .to_vec();
    expectations.extend(TRUE_TEMPORAL.map(|name| NuSmvPropertyExpectation {
        name: name.to_owned(),
        kind: NuSmvPropertyKind::Specification,
        holds: true,
    }));
    expectations.extend(
        FALSE_PROPERTIES.map(|(name, kind)| NuSmvPropertyExpectation {
            name: name.to_owned(),
            kind,
            holds: false,
        }),
    );
    expectations
}

fn trace<'a>(
    traces: &'a [NuSmvCounterexample],
    name: &str,
) -> Result<&'a NuSmvCounterexample, SpatialNuSmvError> {
    traces
        .iter()
        .find(|trace| trace.property_name == name)
        .ok_or_else(|| problem(format!("missing counterexample for {name}")))
}

fn require(state: &NuSmvTraceState, key: &str, expected: &str) -> Result<(), SpatialNuSmvError> {
    match value(state, key) {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(problem(format!(
            "counterexample expected {key}={expected}, found {actual:?}"
        ))),
    }
}

fn value<'a>(state: &'a NuSmvTraceState, key: &str) -> Option<&'a str> {
    state.assignments.get(key).map(String::as_str)
}

fn problem(message: impl Into<String>) -> SpatialNuSmvError {
    SpatialNuSmvError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_nusmv_transition_micro_is_total_and_discriminating() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = check_spatial_nusmv_transition_micro(&root)
            .expect("spatial NuSMV transition scope should pass");
        assert_eq!(report.property_count, 13);
        assert_eq!(report.safety_invariants, 4);
        assert_eq!(report.temporal_properties, 6);
        assert_eq!(report.negative_controls, 3);
        assert!(report.stuck_trace_states >= 2);
        assert_eq!(report.cross_owner_trace_states, 2);
        assert_eq!(report.privacy_leak_trace_states, 2);
    }
}
