use std::path::PathBuf;
use std::process::Command;

#[test]
fn help_exits_cleanly_when_stdout_pipe_closes_early() {
    let output = Command::new("bash")
        .args(["-c", "set -o pipefail; \"$1\" --help | true", "bash"])
        .arg(workroot_binary())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!stderr.contains("Broken pipe"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

fn workroot_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_workroot"))
}
