// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};

use crate::normalize::{
    NormalizedRun, normalize_alloy, normalize_alloy_commands, normalize_nusmv, normalize_prolog,
};
use crate::{
    AlloyCommandExpectation, AlloySuiteReport, NativeBackend, NativeDisposition, NativeReport,
    NuSmvCounterexample, NuSmvFsmDiagnostics, NuSmvPropertyExpectation, NuSmvPropertyKind,
    NuSmvPropertyResult, NuSmvSuiteReport, NuSmvTraceState, PrologFixtureReport, RawInvocation,
    evaluate_fixtures,
};

struct ToolSpec {
    override_variable: &'static str,
    candidates: &'static [&'static str],
    version_arguments: &'static [&'static str],
    accept_nonzero_marker: Option<&'static str>,
}

// Alloy writes fixed receipt/instance names under a caller-selected suite
// directory. Multiple conformance tests intentionally reuse a suite ID while
// composing individual and aggregate gates, so serialize these native runs
// inside one test/process rather than letting them remove or replace each
// other's receipt mid-read.
static ALLOY_SUITE_LOCK: Mutex<()> = Mutex::new(());

/// Run all three installed native tools in a stable order.
#[must_use]
pub fn run_all(root: &Path) -> Vec<NativeReport> {
    [
        NativeBackend::Alloy,
        NativeBackend::NuSmv,
        NativeBackend::ScryerProlog,
    ]
    .into_iter()
    .map(|backend| run_backend(root, backend))
    .collect()
}

/// Execute one explicitly named bounded Alloy command suite.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_alloy_suite(
    root: &Path,
    suite_id: &str,
    model_path: &Path,
    expectations: &[AlloyCommandExpectation],
) -> AlloySuiteReport {
    let _alloy_suite_guard = ALLOY_SUITE_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let evidence_directory = root.join("target").join(suite_id);
    let valid_suite_id = !suite_id.is_empty()
        && suite_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid_suite_id || expectations.is_empty() {
        return alloy_suite_without_raw(
            suite_id,
            NativeDisposition::Failure,
            "suite ID is invalid or command expectations are empty".to_owned(),
            evidence_directory,
        );
    }
    if let Err(error) = fs::create_dir_all(&evidence_directory) {
        return alloy_suite_without_raw(
            suite_id,
            NativeDisposition::Failure,
            format!("could not create evidence directory: {error}"),
            evidence_directory,
        );
    }
    let model = if model_path.is_absolute() {
        model_path.to_owned()
    } else {
        root.join(model_path)
    };
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            return alloy_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!("could not canonicalize repository root: {error}"),
                evidence_directory,
            );
        }
    };
    let canonical_model = match fs::canonicalize(&model) {
        Ok(path) if path.starts_with(&canonical_root) => path,
        Ok(_) => {
            return alloy_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                "Alloy suite model must remain inside the repository".to_owned(),
                evidence_directory,
            );
        }
        Err(error) => {
            return alloy_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!(
                    "could not resolve Alloy suite model {}: {error}",
                    model.display()
                ),
                evidence_directory,
            );
        }
    };
    let spec = tool_spec(NativeBackend::Alloy);
    let (program, version) = match resolve_tool(&spec) {
        Ok(found) => found,
        Err(error) => {
            return alloy_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                error,
                evidence_directory,
            );
        }
    };
    let arguments = vec![
        OsString::from("exec"),
        OsString::from("-c"),
        OsString::from("*"),
        OsString::from("-t"),
        OsString::from("none"),
        OsString::from("-o"),
        evidence_directory.as_os_str().to_owned(),
        OsString::from("-f"),
        OsString::from("-n"),
        external_tool_path(&canonical_model),
    ];
    let output = match Command::new(&program)
        .current_dir(root)
        .args(&arguments)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return alloy_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!("failed to launch Alloy: {error}"),
                evidence_directory,
            );
        }
    };
    let raw = RawInvocation {
        program: program.to_string_lossy().into_owned(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        version,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    if let Err(error) = preserve_raw(&evidence_directory, &raw, &output.stdout, &output.stderr) {
        return AlloySuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve raw evidence: {error}"),
            results: Vec::new(),
            raw: Some(raw),
            evidence_directory,
        };
    }
    let transcript = format!("{}\n{}", raw.stdout, raw.stderr);
    let expected_kinds = expectations
        .iter()
        .map(|expected| (expected.name.as_str(), expected.kind))
        .collect::<Vec<_>>();
    let mut results = match normalize_alloy_commands(&transcript, &expected_kinds) {
        Ok(NormalizedRun::Alloy(results)) => results,
        Ok(_) => unreachable!("Alloy parser returns Alloy evidence"),
        Err(error) => {
            return AlloySuiteReport {
                suite_id: suite_id.to_owned(),
                disposition: if output.status.success() {
                    NativeDisposition::Unknown
                } else {
                    NativeDisposition::Failure
                },
                diagnostic: error,
                results: Vec::new(),
                raw: Some(raw),
                evidence_directory,
            };
        }
    };
    let receipt_path = evidence_directory.join("receipt.json");
    let receipt = match fs::read_to_string(&receipt_path) {
        Ok(receipt) => receipt,
        Err(error) => {
            return AlloySuiteReport {
                suite_id: suite_id.to_owned(),
                disposition: NativeDisposition::Unknown,
                diagnostic: format!("could not read {}: {error}", receipt_path.display()),
                results,
                raw: Some(raw),
                evidence_directory,
            };
        }
    };
    for result in &mut results {
        match extract_alloy_command_source(&receipt, &result.name) {
            Ok(source) => result.command_source = Some(source),
            Err(error) => {
                return AlloySuiteReport {
                    suite_id: suite_id.to_owned(),
                    disposition: NativeDisposition::Unknown,
                    diagnostic: error,
                    results,
                    raw: Some(raw),
                    evidence_directory,
                };
            }
        }
    }
    if let Err(error) =
        preserve_normalized(&evidence_directory, &NormalizedRun::Alloy(results.clone()))
    {
        return AlloySuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve normalized results: {error}"),
            results,
            raw: Some(raw),
            evidence_directory,
        };
    }
    let mismatch = expectations.iter().find(|expected| {
        !results.iter().any(|result| {
            result.name == expected.name
                && result.kind == expected.kind
                && result.outcome == expected.outcome
        })
    });
    let (disposition, diagnostic) = if !output.status.success() {
        (
            NativeDisposition::Failure,
            format!("Alloy exited with {}", output.status),
        )
    } else if let Some(expected) = mismatch {
        (
            NativeDisposition::Failure,
            format!(
                "{} did not have expected {:?} {:?}",
                expected.name, expected.kind, expected.outcome
            ),
        )
    } else {
        (
            NativeDisposition::Success,
            format!(
                "recognized {} expected bounded results with receipt scopes",
                results.len()
            ),
        )
    };
    AlloySuiteReport {
        suite_id: suite_id.to_owned(),
        disposition,
        diagnostic,
        results,
        raw: Some(raw),
        evidence_directory,
    }
}

