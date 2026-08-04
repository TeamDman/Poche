// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Temporal conformance between the exhaustive strict Rust micro graph and a
//! separately handwritten `NuSMV` projection of the same `1,2,1` schedule.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::path::Path;

use poche_check::{
    CheckScope, TerminationReason, analyze_liveness, analyze_with_nonterminal_deadlock,
    analyze_with_nonterminal_stutter, explore, progress_rank,
};
use poche_model::{Game, Player, RoundId, TrickProgress};
use poche_native_tools::{
    NativeDisposition, NuSmvCounterexample, NuSmvPropertyExpectation, NuSmvPropertyKind,
    NuSmvTraceState, run_nusmv_suite,
};

const SUITE_ID: &str = "nusmv-conformance";
const MODEL_PATH: &str = "models/nusmv/conformance.smv";

/// Successful Rust/NuSMV temporal-conformance evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NuSmvConformanceReport {
    /// Named native properties with exact expected truth values.
    pub property_count: usize,
    /// Correct-mode invariants/temporal claims that hold.
    pub correct_properties: usize,
    /// Refined phase/round one-step obligations checked by `NuSMV`.
    pub native_step_obligations: usize,
    /// Distinct coarse phase pairs present in every Rust edge projection.
    pub rust_step_projection_pairs: usize,
    /// Prepared dealer variants compared through native witness traces.
    pub initial_states_compared: usize,
    /// Controlled stutter/deadlock counterexamples paired across engines.
    pub defect_counterexamples_compared: usize,
    /// States in the normalized NuSMV/Rust stutter lasso.
    pub stutter_trace_states: usize,
    /// States in the normalized NuSMV/Rust deadlock prefix.
    pub deadlock_trace_states: usize,
    /// Exhaustive Rust states supporting the comparison.
    pub rust_states: usize,
    /// Exhaustive Rust transitions supporting the comparison.
    pub rust_transitions: usize,
    /// Scope/abstraction qualifications that must accompany the result.
    pub limitations: Vec<&'static str>,
}

/// Native execution, trace parsing, temporal, or observable mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NuSmvConformanceError(String);

