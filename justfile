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
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
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

addon-fcitx-build:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon-fcitx -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    cmake --build target/cpp/fcitx5-addon-fcitx --parallel

addon-install-smoke: addon-fcitx-build
    rm -rf target/tmp/fcitx-addon-install-smoke
    cmake --install target/cpp/fcitx5-addon-fcitx --prefix target/tmp/fcitx-addon-install-smoke
    test -f target/tmp/fcitx-addon-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    test -f target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'Library=fcitx5-vinput' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'Type=SharedLibrary' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'OnDemand=False' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'Configurable=False' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx '0=dbus' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx '1=clipboard' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    ! grep -qE '^(Name|Comment)\[' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    test -f target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Name=org.fcitx.Vinput' target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus' target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

# Stage the Rust daemon, Fcitx addon, metadata, and DBus activation service together.
ime-install-smoke: addon-fcitx-build
    cargo build -p vinput-daemon
    rm -rf target/tmp/fcitx-ime-install-smoke
    install -Dm755 target/debug/vinput-daemon target/tmp/fcitx-ime-install-smoke/usr/local/bin/vinput-daemon
    cmake --install target/cpp/fcitx5-addon-fcitx --prefix target/tmp/fcitx-ime-install-smoke
    test -x target/tmp/fcitx-ime-install-smoke/usr/local/bin/vinput-daemon
    test -f target/tmp/fcitx-ime-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    test -f target/tmp/fcitx-ime-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    test -f target/tmp/fcitx-ime-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus' target/tmp/fcitx-ime-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

# Stage a configured demo IME install that activates command ASR/text backends.
ime-configured-install-smoke:
    cargo build -p vinput-daemon
    rm -rf target/cpp/fcitx5-addon-fcitx-configured target/tmp/fcitx-ime-configured-install-smoke
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon-fcitx-configured -DCMAKE_BUILD_TYPE=Debug -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON -DVINPUT_DAEMON_ARGS='--dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json --wav /usr/local/share/fcitx-vinput/e2e-command-demo.wav'
    cmake --build target/cpp/fcitx5-addon-fcitx-configured --target fcitx5_vinput_addon --parallel
    install -Dm755 target/debug/vinput-daemon target/tmp/fcitx-ime-configured-install-smoke/usr/local/bin/vinput-daemon
    install -Dm644 data/e2e-command-demo-config.json target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo-config.json
    python3 scripts/write-demo-wav.py target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo.wav
    cmake --install target/cpp/fcitx5-addon-fcitx-configured --prefix target/tmp/fcitx-ime-configured-install-smoke
    test -x target/tmp/fcitx-ime-configured-install-smoke/usr/local/bin/vinput-daemon
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo-config.json
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo.wav
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json --wav /usr/local/share/fcitx-vinput/e2e-command-demo.wav' target/tmp/fcitx-ime-configured-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

addon-lint:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}

addon-test:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-smoke:
    clang-format --dry-run --Werror {{addon-sources}}
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-dbus-smoke:
    scripts/run-cpp-dbus-smoke.sh

addon-dbus-activation-smoke:
    scripts/run-cpp-dbus-activation-smoke.sh

addon-dbus-configured-activation-smoke:
    scripts/run-cpp-dbus-configured-activation-smoke.sh

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
    VINPUT_TEST_PIPEWIRE_CONTEXT=1 VINPUT_TEST_PIPEWIRE_ENUMERATE=1 VINPUT_TEST_PIPEWIRE_RECORD=1 cargo test -p vinput-audio --features pipewire-backend pipewire_ -- --nocapture

# Run a deterministic file-input E2E demo through command ASR and text adapter.
e2e-demo:
    python3 scripts/write-demo-wav.py target/tmp/vinput-demo.wav
    cargo run -q -p vinput-daemon -- --config data/e2e-command-demo-config.json --configured-backends --once --wav target/tmp/vinput-demo.wav

# Run the mock legacy D-Bus service on the current session bus.
dbus:
    cargo run -p vinput-daemon -- --dbus

ime-configured-activation-smoke:
    scripts/run-ime-configured-activation-smoke.sh
