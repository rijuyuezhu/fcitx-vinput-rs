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
- Prefer test-first refactors. Pin compatibility behavior with tests before moving code across modules or changing runtime logic.
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

Common commands:

```sh
just fmt          # format Rust code
just fmt-check    # check formatting
just lint         # clippy for the workspace
just test         # cargo test --workspace --all-targets
just dbus-test    # D-Bus integration tests under dbus-run-session
just dbus-lint    # clippy with dbus-integration feature
just check        # fmt-check + lint + test + dbus-test + dbus-lint
just ci           # alias for check
just smoke        # CLI/daemon smoke commands
just e2e-demo     # deterministic file-input command ASR/text demo
just dbus         # run the mock/configured legacy D-Bus service on the current session bus
```

Before proposing a code change, prefer running:

```sh
just ci
just smoke
```

For docs-only changes, at least verify paths and git status. Run full checks when docs alter public contracts, command examples, or test instructions.

## Next work

The next migration phase is refactor-first. Read `docs/plan/review-driven-refactor-plan.md` when it exists locally and follow its execution order:

1. Preserve legacy D-Bus error ABI.
2. Preserve/defer ASR reload semantics.
3. Split `vinput-text`.
4. Split `vinput-daemon::runtime`.
5. Split `vinput-asr`.
6. Split `vinput-registry`.
7. Add config legacy compatibility golden tests.
8. Refresh stable architecture docs.

Feature work such as concrete OpenAI HTTP transport, live PipeWire recording, sherpa-onnx, registry installation, Fcitx frontend work, and packaging should stay blocked until the refactor plan says otherwise or the user explicitly reprioritizes.
