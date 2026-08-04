// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::PrologFixtureReport;
use crate::normalize::{NormalizedRun, normalize_alloy, normalize_nusmv, normalize_prolog};
use crate::{NativeBackend, NativeDisposition, NativeReport, RawInvocation, evaluate_fixtures};

struct ToolSpec {
    override_variable: &'static str,
    candidates: &'static [&'static str],
    version_arguments: &'static [&'static str],
    accept_nonzero_marker: Option<&'static str>,
}

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

/// Execute one finite native Prolog conformance fixture.
///
/// The supplied names are restricted to lowercase ASCII identifiers so they
/// can select a handwritten predicate mode without becoming executable source.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_prolog_fixture(root: &Path, fixture_id: &str, native_goal: &str) -> PrologFixtureReport {
    let evidence_directory = root.join("target/prolog-conformance").join(fixture_id);
    let invalid_fixture = fixture_id.is_empty()
        || !fixture_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let invalid_goal = native_goal.is_empty()
        || !native_goal
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_');
    if invalid_fixture || invalid_goal {
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
    let model = root.join("models/prolog/poche.pl");
    let goal = format!("poche:run_conformance_fixture({native_goal}),halt");
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
                writeln!(output, "{kind}\t{}\t{}", result.expression, result.holds)
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
