set dotenv-load := false

addon-sources := `find cpp/fcitx5-addon -type f \( -name '*.cpp' -o -name '*.h' \) | sort | tr '\n' ' '`
addon-lint-sources := `find cpp/fcitx5-addon -type f -name '*.cpp' | sort | tr '\n' ' '`

fmt:
    clang-format -i {{addon-sources}}
    cargo fmt --all

fmt-check:
    clang-format --dry-run --Werror {{addon-sources}}
    cargo fmt --all -- --check

lint:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
    cargo clippy --workspace --all-targets -- -D warnings

dbus-lint:
    cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings

test:
    cargo test --workspace --all-targets

dbus-test:
    dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration

check: fmt-check lint test dbus-test dbus-lint addon-test

addon-format:
    clang-format -i {{addon-sources}}

addon-format-check:
    clang-format --dry-run --Werror {{addon-sources}}

addon-configure:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json

addon-build:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    cmake --build target/cpp/fcitx5-addon --parallel

addon-lint:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}

addon-test:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-smoke:
    clang-format --dry-run --Werror {{addon-sources}}
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-dbus-smoke:
    scripts/run-cpp-dbus-smoke.sh

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
    cargo test -p vinput-daemon --features=pipewire-backend
    cargo clippy -p vinput-daemon --all-targets --features=pipewire-backend -- -D warnings
    cargo test -p vinput-audio --features=pipewire-backend
    cargo clippy -p vinput-audio --all-targets --features=pipewire-backend -- -D warnings
    cargo test -p vinput-cli --features=pipewire-backend --test audio_devices
    cargo clippy -p vinput-cli --all-targets --features=pipewire-backend -- -D warnings

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
