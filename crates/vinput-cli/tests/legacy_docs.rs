//! Regression tests for the legacy documentation index.

mod common;

use common::{markdown_note_names, workspace_file};

#[test]
fn legacy_index_lists_all_notes() {
    let dir = workspace_file("docs/legacy");
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read legacy index");
    for file_name in markdown_note_names(&dir) {
        assert!(
            index.contains(&file_name),
            "legacy index should link `{file_name}`"
        );
    }
}
