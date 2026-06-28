//! Regression tests for product-facing README demo commands.

mod common;

use common::workspace_file;

#[test]
fn readme_lists_committed_e2e_demo_recipe_and_fixture() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    assert!(
        readme.contains("just e2e-demo"),
        "README should document the one-command E2E demo"
    );
    assert!(
        readme.contains("data/e2e-command-demo-config.json"),
        "README should point to the committed E2E demo config"
    );
    assert!(
        justfile.contains("e2e-demo:"),
        "justfile should expose the E2E demo recipe"
    );
    assert!(
        workspace_file("data/e2e-command-demo-config.json").exists(),
        "committed E2E demo config should exist"
    );
    assert!(
        workspace_file("scripts/write-demo-wav.py").exists(),
        "demo WAV writer should exist"
    );
}
