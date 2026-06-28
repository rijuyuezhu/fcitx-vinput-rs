//! Integration tests for top-level CLI help output.

use std::process::Command;

#[test]
fn help_lists_diagnostic_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .arg("--help")
        .output()
        .expect("run vinput --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("protocol"));
    assert!(stdout.contains("registry"));
}
