# fcitx-vinput-rs

Rust-oriented rewrite workspace for [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The early refactor milestones have produced stable Rust protocol/config/audio/ASR/text/registry/daemon/CLI seams. The current milestone is E2E port acceleration: build a retained thin Fcitx5 frontend bridge, run the Rust daemon as the backend, and get a usable input-method product spine working quickly.

## Current layout

- `crates/vinput-protocol`: D-Bus names, status strings, ASR state, text adapter state, and recognition result JSON.
- `crates/vinput-config`: typed config model for the legacy `data/default-config.json` plus validation.
- `crates/vinput-audio`: pure PCM buffers, capture traits, and deterministic audio transforms.
- `crates/vinput-asr`: ASR backend/session traits, recognition events, command backend seam, and deterministic mock backend.
- `crates/vinput-text`: scene post-processing, prompt rendering, text adapter traits, and command adapter seam.
- `crates/vinput-registry`: registry metadata parsing, validation, and dry-run asset/install planning.
- `crates/vinput-daemon`: mock/configured daemon runtime, diagnostics, and `zbus` service facade for the legacy daemon ABI.
- `crates/vinput-cli`: bootstrap CLI named `vinput` for protocol/config/registry/payload inspection.
- `data/default-config.json`: copied from the original project as the compatibility baseline.
- `AGENT.md`: required short instruction file for coding agents.
- `docs/README.md`: documentation map and required reading order.
- `docs/development.md`: project style, commit message style, and `just` command guide.
- `docs/migration/e2e-port-plan.md`: tracked E2E migration plan and Rust-vs-legacy comparison.
- `docs/migration/agent-kickoff.md`: copyable context for a fresh implementation agent.
- `docs/architecture/README.md`: tracked architecture contract index.
- `docs/legacy/`: tracked original-source annotations.

Local planning notes under `docs/plan/` are intentionally ignored by the root `.gitignore`. Do not manually track them.

## Tooling

The repo pins shared project tooling in:

- `rust-toolchain.toml`: stable Rust with `rustfmt` and `clippy` components.
- `rustfmt.toml`: formatting policy.
- `clippy.toml` plus workspace lints in `Cargo.toml`: lint policy.
- `.pre-commit-config.yaml`: local pre-commit hooks for format and lint checks.
- `justfile`: common commands used locally and mirrored by CI.

Install optional local hooks with:

```sh
pre-commit install
```

## Smoke checks

```sh
just ci
just smoke
```

`just ci` mirrors the GitHub Actions checks, including C++ addon format/lint/test coverage and the D-Bus integration feature lint.

Equivalent raw commands:

```sh
clang-format --dry-run --Werror {{addon-sources}}
cargo fmt --all -- --check
cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings
cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
cmake --build target/cpp/fcitx5-addon --parallel
ctest --test-dir target/cpp/fcitx5-addon --output-on-failure
cargo run -q -p vinput-cli -- protocol
cargo run -q -p vinput-cli -- config
cargo run -q -p vinput-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinput-cli -- asr-state
cargo run -q -p vinput-cli -- asr-state --config data/default-config.json
cargo run -q -p vinput-cli -- audio-devices
cargo run -q -p vinput-cli -- registry
cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
cargo run -q -p vinput-cli -- mock-result '你好'
cargo run -q -p vinput-daemon -- print-config
cargo run -q -p vinput-daemon -- asr-state
cargo run -q -p vinput-daemon -- text-adapters
cargo run -q -p vinput-daemon -- audio-devices
cargo run -q -p vinput-daemon -- --once
```

Use `cargo run -p vinput-cli -- asr-state --config path/to/config.json` to inspect ASR diagnostics for a custom config without starting daemon runtime backends. Use `cargo run -p vinput-cli -- audio-devices` or `cargo run -p vinput-daemon -- audio-devices` to inspect capture-device config and, when built with the optional PipeWire feature, live source enumeration.

`data/default-config.json` and `data/sample-registry-index.json` are stable smoke fixtures for explicit config and registry CLI paths. See [`docs/architecture/config-contract.md`](docs/architecture/config-contract.md) and [`docs/architecture/registry-contract.md`](docs/architecture/registry-contract.md) for their fixture contracts.

## Local E2E demo

Run the deterministic file-input demo with:

```sh
just e2e-demo
```

The recipe generates `target/tmp/vinput-demo.wav`, then runs `vinput-daemon --configured-backends --once --wav` with `data/e2e-command-demo-config.json`. This exercises the current product spine end to end: WAV input, command ASR, command text adapter, and final recognition JSON. The demo ASR reports the input byte count instead of performing real speech recognition, which keeps the path deterministic until the concrete ASR backend lands.

Stage the Rust daemon, Fcitx addon module, addon metadata, and D-Bus activation service together with:

```sh
just ime-install-smoke
just ime-configured-install-smoke
```

`just ime-configured-install-smoke` additionally stages `data/e2e-command-demo-config.json` and wires D-Bus activation to `vinput-daemon --dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json`.

This staged install shape is the current local packaging spine for the input method: Fcitx loads `fcitx5-vinput.so`, the addon talks to `org.fcitx.Vinput`, and the D-Bus service activates `vinput-daemon --dbus` from the same install prefix. To activate configured command ASR/text backends from Fcitx, configure the addon CMake build with `-DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config /path/to/config.json"`.

Run `just addon-dbus-activation-smoke` to verify that a staged D-Bus service file can activate the Rust daemon for the C++ bridge client without manually starting `vinput-daemon` first. Run `just addon-dbus-configured-activation-smoke` to exercise the same activation path with `--configured-backends` and the command ASR demo config.

Run the mock D-Bus service inside an existing session bus with:

```sh
cargo run -p vinput-daemon -- --dbus
```

The daemon now accepts `--audio-backend mock|pipewire` for long-running D-Bus sessions. `mock` remains the default for deterministic CI and staged demos. `pipewire` is feature-gated behind `--features pipewire-backend` and selects the live recorder seam for the next desktop-capture slice.

## Development route

The current route is E2E port acceleration. Start with `AGENT.md`, then read `docs/README.md`, `docs/development.md`, and `docs/migration/e2e-port-plan.md`.

1. Keep `vinput-protocol` ABI-compatible with the legacy Fcitx5 addon contract.
2. Build a retained thin C++ Fcitx5 frontend bridge over the Rust daemon instead of moving backend logic into C++.
3. Keep `just e2e-demo`, `just smoke`, and targeted crate tests green while adding product-spine functionality.
4. Implement the fastest usable path first: daemon launch, frontend trigger/commit, configured command recognition, and configured text finishing.
5. Defer GUI polish, full registry resource orchestration, and release packaging until the input method is usable end to end.
