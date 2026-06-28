//! Regression tests for the README workspace layout section.

mod common;

use common::{workspace_crate_names, workspace_file};

#[test]
fn readme_lists_all_workspace_crates() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    for crate_name in workspace_crate_names() {
        let crate_path = format!("crates/{crate_name}");
        assert!(
            readme.contains(&crate_path),
            "README Current layout should list `{crate_path}`"
        );
    }
}
