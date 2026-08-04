use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use poche_model::{
    PropertyId, RuleOrigin, evaluate_state_property, evaluate_transition_property, property_catalog,
};

use crate::{Counterexample, ExplicitGraph, StateId};

const STATE_PROPERTIES: [PropertyId; 7] = [
    PropertyId::CardConservation,
    PropertyId::LegalActor,
    PropertyId::ObservationConfidentiality,
    PropertyId::TrickCountConservation,
    PropertyId::DeadlockFreedom,
    PropertyId::FinishedAbsorbing,
    PropertyId::FinalWinnerSemantics,
];

const TRANSITION_PROPERTIES: [PropertyId; 7] = [
    PropertyId::FixedBids,
    PropertyId::FollowSuit,
    PropertyId::TrickWinner,
    PropertyId::WinnerLeads,
    PropertyId::ScoringAndPot,
    PropertyId::PhaseProgress,
    PropertyId::FinishedAbsorbing,
];

/// Exact evaluation counts for one catalog property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyMeasurements {
    /// Stable property identity.
    pub property: PropertyId,
    /// Reachable states on which its local obligation was evaluated.
    pub state_evaluations: usize,
    /// Reachable edges on which its transition obligation was evaluated.
    pub transition_evaluations: usize,
}

/// Complete safety/consistency pass over one explicit graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafetyReport {
    /// One entry for every non-liveness catalog property.
    pub properties: Vec<PropertyMeasurements>,
    /// Reachable states considered.
    pub states: usize,
    /// Reachable transitions considered.
    pub transitions: usize,
}

/// First catalog violation in deterministic state/edge/property order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafetyFailure {
    /// Violated claim.
    pub property: PropertyId,
    /// Rule origins retained from the property catalog.
    pub rules: Vec<RuleOrigin>,
    /// Shortest replay prefix through the violating state/edge.
    pub counterexample: Counterexample,
    /// Failure or evaluation detail.
    pub detail: String,
}

impl fmt::Display for SafetyFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "property {:?} failed at depth {}: {}",
            self.property, self.counterexample.depth, self.detail
        )
    }
}

impl Error for SafetyFailure {}

/// Evaluate every reachable-state/transition safety and consistency obligation.
///
/// `UniversalTermination` is intentionally excluded here because its SCC proof
/// is a Phase 5.3 temporal check. `DeadlockFreedom`'s local successor obligation
/// is included; graph-level deadlock reporting is also completed in Phase 5.3.
///
/// # Errors
///
/// Returns the deterministic first failure with a shortest replay prefix and
/// the catalog's rule origins.
///
/// # Panics
///
/// Panics only if an `ExplicitGraph` violates its internal dense-ID/edge
/// invariants; graphs can be constructed only by this crate's checked explorer.
#[allow(
    clippy::too_many_lines,
    reason = "state and edge loops remain adjacent so evaluation counts and first-failure order are auditable"
)]
pub fn check_safety_catalog(graph: &ExplicitGraph) -> Result<SafetyReport, SafetyFailure> {
    let catalog = property_catalog();
    let specs = catalog
        .iter()
        .map(|spec| (spec.id, spec))
        .collect::<BTreeMap<_, _>>();
    let mut counts = BTreeMap::<PropertyId, PropertyMeasurements>::new();

    for (index, state) in graph.states().iter().copied().enumerate() {
        let state_id = StateId(
            u32::try_from(index).expect("explicit graph state count is bounded by StateId"),
        );
        for property in STATE_PROPERTIES {
            let measurement = counts.entry(property).or_insert(PropertyMeasurements {
                property,
                state_evaluations: 0,
                transition_evaluations: 0,
            });
            measurement.state_evaluations += 1;
            let result = evaluate_state_property(property, state);
            match result {
                Ok(true) => {}
                Ok(false) => {
                    return Err(failure(
                        graph,
                        &specs,
                        property,
                        state_id,
                        "reachable-state predicate returned false".to_owned(),
                    ));
                }
                Err(error) => {
                    return Err(failure(
                        graph,
                        &specs,
                        property,
                        state_id,
                        format!("state predicate evaluation failed: {error}"),
                    ));
                }
            }
        }
    }

    for edge in graph.edges().iter().copied() {
        let before = graph
            .state(edge.from)
            .expect("graph edge source identifies a state");
        let transition = before.transition(edge.action).map_err(|error| {
            edge_failure(
                graph,
                &specs,
                PropertyId::PhaseProgress,
                edge,
                format!("replaying graph edge failed: {error}"),
            )
        })?;
        for property in TRANSITION_PROPERTIES {
            let measurement = counts.entry(property).or_insert(PropertyMeasurements {
                property,
                state_evaluations: 0,
                transition_evaluations: 0,
            });
            measurement.transition_evaluations += 1;
            match evaluate_transition_property(property, before, &transition) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(edge_failure(
                        graph,
                        &specs,
                        property,
                        edge,
                        "reachable-transition predicate returned false".to_owned(),
                    ));
                }
                Err(error) => {
                    return Err(edge_failure(
                        graph,
                        &specs,
                        property,
                        edge,
                        format!("transition predicate evaluation failed: {error}"),
                    ));
                }
            }
        }
    }

    let properties = catalog
        .into_iter()
        .filter(|spec| spec.id != PropertyId::UniversalTermination)
        .map(|spec| {
            counts.remove(&spec.id).unwrap_or(PropertyMeasurements {
                property: spec.id,
                state_evaluations: 0,
                transition_evaluations: 0,
            })
        })
        .collect();
    Ok(SafetyReport {
        properties,
        states: graph.states().len(),
        transitions: graph.edges().len(),
    })
}

fn failure(
    graph: &ExplicitGraph,
    specs: &BTreeMap<PropertyId, &poche_model::PropertySpec>,
    property: PropertyId,
    state: StateId,
    detail: String,
) -> SafetyFailure {
    SafetyFailure {
        property,
        rules: specs[&property].rules.clone(),
        counterexample: graph
            .shortest_trace(state)
            .expect("reachable state has a shortest trace"),
        detail,
    }
}

fn edge_failure(
    graph: &ExplicitGraph,
    specs: &BTreeMap<PropertyId, &poche_model::PropertySpec>,
    property: PropertyId,
    edge: crate::Edge,
    detail: String,
) -> SafetyFailure {
    SafetyFailure {
        property,
        rules: specs[&property].rules.clone(),
        counterexample: graph
            .trace_through_edge(edge)
            .expect("reachable edge has a shortest trace"),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TerminationReason, exhaustive_test_graph};

    #[test]
    fn safety_catalog_holds_over_every_reachable_micro_state_and_edge() {
        let graph = exhaustive_test_graph();
        assert_eq!(
            graph.stats().termination,
            TerminationReason::ReachableStateSpaceExhausted
        );
        let report = check_safety_catalog(graph).unwrap();
        assert_eq!(report.properties.len(), 13);
        assert_eq!(report.states, 431_800);
        assert_eq!(report.transitions, 549_896);
        assert!(report.properties.iter().all(|measurement| {
            measurement.state_evaluations != 0 || measurement.transition_evaluations != 0
        }));
        assert!(
            report
                .properties
                .iter()
                .all(|measurement| measurement.property != PropertyId::UniversalTermination)
        );
    }
}
