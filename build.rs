use std::process::Command;

fn main() {
    let git_commit = Command::new("git")
        .args(["describe", "--always", "--dirty", "--broken"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
