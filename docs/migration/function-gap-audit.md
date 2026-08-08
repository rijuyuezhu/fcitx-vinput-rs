# Function gap audit

Reviewed: 2026-08-07

This document is the current implementation/readiness summary. The generated source/callable baseline is tracked under [`../legacy/`](../legacy/README.md), user-task mappings live in [`user-capability-audit.md`](user-capability-audit.md), detailed evidence lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md), and priorities live in [`e2e-replication-plan.md`](e2e-replication-plan.md).

## Review baseline

- Vinpst release-readiness base: protected `main` at `d2771eccef19316e2afc11c7d2cb6fe1491879c2` when this final review started; the publication procedure records the eventual tag commit.
- Upstream reference: `xifan2333/fcitx5-vinput` at `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9` (`v2.3.5-1-g6cdcac8`), refreshed unchanged from `origin/main` on 2026-08-07.
- Generated upstream scope: 164 production C/C++ files, 28,168 lines, and 1,559 function/prototype/signal/slot occurrences.
- Product target: practical user-capability parity under independent Vinpst identities and paths.

## Executive conclusion

Vinpst already provides the core product experience expected for a voice-input system:

- normal dictation with streaming partials and final Fcitx commits;
- selected-text command editing with candidates and failure-safe replacement;
- local, command, and OpenAI-compatible remote ASR;
- PipeWire capture, device selection, gain/normalization, Silero VAD, hotwords, and output ducking;
- scene, LLM provider, adapter, model, and provider management;
- Fcitx keys, Tap/Hold/Both behavior, Scene/ASR menus, notifications, localization, and owner recovery;
- CLI diagnostics and a Rust/Iced management GUI;
- checked Arch, Debian, RPM, Nix, Flatpak, source-archive, manifest, and signing boundaries at different publication-readiness levels.

The project is no longer blocked on a missing user capability. The frozen upstream inventory has no release-candidate delta, and ordinary GUI workflows have deterministic coverage. Positional focus-order management-GUI collectors are intentionally retired and are not release evidence. The active 0.1.0 work is release-candidate rehearsal, provenance/publication operations, artifact-installed validation, and post-publication verification. The GUI accessibility policy is explicit: keyboard operation is supported, screen-reader semantic trees are not, and CLI/Fcitx fallbacks are documented.

Vinpst is not an in-place replacement for the upstream package. Its package, executable, addon, D-Bus, service, environment-variable, and XDG identities remain Vinpst-only, and no upstream migration or pre-0.1.0 internal compatibility is required.

## Readiness summary