fn alloy_suite_without_raw(
    suite_id: &str,
    disposition: NativeDisposition,
    diagnostic: String,
    evidence_directory: PathBuf,
) -> AlloySuiteReport {
    AlloySuiteReport {
        suite_id: suite_id.to_owned(),
        disposition,
        diagnostic,
        results: Vec::new(),
        raw: None,
        evidence_directory,
    }
}

/// Execute one named `NuSMV` suite with property catalog, counterexample, and
/// explicit FSM-totality capture.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_nusmv_suite(
    root: &Path,
    suite_id: &str,
    model_path: &Path,
    expectations: &[NuSmvPropertyExpectation],
) -> NuSmvSuiteReport {
    let evidence_directory = root.join("target").join(suite_id);
    let valid_suite_id = !suite_id.is_empty()
        && suite_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid_suite_id || expectations.is_empty() {
        return nusmv_suite_without_raw(
            suite_id,
            NativeDisposition::Failure,
            "suite ID is invalid or property expectations are empty".to_owned(),
            evidence_directory,
        );
    }
    if let Err(error) = fs::create_dir_all(&evidence_directory) {
        return nusmv_suite_without_raw(
            suite_id,
            NativeDisposition::Failure,
            format!("could not create evidence directory: {error}"),
            evidence_directory,
        );
    }
    let model = if model_path.is_absolute() {
        model_path.to_owned()
    } else {
        root.join(model_path)
    };
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!("could not canonicalize repository root: {error}"),
                evidence_directory,
            );
        }
    };
    let canonical_model = match fs::canonicalize(&model) {
        Ok(path) if path.starts_with(&canonical_root) => path,
        Ok(_) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                "NuSMV suite model must remain inside the repository".to_owned(),
                evidence_directory,
            );
        }
        Err(error) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!(
                    "could not resolve NuSMV suite model {}: {error}",
                    model.display()
                ),
                evidence_directory,
            );
        }
    };
    let source_property_count = match native_property_count(&canonical_model) {
        Ok(count) if count == expectations.len() => count,
        Ok(count) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!(
                    "NuSMV source has {count} properties, but {} were expected",
                    expectations.len()
                ),
                evidence_directory,
            );
        }
        Err(error) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                error,
                evidence_directory,
            );
        }
    };
    let spec = tool_spec(NativeBackend::NuSmv);
    let (program, version) = match resolve_tool(&spec) {
        Ok(found) => found,
        Err(error) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                error,
                evidence_directory,
            );
        }
    };
    let script_path = evidence_directory.join("commands.txt");
    let model_argument = external_tool_path(&canonical_model)
        .to_string_lossy()
        .replace('\\', "/");
    let script = format!(
        "read_model -i \"{model_argument}\"\ngo\nshow_property\ncheck_fsm\ncheck_property\nquit\n"
    );
    if let Err(error) = fs::write(&script_path, script) {
        return nusmv_suite_without_raw(
            suite_id,
            NativeDisposition::Failure,
            format!("could not write NuSMV command script: {error}"),
            evidence_directory,
        );
    }
    let arguments = vec![OsString::from("-source"), external_tool_path(&script_path)];
    let output = match Command::new(&program)
        .current_dir(root)
        .args(&arguments)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return nusmv_suite_without_raw(
                suite_id,
                NativeDisposition::Failure,
                format!("failed to launch NuSMV: {error}"),
                evidence_directory,
            );
        }
    };
    let raw = RawInvocation {
        program: program.to_string_lossy().into_owned(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        version,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    if let Err(error) = preserve_raw(&evidence_directory, &raw, &output.stdout, &output.stderr) {
        return NuSmvSuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve raw evidence: {error}"),
            results: Vec::new(),
            counterexamples: Vec::new(),
            fsm: None,
            raw: Some(raw),
            evidence_directory,
        };
    }
    let transcript = format!("{}\n{}", raw.stdout, raw.stderr);
    let parsed = parse_nusmv_suite(&transcript);
    let (results, counterexamples, fsm) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            return NuSmvSuiteReport {
                suite_id: suite_id.to_owned(),
                disposition: if output.status.success() {
                    NativeDisposition::Unknown
                } else {
                    NativeDisposition::Failure
                },
                diagnostic: error,
                results: Vec::new(),
                counterexamples: Vec::new(),
                fsm: None,
                raw: Some(raw),
                evidence_directory,
            };
        }
    };
    if results.len() != source_property_count {
        return NuSmvSuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Unknown,
            diagnostic: format!(
                "NuSMV returned {} named results for {source_property_count} source properties",
                results.len()
            ),
            results,
            counterexamples,
            fsm: Some(fsm),
            raw: Some(raw),
            evidence_directory,
        };
    }
    let expected = expectations
        .iter()
        .map(|item| (item.name.as_str(), (item.kind, item.holds)))
        .collect::<BTreeMap<_, _>>();
    if expected.len() != expectations.len() {
        return NuSmvSuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: "duplicate NuSMV expectation name".to_owned(),
            results,
            counterexamples,
            fsm: Some(fsm),
            raw: Some(raw),
            evidence_directory,
        };
    }
    let actual = results
        .iter()
        .filter_map(|item| {
            item.name
                .as_deref()
                .map(|name| (name, (item.kind, item.holds)))
        })
        .collect::<BTreeMap<_, _>>();
    let mismatch = expected != actual;
    let false_names = expectations
        .iter()
        .filter(|item| !item.holds)
        .map(|item| item.name.as_str())
        .collect::<BTreeSet<_>>();
    let trace_names = counterexamples
        .iter()
        .map(|trace| trace.property_name.as_str())
        .collect::<BTreeSet<_>>();
    let trace_mismatch = false_names != trace_names;
    if let Err(error) =
        preserve_normalized(&evidence_directory, &NormalizedRun::NuSmv(results.clone()))
            .and_then(|()| preserve_nusmv_traces(&evidence_directory, &counterexamples, &fsm))
    {
        return NuSmvSuiteReport {
            suite_id: suite_id.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve normalized NuSMV evidence: {error}"),
            results,
            counterexamples,
            fsm: Some(fsm),
            raw: Some(raw),
            evidence_directory,
        };
    }
    let (disposition, diagnostic) = if !output.status.success() {
        (
            NativeDisposition::Failure,
            format!("NuSMV exited with {}", output.status),
        )
    } else if mismatch {
        (
            NativeDisposition::Failure,
            "named NuSMV property results differ from expectations".to_owned(),
        )
    } else if trace_mismatch {
        (
            NativeDisposition::Failure,
            format!(
                "NuSMV counterexample inventory differs: expected={false_names:?}, actual={trace_names:?}"
            ),
        )
    } else {
        (
            NativeDisposition::Success,
            format!(
                "recognized {} named properties, {} counterexamples, and check_fsm diagnostics",
                results.len(),
                counterexamples.len()
            ),
        )
    };
    NuSmvSuiteReport {
        suite_id: suite_id.to_owned(),
        disposition,
        diagnostic,
        results,
        counterexamples,
        fsm: Some(fsm),
        raw: Some(raw),
        evidence_directory,
    }
}

