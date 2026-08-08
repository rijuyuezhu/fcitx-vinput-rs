# Development guide

This file defines repository workflow, validation tiers, and commit style. Progress belongs in [`migration/function-gap-audit.md`](migration/function-gap-audit.md); priorities belong in [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md).

## Project boundaries

Keep the workspace split by responsibility:

- `vinpst-protocol`: public D-Bus and JSON wire contracts.
- `vinpst-config`: typed config, defaults, normalization, validation, persistence, and shared diagnostic redaction.
- `vinpst-http`: shared provider HTTP client construction, bounded additional-CA loading, and URL-free transport error categories.
- `vinpst-process`: shared Unix process-group supervision, deadlines, descendant cleanup, and bounded output capture.
- `vinpst-audio`: PCM types, pure processing, recorder traits, and audio backends.
- `vinpst-asr`: ASR traits, sessions, command backends, remote backends, and native backends.
- `vinpst-text`: prompts, context cache, text adapters, and provider transports.
- `vinpst-daemon-control`: shared typed user-service command construction and execution for CLI/GUI daemon lifecycle controls.
- `vinpst-registry`: registry schemas, safe downloads, extraction, and managed publication.
- `vinpst-daemon`: runtime orchestration and D-Bus service facade.
- `vinpst-fcitx-core`: safe, Fcitx-independent frontend state machines and presentation policy.
- `vinpst-fcitx-dbus`: safe blocking zbus transport with typed reply decoding and the legacy 60-second call deadline.
- `vinpst-fcitx-ffi`: the narrow static C ABI consumed by the retained addon.
- `vinpst-cli`: user-facing commands and diagnostics over library crates.
- `vinpst-gui`: the standalone Rust/Iced management application over shared typed APIs.

The retained C++ frontend owns Fcitx API integration, menus, preedit/commit presentation, selected-text handling, notifications, and the bus bridge. Backend state and processing belong in Rust.

### Source organization

Keep public facades thin and place use-case logic behind domain modules:

- `vinpst-cli/src/main.rs` is routing only. Clap data lives under `cli/`, command use cases under `commands/`, daemon lifecycle under `daemon_control/`, and shared path/config/registry/output services in focused support modules.
- `vinpst-config/src/lib.rs` re-exports the public schema. Schema data, defaults/normalization, validation, file behavior, errors, and tests are separate modules.
- `vinpst-asr/src/sherpa/` separates the public typed specification, offline layout/path inference, and the feature-gated runtime backend.
- the retained Fcitx addon separates recording/daemon integration from Scene/ASR menu implementation; do not move backend policy into C++.
- `scripts/` is grouped by deterministic tests, release operations, installation, fixtures, opt-in live evidence, and developer tools. Keep shell/Python here short and functional: reusable behavior and semantic test logic belong in Rust crates whenever practical, while `scripts/tests/` is reserved for process/package integration that cannot be expressed cleanly as crate tests. The `justfile` is a thin facade for broad workflows; specialized gates are invoked directly from their documented script paths.

`scripts/tests/source-layout-check.sh` prevents production Rust/C++ files from growing beyond 1200 lines and gives fixture-heavy tests a 3000-line ceiling. Treat the limits as regression guards, not as targets: split earlier when data, orchestration, transport, formatting, or platform integration form distinct reasons to change.

## Coding rules

- Preserve service names, method and signal names, status strings, recognition JSON, config semantics, and frontend expectations.
- Add focused compatibility tests when a public contract changes.
- Prefer `pub(crate)` for implementation helpers and keep public APIs small.
- Workspace Rust uses edition 2024, MSRV 1.88, and Clippy pedantic warnings. Safe crates inherit `unsafe_code = "forbid"`. `vinpst-fcitx-ffi` is the only exception: the crate denies unsafe by default and explicitly allows it only in raw-pointer translation modules; safe frontend policy and D-Bus transport remain in `vinpst-fcitx-core` and `vinpst-fcitx-dbus`.
- Every C ABI mutation must keep the public header, built archive symbols, C++ adapters, and focused behavior tests aligned. `scripts/tests/check_fcitx_ffi_abi.py` compares the published header with the actual static-library exports; do not replace that contract with source-text assertions.
- Keep code, comments, test names, documentation identifiers, and commit messages in English.
- Prefer milestone-enabling work over generic cleanup.
- Never treat deterministic seams as live desktop proof.
- Never commit files under ignored `docs/plan/`.