impl fmt::Display for NuSmvConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for NuSmvConformanceError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TemporalPhase {
    AwaitingDeal,
    BidFirst,
    BidDealer,
    PlayLead,
    PlayFollow,
    Scoring,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TemporalProjection {
    phase: TemporalPhase,
    dealer: Option<u8>,
    leader: Option<u8>,
    round_index: u8,
    hand_size: u8,
    hand_counts: [u8; 2],
    tricks_played: u8,
    progress_rank: u8,
}

/// Execute the named `NuSMV` conformance suite and compare its initial witnesses,
/// transition obligations, termination result, stutter lasso, and deadlock
/// prefix with the exhaustive Rust micro graph.
///
/// # Errors
///
/// Returns the first missing/malformed native result or unclassified temporal
/// projection difference.
#[allow(
    clippy::too_many_lines,
    reason = "native and explicit-state evidence gates remain visibly adjacent"
)]
pub fn compare_rust_nusmv(root: &Path) -> Result<NuSmvConformanceReport, NuSmvConformanceError> {
    let expectations = property_expectations();
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
        .ok_or_else(|| problem("native NuSMV suite omitted check_fsm diagnostics"))?;
    if fsm.transition_total || fsm.deadlock_free {
        return Err(problem(format!(
            "mixed-mode NuSMV fixture did not expose its controlled deadlock: {fsm:?}"
        )));
    }
    let deadlock_assignment = fsm
        .deadlock_state
        .as_ref()
        .ok_or_else(|| problem("NuSMV check_fsm omitted its deadlock state"))?;
    ensure_mode(deadlock_assignment, "deadlock", "check_fsm deadlock")?;

    let graph = explore(CheckScope::Micro)
        .map_err(|error| problem(format!("Rust micro exploration failed: {error}")))?;
    let stats = graph.stats();
    if stats.termination != TerminationReason::ReachableStateSpaceExhausted {
        return Err(problem("Rust micro exploration was not exhaustive"));
    }
    let liveness = analyze_liveness(&graph)
        .map_err(|error| problem(format!("Rust liveness analysis failed: {error}")))?;
    if !liveness.universal_termination
        || !liveness.nonterminal_deadlocks.is_empty()
        || liveness.nonterminal_cyclic_components != 0
        || !liveness.progress_violations.is_empty()
        || liveness.maximum_progress_rank != 20
    {
        return Err(problem(format!(
            "Rust correct-mode liveness obligations failed: {liveness:?}"
        )));
    }

    compare_initial_witness(
        &graph,
        &native.counterexamples,
        "initial_dealer_p0_witness",
        0,
    )?;
    compare_initial_witness(
        &graph,
        &native.counterexamples,
        "initial_dealer_p1_witness",
        1,
    )?;
    let rust_pairs = compare_rust_step_projection(&graph)?;
    let stutter_states = compare_stutter_counterexample(&graph, &native.counterexamples)?;
    let deadlock_states =
        compare_deadlock_counterexample(&graph, &native.counterexamples, deadlock_assignment)?;

    Ok(NuSmvConformanceReport {
        property_count: expectations.len(),
        correct_properties: expectations.iter().filter(|item| item.holds).count(),
        native_step_obligations: expectations
            .iter()
            .filter(|item| item.name.starts_with("step_") || item.name == "finished_absorbing")
            .count(),
        rust_step_projection_pairs: rust_pairs,
        initial_states_compared: 2,
        defect_counterexamples_compared: 2,
        stutter_trace_states: stutter_states,
        deadlock_trace_states: deadlock_states,
        rust_states: stats.states,
        rust_transitions: stats.transitions,
        limitations: vec![
            "The conformance model is the exact two-player, six-card Rust micro schedule 1,2,1; the independent full NuSMV oracle separately checks the 52-card-count 1..7..1 abstraction.",
            "NuSMV projects card zones to counts and temporal phases; Rust retains card identity, legal actions, scoring, and observations in its exhaustive graph.",
            "The mixed native fixture intentionally contains stutter and deadlock modes, so check_fsm reports the injected global deadlock; correct-mode totality and termination are isolated by named conditional properties.",
        ],
    })
}

fn property_expectations() -> Vec<NuSmvPropertyExpectation> {
    use NuSmvPropertyKind::{Invariant, Specification};
    [
        ("step_deal_round0", Specification, true),
        ("step_deal_round1", Specification, true),
        ("step_deal_round2", Specification, true),
        ("step_bid_first", Specification, true),
        ("step_bid_dealer", Specification, true),
        ("step_play_lead", Specification, true),
        ("step_final_follow", Specification, true),
        ("step_nonfinal_follow", Specification, true),
        ("step_score_round0", Specification, true),
        ("step_score_round1", Specification, true),
        ("step_score_round2", Specification, true),
        ("finished_absorbing", Specification, true),
        ("correct_deadlock_free", Specification, true),
        ("correct_af_terminates", Specification, true),
        ("correct_finished_reachable", Specification, true),
        ("initial_dealer_p0_witness", Specification, false),
        ("initial_dealer_p1_witness", Specification, false),
        ("stutter_refutes_termination", Specification, false),
        ("correct_ltl_terminates", Specification, true),
        ("correct_progress_bounds", Invariant, true),
        ("correct_card_count_bounds", Invariant, true),
        ("correct_finished_shape", Invariant, true),
        ("deadlock_defect_reachable", Invariant, false),
    ]
    .into_iter()
    .map(|(name, kind, holds)| NuSmvPropertyExpectation {
        name: name.to_owned(),
        kind,
        holds,
    })
    .collect()
}