fn nusmv_suite_without_raw(
    suite_id: &str,
    disposition: NativeDisposition,
    diagnostic: String,
    evidence_directory: PathBuf,
) -> NuSmvSuiteReport {
    NuSmvSuiteReport {
        suite_id: suite_id.to_owned(),
        disposition,
        diagnostic,
        results: Vec::new(),
        counterexamples: Vec::new(),
        fsm: None,
        raw: None,
        evidence_directory,
    }
}

fn external_tool_path(path: &Path) -> OsString {
    let text = path.as_os_str().to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        OsString::from(format!(r"\\{unc}"))
    } else if let Some(drive_path) = text.strip_prefix(r"\\?\") {
        OsString::from(drive_path)
    } else {
        path.as_os_str().to_owned()
    }
}

/// Execute one finite native Prolog conformance fixture.
///
/// The supplied names are restricted to lowercase ASCII identifiers so they
/// can select a handwritten predicate mode without becoming executable source.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_prolog_fixture(root: &Path, fixture_id: &str, native_goal: &str) -> PrologFixtureReport {
    run_prolog_model_fixture(
        root,
        fixture_id,
        Path::new("models/prolog/poche.pl"),
        "poche",
        native_goal,
    )
}

/// Execute one finite fixture from a repository-local handwritten Prolog model.
///
/// The module and goal are restricted to lowercase identifiers and the model
/// must canonicalize beneath `root`; none of these values becomes arbitrary
/// executable Prolog source.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_prolog_model_fixture(
    root: &Path,
    fixture_id: &str,
    model_path: &Path,
    module: &str,
    native_goal: &str,
) -> PrologFixtureReport {
    let evidence_directory = root.join("target/prolog-conformance").join(fixture_id);
    let invalid_fixture = fixture_id.is_empty()
        || !fixture_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let invalid_goal = native_goal.is_empty()
        || !native_goal
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_');
    let invalid_module = module.is_empty()
        || !module
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_');
    if invalid_fixture || invalid_goal || invalid_module {
        return PrologFixtureReport {
            fixture_id: fixture_id.to_owned(),
            native_goal: native_goal.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: "fixture ID or native goal contains unsupported characters".to_owned(),
            answers: BTreeSet::new(),
            raw: None,
            evidence_directory,
        };
    }
    if let Err(error) = fs::create_dir_all(&evidence_directory) {
        return prolog_fixture_without_raw(
            fixture_id,
            native_goal,
            NativeDisposition::Failure,
            format!("could not create evidence directory: {error}"),
            evidence_directory,
        );
    }
    let spec = tool_spec(NativeBackend::ScryerProlog);
    let (program, version) = match resolve_tool(&spec) {
        Ok(found) => found,
        Err(error) => {
            return prolog_fixture_without_raw(
                fixture_id,
                native_goal,
                NativeDisposition::Failure,
                error,
                evidence_directory,
            );
        }
    };
    let model = if model_path.is_absolute() {
        model_path.to_owned()
    } else {
        root.join(model_path)
    };
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            return prolog_fixture_without_raw(
                fixture_id,
                native_goal,
                NativeDisposition::Failure,
                format!("could not canonicalize repository root: {error}"),
                evidence_directory,
            );
        }
    };
    let model = match fs::canonicalize(&model) {
        Ok(path) if path.starts_with(&canonical_root) => path,
        Ok(_) => {
            return prolog_fixture_without_raw(
                fixture_id,
                native_goal,
                NativeDisposition::Failure,
                "Prolog fixture model must remain inside the repository".to_owned(),
                evidence_directory,
            );
        }
        Err(error) => {
            return prolog_fixture_without_raw(
                fixture_id,
                native_goal,
                NativeDisposition::Failure,
                format!("could not resolve Prolog fixture model: {error}"),
                evidence_directory,
            );
        }
    };
    let goal = format!("{module}:run_conformance_fixture({native_goal}),halt");
    let arguments = vec![
        OsString::from("-f"),
        model.as_os_str().to_owned(),
        OsString::from("-g"),
        OsString::from(&goal),
    ];
    let output = match Command::new(&program)
        .current_dir(root)
        .args(&arguments)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return prolog_fixture_without_raw(
                fixture_id,
                native_goal,
                NativeDisposition::Failure,
                format!("failed to launch Scryer Prolog: {error}"),
                evidence_directory,
            );
        }
    };
    let raw = RawInvocation {
        program: program.to_string_lossy().into_owned(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        version,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    if let Err(error) = preserve_raw(&evidence_directory, &raw, &output.stdout, &output.stderr) {
        return PrologFixtureReport {
            fixture_id: fixture_id.to_owned(),
            native_goal: native_goal.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve raw evidence: {error}"),
            answers: BTreeSet::new(),
            raw: Some(raw),
            evidence_directory,
        };
    }
    let transcript = format!("{}\n{}", raw.stdout, raw.stderr);
    let answers = match normalize_prolog_fixture(native_goal, &transcript) {
        Ok(answers) => answers,
        Err(error) => {
            return PrologFixtureReport {
                fixture_id: fixture_id.to_owned(),
                native_goal: native_goal.to_owned(),
                disposition: if output.status.success() {
                    NativeDisposition::Unknown
                } else {
                    NativeDisposition::Failure
                },
                diagnostic: error,
                answers: BTreeSet::new(),
                raw: Some(raw),
                evidence_directory,
            };
        }
    };
    if let Err(error) = fs::write(
        evidence_directory.join("normalized-answers.txt"),
        answers.iter().fold(String::new(), |mut output, answer| {
            writeln!(output, "{answer}").expect("writing to String cannot fail");
            output
        }),
    ) {
        return PrologFixtureReport {
            fixture_id: fixture_id.to_owned(),
            native_goal: native_goal.to_owned(),
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve normalized answers: {error}"),
            answers,
            raw: Some(raw),
            evidence_directory,
        };
    }
    let disposition = if output.status.success() {
        NativeDisposition::Success
    } else {
        NativeDisposition::Failure
    };
    PrologFixtureReport {
        fixture_id: fixture_id.to_owned(),
        native_goal: native_goal.to_owned(),
        disposition,
        diagnostic: if disposition == NativeDisposition::Success {
            format!("recognized {} unique answer rows", answers.len())
        } else {
            format!("Scryer Prolog exited with {}", output.status)
        },
        answers,
        raw: Some(raw),
        evidence_directory,
    }
}

fn prolog_fixture_without_raw(
    fixture_id: &str,
    native_goal: &str,
    disposition: NativeDisposition,
    diagnostic: String,
    evidence_directory: PathBuf,
) -> PrologFixtureReport {
    PrologFixtureReport {
        fixture_id: fixture_id.to_owned(),
        native_goal: native_goal.to_owned(),
        disposition,
        diagnostic,
        answers: BTreeSet::new(),
        raw: None,
        evidence_directory,
    }
}

fn normalize_prolog_fixture(
    native_goal: &str,
    transcript: &str,
) -> Result<BTreeSet<String>, String> {
    let begin_marker = format!("POCHE_PROLOG_FIXTURE {native_goal} BEGIN");
    let end_prefix = format!("POCHE_PROLOG_FIXTURE {native_goal} END count=");
    let mut inside_fixture = false;
    let mut saw_end = false;
    let mut declared_count = None;
    let mut answers = BTreeSet::new();
    for line in transcript.lines().map(str::trim) {
        if line == begin_marker {
            if inside_fixture || saw_end {
                return Err("duplicate or misplaced Prolog fixture BEGIN marker".to_owned());
            }
            inside_fixture = true;
        } else if let Some(answer) = line.strip_prefix("POCHE_PROLOG_ANSWER ") {
            if !inside_fixture || saw_end || answer.is_empty() {
                return Err("misplaced or empty Prolog answer row".to_owned());
            }
            if !answers.insert(answer.to_owned()) {
                return Err(format!("duplicate Prolog answer row: {answer}"));
            }
        } else if let Some(count) = line.strip_prefix(&end_prefix) {
            if !inside_fixture || saw_end {
                return Err("duplicate or misplaced Prolog fixture END marker".to_owned());
            }
            declared_count = Some(
                count
                    .parse::<usize>()
                    .map_err(|_| format!("invalid Prolog answer count `{count}`"))?,
            );
            saw_end = true;
        } else if line.starts_with("POCHE_PROLOG_") {
            return Err(format!("unknown Prolog fixture protocol line: {line}"));
        }
    }
    if !inside_fixture || !saw_end || declared_count != Some(answers.len()) {
        return Err(format!(
            "incomplete Prolog fixture protocol: begin={inside_fixture}, end={saw_end}, declared={declared_count:?}, answers={}",
            answers.len()
        ));
    }
    Ok(answers)
}

/// Run one handwritten oracle and preserve both raw and normalized evidence.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_backend(root: &Path, backend: NativeBackend) -> NativeReport {
    let evidence_directory = root.join("target").join(backend.evidence_subdirectory());
    if let Err(error) = fs::create_dir_all(&evidence_directory) {
        return report_without_raw(
            backend,
            NativeDisposition::Failure,
            format!(
                "could not create evidence directory {}: {error}",
                evidence_directory.display()
            ),
            evidence_directory,
        );
    }

    let spec = tool_spec(backend);
    let (program, version) = match resolve_tool(&spec) {
        Ok(resolved) => resolved,
        Err(error) => {
            return report_without_raw(
                backend,
                NativeDisposition::Failure,
                error,
                evidence_directory,
            );
        }
    };
    let (model, arguments) = invocation(root, backend, &evidence_directory);
    if !model.is_file() {
        return report_without_raw(
            backend,
            NativeDisposition::Failure,
            format!("native model is missing: {}", model.display()),
            evidence_directory,
        );
    }

    let output = match Command::new(&program)
        .current_dir(root)
        .args(&arguments)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return report_without_raw(
                backend,
                NativeDisposition::Failure,
                format!("failed to launch {}: {error}", program.to_string_lossy()),
                evidence_directory,
            );
        }
    };

    let raw = RawInvocation {
        program: program.to_string_lossy().into_owned(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        version,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    if let Err(error) = preserve_raw(&evidence_directory, &raw, &output.stdout, &output.stderr) {
        return NativeReport {
            backend,
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve raw native evidence: {error}"),
            raw: Some(raw),
            normalized: None,
            evidence_directory,
        };
    }

    let transcript = format!("{}\n{}", raw.stdout, raw.stderr);
    let normalized = match normalize_backend(root, backend, &transcript, &evidence_directory) {
        Ok(normalized) => normalized,
        Err(error) => {
            let disposition = if output.status.success() {
                NativeDisposition::Unknown
            } else {
                NativeDisposition::Failure
            };
            return NativeReport {
                backend,
                disposition,
                diagnostic: format!(
                    "native output was not accepted: {error}; raw evidence: {}",
                    evidence_directory.display()
                ),
                raw: Some(raw),
                normalized: None,
                evidence_directory,
            };
        }
    };

    if let Err(error) = preserve_normalized(&evidence_directory, &normalized) {
        return NativeReport {
            backend,
            disposition: NativeDisposition::Failure,
            diagnostic: format!("failed to preserve normalized evidence: {error}"),
            raw: Some(raw),
            normalized: Some(normalized),
            evidence_directory,
        };
    }

    let evaluations = evaluate_fixtures(backend, &normalized);
    let unknown_fixtures = evaluations
        .iter()
        .filter(|evaluation| evaluation.disposition == NativeDisposition::Unknown)
        .count();
    let failed_fixtures = evaluations
        .iter()
        .filter(|evaluation| evaluation.disposition == NativeDisposition::Failure)
        .count();
    let (disposition, diagnostic) = if !output.status.success() {
        (
            NativeDisposition::Failure,
            format!("native process exited with {}", output.status),
        )
    } else if !normalized.all_passed() || failed_fixtures > 0 {
        (
            NativeDisposition::Failure,
            format!(
                "recognized {} native results, but at least one result or fixture failed",
                normalized.len()
            ),
        )
    } else if unknown_fixtures > 0 {
        (
            NativeDisposition::Unknown,
            format!("{unknown_fixtures} fixture adapters had no recognized native selector"),
        )
    } else {
        (
            NativeDisposition::Success,
            format!(
                "recognized {} passing native results and {} passing fixture adapters",
                normalized.len(),
                evaluations.len()
            ),
        )
    };

    NativeReport {
        backend,
        disposition,
        diagnostic,
        raw: Some(raw),
        normalized: Some(normalized),
        evidence_directory,
    }
}

fn report_without_raw(
    backend: NativeBackend,
    disposition: NativeDisposition,
    diagnostic: String,
    evidence_directory: PathBuf,
) -> NativeReport {
    NativeReport {
        backend,
        disposition,
        diagnostic,
        raw: None,
        normalized: None,
        evidence_directory,
    }
}

fn tool_spec(backend: NativeBackend) -> ToolSpec {
    match backend {
        NativeBackend::Alloy => ToolSpec {
            override_variable: "ALLOY_BIN",
            candidates: &["alloy", "alloy.exe"],
            version_arguments: &["version"],
            accept_nonzero_marker: None,
        },
        NativeBackend::NuSmv => ToolSpec {
            override_variable: "NUSMV_BIN",
            candidates: &["NuSMV", "NuSMV.exe", "nusmv"],
            version_arguments: &["-h"],
            accept_nonzero_marker: Some("NuSMV"),
        },
        NativeBackend::ScryerProlog => ToolSpec {
            override_variable: "SCRYER_PROLOG_BIN",
            candidates: &["scryer-prolog", "scryer-prolog.exe"],
            version_arguments: &["--version"],
            accept_nonzero_marker: None,
        },
    }
}

fn resolve_tool(spec: &ToolSpec) -> Result<(OsString, String), String> {
    if let Some(explicit) = env::var_os(spec.override_variable) {
        return probe_candidate(explicit, spec, true)?
            .ok_or_else(|| format!("{} points to a missing executable", spec.override_variable));
    }
    for candidate in spec.candidates {
        if let Some(found) = probe_candidate(OsString::from(candidate), spec, false)? {
            return Ok(found);
        }
    }
    Err(format!(
        "tool is unavailable; set {} or add {} to PATH",
        spec.override_variable,
        spec.candidates.join("/")
    ))
}

fn probe_candidate(
    program: OsString,
    spec: &ToolSpec,
    explicit: bool,
) -> Result<Option<(OsString, String)>, String> {
    match Command::new(&program).args(spec.version_arguments).output() {
        Ok(output) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let accepted = output.status.success()
                || spec
                    .accept_nonzero_marker
                    .is_some_and(|marker| combined.contains(marker));
            if !accepted {
                return Err(format!(
                    "version probe for {} exited with {}: {}",
                    program.to_string_lossy(),
                    output.status,
                    first_nonempty_line(&combined).unwrap_or("no version output")
                ));
            }
            Ok(Some((
                program,
                first_nonempty_line(&combined)
                    .unwrap_or("no version output")
                    .to_owned(),
            )))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound && !explicit => Ok(None),
        Err(error) => Err(format!(
            "version probe for {} failed: {error}",
            program.to_string_lossy()
        )),
    }
}

fn invocation(
    root: &Path,
    backend: NativeBackend,
    evidence_directory: &Path,
) -> (PathBuf, Vec<OsString>) {
    match backend {
        NativeBackend::Alloy => {
            let model = root.join("models/alloy/poche.als");
            (
                model.clone(),
                vec![
                    OsString::from("exec"),
                    OsString::from("-c"),
                    OsString::from("*"),
                    OsString::from("-t"),
                    OsString::from("none"),
                    OsString::from("-o"),
                    evidence_directory.as_os_str().to_owned(),
                    OsString::from("-f"),
                    OsString::from("-n"),
                    model.as_os_str().to_owned(),
                ],
            )
        }
        NativeBackend::NuSmv => {
            let model = root.join("models/nusmv/poche.smv");
            (
                model.clone(),
                vec![OsString::from("-coi"), model.as_os_str().to_owned()],
            )
        }
        NativeBackend::ScryerProlog => {
            let model = root.join("models/prolog/poche.pl");
            (
                model.clone(),
                vec![
                    OsString::from("-f"),
                    model.as_os_str().to_owned(),
                    OsString::from("-g"),
                    OsString::from("poche:run_oracle_tests,halt"),
                ],
            )
        }
    }
}

fn normalize_backend(
    root: &Path,
    backend: NativeBackend,
    transcript: &str,
    evidence_directory: &Path,
) -> Result<NormalizedRun, String> {
    match backend {
        NativeBackend::Alloy => {
            let mut normalized = normalize_alloy(transcript)?;
            let receipt_path = evidence_directory.join("receipt.json");
            let receipt = fs::read_to_string(&receipt_path)
                .map_err(|error| format!("could not read {}: {error}", receipt_path.display()))?;
            let NormalizedRun::Alloy(results) = &mut normalized else {
                unreachable!("Alloy parser returns Alloy evidence")
            };
            for result in results {
                result.command_source = Some(extract_alloy_command_source(&receipt, &result.name)?);
            }
            Ok(normalized)
        }
        NativeBackend::NuSmv => {
            let normalized = normalize_nusmv(transcript)?;
            let expected = native_property_count(&root.join("models/nusmv/poche.smv"))?;
            if normalized.len() != expected {
                return Err(format!(
                    "NuSMV returned {} results for {expected} source properties",
                    normalized.len()
                ));
            }
            Ok(normalized)
        }
        NativeBackend::ScryerProlog => normalize_prolog(transcript),
    }
}

fn native_property_count(path: &Path) -> Result<usize, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(source
        .lines()
        .map(str::trim_start)
        .filter(|line| {
            line.starts_with("INVARSPEC")
                || line.starts_with("CTLSPEC")
                || line.starts_with("LTLSPEC")
        })
        .count())
}

