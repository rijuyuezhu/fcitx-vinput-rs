# fcitx-vinput-rs

Rust-oriented rewrite workspace for [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The first milestones are intentionally small: preserve the public daemon/frontend contract, make the config/protocol types testable, run a mock daemon loop, expose that mock runtime through the legacy D-Bus ABI, and introduce an ASR trait boundary before replacing the original C++ backend pieces one by one.

## Current layout

- `crates/vinput-protocol`: D-Bus names, status strings, ASR state, and recognition result JSON.
- `crates/vinput-config`: typed model for the legacy `data/default-config.json` plus initial validation.
- `crates/vinput-asr`: ASR backend/session traits, recognition events, payload conversion, and deterministic mock backend.
- `crates/vinput-daemon`: mock daemon runtime, library modules, and `zbus` service facade for the legacy daemon ABI.
- `crates/vinput-cli`: bootstrap CLI named `vinput` for protocol/config/payload inspection.
- `data/default-config.json`: copied from the original project as the compatibility baseline.
- `docs/architecture/`: tracked architecture notes.
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

Equivalent raw commands:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p vinput-cli -- protocol
cargo run -p vinput-cli -- config
cargo run -p vinput-cli -- mock-result '你好'
cargo run -p vinput-daemon -- --once
```

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