fn compare_initial_witness(
    graph: &poche_check::ExplicitGraph,
    traces: &[NuSmvCounterexample],
    name: &str,
    dealer: usize,
) -> Result<(), NuSmvConformanceError> {
    let trace = trace_by_name(traces, name)?;
    if trace.states.len() != 1 || trace.loop_start.is_some() {
        return Err(problem(format!(
            "{name} was not a one-state finite witness: {trace:?}"
        )));
    }
    ensure_trace_mode(trace, "correct")?;
    let native = native_projection(&trace.states[0])?;
    let rust_game = Game::new(player(dealer)?);
    let rust = rust_projection(rust_game)?;
    ensure_equal(name, &rust, &native)?;
    if !graph
        .initial_states()
        .iter()
        .filter_map(|id| graph.state(*id))
        .any(|game| game == rust_game)
    {
        return Err(problem(format!(
            "Rust exhaustive graph omitted prepared dealer {dealer}"
        )));
    }
    Ok(())
}

fn compare_rust_step_projection(
    graph: &poche_check::ExplicitGraph,
) -> Result<usize, NuSmvConformanceError> {
    let actual = graph
        .edges()
        .iter()
        .map(|edge| {
            let from = graph
                .state(edge.from)
                .ok_or_else(|| problem("Rust edge source is absent"))?;
            let to = graph
                .state(edge.to)
                .ok_or_else(|| problem("Rust edge target is absent"))?;
            Ok((rust_projection(from)?.phase, rust_projection(to)?.phase))
        })
        .collect::<Result<BTreeSet<_>, NuSmvConformanceError>>()?;
    let expected = BTreeSet::from([
        (TemporalPhase::AwaitingDeal, TemporalPhase::BidFirst),
        (TemporalPhase::BidFirst, TemporalPhase::BidDealer),
        (TemporalPhase::BidDealer, TemporalPhase::PlayLead),
        (TemporalPhase::PlayLead, TemporalPhase::PlayFollow),
        (TemporalPhase::PlayFollow, TemporalPhase::PlayLead),
        (TemporalPhase::PlayFollow, TemporalPhase::Scoring),
        (TemporalPhase::Scoring, TemporalPhase::AwaitingDeal),
        (TemporalPhase::Scoring, TemporalPhase::Finished),
        (TemporalPhase::Finished, TemporalPhase::Finished),
    ]);
    if actual != expected {
        return Err(problem(format!(
            "Rust coarse one-step projection differs: expected={expected:?}, actual={actual:?}"
        )));
    }
    for edge in graph.edges() {
        let from = graph
            .state(edge.from)
            .ok_or_else(|| problem("Rust progress edge source is absent"))?;
        let to = graph
            .state(edge.to)
            .ok_or_else(|| problem("Rust progress edge target is absent"))?;
        let valid = if matches!(from, Game::Finished(_)) {
            progress_rank(from) == 0 && progress_rank(to) == 0
        } else {
            progress_rank(from) == progress_rank(to) + 1
        };
        if !valid {
            return Err(problem(format!(
                "Rust edge does not obey the NuSMV progress projection: {edge:?}"
            )));
        }
    }
    Ok(actual.len())
}

