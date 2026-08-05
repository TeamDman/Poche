// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode, Output};

use poche_check::{CheckScope, TerminationReason, analyze_liveness, check_session, explore};
use poche_conformance::{
    Disposition, compare_rust_alloy, compare_rust_models, compare_rust_nusmv, compare_rust_prolog,
};
use poche_native_tools::{
    AlloyCommandExpectation, AlloyCommandKind, AlloyOutcome, NativeBackend, NativeDisposition,
    NuSmvPropertyExpectation, NuSmvPropertyKind, run_all, run_alloy_suite, run_backend,
    run_nusmv_suite, run_prolog_model_fixture,
};
use poche_oracle_rust::{Action, DeckOrder, Game, GameState, Seat, Turn};

const COVERAGE_TRACKS: [(&str, usize); 4] =
    [("rust", 3), ("alloy", 4), ("nusmv", 5), ("prolog", 6)];

const PLAN_AUDIT_CONTRACTS: &[&str] = &[
    include_str!("../../../tools/plan-audit/poche-foundation.conf"),
    include_str!("../../../tools/plan-audit/poche-phase-2.conf"),
];
const REQUIRED_TRIPLE_AUDIT_MARKERS: &[&str] = &[
    "**Pass 1 — extraction:**",
    "**Pass 2 — traceability:**",
    "**Pass 3 — adversarial omission:**",
];
const REQUIRED_READY_SENTENCE: &str = "The plan is only ready once we have literally triple checked that no intent from the user has been omitted without explicit direction from the user.";
const READY_STATUS: &str = "Ready for execution";
const IN_PROGRESS_STATUS: &str = "Execution in progress";
const COMPLETE_STATUS: &str = "Execution complete";

#[derive(Debug)]
struct AuditProfile {
    plan_id: String,
    title: String,
    plan_id_required: bool,
    allowed_statuses: Vec<String>,
    guidance_ids: Vec<u32>,
    gate_section: String,
    gate_ids: Vec<u32>,
    gate_cells: usize,
    working_gate_statuses: Vec<String>,
    complete_gate_statuses: Vec<String>,
    task_ids: Vec<String>,
    require_task_criteria: bool,
    overall_criteria: usize,
    deferred_sections: Vec<String>,
    adversarial_markers: Vec<String>,
    forbid_superseded: bool,
}

#[derive(Debug)]
struct GuidanceAuditReport {
    plan_id: String,
    status: String,
    guidance: usize,
    traceability: usize,
    gates: usize,
    tasks: usize,
    overall_criteria: usize,
    deferred_sections: usize,
}

struct Tool<'a> {
    label: &'a str,
    override_var: Option<&'a str>,
    commands: &'a [&'a str],
    version_args: &'a [&'a str],
    accept_nonzero_with: Option<&'a str>,
}

enum Probe {
    Available { command: OsString, version: String },
    Missing,
    Failed { command: OsString, detail: String },
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let _program = args.next();

    match args.next().as_deref() {
        Some(command) if command == OsStr::new("doctor") => doctor(),
        Some(command) if command == OsStr::new("guidance") => guidance(args),
        Some(command) if command == OsStr::new("protocol") => protocol(args),
        Some(command) if command == OsStr::new("session") => session(args),
        Some(command) if command == OsStr::new("coverage") => coverage(args),
        Some(command) if command == OsStr::new("oracle") => oracle(args),
        Some(command) if command == OsStr::new("compare") => compare(args),
        Some(command) if command == OsStr::new("acceptance") => acceptance(args),
        Some(command) if command == OsStr::new("check") => check(args),
        Some(command) => {
            eprintln!("unknown poche-xtask command: {}", command.to_string_lossy());
            usage();
            ExitCode::from(2)
        }
        None => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  cargo run -p poche-xtask -- doctor\n  \
         cargo run -p poche-xtask -- guidance audit PLAN.md\n  \
         cargo run -p poche-xtask -- protocol replay --all\n  \
         cargo run -p poche-xtask -- session oracle check rust|alloy|nusmv|prolog|all\n  \
         cargo run -p poche-xtask -- coverage audit [--all | --track TRACK]\n  \
         cargo run -p poche-xtask -- oracle check rust|alloy|nusmv|prolog|all\n  \
         cargo run -p poche-xtask -- oracle report\n  \
         cargo run -p poche-xtask -- compare rust-oracle rust-formal\n  \
         cargo run -p poche-xtask -- compare rust prolog --fixtures PATH\n  \
         cargo run -p poche-xtask -- compare rust alloy --scope micro\n  \
         cargo run -p poche-xtask -- compare rust nusmv --scope micro\n  \
         cargo run -p poche-xtask -- compare all --scope micro\n  \
         cargo run -p poche-xtask -- acceptance hashes\n  \
         cargo run -p poche-xtask -- check rust-explicit --scope micro\n  \
         cargo run -p poche-xtask -- check rust-explicit --property game-terminates"
    );
}

