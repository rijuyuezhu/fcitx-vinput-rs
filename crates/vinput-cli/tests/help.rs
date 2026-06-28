//! Integration tests for CLI help output.

mod common;

use common::{assert_stdout_success, vinput_command};

#[test]
fn help_lists_diagnostic_commands() {
    let output = vinput_command()
        .arg("--help")
        .output()
        .expect("run vinput --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("protocol"));
    assert!(stdout.contains("registry"));
}

#[test]
fn asr_state_help_lists_config_option() {
    let output = vinput_command()
        .args(["asr-state", "--help"])
        .output()
        .expect("run vinput asr-state --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("diagnostics from config"));
}
