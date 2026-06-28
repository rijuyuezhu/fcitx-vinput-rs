//! Regression tests for README tooling references.

mod common;

use common::workspace_file;

#[test]
fn readme_lists_existing_tooling_files() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    for path in [
        "rust-toolchain.toml",
        "rustfmt.toml",
        "clippy.toml",
        "Cargo.toml",
        ".pre-commit-config.yaml",
        "justfile",
    ] {
        assert!(
            workspace_file(path).exists(),
            "tooling file should exist: {path}"
        );
        assert!(
            readme.contains(path),
            "README Tooling section should mention `{path}`"
        );
    }
}