fn compare_stutter_counterexample(
    graph: &poche_check::ExplicitGraph,
    traces: &[NuSmvCounterexample],
) -> Result<usize, NuSmvConformanceError> {
    let trace = trace_by_name(traces, "stutter_refutes_termination")?;
    ensure_trace_mode(trace, "stutter")?;
    if trace.loop_start != Some(0) || trace.states.len() != 2 {
        return Err(problem(format!(
            "NuSMV stutter trace is not an initial self-loop: {trace:?}"
        )));
    }
    let native = trace
        .states
        .iter()
        .map(native_projection)
        .collect::<Result<Vec<_>, _>>()?;
    let dealer = native[0]
        .dealer
        .ok_or_else(|| problem("NuSMV stutter state omitted dealer"))?;
    let initial = graph
        .initial_states()
        .iter()
        .copied()
        .find(|id| {
            graph
                .state(*id)
                .and_then(|game| rust_projection(game).ok())
                .is_some_and(|projection| projection.dealer == Some(dealer))
        })
        .ok_or_else(|| problem(format!("Rust omitted initial dealer {dealer}")))?;
    let report = analyze_with_nonterminal_stutter(graph, initial)
        .map_err(|error| problem(format!("Rust stutter analysis failed: {error}")))?;
    let lasso = report
        .lasso
        .as_ref()
        .ok_or_else(|| problem("Rust stutter mutation did not produce a lasso"))?;
    let rust = lasso
        .cycle_states
        .iter()
        .map(|id| {
            graph
                .state(*id)
                .ok_or_else(|| problem("Rust lasso state is absent"))
                .and_then(rust_projection)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ensure_equal("stutter counterexample projection", &rust, &native)?;
    if report.universal_termination || report.nonterminal_cyclic_components != 1 {
        return Err(problem(format!(
            "Rust stutter mutation was not discriminating: {report:?}"
        )));
    }
    Ok(trace.states.len())
}

fn compare_deadlock_counterexample(
    graph: &poche_check::ExplicitGraph,
    traces: &[NuSmvCounterexample],
    fsm_state: &std::collections::BTreeMap<String, String>,
) -> Result<usize, NuSmvConformanceError> {
    let trace = trace_by_name(traces, "deadlock_defect_reachable")?;
    ensure_trace_mode(trace, "deadlock")?;
    if trace.loop_start.is_some() || trace.states.len() != 5 {
        return Err(problem(format!(
            "NuSMV deadlock trace is not the expected finite prefix: {trace:?}"
        )));
    }
    let native = trace
        .states
        .iter()
        .map(native_projection)
        .collect::<Result<Vec<_>, _>>()?;
    let native_target = *native
        .last()
        .ok_or_else(|| problem("NuSMV deadlock trace is empty"))?;
    let fsm_target = native_projection_from_assignments(fsm_state)?;
    ensure_equal(
        "NuSMV check_fsm/trace deadlock",
        &native_target,
        &fsm_target,
    )?;
    let target = graph
        .edges()
        .iter()
        .map(|edge| edge.from)
        .find(|id| {
            graph.state(*id).and_then(|game| rust_projection(game).ok()) == Some(native_target)
        })
        .ok_or_else(|| {
            problem(format!(
                "Rust graph omitted deadlock projection {native_target:?}"
            ))
        })?;
    let rust_trace = graph
        .shortest_trace(target)
        .ok_or_else(|| problem("Rust deadlock target had no shortest trace"))?;
    let rust = rust_trace
        .states
        .into_iter()
        .map(rust_projection)
        .collect::<Result<Vec<_>, _>>()?;
    ensure_equal("deadlock counterexample projection", &rust, &native)?;
    let report = analyze_with_nonterminal_deadlock(graph, target)
        .map_err(|error| problem(format!("Rust deadlock analysis failed: {error}")))?;
    if report.universal_termination
        || report.nonterminal_deadlocks != vec![target]
        || report.lasso.is_some()
    {
        return Err(problem(format!(
            "Rust deadlock mutation was not discriminating: {report:?}"
        )));
    }
    Ok(trace.states.len())
}

fn rust_projection(game: Game) -> Result<TemporalProjection, NuSmvConformanceError> {
    let (phase, ledger, leader, hand_counts, tricks_played) = match game {
        Game::AwaitingDeal(state) => (
            TemporalPhase::AwaitingDeal,
            Some(state.ledger()),
            None,
            [0, 0],
            0,
        ),
        Game::Bidding(state) => (
            if state.bids().iter().all(Option::is_none) {
                TemporalPhase::BidFirst
            } else {
                TemporalPhase::BidDealer
            },
            Some(state.ledger()),
            None,
            [
                state.hand(Player::Zero).len(),
                state.hand(Player::One).len(),
            ],
            0,
        ),
        Game::Playing(state) => {
            let phase = match state.trick() {
                TrickProgress::Lead(_) => TemporalPhase::PlayLead,
                TrickProgress::Follow(_) => TemporalPhase::PlayFollow,
            };
            let won = state.tricks_won();
            (
                phase,
                Some(state.ledger()),
                Some(player_number(state.leader())?),
                [
                    state.hand(Player::Zero).len(),
                    state.hand(Player::One).len(),
                ],
                won[0].get() + won[1].get(),
            )
        }
        Game::Scoring(state) => {
            let won = state.tricks_won();
            (
                TemporalPhase::Scoring,
                Some(state.ledger()),
                None,
                [0, 0],
                won[0].get() + won[1].get(),
            )
        }
        Game::Finished(_) => (TemporalPhase::Finished, None, None, [0, 0], 0),
    };
    let (dealer, round_index, hand_size) = if let Some(ledger) = ledger {
        (
            Some(player_number(ledger.dealer())?),
            round_number(ledger.round()),
            ledger.round().hand_size(),
        )
    } else {
        (None, 2, 1)
    };
    Ok(TemporalProjection {
        phase,
        dealer,
        leader,
        round_index,
        hand_size,
        hand_counts,
        tricks_played,
        progress_rank: progress_rank(game),
    })
}

fn native_projection(state: &NuSmvTraceState) -> Result<TemporalProjection, NuSmvConformanceError> {
    native_projection_from_assignments(&state.assignments)
}

fn native_projection_from_assignments(
    values: &std::collections::BTreeMap<String, String>,
) -> Result<TemporalProjection, NuSmvConformanceError> {
    let phase = match required(values, "phase")? {
        "awaiting_deal" => TemporalPhase::AwaitingDeal,
        "bid_first" => TemporalPhase::BidFirst,
        "bid_dealer" => TemporalPhase::BidDealer,
        "play_lead" => TemporalPhase::PlayLead,
        "play_follow" => TemporalPhase::PlayFollow,
        "scoring" => TemporalPhase::Scoring,
        "finished" => TemporalPhase::Finished,
        other => return Err(problem(format!("unknown NuSMV phase: {other}"))),
    };
    let dealer = Some(parse_native_player(required(values, "dealer")?)?);
    let leader = matches!(phase, TemporalPhase::PlayLead | TemporalPhase::PlayFollow)
        .then(|| parse_native_player(required(values, "leader")?))
        .transpose()?;
    let round_index = parse_u8(required(values, "round_index")?, "round_index")?;
    let hand_size = parse_u8(required(values, "hand_size")?, "hand_size")?;
    let hand_counts = [
        parse_u8(required(values, "hand0_count")?, "hand0_count")?,
        parse_u8(required(values, "hand1_count")?, "hand1_count")?,
    ];
    let tricks_played = parse_u8(required(values, "tricks_played")?, "tricks_played")?;
    let progress_rank = values.get("progress_rank").map_or_else(
        || native_progress_rank(phase, round_index, hand_size, hand_counts),
        |value| parse_u8(value, "progress_rank"),
    )?;
    Ok(TemporalProjection {
        phase,
        dealer,
        leader,
        round_index,
        hand_size,
        hand_counts,
        tricks_played,
        progress_rank,
    })
}

fn native_progress_rank(
    phase: TemporalPhase,
    round_index: u8,
    hand_size: u8,
    hand_counts: [u8; 2],
) -> Result<u8, NuSmvConformanceError> {
    let future = match round_index {
        0 => 14,
        1 => 6,
        2 => 0,
        other => {
            return Err(problem(format!(
                "NuSMV round index is out of range: {other}"
            )));
        }
    };
    Ok(match phase {
        TemporalPhase::AwaitingDeal => future + 4 + 2 * hand_size,
        TemporalPhase::BidFirst => future + 3 + 2 * hand_size,
        TemporalPhase::BidDealer => future + 2 + 2 * hand_size,
        TemporalPhase::PlayLead | TemporalPhase::PlayFollow => {
            future + hand_counts[0] + hand_counts[1] + 1
        }
        TemporalPhase::Scoring => future + 1,
        TemporalPhase::Finished => 0,
    })
}

fn trace_by_name<'a>(
    traces: &'a [NuSmvCounterexample],
    name: &str,
) -> Result<&'a NuSmvCounterexample, NuSmvConformanceError> {
    let matches = traces
        .iter()
        .filter(|trace| trace.property_name == name)
        .collect::<Vec<_>>();
    if let [trace] = matches.as_slice() {
        Ok(trace)
    } else {
        Err(problem(format!(
            "expected one NuSMV counterexample for {name}, found {}",
            matches.len()
        )))
    }
}

