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
        "retained C++ frontend owns selected-text replacement and cleanup",
        "clipboard fallback remains future frontend work",
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
        "without exposing environment keys, environment values, or working directory paths",
        "sanitized per-adapter summaries with `id`, `kind`, `args_count`, `env_count`, `has_working_dir`, `is_running`, and `pid`",
        "never include the configured command path, command arguments, environment keys, environment values, configured working directory path, or forward-compatible adapter fields",
        "Request diagnostics redact the HTTP auth header case-insensitively",
        "leaving the transport request intact",
    ] {
        assert!(
            text_doc.contains(required),
            "text contract doc should pin prompt/context rule: {required}"
        );
    }
}

#[test]
fn config_architecture_pins_summary_redaction_contract() {
    let config_doc = std::fs::read_to_string(architecture_dir().join("config-contract.md"))
        .expect("read config contract doc");

    for required in [
        "`VinputConfig::summary()` is the compact config diagnostic surface",
        "active scene/provider ids, and counts only",
        "must not serialize secret-bearing config fields",
        "LLM API keys",
        "provider or adapter environment values",
        "command arguments",
        "working directories",
        "provider base URLs",
        "forward-compatible extra bodies",
        "`vinput-daemon --config data/default-config.json print-config`",
    ] {
        assert!(
            config_doc.contains(required),
            "config contract doc should pin summary redaction rule: {required}"
        );
    }
}
#[test]
fn audio_architecture_pins_pipewire_live_test_policy() {
    let audio_doc = std::fs::read_to_string(architecture_dir().join("audio-contract.md"))
        .expect("read audio contract doc");

    for required in [
        "VINPUT_TEST_PIPEWIRE_CONTEXT",
        "VINPUT_TEST_PIPEWIRE_ENUMERATE",
        "instead of running in default CI",
        "without requiring a live PipeWire daemon",
        "live probes must only run when those environment variables are set explicitly",
        "`PipeWireAudioRecorder` currently exists behind `pipewire-backend` as an explicit skeleton",
        "returns `RecordingBackendUnavailable` instead of silently falling back to mock capture",
        "future live implementation should negotiate signed 16-bit 16 kHz mono PCM first",
        "`PipeWireStreamConfig` records the selected capture target",
        "pinned `S16LE` 16 kHz mono PCM policy",
        "deterministic chunk planning use frames rather than raw sample count",
        "chunk helpers never split a frame across chunk boundaries",
        "pushes the processed `PcmBuffer` with explicit `PcmSpec` metadata to the active ASR session",
        "`PcmBuffer::chunk_ranges_by_frames` can plan complete-frame chunk ranges without copying",
        "can use complete-frame chunk helpers for deterministic streaming callback tests",
    ] {
        assert!(
            audio_doc.contains(required),
            "audio contract doc should pin PipeWire live-test policy: {required}"
        );
    }
}

#[test]
fn asr_architecture_pins_local_sherpa_runtime_gap() {
    let asr_doc = std::fs::read_to_string(architecture_dir().join("asr-contract.md"))
        .expect("read asr contract doc");

    for required in [
        "Local `sherpa-onnx` has an explicit typed config seam",
        "runtime remains unavailable until the concrete backend is implemented",
        "Local `sherpa-onnx` typed config parsing and local model/hotwords path validation exist as seams",
        "accepts relative or absolute local model and hotwords paths",
        "rejects empty values and URL-like paths",
        "verifies model directories plus regular hotwords files",
        "VAD trimming, warmup, and concrete reload state are not implemented yet",
        "`MockAsrBackend` can attach a shared `MockAsrAudioLog` for deterministic tests",
        "mock-only observation seam for future runtime streaming tests",
        "`MockAsrAudioPush` is serde/schema-ready",
    ] {
        assert!(
            asr_doc.contains(required),
            "ASR contract doc should pin local sherpa runtime gap: {required}"
        );
    }
}

#[test]
fn development_doc_pins_optional_pipewire_recipes() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "just pipewire-check",
        "just pipewire-live",
        "VINPUT_TEST_PIPEWIRE_CONTEXT=1",
        "VINPUT_TEST_PIPEWIRE_ENUMERATE=1",
        "intentionally excluded from `just ci`",
        "without live daemon",
        "CLI/daemon audio-device diagnostics",
    ] {
        assert!(
            development.contains(required),
            "development guide should pin optional PipeWire recipe policy: {required}"
        );
    }

    assert!(justfile.contains("pipewire-check:"));
    assert!(justfile.contains("pipewire-live:"));
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("justfile should define check recipe");
    assert!(!check_line.contains("pipewire-live"));
}

#[test]
fn target_architecture_pins_frontend_packaging_boundary() {
    let target = std::fs::read_to_string(architecture_dir().join("target-architecture.md"))
        .expect("read target architecture doc");

    for required in [
        "retained C++ Fcitx5 frontend bridge",
        "existing `vinput-protocol` D-Bus ABI",
        "Fcitx API integration, menus, preedit/status presentation",
        "selected-text collection",
        "command-mode selected-text replacement",
        "frontend-side cleanup",
        "Backend logic, ASR/text processing, registry operations, and runtime state must stay in Rust crates",
        "Do not replace the Fcitx5 addon with a Rust addon",
        "Packaging/service install artifacts remain future work",
    ] {
        assert!(
            target.contains(required),
            "target architecture should pin T6 frontend/packaging boundary: {required}"
        );
    }
}

#[test]
fn registry_architecture_mentions_root_planning() {
    let registry_doc = std::fs::read_to_string(architecture_dir().join("registry-contract.md"))
        .expect("read registry contract doc");

    assert!(registry_doc.contains("Dry-run install plans keep install roots explicit"));
    assert!(registry_doc.contains("filesystem root stays absolute"));
    assert!(registry_doc.contains("without touching the filesystem"));
}
