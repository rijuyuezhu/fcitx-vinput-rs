set dotenv-load := false

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

dbus-lint:
    cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings

test:
    cargo test --workspace --all-targets

dbus-test:
    dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration

check: fmt-check lint test dbus-test dbus-lint

ci: check

smoke:
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

# Compile and test optional PipeWire feature paths without requiring a live daemon.
pipewire-check:
    cargo test -p vinput-audio --features pipewire-backend
    cargo clippy -p vinput-audio --all-targets --features pipewire-backend -- -D warnings

# Run explicit local PipeWire probes. Requires a live user PipeWire session.
pipewire-live:
    VINPUT_TEST_PIPEWIRE_CONTEXT=1 VINPUT_TEST_PIPEWIRE_ENUMERATE=1 cargo test -p vinput-audio --features pipewire-backend pipewire_ -- --nocapture

# Run a deterministic file-input E2E demo through command ASR and text adapter.
e2e-demo:
    python3 scripts/write-demo-wav.py target/tmp/vinput-demo.wav
    cargo run -q -p vinput-daemon -- --config data/e2e-command-demo-config.json --configured-backends --once --wav target/tmp/vinput-demo.wav

# Run the mock legacy D-Bus service on the current session bus.
dbus:
    cargo run -p vinput-daemon -- --dbus
