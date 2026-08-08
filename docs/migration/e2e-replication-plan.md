# 0.1.0 functional-parity plan

Reviewed: 2026-08-07

This is the active execution plan. Current implementation status belongs in [`function-gap-audit.md`](function-gap-audit.md), the user-task mapping belongs in [`user-capability-audit.md`](user-capability-audit.md), detailed evidence belongs in [`e2e-capability-matrix.md`](e2e-capability-matrix.md), and real-session procedures belong in [`live-desktop-validation.md`](live-desktop-validation.md).

## Product target

Vinpst 0.1.0 should let users complete substantially the same useful tasks as the upstream C++ project:

- install and initialize the product under Vinpst names and paths;
- discover, install, select, and diagnose ASR models/providers;
- dictate normally with visible partial preedit and final application commits;
- speak commands over selected text and replace it safely;
- configure keys, scenes, LLM providers, adapters, devices, VAD, hotwords, and output ducking without requiring manual JSON editing;
- manage resources through the Rust GUI or focused CLI commands;
- diagnose daemon, activation, native runtime, audio, provider, and frontend failures;
- install, update, and remove Vinpst predictably;
- use clear user-facing installation, usage, configuration, troubleshooting, and limitation documentation.

This is practical functional parity, not identity or implementation compatibility. Vinpst keeps its own package, executable, addon, D-Bus, systemd, environment-variable, and XDG identities. It does not replace or migrate another package, and pre-0.1.0 Vinpst interfaces may change when needed.

## Milestones

| Milestone | State | Exit criteria |
| --- | --- | --- |
| M0 Repository health | complete | Clean deterministic checks, bounded source layout, and current developer contracts. |
| M1 Product spine | complete | CLI, daemon, typed config, D-Bus service, retained Fcitx addon, and deterministic command-demo paths work together. |
| M2 Native and provider ASR | complete for the current registry families | Local offline/online models, command providers, remote providers, failure preservation, and representative real-WAV/live paths pass. |
| M3 Real desktop input | complete for the core 0.1.0 path | Normal dictation, command replacement, menus, localization, notifications, focus/owner recovery, model/provider switching, physical microphone, and representative applications are live-proven. |
| M4 Resource management | complete for ordinary workflows | CLI and GUI manage models, providers, adapters, scenes, LLM providers, devices, and hotwords without manual JSON editing. |
| M5 Rust management GUI | complete for 0.1.0 | Control, Resources, LLM, and Hotwords workflows are typed, conflict-aware, redacted, keyboard-operable, packaged, and deterministically covered. Positional focus-order desktop collectors are retired and are not release evidence. Screen-reader semantic trees are explicitly unsupported for 0.1.0 with CLI/Fcitx fallbacks; management-GUI interaction is manual-only and automated coverage stops below the Iced window/widget boundary. |
| M6 Exhaustive user-capability audit | complete for the frozen release baseline | The 164-file/1,559-callable upstream inventory was refreshed unchanged on 2026-08-07, and every meaningful user task is mapped to Vinpst implementation/evidence or an explicit product rationale. |
| M7 User documentation | complete for the release candidate | Strict MkDocs covers installation, quick start, usage, ASR, scenes, settings, accessibility, CLI, troubleshooting, limitations, release readiness, notes, publication, and rollback using Vinpst identities. |
| M8 Release readiness | active | Selected artifacts build from one checked source archive, package transactions run on clean hosted runners, manifest/checksum/provenance and draft-first publication are wired, and the final candidate still needs exact-commit rehearsal plus artifact-installed desktop verification. |
| M9 0.1.0 publication | pending | Version/tag consistency, release notes, publication, post-publication install, normal dictation, command replacement, diagnostics, and removal all pass. |

## Current priority order

### P0: finish the release candidate

1. Merge release-blocking fixes and keep protected-`main` checks green.
2. Run the non-publishing release workflow on the exact final commit.
3. Verify the downloaded manifest, checksums, and GitHub/Sigstore provenance.
4. Install the candidate in an unrelated clean user environment and repeat the required native desktop/removal path.
5. Review release notes and limitations, publish `v0.1.0`, then repeat the smoke from freshly downloaded public assets.

### P1: preserve the completed product path

- Keep normal dictation, command replacement, provider failure preservation, owner recovery, menus, localization, deterministic GUI management, and package transactions green.
- Add implementation or deterministic regression work only when a concrete release-blocking defect is found.
- Do not automate management-GUI interaction as a release condition. Validate the real GUI manually and keep automated coverage in crate-internal semantic state/message/persistence tests below the Iced window/widget boundary.

### P2: user documentation

- Keep the root README concise and task-oriented.
- Build the Markdown tree with MkDocs Material in strict mode.
- Write commands from actual `vinpst --help` surfaces and exercise representative flows in isolated tests.
- Keep migration/evidence detail out of ordinary user procedures; link to limitations rather than embedding test transcripts.
- Borrow useful topic structure from upstream documentation only after rewriting it for Vinpst behavior, names, and paths.
- Continue to use rustdoc for Rust API documentation rather than mixing crate API reference into the user guide.

### P3: release pipeline

- Keep the selected source, Arch x86_64, Debian 12 amd64, Ubuntu 24.04 amd64, and Flatpak x86_64 matrix frozen.
- Keep the completed one-source boundary green: current Arch, Debian, and Flatpak tag jobs consume the exact archive generated by the source job; Arch and Flatpak recheck the consumed archive digest before publication selection.
- Keep strict manifest/checksum generation, GitHub/Sigstore provenance, release notes, and draft-first exact-inventory publication green.
- Verify installation and a basic runtime path from each produced artifact.
- Keep the completed release gate green: tag publication calls the reusable CI workflow for docs, Rust/integration, and Nix checks and also requires every selected package job; configure the same checks as required before merge.
- Exercise release assembly without publishing before creating the tag.

## Completion gate

Do not claim 0.1.0 functional parity until a clean installation can complete this user path without manual JSON editing:

```sh
vinpst init
vinpst model list --available
vinpst model install <id-or-short-id>
vinpst model use <id-or-short-id> --in-place --reload-daemon
vinpst doctor
vinpst daemon status
```

The same installation must then pass:

- live normal dictation with partials and a final commit;
- live command replacement with failure preservation;
- scene and ASR selection;
- restart, reload, owner-loss/recovery, and diagnostics;
- GUI or CLI resource management for the selected release workflows;
- package removal while preserving Vinpst user state;
- strict documentation build and verified release artifacts.

The final review must freeze the current upstream commit and confirm that no known user-facing capability was silently omitted. It must not add upstream package/path identities merely to make names match.

## Work rules

- Prefer user journeys and release blockers over generic cleanup.
- Keep the retained Fcitx C++ layer thin and the standalone GUI in Rust.
- Distinguish `implemented`, `deterministic`, and `live-proven`.
- Keep real-profile mutation explicit and opt-in.
- Preserve stable Vinpst contracts where intentional; do not create compatibility debt for unreleased internal interfaces.
- Keep commits reviewable and update the user-capability audit when status changes.