| Area | State | Release-relevant remainder |
| --- | --- | --- |
| Normal desktop dictation | `live-proven` | Broader application/device breadth is useful but no core task is missing. |
| Command editing | `live-proven` | Broader application and real hosted-provider operations. |
| Trigger modes, keys, menus, candidates | `live-proven` | No practical parity gap currently identified. |
| Local ASR | `deterministic`; representative `live-proven` | Add model layouts only for real registry/user demand. |
| Command ASR | `deterministic`; independent Whisper `live-proven` | No known ordinary workflow gap. |
| Remote ASR | `deterministic`; loopback `live-proven` | Hosted-provider operational and credential evidence. |
| Audio/VAD/device/output ducking | `deterministic`; representative `live-proven` | Additional physical-device and audible-output breadth. |
| Scenes/LLM/adapters | `deterministic`; command replacement `live-proven` | Hosted-provider evidence and broader GUI error categories. |
| Registry/resource lifecycle | `deterministic` | No known ordinary workflow gap; positional management-GUI collectors are intentionally not retained. |
| Fcitx localization/notifications | `live-proven` for English and zh_CN | Additional locales are optional expansion. |
| Remote text HTTP/WebSocket | `deterministic`; same-host browser `live-proven` | A separately confirmed physical-device collector run. |
| CLI management and diagnostics | `deterministic`; public surface audited against a compiled upstream `vinput` at the frozen reference | Continue only concrete message refinement; test/fixture and package-maintenance interfaces stay hidden from ordinary help. |
| Rust management GUI | packaged interactive baseline; deterministic management coverage; `0.1.0` accessibility policy explicit | Management-GUI desktop automation is retired; interaction/visual acceptance is manual and crate tests cover semantics below the Iced window/widget boundary. Post-`0.1.0` semantic-tree work remains. |
| Arch package/repository/signature/candidate | `deterministic`; explicit package smoke; tag job consumes the byte-identical source-job archive | Release-asset provenance, final unrelated-environment installation, and publication; a distribution repository is not selected for 0.1.0. |
| Debian 12 / Ubuntu 24.04 | Docker install/upgrade/remove transactions complete; tag jobs build from the one source-job archive | Production publication and unrelated-environment validation. |
| RPM family | build and isolated transaction baseline | Fedora/openSUSE support claims require distro/repository/signing/SELinux/live-scriptlet evidence. |
| Nix | locked closure build baseline | Binary-cache publication policy if selected. |
| Flatpak | checked extension transaction baseline; tag job consumes the byte-identical source-job archive | Live host desktop/Fcitx/PipeWire/systemd and publication/signing policy. |
| User documentation | installation, quick start, usage, ASR, scenes, settings, accessibility, CLI, troubleshooting, limitations, release notes, and publication procedure implemented | Keep the strict build green and review commands against the final artifacts. |
| Exhaustive upstream review | complete for the frozen 164-file/1,559-callable baseline; upstream `origin/main` refreshed unchanged on 2026-08-07 | Re-run only if upstream changes before the tag. |

## Highest-risk gaps

1. **Final candidate execution:** the exact `main` commit still needs a non-publishing release workflow run and independent verification of the downloaded checked bundle.
2. **Artifact-installed validation:** the selected native candidate must pass initialization, diagnostics, daemon status, normal dictation, command replacement, provider/model switching, and removal in an unrelated clean user environment.
3. **Publication and incident handling:** release notes, draft-first publication, exact remote asset inventory, GitHub/Sigstore provenance, rollback policy, and post-publication verification must remain green.
4. **Operational breadth after 0.1.0:** hosted-provider policies, additional devices/applications, long-duration soak, Flatpak desktop integration, RPM/Nix publication, and GUI semantic-tree support remain explicitly deferred and documented.

These are release-operation and future-breadth risks. They do not indicate a missing core user task and do not justify changing Vinpst identities to upstream names.

## Improvements beyond the upstream implementation

Vinpst intentionally changes implementation and management design where that produces a clearer or safer product:

- Rust-owned typed runtime, configuration, registry, frontend-policy, and GUI boundaries;
- deterministic file-input, private-session-bus, temporary-HOME, package-transaction, and display-independent GUI paths;
- checksum-verified downloads, safe extraction, staged publication, managed-root guards, and conflict-aware atomic config writes;
- bounded process groups, deadlines, descendant cleanup, and independent stdout/stderr limits for helper providers;
- redacted typed diagnostics, owner/runtime visibility, prepare-before-swap provider reload, and failure preservation;
- a standalone Rust management GUI rather than a Qt source-level port;
- generated upstream drift inventory plus a user-task review layer.

## Completion gate

Before 0.1.0, the following path must work from a produced release artifact without manual JSON editing:

```sh
vinpst init
vinpst model list --available
vinpst model install <id-or-short-id>
vinpst model use <id-or-short-id> --in-place --reload-daemon
vinpst doctor
vinpst daemon status
```

The same installation must then pass live normal dictation, command replacement, scene/ASR selection, restart/reload/owner recovery, required GUI management tasks, and removal with Vinpst user state preserved.

The final review must refresh the upstream inventory, resolve every meaningful `missing` user task, document all evidence-only limitations, build the MkDocs site strictly, and validate the selected release artifacts on an unrelated environment.
