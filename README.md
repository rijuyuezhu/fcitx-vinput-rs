# fcitx-vinput-rs

Rust-oriented rewrite workspace for [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The first milestones are intentionally small: preserve the public daemon/frontend contract, make the config/protocol types testable, run a mock daemon loop, expose that mock runtime through the legacy D-Bus ABI, and introduce an ASR trait boundary before replacing the original C++ backend pieces one by one.

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
- `docs/architecture/README.md`: architecture notes index and contract map.
- `docs/legacy/`: tracked original-source annotations.

Local planning notes under `docs/plan/` are intentionally ignored by that directory's local `.gitignore`.

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

`just ci` mirrors the GitHub Actions checks, including the D-Bus integration feature lint.

Equivalent raw commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings
cargo run -q -p vinput-cli -- protocol
cargo run -q -p vinput-cli -- config
cargo run -q -p vinput-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinput-cli -- asr-state
cargo run -q -p vinput-cli -- asr-state --config data/default-config.json
cargo run -q -p vinput-cli -- registry
cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
cargo run -q -p vinput-cli -- mock-result '你好'
cargo run -q -p vinput-daemon -- print-config
cargo run -q -p vinput-daemon -- asr-state
cargo run -q -p vinput-daemon -- text-adapters
cargo run -q -p vinput-daemon -- --once
```

Use `cargo run -p vinput-cli -- asr-state --config path/to/config.json` to inspect ASR diagnostics for a custom config without starting daemon runtime backends.

`data/default-config.json` and `data/sample-registry-index.json` are stable smoke fixtures for explicit config and registry CLI paths. See [`docs/architecture/config-contract.md`](docs/architecture/config-contract.md) and [`docs/architecture/registry-contract.md`](docs/architecture/registry-contract.md) for their fixture contracts.

## Local E2E demo

Run the deterministic file-input demo with:

```sh
just e2e-demo
```

The recipe generates `target/tmp/vinput-demo.wav`, then runs `vinput-daemon --configured-backends --once --wav` with `data/e2e-command-demo-config.json`. This exercises the current product spine end to end: WAV input, command ASR, command text adapter, and final recognition JSON. The demo ASR reports the input byte count instead of performing real speech recognition, which keeps the path deterministic until the concrete ASR backend lands.

Run the mock D-Bus service inside an existing session bus with:

```sh
cargo run -p vinput-daemon -- --dbus
```

## Development route

1. Keep `vinput-protocol` ABI-compatible with the existing C++ Fcitx5 addon.
2. Port config and pure data transformations with tests first.
3. Keep the `zbus` daemon service behind the same methods/signals.
4. Replace mock runtime edges with PipeWire audio capture, concrete ASR sessions, post-processing, registry/download, adapter supervision, and packaging.
5. Annotate every original `fcitx5-vinput/src` file before porting complex behavior.