## Local workflow

Use the narrowest check that proves the change while iterating. Before handoff, run the complete relevant tier.

```sh
just fmt
just fmt-check
just test
just lint
just check
just ci
just docs
```

`just ci` is the deterministic project gate. It includes Rust checks, D-Bus integration, retained-addon checks, staged integration, temporary-HOME user-install smokes, and lightweight Arch, Debian, Nix, RPM, source-archive, and release metadata validation. Live desktop, microphone, and full package builds are excluded by design.

## Validation tiers

### Documentation-only changes

```sh
git diff --check
just docs
```

The MkDocs build runs in strict mode and checks navigation plus internal links. Documentation is still reviewed as documentation: do not add tests that assert exact README wording, architecture prose, docstrings, source declarations, recipe names, or other implementation text. Run behavior tests only when documentation changes public commands, fixtures, generated artifacts, or executable contracts.

The upstream source/callable inventory is generated separately from a clean C++ checkout:

```sh
scripts/tools/generate-upstream-inventory.py \
  --upstream-root /path/to/fcitx5-vinput
scripts/tests/check-upstream-inventory.py
```

The JSON inventory detects source and callable drift. Human review groups those entries by user-visible capability in [`migration/user-capability-audit.md`](migration/user-capability-audit.md); it does not require one Rust function per C++ function.

### Rust and core behavior

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

### D-Bus integration

```sh
just test
just lint
```

`just test` runs the isolated session-bus suite and covers legacy methods, configured backends, reload, adapters, and partial-before-stop behavior.

### Retained C++ frontend

```sh
just fmt-check
just test
```

Run `just lint` when Fcitx5 headers and `clang-tidy` are available.

### Deterministic addon and IME paths

```sh
scripts/tests/cpp/run-cpp-dbus-smoke.sh
scripts/tests/cpp/run-cpp-dbus-asr-menu-smoke.sh
scripts/tests/cpp/run-cpp-dbus-activation-smoke.sh
scripts/tests/cpp/run-cpp-dbus-configured-activation-smoke.sh
scripts/tests/daemon/run-dbus-adapter-lifecycle-smoke.sh
scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh
scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh
scripts/tests/daemon/run-daemon-handoff-smoke.sh
scripts/tests/install/run-ime-e2e-smoke.sh
```

`scripts/tests/install/run-ime-e2e-smoke.sh` includes fake outcome sink coverage. `scripts/tests/daemon/run-dbus-adapter-lifecycle-smoke.sh` verifies configured text adapter start/duplicate-start/stop diagnostics through the Rust CLI and daemon D-Bus API. `scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh` launches the normal daemon in a private session, proves its HTTP health endpoint, D-Bus owner, and redacted endpoint diagnostics, sends `SIGTERM`, and verifies listener release. `scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh` proves that `daemon status` detects both a D-Bus owner running from a different daemon path and a replaced executable whose old inode appears as ` (deleted)`, while remaining non-mutating. `scripts/tests/daemon/run-daemon-handoff-smoke.sh` proves the explicit conditional restart command: current owners never invoke systemctl, stale owners restart and pass a fresh owner-path check, and failed service control leaves the old owner alive.

### User installation

Use a temporary `HOME` unless mutation of the real profile is explicitly requested:

```sh
scripts/tests/install/run-user-ime-command-demo-smoke.sh
scripts/tests/install/run-user-ime-activation-owner-smoke.sh
scripts/tests/install/run-user-ime-real-command-asr-wav-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh
scripts/tests/install/run-user-ime-sherpa-sense-voice-smoke.sh
```

`scripts/install/install-user-ime.sh` normally uses `target/debug/vinpst` and `target/debug/vinpst-daemon`. Tests that provide stubs must use `VINPST_USER_CLI_BINARY` and `VINPST_USER_DAEMON_BINARY` under their own temporary tree. Never overwrite Cargo outputs: Cargo fingerprints do not detect external binary replacement reliably.

