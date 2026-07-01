# E2E Rust port acceleration plan

This plan supersedes the earlier refactor-first phase. The current objective is to turn the Rust rewrite into a usable Fcitx vinput input method as quickly as possible while preserving the legacy ABI and test discipline that already exists.

## Current objective

Build a thin, installable Fcitx5 input-method frontend that talks to the Rust daemon over the existing `vinput-protocol` D-Bus ABI and can complete a real end-to-end recognition flow:

```text
Fcitx5 key/menu action
  -> Rust daemon over D-Bus
  -> audio capture or file/mock capture
  -> ASR backend
  -> optional text post-processing
  -> recognition payload
  -> Fcitx commit/candidate UI
```

The next phase should optimize for a working E2E product spine, not for perfect internal refactor completion. New implementation is acceptable when it is behind an explicit seam and has focused tests.

## What is already strong in the Rust rewrite

- `vinput-protocol` pins legacy D-Bus names, status strings, recognition JSON, ASR state, and text adapter diagnostics.
- `vinput-config` parses, normalizes, validates, summarizes, and redacts config diagnostics.
- `vinput-audio` owns PCM/WAV/raw-byte helpers, device enumeration traits, deterministic recorders, and a PipeWire feature skeleton.
- `vinput-asr` has mock, command batch, command streaming, config factory, payload conversion, and local sherpa config/path validation seams.
- `vinput-text` has prompt rendering, context cache helpers, OpenAI-compatible request/transport seams, command adapters, and adapter supervisor pid-file lifecycle.
- `vinput-registry` has metadata parsing, mirror fetch, cache, checksum, asset staging, safe archive extraction, materialization, staging path planning, and CLI dry-run planning.
- `vinput-daemon` has mock/configured runtime paths, D-Bus service facade, diagnostics, `--once`, and deterministic configured-backend demos.
- `vinput-cli` has diagnostics and smoke coverage over protocol/config/registry/audio/daemon paths.

## Legacy C++ vs current Rust rewrite

| Legacy area | Legacy C++ source | Current Rust status | Gap for usable E2E |
|---|---|---|---|
| Fcitx frontend addon | `src/addon/*` | Retained thin C++ addon bridge exists under `cpp/fcitx5-addon`: trigger/key handling, bus client wrapper, preedit/status, candidate UI, selection replacement, install metadata, and focused smoke coverage. | Keep exercising it against the Rust daemon, document local install/run paths, and add only thin frontend behavior needed for E2E. |
| D-Bus daemon API | `src/daemon/runtime/dbus_service.*`, `src/common/dbus/*` | `vinput-protocol` + `vinput-daemon` expose legacy names/methods plus diagnostics. | Exercise from the real Fcitx addon and keep ABI tests green. |
| Runtime pipeline | `src/daemon/runtime/*` | `RuntimeState` supports mock/configured paths, `--once`, D-Bus facade, command ASR/text flows. | Wire live frontend start/stop/cancel and selected text; tighten actor/race handling only as needed for E2E. |
| Audio capture | `src/daemon/audio/*`, `src/common/audio/pipewire_device.*` | Pure PCM and recorder traits are strong; PipeWire enumeration and recorder skeleton exist. | Implement minimal live PipeWire recorder or provide a dev fallback path that can be used from the addon. |
| ASR command backend | `src/daemon/asr/backends/command_*` | Command batch/streaming runners are runtime-wired behind configured backends. | Use this as the fastest first real ASR path; document a simple helper contract for local E2E. |
| ASR sherpa backend | `src/daemon/asr/backends/sherpa_*`, `src/daemon/asr/vad_trimmer.*` | Typed config/path validation exists; concrete runtime intentionally unavailable. | Implement minimal offline sherpa-onnx backend after the addon/product spine works. VAD/warmup can follow. |
| Text post-processing | `src/daemon/postprocess/*`, `src/common/scene/*`, `src/common/llm/*` | Prompt/context, command adapter, OpenAI-compatible provider, and adapter supervisor seams exist. | Keep configured command/OpenAI paths usable from daemon; frontend only needs payload commit/candidate display first. |
| Config CLI/GUI management | `src/cli/config/*`, `src/gui/*` | Typed config and diagnostics exist; no Rust GUI; legacy GUI not ported. | Do not block E2E on GUI. Provide CLI/config-file workflow and minimal dev docs first. |
| Registry/resources | `src/common/registry/*`, GUI resource page | Rust registry library is strong; no full user-level install orchestration/config mutation. | Add a simple install command later; not required for first E2E if config can point at existing helper/model paths. |
| Packaging/systemd | `systemd/*`, `packaging/*`, addon metadata | Rust CI/tooling exists; install artifacts not ported. | Add dev install recipe: daemon binary, service file, addon metadata, and Fcitx addon build/install. |
| i18n/GUI polish | `po/*`, `i18n/*`, `src/gui/*` | Mostly not ported. | Defer until E2E input works. Keep strings simple and English-first for dev path. |