#[derive(Clone, Debug)]
struct NuSmvCatalogEntry {
    name: String,
    kind: NuSmvPropertyKind,
    expression: String,
}

#[derive(Clone, Debug)]
struct RawNuSmvTrace {
    expression: String,
    states: Vec<NuSmvTraceState>,
    loop_start: Option<usize>,
}

fn parse_nusmv_suite(
    transcript: &str,
) -> Result<
    (
        Vec<NuSmvPropertyResult>,
        Vec<NuSmvCounterexample>,
        NuSmvFsmDiagnostics,
    ),
    String,
> {
    let catalog = parse_nusmv_catalog(transcript)?;
    let NormalizedRun::NuSmv(mut results) = normalize_nusmv(transcript)? else {
        unreachable!("NuSMV parser returns NuSMV evidence")
    };
    if catalog.len() != results.len() {
        return Err(format!(
            "NuSMV property catalog has {} entries but {} results",
            catalog.len(),
            results.len()
        ));
    }
    let mut names = BTreeSet::new();
    let mut ordered_results = Vec::with_capacity(results.len());
    for entry in &catalog {
        if !names.insert(entry.name.as_str()) {
            return Err(format!("duplicate NuSMV property name: {}", entry.name));
        }
        let expression = collapse_spaces(&entry.expression);
        let matches = results
            .iter()
            .enumerate()
            .filter(|(_, result)| {
                entry.kind == result.kind && collapse_spaces(&result.expression) == expression
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [index] = matches.as_slice() else {
            return Err(format!(
                "NuSMV property catalog/result identity is not unique at {}: {:?}",
                entry.name, entry.expression
            ));
        };
        let mut result = results.remove(*index);
        result.name = Some(entry.name.clone());
        ordered_results.push(result);
    }
    if !results.is_empty() {
        return Err("NuSMV emitted results absent from its named catalog".to_owned());
    }
    let results = ordered_results;
    let raw_traces = parse_nusmv_counterexamples(transcript)?;
    let mut counterexamples = Vec::with_capacity(raw_traces.len());
    for trace in raw_traces {
        let expression = collapse_spaces(&trace.expression);
        let matches = catalog
            .iter()
            .filter(|entry| collapse_spaces(&entry.expression) == expression)
            .collect::<Vec<_>>();
        let [entry] = matches.as_slice() else {
            return Err(format!(
                "counterexample expression did not identify exactly one named property: {}",
                trace.expression
            ));
        };
        counterexamples.push(NuSmvCounterexample {
            property_name: entry.name.clone(),
            property_expression: trace.expression,
            states: trace.states,
            loop_start: trace.loop_start,
        });
    }
    Ok((results, counterexamples, parse_nusmv_fsm(transcript)?))
}

fn parse_nusmv_catalog(transcript: &str) -> Result<Vec<NuSmvCatalogEntry>, String> {
    let mut catalog = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for raw in transcript.lines() {
        let line = nusmv_payload(raw);
        if let Some((prefix, expression)) = line.split_once(':')
            && let Ok(index) = prefix.trim().parse::<usize>()
        {
            pending = Some((index, expression.trim().to_owned()));
            continue;
        }
        let Some((index, expression)) = pending.take_if(|_| line.starts_with('[')) else {
            continue;
        };
        let metadata = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .ok_or_else(|| format!("malformed NuSMV property metadata: {line}"))?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        let native_kind = fields
            .first()
            .ok_or_else(|| format!("empty NuSMV property metadata: {line}"))?;
        let name = fields
            .last()
            .filter(|name| **name != "N/A")
            .ok_or_else(|| format!("NuSMV property {index:03} has no stable name"))?;
        let kind = match *native_kind {
            "CTL" | "LTL" => NuSmvPropertyKind::Specification,
            "Invar" => NuSmvPropertyKind::Invariant,
            other => return Err(format!("unknown NuSMV catalog kind: {other}")),
        };
        if index != catalog.len() {
            return Err(format!(
                "NuSMV property IDs are not contiguous: expected {}, found {index}",
                catalog.len()
            ));
        }
        catalog.push(NuSmvCatalogEntry {
            name: (*name).to_owned(),
            kind,
            expression,
        });
    }
    if catalog.is_empty() {
        return Err("NuSMV show_property emitted no named catalog".to_owned());
    }
    Ok(catalog)
}

fn parse_nusmv_counterexamples(transcript: &str) -> Result<Vec<RawNuSmvTrace>, String> {
    let mut traces = Vec::new();
    let mut current: Option<RawNuSmvTrace> = None;
    let mut inside_state = false;
    let mut pending_loop = false;
    for raw in transcript.lines() {
        let line = nusmv_payload(raw);
        if let Some((_kind, expression, holds)) = parse_nusmv_result_line(line)? {
            finish_raw_trace(&mut traces, current.take())?;
            inside_state = false;
            pending_loop = false;
            if !holds {
                current = Some(RawNuSmvTrace {
                    expression,
                    states: Vec::new(),
                    loop_start: None,
                });
            }
            continue;
        }
        let Some(trace) = &mut current else {
            continue;
        };
        if line == "-- Loop starts here" {
            pending_loop = true;
            continue;
        }
        if line.contains("-> State:") {
            if pending_loop {
                if trace.loop_start.replace(trace.states.len()).is_some() {
                    return Err("NuSMV trace contains multiple loop markers".to_owned());
                }
                pending_loop = false;
            }
            let assignments = trace
                .states
                .last()
                .map_or_else(BTreeMap::new, |state| state.assignments.clone());
            trace.states.push(NuSmvTraceState { assignments });
            inside_state = true;
            continue;
        }
        if line.contains("-> Input:") {
            inside_state = false;
            continue;
        }
        if inside_state && let Some((name, value)) = parse_assignment(line) {
            let state = trace
                .states
                .last_mut()
                .ok_or_else(|| "NuSMV assignment preceded its state".to_owned())?;
            state.assignments.insert(name, value);
        }
    }
    finish_raw_trace(&mut traces, current)?;
    Ok(traces)
}

fn finish_raw_trace(
    traces: &mut Vec<RawNuSmvTrace>,
    trace: Option<RawNuSmvTrace>,
) -> Result<(), String> {
    if let Some(trace) = trace {
        if trace.states.is_empty() {
            return Err(format!(
                "false NuSMV property had no counterexample states: {}",
                trace.expression
            ));
        }
        traces.push(trace);
    }
    Ok(())
}

fn parse_nusmv_result_line(
    line: &str,
) -> Result<Option<(NuSmvPropertyKind, String, bool)>, String> {
    let parsed = if let Some(rest) = line.strip_prefix("-- specification") {
        Some((NuSmvPropertyKind::Specification, rest))
    } else {
        line.strip_prefix("-- invariant")
            .map(|rest| (NuSmvPropertyKind::Invariant, rest))
    };
    let Some((kind, rest)) = parsed else {
        return Ok(None);
    };
    let Some((expression, value)) = rest.rsplit_once(" is ") else {
        return Err(format!("malformed NuSMV result line: {line}"));
    };
    let holds = match value.trim() {
        "true" => true,
        "false" => false,
        other => return Err(format!("unknown NuSMV truth value `{other}`")),
    };
    Ok(Some((kind, expression.trim().to_owned(), holds)))
}

fn parse_nusmv_fsm(transcript: &str) -> Result<NuSmvFsmDiagnostics, String> {
    let total_and_deadlock_free =
        transcript.contains("The transition relation is total: No deadlock state exists");
    let transition_total = if transcript.contains("The transition relation is not total.") {
        false
    } else if total_and_deadlock_free || transcript.contains("The transition relation is total.") {
        true
    } else {
        return Err("NuSMV check_fsm omitted transition-totality result".to_owned());
    };
    let deadlock_free = if transcript.contains("transition relation is not deadlock-free.") {
        false
    } else if total_and_deadlock_free
        || transcript.contains("transition relation is deadlock-free.")
    {
        true
    } else {
        return Err("NuSMV check_fsm omitted deadlock result".to_owned());
    };
    let mut deadlock_state = None;
    let mut inside = false;
    for raw in transcript.lines() {
        let line = nusmv_payload(raw);
        if line == "A deadlock state is:" {
            inside = true;
            deadlock_state = Some(BTreeMap::new());
            continue;
        }
        if inside && line.starts_with("###") {
            break;
        }
        if inside && let Some((name, value)) = parse_assignment(line) {
            deadlock_state
                .as_mut()
                .expect("deadlock map was initialized")
                .insert(name, value);
        }
    }
    if !deadlock_free && deadlock_state.as_ref().is_none_or(BTreeMap::is_empty) {
        return Err("NuSMV reported a deadlock without a state assignment".to_owned());
    }
    Ok(NuSmvFsmDiagnostics {
        transition_total,
        deadlock_free,
        deadlock_state,
    })
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let (name, value) = line.split_once(" = ")?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() || name.contains(' ') {
        return None;
    }
    Some((name.to_owned(), value.to_owned()))
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nusmv_payload(mut line: &str) -> &str {
    line = line.trim();
    while let Some(rest) = line.strip_prefix("NuSMV >") {
        line = rest.trim_start();
    }
    line
}

fn extract_alloy_command_source(receipt: &str, name: &str) -> Result<String, String> {
    let commands = receipt
        .find("\"commands\":{")
        .ok_or_else(|| "Alloy receipt has no commands object".to_owned())?;
    let key = format!("\"{name}\":");
    let relative = receipt[commands..]
        .find(&key)
        .ok_or_else(|| format!("Alloy receipt omitted command {name}"))?;
    let key_start = commands + relative;
    let object_start = receipt[key_start + key.len()..]
        .find('{')
        .map(|offset| key_start + key.len() + offset)
        .ok_or_else(|| format!("Alloy receipt command {name} is not an object"))?;
    let object_end = matching_object_end(receipt, object_start)
        .ok_or_else(|| format!("Alloy receipt command {name} has an unclosed object"))?;
    let object = &receipt[object_start..=object_end];
    extract_json_string(object, "source")
        .ok_or_else(|| format!("Alloy receipt command {name} has no source/scope"))
}

fn matching_object_end(value: &str, start: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, byte) in bytes.get(start..)?.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_json_string(object: &str, field: &str) -> Option<String> {
    let marker = format!("\"{field}\":\"");
    let start = object.find(&marker)? + marker.len();
    let mut value = String::new();
    let mut escaped = false;
    for character in object[start..].chars() {
        if escaped {
            value.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                _ => return None,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(value);
        } else {
            value.push(character);
        }
    }
    None
}

fn preserve_raw(
    directory: &Path,
    raw: &RawInvocation,
    stdout: &[u8],
    stderr: &[u8],
) -> io::Result<()> {
    fs::write(directory.join("stdout.log"), stdout)?;
    fs::write(directory.join("stderr.log"), stderr)?;
    fs::write(directory.join("version.txt"), format!("{}\n", raw.version))?;
    let mut command = raw.program.clone();
    for argument in &raw.arguments {
        command.push(' ');
        command.push_str(argument);
    }
    fs::write(directory.join("command.txt"), format!("{command}\n"))?;
    fs::write(
        directory.join("exit-code.txt"),
        format!("{:?}\n", raw.exit_code),
    )?;
    Ok(())
}

fn preserve_normalized(directory: &Path, normalized: &NormalizedRun) -> io::Result<()> {
    let mut output = String::new();
    match normalized {
        NormalizedRun::Alloy(results) => {
            for result in results {
                let kind = match result.kind {
                    crate::AlloyCommandKind::Witness => "witness",
                    crate::AlloyCommandKind::Assertion => "assertion",
                };
                let outcome = match result.outcome {
                    crate::AlloyOutcome::Sat => "sat",
                    crate::AlloyOutcome::Unsat => "unsat",
                };
                writeln!(
                    output,
                    "{kind}\t{}\t{outcome}\tinstances={:?}\tsource={}",
                    result.name,
                    result.instances,
                    result
                        .command_source
                        .as_deref()
                        .unwrap_or("<missing>")
                        .replace('\n', "\\n")
                )
                .expect("writing to String cannot fail");
            }
        }
        NormalizedRun::NuSmv(results) => {
            for result in results {
                let kind = match result.kind {
                    crate::NuSmvPropertyKind::Specification => "specification",
                    crate::NuSmvPropertyKind::Invariant => "invariant",
                };
                writeln!(
                    output,
                    "{kind}\t{}\t{}\t{}",
                    result.name.as_deref().unwrap_or("<unnamed>"),
                    result.expression,
                    result.holds
                )
                .expect("writing to String cannot fail");
            }
        }
        NormalizedRun::ScryerProlog(results) => {
            for result in results {
                writeln!(output, "test\t{}\t{}", result.name, result.passed)
                    .expect("writing to String cannot fail");
            }
        }
    }
    fs::write(directory.join("normalized-results.txt"), output)
}

fn preserve_nusmv_traces(
    directory: &Path,
    traces: &[NuSmvCounterexample],
    fsm: &NuSmvFsmDiagnostics,
) -> io::Result<()> {
    let mut output = String::new();
    writeln!(
        output,
        "fsm\ttotal={}\tdeadlock_free={}",
        fsm.transition_total, fsm.deadlock_free
    )
    .expect("writing to String cannot fail");
    if let Some(state) = &fsm.deadlock_state {
        for (name, value) in state {
            writeln!(output, "deadlock\t{name}={value}").expect("writing to String cannot fail");
        }
    }
    for trace in traces {
        writeln!(
            output,
            "trace\t{}\tloop_start={:?}\texpression={}",
            trace.property_name, trace.loop_start, trace.property_expression
        )
        .expect("writing to String cannot fail");
        for (index, state) in trace.states.iter().enumerate() {
            write!(output, "state\t{}\t{index}", trace.property_name)
                .expect("writing to String cannot fail");
            for (name, value) in &state.assignments {
                write!(output, "\t{name}={value}").expect("writing to String cannot fail");
            }
            output.push('\n');
        }
    }
    fs::write(directory.join("normalized-counterexamples.txt"), output)
}

fn first_nonempty_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn receipt_source_parser_retains_exact_scope() {
        let receipt = r#"{"commands":{"Witness":{"solution":[{"values":{"x":{}}}],"source":"run Witness for exactly 2 Player,\n  7 Int","type":"run"}},"solver":"sat4j"}"#;
        assert_eq!(
            extract_alloy_command_source(receipt, "Witness").as_deref(),
            Ok("run Witness for exactly 2 Player,\n  7 Int")
        );
    }

    #[test]
    fn unclosed_receipt_object_is_rejected() {
        let receipt = r#"{"commands":{"Witness":{"source":"run Witness""#;
        assert!(extract_alloy_command_source(receipt, "Witness").is_err());
    }

    #[test]
    fn invocation_uses_handwritten_source_files() {
        let root = Path::new("C:/workspace");
        let evidence = root.join("target/alloy-oracle");
        let (model, arguments) = invocation(root, NativeBackend::Alloy, &evidence);
        assert!(model.ends_with("models/alloy/poche.als"));
        assert_eq!(arguments.last(), Some(&model.into_os_string()));
    }

    #[test]
    fn external_tools_receive_non_verbatim_windows_paths() {
        assert_eq!(
            external_tool_path(Path::new(r"\\?\D:\repo\model.als")),
            OsString::from(r"D:\repo\model.als")
        );
        assert_eq!(
            external_tool_path(Path::new(r"\\?\UNC\server\share\model.als")),
            OsString::from(r"\\server\share\model.als")
        );
    }

    #[test]
    fn nusmv_suite_parser_joins_names_traces_and_deadlock() {
        let transcript = r"
000 :AF phase = finished
  [CTL Unchecked N/A terminates]
001 :!bad
  [Invar Unchecked N/A bad_reachable]
The transition relation is not total.
The transition relation is not deadlock-free.
A deadlock state is:
mode = deadlock
phase = stuck
##########################################################
-- invariant !bad  is false
Trace Description: Counterexample
Trace Type: Counterexample
  -> State: 1.1 <-
    mode = deadlock
    phase = start
  -> Input: 1.2 <-
  -> State: 1.2 <-
    phase = stuck
-- specification AF phase = finished  is true
";
        let (results, traces, fsm) = parse_nusmv_suite(transcript).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name.as_deref(), Some("terminates"));
        assert_eq!(results[1].name.as_deref(), Some("bad_reachable"));
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].property_name, "bad_reachable");
        assert_eq!(traces[0].states.len(), 2);
        assert_eq!(traces[0].states[1].assignments["mode"], "deadlock");
        assert!(!fsm.transition_total);
        assert!(!fsm.deadlock_free);
        assert_eq!(fsm.deadlock_state.unwrap()["phase"], "stuck");
    }

    #[test]
    fn nusmv_fsm_parser_accepts_combined_totality_diagnostic() {
        let fsm = parse_nusmv_fsm("The transition relation is total: No deadlock state exists\n")
            .unwrap();
        assert!(fsm.transition_total);
        assert!(fsm.deadlock_free);
        assert!(fsm.deadlock_state.is_none());
    }

    #[test]
    fn os_string_is_accepted_by_command_contract() {
        fn accepts_os_str(_: &OsStr) {}
        accepts_os_str(OsStr::new("alloy"));
    }

    #[test]
    fn prolog_fixture_protocol_is_order_independent_and_fail_closed() {
        let transcript = "POCHE_PROLOG_FIXTURE legal_actions BEGIN\n\
            POCHE_PROLOG_ANSWER legal(z)\n\
            POCHE_PROLOG_ANSWER legal(a)\n\
            POCHE_PROLOG_FIXTURE legal_actions END count=2\n";
        let answers = normalize_prolog_fixture("legal_actions", transcript)
            .expect("complete known protocol parses");
        assert_eq!(
            answers.into_iter().collect::<Vec<_>>(),
            ["legal(a)", "legal(z)"]
        );
        assert!(
            normalize_prolog_fixture(
                "legal_actions",
                "POCHE_PROLOG_FIXTURE legal_actions BEGIN\n\
                 POCHE_PROLOG_ANSWER legal(a)\n\
                 POCHE_PROLOG_ANSWER legal(a)\n\
                 POCHE_PROLOG_FIXTURE legal_actions END count=2\n"
            )
            .is_err()
        );
        assert!(normalize_prolog_fixture("legal_actions", "unstructured success").is_err());
    }
}
