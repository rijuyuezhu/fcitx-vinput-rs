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
    cargo run -q -p vinput-cli -- registry
    cargo run -q -p vinput-cli -- mock-result '你好'
    cargo run -q -p vinput-daemon -- print-config
    cargo run -q -p vinput-daemon -- asr-state
    cargo run -q -p vinput-daemon -- text-adapters
    cargo run -q -p vinput-daemon -- --once

# Run the mock legacy D-Bus service on the current session bus.
dbus:
    cargo run -p vinput-daemon -- --dbus