## New phase target: usable E2E IME

A task is considered done only when it moves toward one of these acceptance checks:

1. A user can build/install the Rust daemon and a thin Fcitx5 addon from this repository.
2. Fcitx shows a vinput entry/menu/key action.
3. Pressing the trigger starts recording or command recording through the daemon.
4. Stopping recognition produces a legacy recognition payload.
5. The Fcitx addon commits the final text and can show candidate/menu state when available.
6. A deterministic dev path works without network credentials: command ASR helper plus command text adapter or mock backend.
7. `just e2e-demo`, `just addon-dbus-smoke`, and `just addon-install-smoke` remain green so the backend demo, daemon D-Bus ABI, and staged Fcitx addon install shape stay locally verifiable within CI/desktop constraints.


## Implementation phases

### Phase 0 — keep the repo aligned

- Keep main green.
- Preserve protocol compatibility tests for every wire-shape change.
- Use existing Rust crates instead of adding backend logic to the frontend bridge.
- Build the first usable input-method path before polishing every internal seam.

### Phase 1 — retained frontend bridge

Goal: maintain the smallest Fcitx5 frontend bridge that triggers the Rust backend, commits returned text, and covers only the frontend behavior needed for the E2E product spine.

Tasks:

- Maintain the tracked C++ frontend bridge directory under `cpp/fcitx5-addon`.
- Keep only minimal legacy frontend behavior from `src/addon/*`.
- Preserve registration, trigger action, bus client wrapper, basic state display, final text commit, candidate menu, and selected-text replacement smoke coverage.
- Add new frontend behavior only when it is needed for the E2E product spine.
- Keep build notes and dev install metadata aligned with the retained addon bridge.


### Phase 2 — local run path

Goal: make the Rust backend easy to start for local desktop testing.

Tasks:

- Document how to run `vinput-daemon` on the session bus.
- Keep manual install notes aligned with the retained frontend bridge and addon metadata.
- Keep `--configured-backends` explicit for real command/OpenAI paths.
- Keep diagnostics safe and redacted.


### Phase 3 — minimal audio input path

Goal: move beyond file/mock input for local E2E testing.

Tasks:

- Fill in the live recorder path behind the existing optional audio feature.
- Use signed 16-bit 16 kHz mono PCM first.
- Forward complete-frame chunks through the recorder callback.
- Return captured PCM on stop.
- Keep local desktop probes separate from default CI.

### Phase 4 — recognition path

Goal: make a configured recognizer produce final text in the product path.

Tasks:

- Start with the helper route because it is already wired and deterministic.
- Add a documented example helper for local E2E testing.
- Add the minimal local model runtime after the frontend bridge works.
- Leave streaming polish, VAD, warmup, and reload polish for later slices.

### Phase 5 — text finishing path

Goal: keep configured text finishing usable from the product flow.

Tasks:

- Preserve legacy candidate order.
- Keep configured adapters working.

### Phase 6 — registry resource path

Goal: make model and adapter resources easier to prepare.

Tasks:

- Add a CLI path that composes existing registry boundaries.
- Print follow-up config edits first.
- Add automatic config edits later after rollback tests exist.

### Phase 7 — release polish

Goal: move from dev workflow to release outputs.

Tasks:

- Add release files after the input path works.
- Decide whether to retain the legacy Qt UI or defer UI work.
- Port i18n and resource UI later.

## Work selection rules for agents

- Prefer E2E-enabling work over generic cleanup.
- Do not start large rewrites without a direct acceptance check.
- Keep commits small but not artificially tiny.
- Every behavior or ABI change needs a focused test.
- Docs-only changes should still run relevant docs or architecture tests.

## Recommended next task for a new agent

Continue Phase 1/2 with focused retained frontend bridge slices: keep addon smoke coverage tight, exercise the bridge against the Rust daemon, and align local run/install documentation with the current `cpp/fcitx5-addon` workflow. Defer local model runtime, registry resource preparation, and GUI work until the product spine remains easy to validate.
