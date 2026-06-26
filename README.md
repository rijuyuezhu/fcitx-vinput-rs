# fcitx-vinput-rs

Rust-oriented rewrite workspace for [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The first milestone is intentionally small: preserve the public daemon/frontend contract, make the config/protocol types testable, and run a mock daemon loop before replacing the original C++ backend pieces one by one.

## Current layout

- `crates/vinput-protocol`: D-Bus names, status strings, ASR state, and recognition result JSON.
- `crates/vinput-config`: typed model for the legacy `data/default-config.json` plus initial validation.
- `crates/vinput-daemon`: mock daemon runtime and one-shot command for early end-to-end checks.
- `crates/vinput-cli`: bootstrap CLI named `vinput` for protocol/config/payload inspection.
- `data/default-config.json`: copied from the original project as the compatibility baseline.
- `docs/architecture/`: tracked architecture notes.

Local planning notes under `docs/plan/` are intentionally ignored outside git.

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

## Development route

1. Keep `vinput-protocol` ABI-compatible with the existing C++ Fcitx5 addon.
2. Port config and pure data transformations with tests first.
3. Add a real `zbus` daemon service behind the same methods/signals.
4. Replace mock runtime edges with PipeWire audio capture, ASR sessions, post-processing, registry/download, adapter supervision, and packaging.
5. Annotate every original `fcitx5-vinput/src` file before porting complex behavior.