The `sherpa-native-live` profile validates and copies `libsherpa-onnx` and `libonnxruntime`, creates `vinpst-daemon-with-vinpst-env.sh`, and runs `runtime-status` through the installed bundle. `sherpa-native-command-live` uses the same native runtime and adds a deterministic command adapter for real frontend validation; `sherpa-sense-voice-live` remains a compatibility alias. Set `VINPST_USER_RUNTIME_STATUS=0` only for file-placement debugging.

### Arch packaging

The checked source of truth is `packaging/arch/PKGBUILD.in`; render release-specific source metadata with `scripts/release/render-arch-pkgbuild.py`.

```sh
scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-signature.sh
scripts/release/check-arch-release-candidate.sh
just package-smoke
scripts/release/run-arch-package-transaction-smoke.sh
scripts/release/run-arch-repository-smoke.sh
scripts/release/run-arch-signing-smoke.sh
scripts/release/run-arch-release-bundle-smoke.sh
```

`scripts/release/check-release-manifest.sh` validates the strict flat-bundle schema, exact inventory, sorted checksums, atomic staging, safe `--force` replacement, and negative mutation/extra/symlink cases with tiny local fixtures. `scripts/release/check-release-signature.sh` creates ephemeral keys under `target/tmp`, proves atomic detached-manifest signing plus isolated external-key/fingerprint verification, and rejects missing/tampered signatures, manifest or artifact changes, wrong trust roots, bundled-key trust, and stale signatures after bundle rebuild. `scripts/release/check-arch-release-candidate.sh` builds minimal signed Arch packages and proves that promotion selects only the formal package, rebuilds single-version repository metadata, removes every test/synthetic role, signs the new candidate, and refuses unsafe force/output paths. `scripts/release/check-arch-install-script.sh` executes package hooks with an empty `PATH`; it proves post-install/post-remove guidance, successful and failing upgrade-helper propagation, removal-helper invocation, and the absence of unqualified user-session commands. `scripts/release/check-arch-pkgbuild.sh` and `scripts/release/check-rpm-spec.sh` are the lightweight deterministic package metadata gates included in `just ci`; both use the same strict runtime-bundle loader. `just package-smoke` is the explicit Arch release gate: it downloads checksum-pinned sherpa/ONNX Runtime assets when absent, builds a clean package through `makepkg`, verifies the embedded `.INSTALL`, extracts it without touching the host profile, validates the full file set and private rpaths, runs the packaged CLI/daemon/GUI including the display-independent GUI binary/config self-check, creates a `pkgrel=2` repackage, and proves direct pacman install/upgrade/same-version rollback/removal, local-repository install/upgrade, and signed-repository trust/tamper enforcement. `just rpm-package-smoke` renders and builds RPM releases 1/2, validates metadata/scriptlets/payload/rpaths/linkage/GUI, and uses an unprivileged user namespace for isolated install/upgrade/verify/removal while preserving unsupported future user config. Full package builds remain outside routine CI because they compile release artifacts and may download fixed assets on a cold cache. `scripts/release/run-arch-package-transaction-smoke.sh` reruns only the fast fakeroot direct-package transaction; `scripts/release/run-arch-repository-smoke.sh` reruns the unsigned `repo-add` plus `file://` path; `scripts/release/run-arch-signing-smoke.sh` creates only ephemeral keys under `target/tmp` and proves trusted signatures plus unknown-signer and tamper rejection. `scripts/release/run-arch-release-bundle-smoke.sh` assembles the source archive, rendered Arch metadata, both release-gate package revisions, package/database signatures, repository databases, and ephemeral public key into an exact `manifest.json` plus `SHA256SUMS` inventory, signs `manifest.json`, and verifies `manifest.json.sig` against the public key outside the bundle and a pinned fingerprint; the synthetic `pkgrel=2` and test key are explicitly labeled as test roles rather than public release assets. The same gate then promotes only `pkgrel=1` into an 11-role candidate with freshly signed repository metadata and verifies that no test role or `pkgrel=2` file remains.

### Debian and Nix packaging

```sh
just deb-package-smoke
just nix-package-smoke
```

The Debian gate builds and transaction-tests Debian 12 and Ubuntu 24.04 packages inside Docker. The Nix gate evaluates the locked flake, builds the closure, runs the display-independent GUI binary/config check, and validates the addon and activation metadata. Both are release-grade build gates; production Nix binary-cache publication remains separate.

### Native ASR evidence

Generic local recipes validate typed `vinpst-model.json`, runtime construction, and one WAV recognition outside Fcitx5:

