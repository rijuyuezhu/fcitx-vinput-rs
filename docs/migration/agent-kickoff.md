# New agent kickoff context

Use this file to start the next implementation session.

## Mission

Continue `fcitx-vinput-rs` as an accelerated Rust port of the original C++ project. The next product goal is a usable E2E Fcitx vinput input method, not another open-ended refactor pass.

Use Chinese when talking to the user. Use English for code, comments, tests, file paths, and commit messages.

## Repositories

- Rust rewrite: `/workspace/fcitx-vinput-rs`
- Legacy C++ project: `/workspace/fcitx5-vinput`
- Rust remote: `git@github.com:rijuyuezhu/fcitx-vinput-rs.git`
- Legacy upstream: `https://github.com/xifan2333/fcitx5-vinput`

## Start-of-session checks

Run these from `/workspace/fcitx-vinput-rs` before editing:

```sh
git status --short --branch
git log -1 --oneline --decorate
gh run list --repo rijuyuezhu/fcitx-vinput-rs --limit 10
```

Then read:

1. `AGENT.md`
2. `docs/README.md`
3. `docs/development.md`
4. `docs/migration/e2e-port-plan.md`
5. `docs/migration/agent-kickoff.md`
6. `docs/architecture/README.md`
7. The subsystem contract for the files you will touch
8. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` when comparing legacy behavior

## First recommended implementation slice

The retained C++ Fcitx5 frontend bridge now exists under `cpp/fcitx5-addon`. Continue with small E2E-product-spine slices: keep addon smoke coverage tight, exercise the bridge against the Rust daemon, and align local run/install documentation with the retained addon skeleton.

Keep backend logic in Rust crates. Add frontend behavior only when it is needed for trigger handling, status/preedit, candidate UI, selected-text replacement, bus cleanup, or local E2E validation.

## Project style

- Prefer product-spine work over generic cleanup.
- Preserve public wire formats and frontend expectations.
- Keep the Fcitx frontend thin.
- Keep backend logic in Rust crates.

## Testing style

Use focused checks first, then broader checks when needed.

```sh
cargo test --package vinput-protocol
cargo test -p vinput-daemon --test cli
cargo test -p vinput-cli --test architecture_docs
cargo test --workspace --all-targets
just addon-smoke
just smoke
just e2e-demo
```

For bus compatibility work, also run the daemon bus integration test under a session bus. For docs or contract work, run `cargo test -p vinput-cli --test architecture_docs`.

## Commit message style

Use concise Conventional Commit messages in English:

```text
feat(addon): add fcitx bridge skeleton
feat(dbus): add addon client wrapper
feat(daemon): support frontend context
fix(protocol): preserve legacy method shape
test(addon): cover trigger flow contract
docs(migration): update e2e port plan
```

Prefer small commits with one reason to change.