fn ensure_trace_mode(trace: &NuSmvCounterexample, mode: &str) -> Result<(), NuSmvConformanceError> {
    for state in &trace.states {
        ensure_mode(&state.assignments, mode, &trace.property_name)?;
    }
    Ok(())
}

fn ensure_mode(
    values: &std::collections::BTreeMap<String, String>,
    mode: &str,
    context: &str,
) -> Result<(), NuSmvConformanceError> {
    if required(values, "mode")? == mode {
        Ok(())
    } else {
        Err(problem(format!("{context} did not remain in mode {mode}")))
    }
}

fn required<'a>(
    values: &'a std::collections::BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, NuSmvConformanceError> {
    values
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| problem(format!("NuSMV state omitted {name}")))
}

fn parse_native_player(value: &str) -> Result<u8, NuSmvConformanceError> {
    match value {
        "p0" => Ok(0),
        "p1" => Ok(1),
        other => Err(problem(format!("unknown NuSMV player: {other}"))),
    }
}

fn parse_u8(value: &str, name: &str) -> Result<u8, NuSmvConformanceError> {
    value
        .parse()
        .map_err(|error| problem(format!("invalid NuSMV {name} {value:?}: {error}")))
}

const fn round_number(round: RoundId) -> u8 {
    match round {
        RoundId::OneAscending => 0,
        RoundId::Two => 1,
        RoundId::OneDescending => 2,
    }
}

