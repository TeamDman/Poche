use std::process::{Command, Output};

fn poche(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_poche"))
        .args(arguments)
        .output()
        .expect("poche process should start")
}

#[test]
fn help_and_version_are_stdout_only_round_trips() {
    for arguments in [&["--help"][..], &["room", "ready", "--help"][..]] {
        let output = poche(arguments);
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("USAGE:"));
        assert!(output.stderr.is_empty());
    }

    let output = poche(&["--version"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version should be UTF-8");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
    assert!(stdout.contains("repo "));
    assert!(stdout.contains("branch "));
    assert!(stdout.contains("rev "));
    assert!(stdout.contains("worktree "));
    assert!(stdout.contains("built-unix "));
    assert!(output.stderr.is_empty());
}

#[test]
fn text_and_json_outputs_keep_diagnostics_on_stderr() {
    let text = poche(&["--output", "text", "identity", "show"]);
    assert!(text.status.success());
    let stdout = String::from_utf8(text.stdout).expect("text output should be UTF-8");
    assert!(stdout.contains("command: identity.show"));
    assert!(!stdout.contains(" INFO "));
    assert!(String::from_utf8_lossy(&text.stderr).contains("command parsed"));

    let json = poche(&["--output", "json", "identity", "show"]);
    assert!(json.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("stdout should be one JSON value");
    assert_eq!(value["command"], "identity.show");
    assert_eq!(value["status"], "parsed");
    assert!(String::from_utf8_lossy(&json.stderr).contains("command parsed"));
}

#[test]
fn debug_and_ndjson_logs_never_copy_invite_material() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock should follow the epoch")
        .as_nanos();
    let log_path = std::env::temp_dir().join(format!("poche-cli-{nonce}.ndjson"));
    let invite = "room-code-super-secret-123";
    let output = Command::new(env!("CARGO_BIN_EXE_poche"))
        .args([
            "--debug",
            "--log-file",
            &log_path.to_string_lossy(),
            "--output",
            "json",
            "room",
            "join",
            invite,
        ])
        .output()
        .expect("poche process should start");
    assert!(output.status.success());
    let ndjson = std::fs::read_to_string(&log_path).expect("NDJSON log should exist");
    let combined = format!(
        "{}{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        ndjson
    );
    assert!(
        !combined.contains(invite),
        "invite leaked into output or logs"
    );
    assert!(
        ndjson
            .lines()
            .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
    );
    std::fs::remove_file(log_path).expect("temporary NDJSON log should be removable");
}

#[test]
fn invalid_input_fails_with_diagnostics_only() {
    let output = poche(&["room", "join"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn zero_deadline_cancels_before_machine_output() {
    let output = poche(&["--stop-after-ms", "0", "identity", "show"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--stop-after-ms elapsed"));
}
