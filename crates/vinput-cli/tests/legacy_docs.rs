//! Regression tests for the legacy documentation index.

mod common;

use common::workspace_file;

#[test]
fn legacy_index_lists_all_notes() {
    let dir = workspace_file("docs/legacy");
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read legacy index");
    let mut note_files = std::fs::read_dir(&dir)
        .expect("read legacy dir")
        .map(|entry| entry.expect("read legacy entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .filter_map(|path| path.file_name().map(ToOwned::to_owned))
        .filter(|name| name != "README.md")
        .collect::<Vec<_>>();
    note_files.sort();

    assert!(!note_files.is_empty(), "legacy notes should exist");
    for file_name in note_files {
        let file_name = file_name.to_string_lossy();
        assert!(
            index.contains(file_name.as_ref()),
            "legacy index should link `{file_name}`"
        );
    }
}
