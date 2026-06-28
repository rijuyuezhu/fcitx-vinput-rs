//! Regression tests for the public architecture documentation index.

mod common;

use std::path::{Path, PathBuf};

use common::{workspace_crate_names, workspace_file};

fn architecture_dir() -> PathBuf {
    workspace_file("docs/architecture")
}

fn has_markdown_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn markdown_link_targets(markdown: &str) -> Vec<&str> {
    markdown
        .split(']')
        .filter_map(|suffix| suffix.strip_prefix('('))
        .filter_map(|suffix| suffix.split_once(')').map(|(target, _)| target))
        .filter(|target| has_markdown_extension(target))
        .collect()
}

#[test]
fn architecture_index_lists_all_notes() {
    let dir = architecture_dir();
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read architecture index");
    let mut note_files = std::fs::read_dir(&dir)
        .expect("read architecture dir")
        .map(|entry| entry.expect("read architecture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .filter_map(|path| path.file_name().map(ToOwned::to_owned))
        .filter(|name| name != "README.md")
        .collect::<Vec<_>>();
    note_files.sort();

    assert!(!note_files.is_empty(), "architecture notes should exist");
    for file_name in note_files {
        let file_name = file_name.to_string_lossy();
        assert!(
            index.contains(file_name.as_ref()),
            "architecture index should link `{file_name}`"
        );
    }
}

#[test]
fn architecture_index_links_existing_notes() {
    let dir = architecture_dir();
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read architecture index");
    let targets = markdown_link_targets(&index);

    assert!(!targets.is_empty(), "architecture index should link notes");
    for target in targets {
        assert!(
            dir.join(target).exists(),
            "architecture index link should exist: {target}"
        );
    }
}

#[test]
fn bootstrap_doc_lists_all_workspace_crates() {
    let bootstrap = std::fs::read_to_string(architecture_dir().join("bootstrap.md"))
        .expect("read bootstrap architecture doc");
    for crate_name in workspace_crate_names() {
        assert!(
            bootstrap.contains(&crate_name),
            "bootstrap architecture doc should list `{crate_name}`"
        );
    }
}
