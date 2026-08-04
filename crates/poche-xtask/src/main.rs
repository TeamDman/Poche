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
         cargo run -p poche-xtask -- oracle check rust|alloy|nusmv"
    );
}

fn oracle(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let check = args.next();
    let backend = args.next();
    if check.as_deref() != Some(OsStr::new("check")) || args.next().is_some() {
        usage();
        return ExitCode::from(2);
    }
    match backend.as_deref() {
        Some(value) if value == OsStr::new("rust") => check_rust_oracle(),
        Some(value) if value == OsStr::new("alloy") => check_alloy_oracle(),
        Some(value) if value == OsStr::new("nusmv") => check_nusmv_oracle(),
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

fn check_nusmv_oracle() -> ExitCode {
    let tool = Tool {
        label: "NuSMV",
        override_var: Some("NUSMV_BIN"),
        commands: &["NuSMV", "NuSMV.exe", "nusmv"],
        version_args: &["-h"],
        accept_nonzero_with: Some("NuSMV"),
    };
    let command = match probe(&tool) {
        Probe::Available { command, .. } => command,
        Probe::Missing => {
            eprintln!("NuSMV is unavailable; set NUSMV_BIN or add NuSMV to PATH");
            return ExitCode::FAILURE;
        }
        Probe::Failed { command, detail } => {
            eprintln!(
                "NuSMV probe failed for {}: {detail}",
                command.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model = root.join("models/nusmv/poche.smv");
    let evidence_dir = root.join("target/nusmv-oracle");
    if let Err(error) = fs::create_dir_all(&evidence_dir) {
        eprintln!("failed to create {}: {error}", evidence_dir.display());
        return ExitCode::FAILURE;
    }

    let output = match Command::new(command)
        .current_dir(&root)
        .arg("-coi")
        .arg(&model)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("failed to execute NuSMV: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let transcript = format!("{stdout}\n{stderr}");
    let native_log = evidence_dir.join("native.log");
    if let Err(error) = fs::write(&native_log, &transcript) {
        eprintln!("failed to write {}: {error}", native_log.display());
        return ExitCode::FAILURE;
    }

    let result_lines: Vec<&str> = transcript
        .lines()
        .filter(|line| line.starts_with("-- specification") || line.starts_with("-- invariant"))
        .collect();
    let normalized = result_lines.join("\n") + "\n";
    let normalized_log = evidence_dir.join("normalized-results.txt");
    if let Err(error) = fs::write(&normalized_log, normalized) {
        eprintln!("failed to write {}: {error}", normalized_log.display());
        return ExitCode::FAILURE;
    }

    if !output.status.success() {
        eprintln!(
            "NuSMV oracle failed with {}; transcript preserved at {}",
            output.status,
            native_log.display()
        );
        return ExitCode::FAILURE;
    }
    if result_lines.len() < 40 {
        eprintln!(
            "NuSMV reported only {} properties; expected at least 40; transcript: {}",
            result_lines.len(),
            native_log.display()
        );
        return ExitCode::FAILURE;
    }
    if result_lines.iter().any(|line| !line.ends_with("is true")) {
        eprintln!(
            "NuSMV reported a false property; counterexample preserved at {}",
            native_log.display()
        );
        return ExitCode::FAILURE;
    }

    println!(
        "NuSMV oracle: passed; {} exhaustive properties; normalized evidence in {}",
        result_lines.len(),
        normalized_log.display()
    );
    ExitCode::SUCCESS
}

fn check_alloy_oracle() -> ExitCode {
    let tool = Tool {
        label: "Alloy",
        override_var: Some("ALLOY_BIN"),
        commands: &["alloy", "alloy.exe"],
        version_args: &["version"],
        accept_nonzero_with: None,
    };
    let command = match probe(&tool) {
        Probe::Available { command, .. } => command,
        Probe::Missing => {
            eprintln!("Alloy is unavailable; set ALLOY_BIN or add alloy to PATH");
            return ExitCode::FAILURE;
        }
        Probe::Failed { command, detail } => {
            eprintln!(
                "Alloy probe failed for {}: {detail}",
                command.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model = root.join("models/alloy/poche.als");
    let output_dir = root.join("target/alloy-oracle");
    let output = match Command::new(command)
        .current_dir(&root)
        .arg("exec")
        .args(["-c", "*", "-t", "none", "-o"])
        .arg(&output_dir)
        .args(["-f", "-n"])
        .arg(&model)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("failed to execute Alloy: {error}");
            return ExitCode::FAILURE;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let transcript = format!("{stdout}\n{stderr}");
    if !output.status.success() {
        eprintln!(
            "Alloy oracle failed with {}\n{stdout}\n{stderr}",
            output.status
        );
        return ExitCode::FAILURE;
    }

    let witnesses = [
        "CompleteRoundWitness",
        "UnrestrictedBidWitness",
        "ZeroBidSuccessWitness",
        "SharedWinnerWitness",
        "FirstJackWitness",
        "RepeatedHighCardWitness",
        "ParameterBoundaryWitness",
    ];
    let assertions = [
        "CompleteDeckIsExactly52",
        "CardConservationAndPartition",
        "FollowSuitIsEnforced",
        "DealerBidsLastInClockwiseOrder",
        "WinnerIsEligibleAndHighest",
        "ScoreAndPaymentAgree",
        "ScheduleBoundariesAndFeasibility",
        "FinalWinnersAreExactlyTheMaxima",
    ];

    for name in witnesses {
        let Some(line) = transcript.lines().find(|line| line.contains(name)) else {
            eprintln!("Alloy receipt output omitted witness {name}\n{transcript}");
            return ExitCode::FAILURE;
        };
        if !line.contains("SAT") || line.contains("UNSAT") {
            eprintln!("Alloy witness {name} was not satisfiable: {line}");
            return ExitCode::FAILURE;
        }
    }
    for name in assertions {
        let Some(line) = transcript.lines().find(|line| line.contains(name)) else {
            eprintln!("Alloy receipt output omitted assertion {name}\n{transcript}");
            return ExitCode::FAILURE;
        };
        if !line.contains("UNSAT") {
            eprintln!("Alloy found a counterexample to {name}: {line}");
            return ExitCode::FAILURE;
        }
    }

    let receipt = output_dir.join("receipt.json");
    if !receipt.is_file() {
        eprintln!("Alloy did not create {}", receipt.display());
        return ExitCode::FAILURE;
    }

    println!(
        "Alloy oracle: passed; {} SAT witnesses, {} UNSAT assertion checks; scopes recorded in {}",
        witnesses.len(),
        assertions.len(),
        receipt.display()
    );
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
