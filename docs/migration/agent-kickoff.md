# Agent kickoff

Reviewed: 2026-08-07

Use this as the compact handoff for implementation work. It points to current sources of truth; it is not another progress log.

## Mission

Prepare `fcitx-vinpst` 0.1.0 by completing the exhaustive user-capability audit, closing meaningful functional gaps, finishing user-facing documentation, and integrating the selected release artifacts.

Vinpst is an independent product. Keep `vinpst` / `fcitx-vinpst` package, executable, addon, D-Bus, systemd, environment-variable, and XDG identities. Do not add upstream package replacement, old-name aliases, automatic migration, or pre-0.1.0 internal compatibility unless the product decision is explicitly changed.

The target is practical feature parity: users should be able to complete substantially the same useful voice-input, command-editing, configuration, resource-management, and diagnostic tasks as the upstream C++ project. It is not a source-level or interface-name port.

## Repositories

- Vinpst: `/workspace/fcitx-vinpst`
- Upstream reference: `/workspace/fcitx5-vinput`

## Required reading

1. the repository `AGENTS.md`
2. [`../development-index.md`](../development-index.md)
3. [`../development.md`](../development.md)
4. [`function-gap-audit.md`](function-gap-audit.md)
5. [`user-capability-audit.md`](user-capability-audit.md)
6. [`e2e-capability-matrix.md`](e2e-capability-matrix.md)
7. [`e2e-replication-plan.md`](e2e-replication-plan.md)
8. [`live-desktop-validation.md`](live-desktop-validation.md) when touching live behavior
9. the relevant [`../architecture/README.md`](../architecture/README.md) contract
10. [`../legacy/README.md`](../legacy/README.md) when refreshing or reviewing upstream source/callable drift

## Current baseline

- CLI, daemon, typed configuration, registry lifecycle, native/command/remote ASR, text processing, diagnostics, and the retained Fcitx addon are broadly implemented and tested.
- Normal dictation and selected-text command replacement are live-proven across representative applications, provider paths, menus, localization, notifications, focus/owner recovery, and physical/isolated audio boundaries.
- The Rust/Iced GUI provides Control, Resources, LLM, and Hotwords workflows with typed persistence, safe resource operations, redacted diagnostics, and deterministic coverage. Wayland/X11 management-GUI automation is retired; real GUI interaction is manually validated and automated coverage stays in crate-internal semantic tests below the Iced window/widget boundary. The `0.1.0` policy supports keyboard operation but not screen-reader semantic trees, with `vinpst`, `fcitx5-configtool`, and guarded terminal configuration-file fallbacks.
- Source, Arch x86_64, Debian 12 amd64, Ubuntu 24.04 amd64, and Flatpak x86_64 assemble from one checked source archive behind reusable docs/Rust/Nix quality gates. Release assets use manifest/checksum verification plus GitHub/Sigstore provenance, and publication is draft-first with exact remote inventory checks. The remaining work is final candidate rehearsal, artifact-installed desktop validation, tag publication, and post-publication smoke; RPM and Nix publication stay outside the selected initial bundle.
- The upstream baseline was refreshed unchanged on 2026-08-07 at `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9`: 164 production C/C++ files and 1,559 callable occurrences. Human review maps those entries to user journeys rather than requiring one Rust function per C++ function.
- MkDocs Material is the user/developer documentation generator; rustdoc remains the Rust API reference.

Do not maintain or quote a parity percentage. Use `implemented`, `deterministic`, `live-proven`, `partial`, `not applicable`, and `missing`.

## Start-of-session checks

```sh
cd /workspace/fcitx-vinpst
git status --porcelain=v1 -b
git log -8 --oneline --decorate
```

Run the narrowest relevant check while iterating and the complete relevant tier before handoff.

## Implementation rules

- Communicate with the user in Chinese.
- Keep code, comments, test names, documentation identifiers, and commit messages in English.
- Prefer user journeys and release blockers over generic cleanup.
- Keep the retained C++ frontend thin; backend policy and the standalone GUI belong in Rust.
- Preserve intentionally stable Vinpst contracts; do not create compatibility debt for unreleased internal interfaces.
- Treat mock, file, session-bus, package, and temporary-HOME evidence as deterministic, not live.
- Keep real-profile changes explicit and opt-in.
- Add focused regression coverage for every live-facing fix.
- Keep commits reviewable and Conventional Commit formatted.

## Validation

Documentation and audit:

```sh
git diff --check
just docs
scripts/tests/check-upstream-inventory.py
```

Rust/core:

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Complete deterministic gate:

```sh
just ci
```

Package and live gates are selected from [`../development.md`](../development.md) according to the changed subsystem and release claim.