```sh
scripts/tests/asr/run-sherpa-offline-local-smoke.sh
scripts/tests/asr/run-sherpa-sense-voice-local-smoke.sh
scripts/tests/asr/run-sherpa-family-smoke.sh offline-transducer
scripts/tests/asr/run-sherpa-family-smoke.sh dolphin
scripts/tests/asr/run-sherpa-family-smoke.sh paraformer
scripts/tests/asr/run-sherpa-family-smoke.sh qwen3
scripts/tests/asr/run-sherpa-online-local-smoke.sh
scripts/tests/asr/run-sherpa-family-smoke.sh online-transducer
scripts/tests/asr/run-sherpa-family-smoke.sh zipformer2-ctc
scripts/tests/asr/run-sherpa-family-smoke.sh moonshine-reload
```

Model-dependent recipes require the documented `VINPST_SHERPA_*` environment values. They are evidence for model/runtime support, not proof of live microphone or application behavior.

### Optional live checks

Run only when the corresponding real PipeWire, Fcitx5, browser, network, or desktop boundary is available:

```sh
scripts/tests/pipewire-check.sh
scripts/live/audio/run-pipewire-tests-live.sh
VINPST_TEST_PIPEWIRE_RECORD=1 VINPST_TEST_PIPEWIRE_RECORD_MS=12000 VINPST_TEST_PIPEWIRE_MIN_PEAK=1000 cargo test -p vinpst-audio --features pipewire-backend pipewire_recorder_live_capture_when_enabled -- --nocapture
scripts/live/audio/run-cpp-dbus-pipewire-live-smoke.sh
scripts/live/audio/run-ime-pipewire-live-smoke.sh
scripts/live/audio/run-ime-configured-pipewire-live-smoke.sh
scripts/live/niri/run-ime-fcitx-live-probe.sh
VINPST_REMOTE_TEXT_BROWSER=/path/to/chromium scripts/live/network/run-remote-text-chromium-lan-live.sh
VINPST_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1 VINPST_REMOTE_TEXT_EXTERNAL_TIMEOUT=180 scripts/live/network/run-remote-text-external-device-live.sh
scripts/live/niri/run-ime-fcitx-remote-asr-live.sh
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav VINPST_LIVE_NATIVE_MODES=command VINPST_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter VINPST_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' VINPST_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-command-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav VINPST_LIVE_NATIVE_FOCUS_SWITCH=1 VINPST_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-focus-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav VINPST_LIVE_NATIVE_OWNER_LOSS=1 VINPST_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-owner-loss-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav VINPST_LIVE_RELOAD_BEFORE_PROBE=1 VINPST_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-reload-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPST_LIVE_TOOLKIT_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk3-native-live.sh normal
VINPST_LIVE_TOOLKIT_WAV=/path/to/speech.wav scripts/live/niri/run-ime-qt6-native-live.sh normal
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-vscode-virtual-live.sh normal
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-vscode-virtual-live.sh command
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh normal 10
VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh command 10
scripts/tools/bench-capture-cold-start.sh --follow
```

The live recipes are intentionally excluded from `just ci`. Management-GUI interaction automation is not retained. Real window/widget interaction is validated manually; automated GUI coverage stops below the Iced window/widget boundary and belongs in crate-internal semantic state/message/persistence tests. Retained live recipes exercise Fcitx/input-method, toolkit text-entry, audio, notifications, and provider transport. `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` remains the primary isolated audio evidence gate and restores configuration, backup state, daemon ownership, and virtual PipeWire nodes. The remote-ASR and remote-text gates exercise real provider transports against isolated fixtures, while deterministic network smokes cover proxy routing, authentication, CA rotation, timeouts, redirects, response bounds, and credential redaction. Preserve JSONL output for every claimed live result, and classify failures at the session, target, format, sample-rate, channel-plan, capture, ASR, frontend, or application boundary.

## Commit style

Use concise English Conventional Commits:

```text
<type>(optional-scope): <imperative summary>
```

Common types are `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `build`, and `chore`.

- Keep one reason to change per commit.
- Do not mix broad refactors with feature work.
- Do not mix implementation, tests, and documentation unless they are inseparable parts of one small change.
- Before commit, run `git diff --check`, inspect the staged diff, and run the relevant validation tier.
