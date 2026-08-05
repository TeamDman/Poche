use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");
    add_git_input("config");
    add_git_input("HEAD");
    add_git_input("index");

    let revision = git(&["rev-parse", "--short", "HEAD"]);
    let branch = git(&["branch", "--show-current"]);
    let repository = git(&["remote", "get-url", "origin"]);
    let worktree = match git_allow_empty(&["status", "--short", "--untracked-files=no"]) {
        Some(status) if status.is_empty() => "clean".to_owned(),
        Some(_) => "dirty".to_owned(),
        None => "unknown".to_owned(),
    };
    let built_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
        |_| "unknown".to_owned(),
        |value| value.as_secs().to_string(),
    );

    println!("cargo:rustc-env=POCHE_GIT_REVISION={revision}");
    println!("cargo:rustc-env=POCHE_GIT_BRANCH={branch}");
    println!("cargo:rustc-env=POCHE_GIT_REPOSITORY={repository}");
    println!("cargo:rustc-env=POCHE_GIT_WORKTREE={worktree}");
    println!("cargo:rustc-env=POCHE_BUILD_UNIX={built_at}");
}

fn add_git_input(name: &str) {
    if let Some(path) = git_allow_empty(&["rev-parse", "--git-path", name])
        && !path.is_empty()
    {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn git(arguments: &[&str]) -> String {
    git_allow_empty(arguments)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn git_allow_empty(arguments: &[&str]) -> Option<String> {
    Command::new("git")
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
}
