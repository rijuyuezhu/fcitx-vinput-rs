//! Regression tests for the README workspace layout section.

use std::path::PathBuf;

fn workspace_file(path: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("../..");
    root.push(path);
    root
}

#[test]
fn readme_lists_all_workspace_crates() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let mut crates = std::fs::read_dir(workspace_file("crates"))
        .expect("read crates directory")
        .map(|entry| entry.expect("read crate directory entry").path())
        .filter(|path| path.is_dir())
        .filter_map(|path| path.file_name().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    crates.sort();

    assert!(!crates.is_empty(), "workspace crates should exist");
    for crate_name in crates {
        let crate_path = format!("crates/{}", crate_name.to_string_lossy());
        assert!(
            readme.contains(&crate_path),
            "README Current layout should list `{crate_path}`"
        );
    }
}
