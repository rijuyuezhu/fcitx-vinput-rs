# Development guide

## Project style

- Keep the Rust workspace split by responsibility:
  - `vinput-protocol`: wire names and JSON/D-Bus compatibility.
  - `vinput-config`: typed config, defaults, validation, and legacy normalization decisions.
  - `vinput-audio`: PCM data, audio processing, capture traits, and audio backends.
  - `vinput-asr`: ASR traits, sessions, mock/command backends, and future local ASR backends.
  - `vinput-text`: prompt rendering, context cache, text adapters, and future provider transports.
  - `vinput-registry`: registry schema, validation, planning, and future install mechanics.
  - `vinput-daemon`: runtime orchestration and D-Bus service facade.
  - `vinput-cli`: diagnostics and user-facing command entry points over library crates.
- Preserve user-visible legacy behavior before improving internals: D-Bus ABI, status strings, recognition JSON, config semantics, command-mode behavior, and frontend expectations must stay explicit and tested.
- Prefer E2E-enabling implementation over generic cleanup. The current phase is to get a usable Fcitx input-method product spine working quickly.
- Prefer test-first changes. Pin compatibility behavior with tests before moving code across modules or changing runtime logic.
- Treat mock/seam coverage as contract coverage, not feature parity.
- Keep public APIs small. Prefer `pub(crate)` for helpers after module splits.
- Do not log secrets, environment values, or credential-bearing headers.
- Keep assistant/user communication in Chinese. Keep code, comments, test names, docs identifiers, and commit messages in English unless existing surrounding text requires otherwise.

## Commit message style

Use concise Conventional Commit style:

```text
<type>(optional-scope): <imperative summary>
```

Common types:

- `feat`: user-visible or crate-visible capability.
- `fix`: bug or compatibility fix.
- `refactor`: behavior-preserving restructuring.
- `test`: test-only changes.
- `docs`: documentation-only changes.
- `ci`: CI workflow changes.
- `build`: build system or dependency changes.
- `chore`: maintenance without behavior change.

Examples:

```text
refactor(text): split prompt rendering module
fix(dbus): preserve legacy operation error name
test(config): add legacy normalization golden cases
docs: add agent reading order
ci: run pipewire feature checks
```

Rules:

- Use English commit messages.
- Keep the summary short and imperative.
- Prefer small commits with one reason to change.
- Do not mix pure refactor with feature implementation unless explicitly approved.

## Checks and tests

Use `just` as the primary local interface. The recipes mirror CI and make command intent explicit.

Arch Linux local native dependencies for the current C++ addon slice:

```sh
sudo pacman -S --needed base-devel cmake clang just pkgconf fcitx5
```

`fcitx5` provides the Fcitx5 Core/Utils headers and CMake/pkg-config metadata used by `addon-lint` and `addon-fcitx-build`. Extra Fcitx module development packages are not required for the current thin addon slice.

Common commands:

```sh
just fmt          # format Rust code and C++ addon sources
just fmt-check    # check Rust and C++ addon formatting
just lint         # clang-tidy for addon sources plus clippy for the workspace
just test         # cargo test --workspace --all-targets
just dbus-test    # D-Bus integration tests under dbus-run-session
just dbus-lint    # clippy with dbus-integration feature
just addon-format # format the C++ Fcitx bridge sources with clang-format
just addon-format-check # check C++ Fcitx bridge formatting
just addon-configure # configure the C++ Fcitx bridge CMake project
just addon-build  # build the C++ bridge core without requiring Fcitx desktop deps
just addon-fcitx-build # require Fcitx5Core and build fcitx5-vinput.so
just addon-install-smoke # stage-install fcitx5-vinput.so and verify vinput.conf metadata
just addon-lint   # require Fcitx5Core and lint all C++ addon sources with clang-tidy
just addon-test   # run CTest for the C++ Fcitx bridge core
just addon-smoke  # addon-format-check plus addon-lint plus addon-test
just addon-dbus-smoke # run C++ bridge against Rust daemon over DBus
just check        # fmt-check plus lint plus test plus dbus-test plus dbus-lint plus addon-test
just ci           # alias for check
just smoke        # CLI/daemon smoke commands
just e2e-demo     # deterministic file-input command ASR/text demo
just pipewire-check # optional PipeWire feature compile/tests without live daemon
just pipewire-live  # explicit local PipeWire probes gated by env variables
just dbus         # run the mock/configured legacy D-Bus service on the current session bus
```

`just pipewire-check` is safe for machines with PipeWire development libraries because it does not require a live PipeWire daemon and covers the audio crate plus CLI/daemon audio-device diagnostics with the optional feature enabled. `just pipewire-live` is intentionally excluded from `just ci`; it sets `VINPUT_TEST_PIPEWIRE_CONTEXT=1` and `VINPUT_TEST_PIPEWIRE_ENUMERATE=1` and should only be run on a desktop session where live PipeWire probes are expected to work.

Before proposing a code change, prefer running:

```sh
just ci
just smoke
```

For docs-only changes, at least verify paths and git status. Run full checks when docs alter public contracts, command examples, or test instructions.

## Next work

The next migration phase is E2E port acceleration. Read `docs/migration/e2e-port-plan.md` first and use it as the tracked source of truth for product direction. The old ignored `docs/plan/review-driven-refactor-plan.md` may contain useful notes, but it is no longer the primary plan.

Current priority order:

1. Build a retained thin C++ Fcitx5 frontend bridge that talks to the Rust daemon and commits a mock/configured result.
2. Add local run and dev install documentation for the daemon plus frontend bridge.
3. Fill in the minimal live audio input path behind the existing optional audio feature.
4. Use the configured command recognizer path as the fastest first non-mock recognition route.
5. Keep configured text finishing usable from the frontend flow.
6. Add registry resource preparation only after the product spine works.
7. Defer GUI/i18n/release polish until the input method is usable.

Feature work is now allowed when it directly advances the E2E product spine and stays behind explicit seams with focused tests. Do not add broad cleanup that does not move one of the priority items above.
