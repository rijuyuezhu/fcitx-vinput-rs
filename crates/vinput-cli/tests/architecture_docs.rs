//! Regression tests for the public architecture documentation index.

mod common;

use std::path::{Path, PathBuf};

use common::{markdown_note_names, workspace_crate_names, workspace_file};

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
    for file_name in markdown_note_names(&dir) {
        assert!(
            index.contains(&file_name),
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
fn development_doc_lists_all_workspace_crates() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    for crate_name in workspace_crate_names() {
        assert!(
            development.contains(&crate_name),
            "development guide should list `{crate_name}`"
        );
    }
}

#[test]
fn target_architecture_lists_all_workspace_crates() {
    let target = std::fs::read_to_string(architecture_dir().join("target-architecture.md"))
        .expect("read target architecture doc");
    for crate_name in workspace_crate_names() {
        assert!(
            target.contains(&crate_name),
            "target architecture doc should list `{crate_name}`"
        );
    }
}

#[test]
fn dbus_architecture_labels_diagnostic_extension_and_postprocessing_gap() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");

    assert!(
        dbus_doc.contains("GetTextAdapterState` is a Rust diagnostic extension"),
        "D-Bus docs must label GetTextAdapterState as a Rust diagnostic extension"
    );
    assert!(
        dbus_doc.contains("not part of the original C++ daemon vtable"),
        "D-Bus docs must keep the legacy-vs-extension boundary explicit"
    );
    assert!(
        dbus_doc.contains("A real legacy `postprocessing` runtime phase is still not wired"),
        "D-Bus docs must keep the current postprocessing runtime gap explicit"
    );
}

#[test]
fn text_architecture_pins_command_mode_payload_contract() {
    let text_doc = std::fs::read_to_string(architecture_dir().join("text-contract.md"))
        .expect("read text contract doc");

    for required in [
        "selected text as a `raw` candidate",
        "recognized command text as an `asr` candidate",
        "LLM/post-processing candidates as `llm` candidates",
        "Commit text prefers the first LLM/post-processing candidate",
        "falls back to the selected text when present",
        "Frontend-side selected-text deletion, clipboard fallback",
        "remain future frontend work",
    ] {
        assert!(
            text_doc.contains(required),
            "text contract doc should pin command-mode rule: {required}"
        );
    }
}

#[test]
fn text_architecture_pins_prompt_file_and_context_cache_rules() {
    let text_doc = std::fs::read_to_string(architecture_dir().join("text-contract.md"))
        .expect("read text contract doc");

    for required in [
        "only literal `file:///absolute/path` URIs are accepted",
        "path is loaded only when it points to a regular file",
        "reads are capped at 256 KiB",
        "unsupported variables are preserved verbatim",
        "frontend-facing code can buffer committed fragments",
        "daemon-facing request builders read raw non-empty lines",
        "XDG_CACHE_HOME/vinput/context.jsonl",
        "$HOME/.cache/vinput/context.jsonl",
    ] {
        assert!(
            text_doc.contains(required),
            "text contract doc should pin prompt/context rule: {required}"
        );
    }
}
