# AGENT

Before doing any work in this repository, read these files in order:

1. `docs/README.md` — documentation map and required reading order.
2. `docs/development.md` — project style, commit message style, and test commands.
3. `docs/migration/e2e-port-plan.md` — current tracked plan for accelerating the Rust port toward a usable E2E Fcitx vinput input method.
4. `docs/migration/agent-kickoff.md` — copyable context for a fresh implementation agent.
5. `docs/architecture/README.md` — tracked architecture contract index; then read the contract document for the area you will touch.
6. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` — legacy C++ source map when comparing behavior with `fcitx5-vinput`.
7. `docs/plan/review-driven-refactor-plan.md` — local ignored scratch notes, when present. These are no longer the primary plan.

Rules for agents:

- Communicate with the user in Chinese; keep code, comments, tests, and commit messages in English unless existing code requires otherwise.
- Current priority: accelerate the Rust port toward a usable E2E Fcitx vinput input method.
- Prefer product-spine implementation over generic cleanup.
- Preserve public wire formats and frontend expectations with focused tests.
- Keep the retained Fcitx frontend thin. Backend logic belongs in Rust crates and `vinput-daemon`.
- Never manually track files under `docs/plan/`; it is local scratch and must remain ignored.
- Use test commands from `docs/development.md`. Prefer focused checks while iterating, and broader checks before handoff.
