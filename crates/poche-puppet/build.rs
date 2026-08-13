use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for input in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", input]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let revision = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let worktree =
        git(&["status", "--short", "--untracked-files=no"]).map_or("unknown", |status| {
            if status.is_empty() { "clean" } else { "dirty" }
        });
    println!("cargo:rustc-env=POCHE_PUPPET_GIT_REVISION={revision}");
    println!("cargo:rustc-env=POCHE_PUPPET_GIT_WORKTREE={worktree}");
}

fn git(arguments: &[&str]) -> Option<String> {
    Command::new("git")
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
}
