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

use poche_check::{CheckScope, TerminationReason, analyze_liveness, explore};
use poche_conformance::{
    Disposition, compare_rust_alloy, compare_rust_models, compare_rust_nusmv, compare_rust_prolog,
};
use poche_native_tools::{NativeBackend, NativeDisposition, run_all, run_backend};
use poche_oracle_rust::{Action, DeckOrder, Game, GameState, Seat, Turn};

const COVERAGE_TRACKS: [(&str, usize); 4] =
    [("rust", 3), ("alloy", 4), ("nusmv", 5), ("prolog", 6)];

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
    use super::{first_nonempty_line, is_disposition, is_source_anchor, markdown_cells};

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
}