fn session(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let group = args.next();
    let action = args.next();
    let backend = args.next();
    if group.as_deref() != Some(OsStr::new("oracle"))
        || action.as_deref() != Some(OsStr::new("check"))
        || args.next().is_some()
    {
        usage();
        return ExitCode::from(2);
    }
    match backend.as_deref() {
        Some(value) if value == OsStr::new("rust") => session_rust(),
        Some(value) if value == OsStr::new("alloy") => session_alloy(),
        Some(value) if value == OsStr::new("nusmv") => session_nusmv(),
        Some(value) if value == OsStr::new("prolog") => session_prolog(),
        Some(value) if value == OsStr::new("all") => {
            let results = [
                session_rust(),
                session_alloy(),
                session_nusmv(),
                session_prolog(),
            ];
            if results
                .into_iter()
                .all(|result| result == ExitCode::SUCCESS)
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn session_rust() -> ExitCode {
    let report = check_session();
    println!(
        "session Rust: scope={} states={} transitions={} accepted={} denied={} terminal={} max_depth={} safety={} semantic_hash={}",
        report.scope,
        report.stats.states,
        report.stats.transitions,
        report.stats.accepted_transitions,
        report.stats.denied_transitions,
        report.stats.terminal_states,
        report.stats.maximum_depth,
        report.safety_properties,
        report.semantic_hash
    );
    for witness in &report.false_invariants {
        println!(
            "  expected-false {} minimal_actions={} trace={:?}",
            witness.name,
            witness.actions.len(),
            witness.actions
        );
    }
    println!(
        "  liveness unconditional=false; conditional_assumptions={}",
        report.liveness_assumptions.join(",")
    );
    let false_claims = report
        .false_invariants
        .iter()
        .map(|witness| (witness.name, witness.actions.len()))
        .collect::<Vec<_>>();
    let pinned = report.stats.states == 800
        && report.stats.transitions == 38_400
        && report.stats.accepted_transitions == 5_872
        && report.stats.denied_transitions == 32_528
        && report.stats.terminal_states == 272
        && report.stats.maximum_depth == 14
        && report.safety_properties == 10
        && report.semantic_hash
            == "89c626a11bcef07d93007ba5a7bf097fa9dd88c94244775dec5c138a17e023b1"
        && false_claims
            == [
                ("all-reachable-states-are-terminal", 0),
                ("pause-is-unreachable", 5),
                ("spectator-never-sees-a-hand", 2),
            ];
    if pinned
        && !report.unconditional_termination
        && report.every_state_has_terminal_path
        && report.false_invariants.len() == 3
    {
        ExitCode::SUCCESS
    } else {
        eprintln!("session Rust fixed-point counts/hash differ from the pinned scope");
        ExitCode::FAILURE
    }
}

fn session_alloy() -> ExitCode {
    use AlloyCommandKind::{Assertion, Witness};
    use AlloyOutcome::{Sat, Unsat};

    let assertions = [
        "DefaultDeny",
        "SingleSeatOwnership",
        "NoStartWithoutReadiness",
        "AtMostOnceStart",
        "ScopedSpectatorGrant",
        "RevocationStopsFutureKnowledge",
        "NoUnauthorizedKnowledge",
        "PauseResumePreserveRoom",
    ];
    let witnesses = [
        "ValidRoomWitness",
        "PauseResumeWitness",
        "DefectiveDuplicateSeatWitness",
        "DefectiveStartUnreadyWitness",
        "DefectiveRoomWideHandWitness",
    ];
    let mut expectations: Vec<_> = assertions
        .into_iter()
        .map(|name| AlloyCommandExpectation {
            name: name.to_owned(),
            kind: Assertion,
            outcome: Unsat,
        })
        .collect();
    expectations.extend(witnesses.into_iter().map(|name| AlloyCommandExpectation {
        name: name.to_owned(),
        kind: Witness,
        outcome: Sat,
    }));
    let report = run_alloy_suite(
        Path::new("."),
        "session-alloy",
        Path::new("models/alloy/session.als"),
        &expectations,
    );
    println!(
        "session Alloy: {:?}; {}",
        report.disposition, report.diagnostic
    );
    for result in &report.results {
        println!(
            "  {} {:?} {:?} scope={}",
            result.name,
            result.kind,
            result.outcome,
            result.command_source.as_deref().unwrap_or("<missing>")
        );
    }
    if report.succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn session_nusmv() -> ExitCode {
    use NuSmvPropertyKind::{Invariant, Specification};

    let expectations = [
        ("countdown_is_ready", Invariant, true),
        ("started_phase_has_one_start", Invariant, true),
        ("durable_membership_survives_route_loss", Invariant, true),
        ("closed_is_absorbing", Specification, true),
        ("abort_wins_before_later_expiry", Specification, true),
        ("stale_expiry_cannot_start", Specification, true),
        ("paused_game_action_does_not_advance", Specification, true),
        ("pause_and_resume_are_player_gated", Specification, true),
        ("session_deadlock_free", Specification, true),
        ("unconditional_session_termination", Specification, false),
        ("conditional_session_termination", Specification, true),
        ("conditional_ltl_session_termination", Specification, true),
        ("missing_readiness_refutes_liveness", Specification, false),
        ("missing_expiry_refutes_liveness", Specification, false),
        ("missing_action_refutes_liveness", Specification, false),
        ("missing_resume_refutes_liveness", Specification, false),
    ]
    .into_iter()
    .map(|(name, kind, holds)| NuSmvPropertyExpectation {
        name: name.to_owned(),
        kind,
        holds,
    })
    .collect::<Vec<_>>();
    let report = run_nusmv_suite(
        Path::new("."),
        "session-nusmv",
        Path::new("models/nusmv/session.smv"),
        &expectations,
    );
    println!(
        "session NuSMV: {:?}; {}",
        report.disposition, report.diagnostic
    );
    for result in &report.results {
        println!(
            "  {} {:?} holds={}",
            result.name.as_deref().unwrap_or("<unnamed>"),
            result.kind,
            result.holds
        );
    }
    if let Some(fsm) = &report.fsm {
        println!(
            "  FSM transition_total={} deadlock_free={}",
            fsm.transition_total, fsm.deadlock_free
        );
    }
    let expected_counterexample_modes = [
        ("missing_readiness_refutes_liveness", "missing_readiness"),
        ("missing_expiry_refutes_liveness", "missing_expiry"),
        ("missing_action_refutes_liveness", "missing_action"),
        ("missing_resume_refutes_liveness", "missing_resume"),
    ];
    let traces_match = expected_counterexample_modes.iter().all(|(name, mode)| {
        report.counterexamples.iter().any(|trace| {
            trace.property_name == *name
                && trace.loop_start.is_some()
                && trace.states.iter().any(|state| {
                    state.assignments.get("mode").map(String::as_str) == Some(*mode)
                        && state.assignments.get("terminal").map(String::as_str) == Some("FALSE")
                })
        })
    }) && report.counterexamples.iter().any(|trace| {
        trace.property_name == "unconditional_session_termination"
            && trace.loop_start.is_some()
            && trace
                .states
                .iter()
                .any(|state| state.assignments.get("terminal").map(String::as_str) == Some("FALSE"))
    });
    if !traces_match {
        eprintln!("session NuSMV counterexamples did not match their named omission modes");
    }
    if report.succeeded() && traces_match {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn session_prolog() -> ExitCode {
    let fixtures = [
        (
            "session-policy",
            "policy_decisions",
            11_usize,
            "bc866fb812a92afa20d26f197a356bc86307b1479af6d836624117b6402e04f6",
        ),
        (
            "session-successors",
            "successors",
            13,
            "6d2fdd3044e24e09d532d3faf37652fd668fdf5d2f205f2e870413259d6372cb",
        ),
        (
            "session-predecessors",
            "predecessors",
            12,
            "fce25d53cce3449006108f711a5c28957363064699b57aec87dc519956fb9387",
        ),
        (
            "session-visibility",
            "visibility",
            16,
            "8ebef323c2929242b53ddb774569c67e6facd4f1b232b70c5eb6c3e7ba8e72cc",
        ),
        (
            "session-grants",
            "grant_chains",
            4,
            "98d2652614fc505cbfab64245a6f35325effbeabadfb04b6f3780a9689c1b00d",
        ),
        (
            "session-histories",
            "history_causes",
            3,
            "a35c7c6a3428854928cc3eaa02f59014a2f2e7212231da4913f9a456c86244b5",
        ),
        (
            "session-defects",
            "controlled_defects",
            4,
            "7883641e7f9687f4765beae11ed34ca4b6b2e3b1db0d51d7e504acfec721b7e9",
        ),
    ];
    let mut succeeded = true;
    for (fixture_id, goal, expected_count, expected_hash) in fixtures {
        let report = run_prolog_model_fixture(
            Path::new("."),
            fixture_id,
            Path::new("models/prolog/session.pl"),
            "poche_session",
            goal,
        );
        let mut canonical = String::new();
        for answer in &report.answers {
            canonical.push_str(answer);
            canonical.push('\n');
        }
        let hash = blake3::hash(canonical.as_bytes()).to_hex().to_string();
        println!(
            "session Prolog {goal}: {:?}; answers={} blake3={hash}; {}",
            report.disposition,
            report.answers.len(),
            report.diagnostic
        );
        succeeded &= report.disposition == NativeDisposition::Success
            && report.answers.len() == expected_count
            && hash == expected_hash;
    }
    if succeeded {
        ExitCode::SUCCESS
    } else {
        eprintln!("session Prolog answer-set count or digest differs from the pinned corpus");
        ExitCode::FAILURE
    }
}

fn protocol(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let subcommand = args.next();
    let mode = args.next();
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }
    if subcommand.as_deref() == Some(OsStr::new("render-builtin")) && mode.is_none() {
        return match poche_runtime::render_builtin_transcript() {
            Ok(transcript) => {
                println!("{transcript}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("protocol transcript render failed: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if subcommand.as_deref() != Some(OsStr::new("replay"))
        || mode.as_deref() != Some(OsStr::new("--all"))
    {
        usage();
        return ExitCode::from(2);
    }
    let fixture_root = Path::new("tests/fixtures/protocol");
    let entries = match fs::read_dir(fixture_root) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!(
                "protocol replay could not list {}: {error}",
                fixture_root.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new("json")))
        .collect();
    paths.sort();
    if paths.is_empty() {
        eprintln!("protocol replay found no JSON fixtures");
        return ExitCode::FAILURE;
    }
    for path in &paths {
        let fixture = match fs::read_to_string(path) {
            Ok(fixture) => fixture,
            Err(error) => {
                eprintln!("protocol replay could not read {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let report = match poche_runtime::verify_transcript_codec_parity(&fixture) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("protocol replay failed for {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        println!(
            "protocol replay: passed; codecs=typed,canonical-ndjson fixture={} steps={} final-state={}",
            report.fixture_id, report.steps, report.final_state_hash
        );
    }
    println!("protocol replay all: passed; fixtures={}", paths.len());
    ExitCode::SUCCESS
}

fn check(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let backend = args.next();
    let mode = args.next();
    let value = args.next();
    if backend.as_deref() != Some(OsStr::new("rust-explicit")) || args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }
    let graph = match explore(CheckScope::Micro) {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("explicit Rust exploration failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stats = graph.stats();
    println!("scope: {}", graph.scope().id());
    println!(
        "states={} transitions={} duplicate-state-hits={} max-depth={}",
        stats.states, stats.transitions, stats.duplicate_state_hits, stats.maximum_depth
    );
    println!(
        "initial-states={} finished-states={} termination={:?}",
        stats.initial_states, stats.finished_states, stats.termination
    );
    if stats.termination != TerminationReason::ReachableStateSpaceExhausted {
        eprintln!("explicit Rust exploration was not exhaustive");
        return ExitCode::FAILURE;
    }
    match (mode.as_deref(), value.as_deref()) {
        (Some(flag), Some(scope))
            if flag == OsStr::new("--scope") && scope == OsStr::new("micro") =>
        {
            println!("explicit Rust exploration: passed");
            ExitCode::SUCCESS
        }
        (Some(flag), Some(property))
            if flag == OsStr::new("--property") && property == OsStr::new("game-terminates") =>
        {
            let report = match analyze_liveness(&graph) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("liveness analysis failed: {error}");
                    return ExitCode::FAILURE;
                }
            };
            println!(
                "sccs={} cyclic={} nonterminal-cyclic={} deadlocks={}",
                report.strongly_connected_components,
                report.cyclic_components,
                report.nonterminal_cyclic_components,
                report.nonterminal_deadlocks.len()
            );
            println!(
                "terminal-edge-violations={} progress-violations={} max-progress-rank={}",
                report.terminal_edge_violations.len(),
                report.progress_violations.len(),
                report.maximum_progress_rank
            );
            if !report.universal_termination || !report.progress_violations.is_empty() {
                eprintln!("universal micro-game termination was not established");
                return ExitCode::FAILURE;
            }
            println!(
                "property game-terminates: proven for {}",
                graph.scope().id()
            );
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn compare(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let left = args.next();
    let right = args.next();
    if left.as_deref() == Some(OsStr::new("all")) && right.as_deref() == Some(OsStr::new("--scope"))
    {
        if args.next().as_deref() != Some(OsStr::new("micro")) || args.next().is_some() {
            usage();
            return ExitCode::from(2);
        }
        return compare_all_command();
    }
    if left.as_deref() == Some(OsStr::new("rust")) && right.as_deref() == Some(OsStr::new("alloy"))
    {
        return compare_rust_alloy_command(args);
    }
    if left.as_deref() == Some(OsStr::new("rust")) && right.as_deref() == Some(OsStr::new("nusmv"))
    {
        return compare_rust_nusmv_command(args);
    }
    if left.as_deref() == Some(OsStr::new("rust")) && right.as_deref() == Some(OsStr::new("prolog"))
    {
        if args.next().as_deref() != Some(OsStr::new("--fixtures")) {
            usage();
            return ExitCode::from(2);
        }
        let Some(fixture_path) = args.next() else {
            usage();
            return ExitCode::from(2);
        };
        if args.next().is_some() {
            usage();
            return ExitCode::from(2);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let supplied = Path::new(&fixture_path);
        let fixtures = if supplied.is_absolute() {
            supplied.to_owned()
        } else {
            root.join(supplied)
        };
        let report = match compare_rust_prolog(&root, &fixtures) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("Rust/Prolog conformance failed: {error}");
                return ExitCode::FAILURE;
            }
        };
        for fixture in &report.fixtures {
            println!(
                "{}: {} exact answer rows; rules={}",
                fixture.id,
                fixture.answer_count,
                fixture.rules.join(",")
            );
        }
        println!("collect boundary: {}", report.collect_boundary);
        println!("predecessor boundary: {}", report.predecessor_boundary);
        println!(
            "Rust/Scryer Prolog conformance: {} fixtures, {} exact normalized answer rows, {} rule explanations; passed",
            report.fixtures.len(),
            report.answer_count,
            report.explanation_count
        );
        return ExitCode::SUCCESS;
    }
    if left.as_deref() != Some(OsStr::new("rust-oracle"))
        || right.as_deref() != Some(OsStr::new("rust-formal"))
        || args.next().is_some()
    {
        usage();
        return ExitCode::from(2);
    }
    let report = match compare_rust_models() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust model conformance failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    for case in &report.cases {
        if let Disposition::ClassifiedDifference(class) = case.disposition {
            println!(
                "classified difference: {} ({class:?}); rules={}",
                case.id,
                case.rule_ids.join(",")
            );
        }
    }
    println!(
        "Rust model conformance: {} matches, {} classified differences",
        report.match_count(),
        report.difference_count()
    );
    println!(
        "exact common prefix: {} observations, {} legal-action sets, {} transitions",
        report.observations_compared,
        report.legal_action_sets_compared,
        report.transitions_compared
    );
    println!("Rust model conformance: passed with zero unclassified differences");
    ExitCode::SUCCESS
}

fn compare_rust_alloy_command(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    if args.next().as_deref() != Some(OsStr::new("--scope"))
        || args.next().as_deref() != Some(OsStr::new("micro"))
        || args.next().is_some()
    {
        usage();
        return ExitCode::from(2);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = match compare_rust_alloy(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/Alloy conformance failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    for (index, scope) in report.command_scopes.iter().enumerate() {
        println!("scope[{index}]: {}", scope.replace('\n', " "));
    }
    for limitation in &report.limitations {
        println!("boundedness: {limitation}");
    }
    println!(
        "Rust/Alloy conformance: {} commands, {} valid instance, {} invalid structures rejected, {} assertion, {} controlled defect witnesses, {} relational projection groups; passed",
        report.command_count,
        report.valid_instances,
        report.invalid_structures_rejected,
        report.assertions_checked,
        report.controlled_defect_witnesses,
        report.projection_groups_compared
    );
    ExitCode::SUCCESS
}

fn compare_rust_nusmv_command(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    if args.next().as_deref() != Some(OsStr::new("--scope"))
        || args.next().as_deref() != Some(OsStr::new("micro"))
        || args.next().is_some()
    {
        usage();
        return ExitCode::from(2);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = match compare_rust_nusmv(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/NuSMV conformance failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    for limitation in &report.limitations {
        println!("scope: {limitation}");
    }
    println!(
        "Rust explicit scope: {} states, {} transitions, {} coarse one-step phase pairs",
        report.rust_states, report.rust_transitions, report.rust_step_projection_pairs
    );
    println!(
        "Rust/NuSMV conformance: {} named properties ({} correct-mode holds), {} refined step obligations, {} initial states, {} controlled defect counterexamples; stutter states={}, deadlock states={}; passed",
        report.property_count,
        report.correct_properties,
        report.native_step_obligations,
        report.initial_states_compared,
        report.defect_counterexamples_compared,
        report.stutter_trace_states,
        report.deadlock_trace_states
    );
    ExitCode::SUCCESS
}

const ACCEPTANCE_HASH_GROUPS: &[(&str, &[&str])] = &[
    ("rules", &["docs/main.typ", "docs/rules-coverage.md"]),
    ("rust-oracle", &["crates/poche-oracle-rust/src/lib.rs"]),
    (
        "rust-formal",
        &[
            "crates/poche-model/src/finite.rs",
            "crates/poche-model/src/formal.rs",
            "crates/poche-model/src/lib.rs",
            "crates/poche-model/src/property.rs",
            "crates/poche-model/src/proptest_tests.rs",
            "crates/poche-model/src/semantics.rs",
            "crates/poche-model/src/state.rs",
            "crates/poche-check/src/evidence.rs",
            "crates/poche-check/src/lib.rs",
            "crates/poche-check/src/liveness.rs",
            "crates/poche-check/src/safety.rs",
        ],
    ),
    (
        "contracts",
        &[
            "crates/poche-domain/src/lib.rs",
            "crates/poche-environment/src/lib.rs",
            "crates/poche-formal/src/lib.rs",
            "crates/poche-interchange/src/lib.rs",
            "crates/poche-interchange/src/validate.rs",
            "crates/poche-interchange/src/wire.rs",
        ],
    ),
    (
        "alloy",
        &["models/alloy/poche.als", "models/alloy/conformance.als"],
    ),
    (
        "nusmv",
        &["models/nusmv/poche.smv", "models/nusmv/conformance.smv"],
    ),
    ("prolog", &["models/prolog/poche.pl"]),
    (
        "conformance",
        &[
            "crates/poche-conformance/src/alloy.rs",
            "crates/poche-conformance/src/lib.rs",
            "crates/poche-conformance/src/nusmv.rs",
            "crates/poche-conformance/src/prolog.rs",
            "crates/poche-native-tools/src/adapters.rs",
            "crates/poche-native-tools/src/lib.rs",
            "crates/poche-native-tools/src/normalize.rs",
            "crates/poche-native-tools/src/runner.rs",
        ],
    ),
    (
        "toolchain",
        &["Cargo.lock", "rust-toolchain.toml", "tools/versions.toml"],
    ),
];

fn acceptance(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    if args.next().as_deref() != Some(OsStr::new("hashes")) || args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    match acceptance_hashes(&root) {
        Ok(hashes) => {
            for (group, hash) in hashes {
                println!("{group}\tblake3:{hash}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("acceptance hashes failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the aggregate command reports each independent evidence gate before acceptance"
)]
fn compare_all_command() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if check_rust_oracle() != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    let native = run_all(&root);
    for report in &native {
        if report.disposition != NativeDisposition::Success {
            eprintln!(
                "{} full oracle failed: {:?}: {}",
                report.backend.id(),
                report.disposition,
                report.diagnostic
            );
            return ExitCode::FAILURE;
        }
        let results = report
            .normalized
            .as_ref()
            .map_or(0, poche_native_tools::NormalizedRun::len);
        let version = report
            .raw
            .as_ref()
            .map_or("<missing>", |raw| raw.version.as_str());
        println!(
            "{} full oracle: {results} results; version={version}",
            report.backend.id()
        );
    }

    let rust = match compare_rust_models() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/Rust comparison failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let prolog = match compare_rust_prolog(&root, &root.join("tests/fixtures/prolog")) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/Prolog comparison failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let alloy = match compare_rust_alloy(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/Alloy comparison failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let nusmv = match compare_rust_nusmv(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Rust/NuSMV comparison failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let hashes = match verify_acceptance_matrix(&root) {
        Ok(hashes) => hashes,
        Err(error) => {
            eprintln!("acceptance matrix failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "Rust/Rust: {} matches, {} classified scope/boundary differences, {} exact transitions",
        rust.match_count(),
        rust.difference_count(),
        rust.transitions_compared
    );
    println!(
        "Rust/Prolog: {} fixtures, {} exact rows, {} explanations",
        prolog.fixtures.len(),
        prolog.answer_count,
        prolog.explanation_count
    );
    println!(
        "Rust/Alloy: {} commands, {} projection groups, {} defect witnesses",
        alloy.command_count, alloy.projection_groups_compared, alloy.controlled_defect_witnesses
    );
    println!(
        "Rust/NuSMV: {} properties, {} Rust states, {} Rust transitions, {} defect counterexamples",
        nusmv.property_count,
        nusmv.rust_states,
        nusmv.rust_transitions,
        nusmv.defect_counterexamples_compared
    );
    println!(
        "acceptance revisions: {} verified BLAKE3 groups",
        hashes.len()
    );
    println!("cross-model acceptance: passed with zero unclassified differences");
    ExitCode::SUCCESS
}

fn acceptance_hashes(root: &Path) -> Result<Vec<(&'static str, String)>, String> {
    ACCEPTANCE_HASH_GROUPS
        .iter()
        .map(|(group, paths)| {
            artifact_hash(root, paths)
                .map(|hash| (*group, hash))
                .map_err(|error| format!("{group}: {error}"))
        })
        .collect()
}

fn artifact_hash(root: &Path, paths: &[&str]) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    for relative in paths {
        let bytes = fs::read(root.join(relative))?;
        hasher.update(relative.as_bytes());
        hasher.update(&[0]);
        let length = u64::try_from(bytes.len()).map_err(io::Error::other)?;
        hasher.update(&length.to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn verify_acceptance_matrix(root: &Path) -> Result<Vec<(&'static str, String)>, String> {
    let path = root.join("docs/acceptance-matrix.md");
    let matrix = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let hashes = acceptance_hashes(root)?;
    let missing_hashes = hashes
        .iter()
        .filter(|(group, hash)| !matrix.contains(&format!("| {group} | `blake3:{hash}` |")))
        .map(|(group, hash)| format!("{group}=blake3:{hash}"))
        .collect::<Vec<_>>();
    if !missing_hashes.is_empty() {
        return Err(format!(
            "stale/missing revision rows: {}",
            missing_hashes.join(", ")
        ));
    }
    for required in [
        "Rust 1.96.0",
        "Alloy 6.2.0",
        "NuSMV 2.7.1",
        "Scryer Prolog 0.10.0-17-ge4d96925",
        "61 stable rules",
        "431,800",
        "549,896",
        "compare all --scope micro",
    ] {
        if !matrix.contains(required) {
            return Err(format!(
                "acceptance matrix omitted required marker {required:?}"
            ));
        }
    }
    Ok(hashes)
}

fn oracle(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let Some(subcommand) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if subcommand == OsStr::new("report") {
        if args.next().is_some() {
            usage();
            return ExitCode::from(2);
        }
        return oracle_report();
    }
    if subcommand != OsStr::new("check") {
        usage();
        return ExitCode::from(2);
    }

    let backend = args.next();
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }
    match backend.as_deref() {
        Some(value) if value == OsStr::new("rust") => check_rust_oracle(),
        Some(value) if value == OsStr::new("alloy") => check_alloy_oracle(),
        Some(value) if value == OsStr::new("nusmv") => check_nusmv_oracle(),
        Some(value) if value == OsStr::new("prolog") => check_prolog_oracle(),
        Some(value) if value == OsStr::new("all") => check_all_oracles(),
        Some(value) => {
            eprintln!(
                "oracle backend is not implemented yet: {}",
                value.to_string_lossy()
            );
            ExitCode::from(2)
        }
        None => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn oracle_report() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let required = [
        "models/alloy/poche.als",
        "models/nusmv/poche.smv",
        "models/prolog/poche.pl",
        "crates/poche-oracle-rust/src/lib.rs",
        "docs/alloy-oracle.md",
        "docs/nusmv-oracle.md",
        "docs/prolog-oracle.md",
        "docs/oracle-audit.md",
        "docs/acceptance-matrix.md",
        "fixtures/oracle-inventory.toml",
    ];
    for relative in required {
        let path = root.join(relative);
        if !path.is_file() {
            eprintln!(
                "oracle report: required artifact is missing: {}",
                path.display()
            );
            return ExitCode::FAILURE;
        }
    }

    let coverage_path = root.join("docs/rules-coverage.md");
    let plan_path = root.join("PLAN.md");
    let inventory_path = root.join("fixtures/oracle-inventory.toml");
    let audit_path = root.join("docs/oracle-audit.md");
    let read = |path: &Path| match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            eprintln!("oracle report: failed to read {}: {error}", path.display());
            None
        }
    };
    let Some(coverage) = read(&coverage_path) else {
        return ExitCode::FAILURE;
    };
    let Some(plan) = read(&plan_path) else {
        return ExitCode::FAILURE;
    };
    if run_coverage_audit(&coverage, &plan, true, None) != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    let Some(inventory) = read(&inventory_path) else {
        return ExitCode::FAILURE;
    };
    let Some(audit) = read(&audit_path) else {
        return ExitCode::FAILURE;
    };

    let scenarios = inventory.matches("[[scenario]]").count();
    let properties = inventory.matches("[[property]]").count();
    let queries = inventory.matches("[[query]]").count();
    let differences = audit
        .lines()
        .filter(|line| line.starts_with("| D-"))
        .count();
    if (scenarios, properties, queries, differences) != (15, 4, 3, 11)
        || !audit.contains("Status: Phase 6 cross-model acceptance complete")
    {
        eprintln!(
            "oracle report: inventory/audit shape changed unexpectedly: scenarios={scenarios}, properties={properties}, queries={queries}, differences={differences}"
        );
        return ExitCode::FAILURE;
    }

    println!("oracle completeness: G11 satisfied across 61 rules and 4 tracks");
    println!(
        "shared inventory: {scenarios} scenarios, {properties} properties, {queries} reverse/action queries"
    );
    println!("explicit differences: {differences}; missing-rule gaps: 0");
    println!("oracle report: passed");
    ExitCode::SUCCESS
}

fn check_prolog_oracle() -> ExitCode {
    check_native_oracle(NativeBackend::ScryerProlog)
}

fn check_nusmv_oracle() -> ExitCode {
    check_native_oracle(NativeBackend::NuSmv)
}

fn check_alloy_oracle() -> ExitCode {
    check_native_oracle(NativeBackend::Alloy)
}

fn check_native_oracle(backend: NativeBackend) -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = run_backend(&root, backend);
    let result_count = report
        .normalized
        .as_ref()
        .map_or(0, poche_native_tools::NormalizedRun::len);
    match report.disposition {
        NativeDisposition::Success => {
            println!(
                "{} oracle: passed; {result_count} typed native results; {}",
                backend.id(),
                report.diagnostic
            );
            println!("evidence: {}", report.evidence_directory.display());
            ExitCode::SUCCESS
        }
        NativeDisposition::Failure | NativeDisposition::Unknown => {
            eprintln!(
                "{} oracle: {:?}; {}",
                backend.id(),
                report.disposition,
                report.diagnostic
            );
            eprintln!("evidence: {}", report.evidence_directory.display());
            ExitCode::FAILURE
        }
    }
}

fn check_all_oracles() -> ExitCode {
    if check_rust_oracle() != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    for backend in [
        NativeBackend::Alloy,
        NativeBackend::NuSmv,
        NativeBackend::ScryerProlog,
    ] {
        if check_native_oracle(backend) != ExitCode::SUCCESS {
            return ExitCode::FAILURE;
        }
    }
    println!("all four independent oracle tracks: passed");
    ExitCode::SUCCESS
}

fn check_rust_oracle() -> ExitCode {
    let first_dealer = Seat::<2>::new(0).expect("two-player seat is valid");
    let mut game = Game::new(first_dealer).expect("two-player game is valid");
    let mut transitions = 0_u32;
    let mut settled_rounds = 0_u32;

    while transitions < 2_000 {
        if let Err(error) = game.validate() {
            eprintln!("Rust oracle invariant failed: {error:?}");
            return ExitCode::FAILURE;
        }
        let action = match game.turn() {
            Turn::Chance => Action::Deal(DeckOrder::standard()),
            Turn::Player(_) => game
                .legal_player_actions()
                .into_iter()
                .next()
                .expect("an acting player has a legal action"),
            Turn::Environment => Action::SettleRound,
            Turn::Finished => break,
        };
        let transition = game.transition(action).expect("selected action is legal");
        if transition.round_scores.is_some() {
            settled_rounds += 1;
        }
        game = transition.next;
        transitions += 1;
    }

    let GameState::Finished(finished) = game.state() else {
        eprintln!("Rust oracle did not terminate within the smoke bound");
        return ExitCode::FAILURE;
    };
    println!(
        "Rust oracle: passed; players=2 rounds={settled_rounds} transitions={transitions} scores={:?} pot_cents={}",
        finished.scores, finished.pot_cents
    );
    ExitCode::SUCCESS
}

fn guidance(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    if args.next().as_deref() != Some(OsStr::new("audit")) {
        usage();
        return ExitCode::from(2);
    }
    let Some(path) = args.next() else {
        eprintln!("guidance audit requires a plan path");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }

    let path = Path::new(&path);
    let plan = match fs::read_to_string(path) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("guidance audit could not read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };

    match audit_guidance_plan(&plan) {
        Ok(report) => {
            println!("plan profile: {} ({})", report.plan_id, report.status);
            println!("guidance ledger: {} active requirements", report.guidance);
            println!("traceability: {} requirement mappings", report.traceability);
            println!("architecture gates: {} registered", report.gates);
            println!(
                "implementation tasks: {} registered with completion notes",
                report.tasks
            );
            println!("overall criteria: {} registered", report.overall_criteria);
            println!(
                "deferred follow-ups: {} retained outside completion",
                report.deferred_sections
            );
            println!("triple audit pass 1: extraction record present");
            println!("triple audit pass 2: one-to-one traceability verified");
            println!("triple audit pass 3: adversarial omission markers verified");
            println!(
                "guidance audit: passed for declarative profile {}",
                report.plan_id
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in &errors {
                eprintln!("guidance audit: {error}");
            }
            eprintln!("guidance audit: failed with {} error(s)", errors.len());
            ExitCode::FAILURE
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the release audit intentionally checks every independent plan-preservation surface"
)]
fn audit_guidance_plan(plan: &str) -> Result<GuidanceAuditReport, Vec<String>> {
    let mut errors = Vec::new();
    let profiles = match load_audit_profiles() {
        Ok(profiles) => profiles,
        Err(error) => return Err(vec![error]),
    };
    let Some(profile) = select_audit_profile(plan, &profiles, &mut errors) else {
        return Err(errors);
    };
    let normalized = plan.split_whitespace().collect::<Vec<_>>().join(" ");

    let status = plan_status(plan).unwrap_or_else(|| {
        errors.push("plan status is missing".to_owned());
        String::new()
    });
    if !profile.allowed_statuses.contains(&status) {
        errors.push(format!(
            "plan status `{status}` is not allowed by profile `{}`",
            profile.plan_id
        ));
    }
    let is_complete = status == COMPLETE_STATUS;
    if !normalized.contains(REQUIRED_READY_SENTENCE) {
        errors.push("literal triple-check readiness rule is missing".to_owned());
    }

    let ledger = required_section(plan, "## Authoritative user guidance ledger", &mut errors)
        .map_or_else(BTreeMap::new, |section| {
            numbered_table_rows(section, 'U', 3, &mut errors)
        });
    validate_exact_ids(
        &ledger,
        'U',
        &profile.guidance_ids,
        "guidance ledger",
        &mut errors,
    );
    for (number, cells) in &ledger {
        if cells[1].is_empty() || cells[2].is_empty() {
            errors.push(format!("U{number} has empty guidance or consequence text"));
        }
        if profile.forbid_superseded && cells[1].contains("Superseded by") {
            errors.push(format!(
                "U{number} is superseded, but profile `{}` requires every registered row to remain active",
                profile.plan_id
            ));
        }
    }

    let traceability = required_section(plan, "## Guidance traceability", &mut errors)
        .map_or_else(BTreeMap::new, |section| {
            numbered_table_rows(section, 'U', 2, &mut errors)
        });
    validate_exact_ids(
        &traceability,
        'U',
        &profile.guidance_ids,
        "guidance traceability",
        &mut errors,
    );
    for (number, cells) in &traceability {
        if cells[1].is_empty() || cells[1].eq_ignore_ascii_case("none") {
            errors.push(format!("U{number} has no concrete plan mapping"));
        }
    }

    let intent = required_section(plan, "## Intent audit evidence", &mut errors).unwrap_or("");
    let normalized_intent = intent.split_whitespace().collect::<Vec<_>>().join(" ");
    for marker in REQUIRED_TRIPLE_AUDIT_MARKERS {
        require_exactly_once(intent, marker, "intent-audit marker", &mut errors);
    }
    for marker in &profile.adversarial_markers {
        if !normalized_intent.contains(marker) {
            errors.push(format!(
                "adversarial omission pass does not preserve `{marker}`"
            ));
        }
    }
    if !intent.contains("**Known source limitation:** None") {
        errors.push("intent audit does not record the known source limitation".to_owned());
    }

    let gates = required_section(plan, &profile.gate_section, &mut errors)
        .map_or_else(BTreeMap::new, |section| {
            numbered_table_rows(section, 'G', profile.gate_cells, &mut errors)
        });
    validate_exact_ids(&gates, 'G', &profile.gate_ids, "design gates", &mut errors);
    let allowed_gate_statuses = if is_complete {
        &profile.complete_gate_statuses
    } else {
        &profile.working_gate_statuses
    };
    for (number, cells) in &gates {
        if !allowed_gate_statuses.contains(&cells[1]) {
            errors.push(format!(
                "G{number} has status `{}`; profile `{}` allows {}",
                cells[1],
                profile.plan_id,
                allowed_gate_statuses.join(", ")
            ));
        }
        if cells.iter().skip(2).any(String::is_empty) {
            errors.push(format!("G{number} has an empty decision field"));
        }
    }

    let tasks = audit_task_completion(plan, profile, &status, &mut errors);

    let overall =
        required_section(plan, "## Overall completion criteria", &mut errors).unwrap_or("");
    let mut overall_criteria = 0_usize;
    for line in overall.lines() {
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let Some((status, _)) = rest.split_once("] ") else {
            errors.push(format!("malformed overall completion criterion `{line}`"));
            continue;
        };
        overall_criteria += 1;
        if is_complete && status != "x" {
            errors.push(format!(
                "overall completion criterion is not complete: `{line}`"
            ));
        } else if !matches!(status, " " | "x") {
            errors.push(format!("invalid overall criterion status in `{line}`"));
        }
    }
    if overall_criteria != profile.overall_criteria {
        errors.push(format!(
            "overall completion has {overall_criteria} criteria; profile `{}` expects {}",
            profile.plan_id, profile.overall_criteria
        ));
    }

    for marker in &profile.deferred_sections {
        require_exactly_once(plan, marker, "deferred follow-up", &mut errors);
    }

    if errors.is_empty() {
        Ok(GuidanceAuditReport {
            plan_id: profile.plan_id.clone(),
            status,
            guidance: ledger.len(),
            traceability: traceability.len(),
            gates: gates.len(),
            tasks,
            overall_criteria,
            deferred_sections: profile.deferred_sections.len(),
        })
    } else {
        Err(errors)
    }
}

fn load_audit_profiles() -> Result<Vec<AuditProfile>, String> {
    let mut profiles = PLAN_AUDIT_CONTRACTS
        .iter()
        .map(|contract| parse_audit_profile(contract))
        .collect::<Result<Vec<_>, _>>()?;
    profiles.sort_by(|left, right| left.plan_id.cmp(&right.plan_id));
    for pair in profiles.windows(2) {
        if pair[0].plan_id == pair[1].plan_id {
            return Err(format!("duplicate audit profile `{}`", pair[0].plan_id));
        }
    }
    Ok(profiles)
}

fn parse_audit_profile(contract: &str) -> Result<AuditProfile, String> {
    let mut fields = BTreeMap::<String, String>::new();
    for (index, raw_line) in contract.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("audit contract line {} has no `=`", index + 1));
        };
        let key = key.trim().to_owned();
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Err(format!("audit contract field `{key}` is empty"));
        }
        if fields.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate audit contract field `{key}`"));
        }
    }

    let plan_id = contract_field(&fields, "plan_id")?.to_owned();
    let guidance_ids = parse_contract_ids(contract_field(&fields, "guidance_ids")?)?;
    let gate_ids = parse_contract_ids(contract_field(&fields, "gate_ids")?)?;
    require_contiguous_ids(&plan_id, "guidance_ids", &guidance_ids)?;
    require_contiguous_ids(&plan_id, "gate_ids", &gate_ids)?;

    let profile = AuditProfile {
        plan_id,
        title: contract_field(&fields, "title")?.to_owned(),
        plan_id_required: parse_contract_bool(contract_field(&fields, "plan_id_required")?)?,
        allowed_statuses: parse_contract_list(contract_field(&fields, "allowed_statuses")?)?,
        guidance_ids,
        gate_section: contract_field(&fields, "gate_section")?.to_owned(),
        gate_ids,
        gate_cells: parse_contract_usize(contract_field(&fields, "gate_cells")?)?,
        working_gate_statuses: parse_contract_list(contract_field(
            &fields,
            "working_gate_statuses",
        )?)?,
        complete_gate_statuses: parse_contract_list(contract_field(
            &fields,
            "complete_gate_statuses",
        )?)?,
        task_ids: parse_contract_list(contract_field(&fields, "task_ids")?)?,
        require_task_criteria: parse_contract_bool(contract_field(
            &fields,
            "require_task_criteria",
        )?)?,
        overall_criteria: parse_contract_usize(contract_field(&fields, "overall_criteria")?)?,
        deferred_sections: parse_contract_list(contract_field(&fields, "deferred_sections")?)?,
        adversarial_markers: parse_contract_list(contract_field(&fields, "adversarial_markers")?)?,
        forbid_superseded: parse_contract_bool(contract_field(&fields, "forbid_superseded")?)?,
    };
    let mut task_ids = profile.task_ids.clone();
    task_ids.sort();
    task_ids.dedup();
    if task_ids.len() != profile.task_ids.len() {
        return Err(format!(
            "audit profile `{}` contains duplicate task IDs",
            profile.plan_id
        ));
    }
    Ok(profile)
}

fn contract_field<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    fields
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("audit contract is missing `{key}`"))
}

fn parse_contract_list(value: &str) -> Result<Vec<String>, String> {
    let values = value
        .split("||")
        .map(str::trim)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(String::is_empty) {
        Err(format!(
            "invalid empty value in audit contract list `{value}`"
        ))
    } else {
        Ok(values)
    }
}

fn parse_contract_ids(value: &str) -> Result<Vec<u32>, String> {
    let mut ids = Vec::new();
    for part in value.split("||").map(str::trim) {
        if let Some((start, end)) = part.split_once('-') {
            let start = start
                .parse::<u32>()
                .map_err(|error| format!("invalid ID range start `{start}`: {error}"))?;
            let end = end
                .parse::<u32>()
                .map_err(|error| format!("invalid ID range end `{end}`: {error}"))?;
            if start > end {
                return Err(format!("descending audit ID range `{part}`"));
            }
            ids.extend(start..=end);
        } else {
            ids.push(
                part.parse::<u32>()
                    .map_err(|error| format!("invalid audit ID `{part}`: {error}"))?,
            );
        }
    }
    Ok(ids)
}

fn parse_contract_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("invalid audit contract boolean `{value}`")),
    }
}

fn parse_contract_usize(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|error| format!("invalid audit contract integer `{value}`: {error}"))
}

fn require_contiguous_ids(plan_id: &str, field: &str, ids: &[u32]) -> Result<(), String> {
    if ids.is_empty() {
        return Err(format!("audit profile `{plan_id}` has no {field}"));
    }
    if ids.windows(2).any(|pair| pair[1] != pair[0] + 1) {
        return Err(format!(
            "audit profile `{plan_id}` has noncontiguous or duplicate {field}"
        ));
    }
    Ok(())
}

fn select_audit_profile<'a>(
    plan: &str,
    profiles: &'a [AuditProfile],
    errors: &mut Vec<String>,
) -> Option<&'a AuditProfile> {
    let title = first_nonempty_line(plan).unwrap_or("");
    let declared_id = plan.lines().find_map(|line| {
        line.strip_prefix("**Plan ID:**")
            .map(str::trim)
            .map(|value| value.trim_matches('`'))
            .filter(|value| !value.is_empty())
    });
    let profile = if let Some(plan_id) = declared_id {
        let Some(profile) = profiles.iter().find(|profile| profile.plan_id == plan_id) else {
            errors.push(format!("unknown plan ID `{plan_id}`"));
            return None;
        };
        profile
    } else {
        let Some(profile) = profiles
            .iter()
            .find(|profile| !profile.plan_id_required && profile.title == title)
        else {
            errors.push("plan has no registered Plan ID or legacy title".to_owned());
            return None;
        };
        profile
    };
    if profile.title != title {
        errors.push(format!(
            "plan title `{title}` does not match profile `{}` title `{}`",
            profile.plan_id, profile.title
        ));
    }
    if profile.plan_id_required && declared_id.is_none() {
        errors.push(format!(
            "profile `{}` requires an explicit Plan ID",
            profile.plan_id
        ));
    }
    Some(profile)
}

fn plan_status(plan: &str) -> Option<String> {
    plan.lines()
        .find_map(|line| line.strip_prefix("**Plan status:**"))
        .map(str::trim)
        .map(str::to_owned)
}

fn required_section<'a>(plan: &'a str, heading: &str, errors: &mut Vec<String>) -> Option<&'a str> {
    let Some(start) = plan.find(heading) else {
        errors.push(format!("missing required section `{heading}`"));
        return None;
    };
    if plan[start + heading.len()..].contains(heading) {
        errors.push(format!(
            "required section `{heading}` appears more than once"
        ));
        return None;
    }
    let body = &plan[start + heading.len()..];
    let end = body.find("\n## ").unwrap_or(body.len());
    Some(&body[..end])
}

fn numbered_table_rows(
    section: &str,
    prefix: char,
    expected_cells: usize,
    errors: &mut Vec<String>,
) -> BTreeMap<u32, Vec<String>> {
    let mut rows = BTreeMap::new();
    for line in section.lines() {
        let cells = markdown_cells(line);
        let Some(number) = cells
            .first()
            .and_then(|cell| parse_numbered_id(cell, prefix))
        else {
            continue;
        };
        if cells.len() != expected_cells {
            errors.push(format!(
                "{prefix}{number} has {} table cells; expected {expected_cells}",
                cells.len()
            ));
            continue;
        }
        let owned = cells.into_iter().map(str::to_owned).collect::<Vec<_>>();
        if rows.insert(number, owned).is_some() {
            errors.push(format!("duplicate {prefix}{number} table row"));
        }
    }
    rows
}

fn parse_numbered_id(value: &str, prefix: char) -> Option<u32> {
    value.strip_prefix(prefix)?.parse().ok()
}

fn validate_exact_ids(
    rows: &BTreeMap<u32, Vec<String>>,
    prefix: char,
    expected: &[u32],
    label: &str,
    errors: &mut Vec<String>,
) {
    for number in expected {
        if !rows.contains_key(number) {
            errors.push(format!("{label} is missing {prefix}{number}"));
        }
    }
    for number in rows
        .keys()
        .copied()
        .filter(|number| !expected.contains(number))
    {
        errors.push(format!(
            "{label} contains unregistered {prefix}{number}; update its declarative audit contract intentionally"
        ));
    }
}

fn require_exactly_once(text: &str, marker: &str, label: &str, errors: &mut Vec<String>) {
    let count = text.match_indices(marker).count();
    if count != 1 {
        errors.push(format!(
            "{label} `{marker}` appears {count} times; expected exactly once"
        ));
    }
}

fn audit_task_completion(
    plan: &str,
    profile: &AuditProfile,
    plan_status: &str,
    errors: &mut Vec<String>,
) -> usize {
    let lines = plan.lines().collect::<Vec<_>>();
    let mut tasks = BTreeMap::<String, (String, usize)>::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("### [") else {
            continue;
        };
        let Some((status, description)) = rest.split_once("] ") else {
            errors.push(format!("malformed task heading `{line}`"));
            continue;
        };
        let Some(id) = description.split_whitespace().next() else {
            errors.push(format!("task heading has no ID `{line}`"));
            continue;
        };
        if tasks
            .insert(id.to_owned(), (status.to_owned(), index))
            .is_some()
        {
            errors.push(format!("duplicate implementation task {id}"));
        }
    }

    let mut in_progress = 0_usize;
    let mut started = 0_usize;
    for required in &profile.task_ids {
        let Some((status, start)) = tasks.get(required) else {
            errors.push(format!("missing implementation task {required}"));
            continue;
        };
        if !matches!(status.as_str(), " " | "~" | "x" | "!") {
            errors.push(format!(
                "implementation task {required} has invalid status [{status}]"
            ));
        }
        if status == "~" {
            in_progress += 1;
        }
        if status != " " {
            started += 1;
        }
        if plan_status == COMPLETE_STATUS && status != "x" {
            errors.push(format!(
                "implementation task {required} has status [{status}], expected [x]"
            ));
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find_map(|(index, line)| line.starts_with("### ").then_some(index))
            .unwrap_or(lines.len());
        let task_body = &lines[start + 1..end];
        if !task_body
            .iter()
            .any(|line| line.starts_with("**Completion notes"))
        {
            errors.push(format!(
                "implementation task {required} has no adjacent completion notes"
            ));
        }
        if profile.require_task_criteria
            && !task_body
                .iter()
                .any(|line| line.starts_with("**Completion criteria:**"))
        {
            errors.push(format!(
                "implementation task {required} has no adjacent completion criteria"
            ));
        }
        if status == "x"
            && task_body.iter().any(|line| {
                line.starts_with("**Completion notes")
                    && (line.contains("Not started") || line.contains("In progress"))
            })
        {
            errors.push(format!(
                "implementation task {required} is complete but its notes still report unfinished work"
            ));
        }
    }
    for id in tasks.keys().filter(|id| !profile.task_ids.contains(id)) {
        errors.push(format!(
            "unregistered implementation task {id}; update the declarative audit contract intentionally"
        ));
    }
    if in_progress > 1 {
        errors.push(format!(
            "plan has {in_progress} tasks in progress; expected at most one"
        ));
    }
    if plan_status == READY_STATUS && started != 0 {
        errors.push(format!(
            "plan status is `{READY_STATUS}` but {started} implementation tasks have started"
        ));
    }
    if plan_status == IN_PROGRESS_STATUS && started == 0 {
        errors.push(format!(
            "plan status is `{IN_PROGRESS_STATUS}` but no implementation task has started"
        ));
    }
    tasks.len()
}

fn coverage(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    if args.next().as_deref() != Some(OsStr::new("audit")) {
        usage();
        return ExitCode::from(2);
    }

    let mut strict_track = None;
    let mut strict_all = false;
    while let Some(argument) = args.next() {
        if argument == OsStr::new("--all") {
            strict_all = true;
        } else if argument == OsStr::new("--track") {
            let Some(track) = args.next() else {
                eprintln!("--track requires rust, alloy, nusmv, or prolog");
                return ExitCode::from(2);
            };
            strict_track = Some(track.to_string_lossy().into_owned());
        } else {
            eprintln!(
                "unknown coverage-audit option: {}",
                argument.to_string_lossy()
            );
            return ExitCode::from(2);
        }
    }

    if strict_all && strict_track.is_some() {
        eprintln!("choose either --all or --track, not both");
        return ExitCode::from(2);
    }

    if let Some(selected) = strict_track.as_deref()
        && !COVERAGE_TRACKS.iter().any(|(track, _)| *track == selected)
    {
        eprintln!("unknown coverage track: {selected}");
        return ExitCode::from(2);
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let coverage_path = root.join("docs/rules-coverage.md");
    let plan_path = root.join("PLAN.md");
    let coverage_text = match fs::read_to_string(&coverage_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("failed to read {}: {error}", coverage_path.display());
            return ExitCode::FAILURE;
        }
    };
    let plan_text = match fs::read_to_string(&plan_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("failed to read {}: {error}", plan_path.display());
            return ExitCode::FAILURE;
        }
    };

    run_coverage_audit(
        &coverage_text,
        &plan_text,
        strict_all,
        strict_track.as_deref(),
    )
}

fn run_coverage_audit(
    coverage_text: &str,
    plan_text: &str,
    strict_all: bool,
    strict_track: Option<&str>,
) -> ExitCode {
    let mut errors = Vec::new();
    let mut seen_rules = BTreeMap::new();
    let mut pending = BTreeMap::from([
        ("rust", 0_usize),
        ("alloy", 0_usize),
        ("nusmv", 0_usize),
        ("prolog", 0_usize),
    ]);

    for (line_index, line) in coverage_text.lines().enumerate() {
        let cells = markdown_cells(line);
        let Some(rule_id) = cells.first().filter(|cell| cell.starts_with("R-")) else {
            continue;
        };
        let line_number = line_index + 1;

        if cells.len() != 9 {
            errors.push(format!(
                "{rule_id} on line {line_number} has {} cells; expected 9",
                cells.len()
            ));
            continue;
        }
        if let Some(first_line) = seen_rules.insert((*rule_id).to_owned(), line_number) {
            errors.push(format!(
                "duplicate {rule_id} on lines {first_line} and {line_number}"
            ));
        }
        if !is_source_anchor(cells[1]) {
            errors.push(format!(
                "{rule_id} has invalid source anchor `{}`",
                cells[1]
            ));
        }
        if cells[2].is_empty() || cells[7].is_empty() || cells[8].is_empty() {
            errors.push(format!(
                "{rule_id} must have a rule, future-RL flag, and notes"
            ));
        }

        for (track, index) in COVERAGE_TRACKS {
            let disposition = cells[index];
            if !is_disposition(disposition) {
                errors.push(format!(
                    "{rule_id} has invalid {track} disposition `{disposition}`"
                ));
                continue;
            }
            if disposition == "todo" {
                *pending.get_mut(track).expect("known track") += 1;
                let selected = strict_all || strict_track == Some(track);
                if selected {
                    errors.push(format!("{rule_id} remains todo for {track}"));
                }
            }
        }
    }

    if seen_rules.is_empty() {
        errors.push("no rule rows found".to_owned());
    }
    audit_guidance(plan_text, &mut errors);

    println!("coverage rules: {}", seen_rules.len());
    for (track, _) in COVERAGE_TRACKS {
        println!("{track} todo: {}", pending[track]);
    }

    if errors.is_empty() {
        println!("coverage audit: passed");
        ExitCode::SUCCESS
    } else {
        for error in &errors {
            eprintln!("coverage audit: {error}");
        }
        eprintln!("coverage audit: failed with {} error(s)", errors.len());
        ExitCode::FAILURE
    }
}

fn markdown_cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn is_source_anchor(value: &str) -> bool {
    value.starts_with("`docs/main.typ:") && value.ends_with('`')
}

fn is_disposition(value: &str) -> bool {
    value == "todo"
        || [
            "modeled: ",
            "checked: ",
            "queried: ",
            "validated: ",
            "n/a: ",
            "deferred: ",
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix) && value.len() > prefix.len())
}

fn audit_guidance(plan: &str, errors: &mut Vec<String>) {
    let mut counts = BTreeMap::<u32, usize>::new();
    for line in plan.lines() {
        let cells = markdown_cells(line);
        let Some(id) = cells.first().and_then(|cell| cell.strip_prefix('U')) else {
            continue;
        };
        if let Ok(number) = id.parse::<u32>() {
            *counts.entry(number).or_default() += 1;
        }
    }

    let Some(maximum) = counts.keys().next_back().copied() else {
        errors.push("PLAN.md contains no guidance rows".to_owned());
        return;
    };
    for number in 1..=maximum {
        match counts.get(&number).copied().unwrap_or_default() {
            2 => {}
            0 => errors.push(format!("PLAN.md is missing U{number}")),
            count => errors.push(format!(
                "PLAN.md has {count} table rows for U{number}; expected ledger plus traceability"
            )),
        }
    }
}

fn doctor() -> ExitCode {
    let tools = [
        Tool {
            label: "Rust",
            override_var: None,
            commands: &["rustc"],
            version_args: &["--version"],
            accept_nonzero_with: None,
        },
        Tool {
            label: "Alloy",
            override_var: Some("ALLOY_BIN"),
            commands: &["alloy", "alloy.exe"],
            version_args: &["version"],
            accept_nonzero_with: None,
        },
        Tool {
            label: "NuSMV",
            override_var: Some("NUSMV_BIN"),
            commands: &["NuSMV", "NuSMV.exe", "nusmv"],
            version_args: &["-h"],
            accept_nonzero_with: Some("NuSMV"),
        },
        Tool {
            label: "Scryer Prolog",
            override_var: Some("SCRYER_PROLOG_BIN"),
            commands: &["scryer-prolog", "scryer-prolog.exe"],
            version_args: &["--version"],
            accept_nonzero_with: None,
        },
        Tool {
            label: "Typst",
            override_var: Some("TYPST_BIN"),
            commands: &["typst", "typst.exe"],
            version_args: &["--version"],
            accept_nonzero_with: None,
        },
    ];

    let mut failed_override = false;
    for tool in tools {
        match probe(&tool) {
            Probe::Available { command, version } => println!(
                "{:<14} available  {:<24} {}",
                tool.label,
                command.to_string_lossy(),
                version
            ),
            Probe::Missing => println!(
                "{:<14} unavailable (set {} or add {} to PATH)",
                tool.label,
                tool.override_var.unwrap_or("PATH"),
                tool.commands.join("/")
            ),
            Probe::Failed { command, detail } => {
                failed_override = true;
                println!(
                    "{:<14} failed     {:<24} {}",
                    tool.label,
                    command.to_string_lossy(),
                    detail
                );
            }
        }
    }

    if failed_override {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn probe(tool: &Tool<'_>) -> Probe {
    if let Some(variable) = tool.override_var
        && let Some(command) = env::var_os(variable)
    {
        return run_version(command, tool.version_args, tool.accept_nonzero_with, true);
    }

    for command in tool.commands {
        match run_version(
            OsString::from(command),
            tool.version_args,
            tool.accept_nonzero_with,
            false,
        ) {
            Probe::Missing => {}
            result => return result,
        }
    }
    Probe::Missing
}

fn run_version(
    command: OsString,
    args: &[&str],
    accept_nonzero_with: Option<&str>,
    explicit: bool,
) -> Probe {
    match Command::new(&command).args(args).output() {
        Ok(output)
            if output.status.success()
                || accept_nonzero_with.is_some_and(|marker| output_contains(&output, marker)) =>
        {
            Probe::Available {
                command,
                version: summarize_output(&output),
            }
        }
        Ok(output) => Probe::Failed {
            command,
            detail: format!("exit {}: {}", output.status, summarize_output(&output)),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound && !explicit => Probe::Missing,
        Err(error) => Probe::Failed {
            command,
            detail: error.to_string(),
        },
    }
}

fn output_contains(output: &Output, marker: &str) -> bool {
    String::from_utf8_lossy(&output.stdout).contains(marker)
        || String::from_utf8_lossy(&output.stderr).contains(marker)
}

fn summarize_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    first_nonempty_line(&stdout)
        .or_else(|| first_nonempty_line(&stderr))
        .unwrap_or("no version output")
        .to_owned()
}

fn first_nonempty_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{
        AuditProfile, COMPLETE_STATUS, IN_PROGRESS_STATUS, REQUIRED_READY_SENTENCE,
        REQUIRED_TRIPLE_AUDIT_MARKERS, audit_guidance_plan, first_nonempty_line, is_disposition,
        is_source_anchor, load_audit_profiles, markdown_cells, parse_audit_profile,
    };

    #[test]
    fn version_summary_uses_first_nonempty_line() {
        assert_eq!(
            first_nonempty_line("\n  version 1.2\nmore"),
            Some("version 1.2")
        );
        assert_eq!(first_nonempty_line("\n\r\n"), None);
    }

    #[test]
    fn coverage_rows_are_split_and_validated() {
        let cells = markdown_cells("| R-X-001 | `docs/main.typ:1-2` | Rule | todo |");
        assert_eq!(cells, ["R-X-001", "`docs/main.typ:1-2`", "Rule", "todo"]);
        assert!(is_source_anchor(cells[1]));
        assert!(is_disposition(cells[3]));
        assert!(is_disposition("checked: property P1"));
        assert!(!is_disposition("checked:"));
        assert!(!is_disposition("pending"));
    }

    #[test]
    fn complete_predecessor_profile_passes_release_audit() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let report = audit_guidance_plan(&plan_fixture(profile, COMPLETE_STATUS)).unwrap();
        assert_eq!(report.plan_id, "poche-foundation");
        assert_eq!(report.guidance, 29);
        assert_eq!(report.traceability, 29);
        assert_eq!(report.gates, 14);
        assert_eq!(report.tasks, 30);
        assert_eq!(report.overall_criteria, 16);
        assert_eq!(report.deferred_sections, 3);
    }

    #[test]
    fn in_progress_phase_two_profile_passes_audit() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-phase-2");
        let report = audit_guidance_plan(&plan_fixture(profile, IN_PROGRESS_STATUS)).unwrap();
        assert_eq!(report.plan_id, "poche-phase-2");
        assert_eq!(report.guidance, 21);
        assert_eq!(report.traceability, 21);
        assert_eq!(report.gates, 18);
        assert_eq!(report.tasks, 42);
        assert_eq!(report.overall_criteria, 19);
        assert_eq!(report.deferred_sections, 4);
    }

    #[test]
    fn guidance_audit_rejects_a_missing_mapping() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let plan = plan_fixture(profile, COMPLETE_STATUS).replace("| U17 | task 17 |\n", "");
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("traceability is missing U17"))
        );
    }

    #[test]
    fn guidance_audit_rejects_noncontiguous_plan_ids() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let plan = plan_fixture(profile, COMPLETE_STATUS)
            .replace("| U16 | active guidance 16 | consequence 16 |\n", "");
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("guidance ledger is missing U16"))
        );
    }

    #[test]
    fn guidance_audit_rejects_an_unfinished_task() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let plan = plan_fixture(profile, COMPLETE_STATUS).replace("### [x] 1.1 ", "### [ ] 1.1 ");
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("task 1.1 has status [ ]"))
        );
    }

    #[test]
    fn guidance_audit_rejects_a_missing_audit_pass() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let plan = plan_fixture(profile, COMPLETE_STATUS).replace(
            "**Pass 2 — traceability:** checked\n",
            "traceability checked\n",
        );
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("intent-audit marker"))
        );
    }

    #[test]
    fn guidance_audit_rejects_duplicate_task_ids() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-foundation");
        let plan = plan_fixture(profile, COMPLETE_STATUS).replace(
            "### [x] 1.1 task\n",
            "### [x] 1.1 task\n\n### [x] 1.1 duplicate\n",
        );
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate implementation task 1.1"))
        );
    }

    #[test]
    fn guidance_audit_rejects_unknown_plan_ids() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-phase-2");
        let plan =
            plan_fixture(profile, IN_PROGRESS_STATUS).replace("`poche-phase-2`", "`unknown-plan`");
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown plan ID `unknown-plan`"))
        );
    }

    #[test]
    fn guidance_audit_cannot_misidentify_a_plan_as_another_profile() {
        let profiles = load_audit_profiles().unwrap();
        let profile = profile(&profiles, "poche-phase-2");
        let plan = plan_fixture(profile, IN_PROGRESS_STATUS)
            .replace("`poche-phase-2`", "`poche-foundation`");
        let errors = audit_guidance_plan(&plan).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("does not match profile"))
        );
    }

    #[test]
    fn audit_contract_rejects_noncontiguous_registered_ids() {
        let contract =
            super::PLAN_AUDIT_CONTRACTS[1].replace("guidance_ids=30-50", "guidance_ids=30||32");
        let error = parse_audit_profile(&contract).unwrap_err();
        assert!(error.contains("noncontiguous or duplicate guidance_ids"));
    }

    fn profile<'a>(profiles: &'a [AuditProfile], plan_id: &str) -> &'a AuditProfile {
        profiles
            .iter()
            .find(|profile| profile.plan_id == plan_id)
            .unwrap()
    }

    fn plan_fixture(profile: &AuditProfile, status: &str) -> String {
        let mut plan = format!("{}\n\n", profile.title);
        if profile.plan_id_required {
            writeln!(plan, "**Plan ID:** `{}`\n", profile.plan_id)
                .expect("writing to a String cannot fail");
        }
        writeln!(
            plan,
            "**Plan status:** {status}\n\n{REQUIRED_READY_SENTENCE}\n"
        )
        .expect("writing to a String cannot fail");
        plan.push_str("## Authoritative user guidance ledger\n\n");
        for number in &profile.guidance_ids {
            writeln!(
                plan,
                "| U{number} | active guidance {number} | consequence {number} |\n"
            )
            .expect("writing to a String cannot fail");
        }
        plan.push_str("\n## Intent audit evidence\n\n");
        for marker in REQUIRED_TRIPLE_AUDIT_MARKERS {
            plan.push_str(marker);
            plan.push(' ');
            plan.push_str("checked\n");
        }
        for marker in &profile.adversarial_markers {
            plan.push_str(marker);
            plan.push('\n');
        }
        plan.push_str("**Known source limitation:** None\n\n");

        writeln!(plan, "{}\n", profile.gate_section).expect("writing to a String cannot fail");
        let gate_status = if status == COMPLETE_STATUS {
            &profile.complete_gate_statuses[0]
        } else {
            &profile.working_gate_statuses[0]
        };
        for number in &profile.gate_ids {
            let mut cells = vec![format!("G{number}"), gate_status.clone()];
            for index in 2..profile.gate_cells {
                cells.push(format!("field {index}"));
            }
            writeln!(plan, "| {} |", cells.join(" | ")).expect("writing to a String cannot fail");
        }

        plan.push_str("\n## Guidance traceability\n\n");
        for number in &profile.guidance_ids {
            writeln!(plan, "| U{number} | task {number} |\n")
                .expect("writing to a String cannot fail");
        }

        plan.push_str("\n## Implementation\n\n");
        for (index, id) in profile.task_ids.iter().enumerate() {
            let task_status = if status == COMPLETE_STATUS {
                "x"
            } else if status == IN_PROGRESS_STATUS && index == 0 {
                "~"
            } else {
                " "
            };
            writeln!(plan, "### [{task_status}] {id} task\n")
                .expect("writing to a String cannot fail");
            if profile.require_task_criteria {
                plan.push_str("\n**Completion criteria:** checked when complete.\n");
            }
            let note = match task_status {
                "x" => "done",
                "~" => "In progress",
                _ => "Not started",
            };
            writeln!(plan, "\n**Completion notes (2026-08-04):** {note}\n")
                .expect("writing to a String cannot fail");
        }
        for marker in &profile.deferred_sections {
            plan.push_str(marker);
            plan.push_str("\n\n### Future work\n\nNot current.\n\n");
        }

        plan.push_str("## Overall completion criteria\n\n");
        let overall_status = if status == COMPLETE_STATUS { "x" } else { " " };
        for number in 1..=profile.overall_criteria {
            writeln!(plan, "- [{overall_status}] criterion {number}")
                .expect("writing to a String cannot fail");
        }
        plan
    }
}