fn player(index: usize) -> Result<Player, NuSmvConformanceError> {
    Player::ALL
        .get(index)
        .copied()
        .ok_or_else(|| problem(format!("invalid two-player index: {index}")))
}

fn player_number(player: Player) -> Result<u8, NuSmvConformanceError> {
    u8::try_from(player.index())
        .map_err(|error| problem(format!("player index does not fit u8: {error}")))
}

fn ensure_equal<T: fmt::Debug + PartialEq>(
    context: &str,
    expected: &T,
    actual: &T,
) -> Result<(), NuSmvConformanceError> {
    if expected == actual {
        Ok(())
    } else {
        Err(problem(format!(
            "{context} differs: expected={expected:?}, actual={actual:?}"
        )))
    }
}

fn problem(message: impl Into<String>) -> NuSmvConformanceError {
    NuSmvConformanceError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nusmv_temporal_and_counterexample_agreement() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = compare_rust_nusmv(&root).expect("Rust and NuSMV should agree");
        assert_eq!(report.property_count, 23);
        assert_eq!(report.correct_properties, 19);
        assert_eq!(report.native_step_obligations, 12);
        assert_eq!(report.rust_step_projection_pairs, 9);
        assert_eq!(report.initial_states_compared, 2);
        assert_eq!(report.defect_counterexamples_compared, 2);
        assert_eq!(report.stutter_trace_states, 2);
        assert_eq!(report.deadlock_trace_states, 5);
    }
}
