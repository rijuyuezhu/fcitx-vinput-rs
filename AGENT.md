# AGENT

Before doing any work in this repository, read these files in order:

1. `docs/README.md` — documentation map and required reading order.
2. `docs/development.md` — project style, commit message style, and `just` test commands.
3. `docs/architecture/README.md` — tracked architecture contract index; then read the contract document for the area you will touch.
4. `docs/plan/review-driven-refactor-plan.md` — local ignored scratch plan and current single source of truth for next refactor work, when present.
5. `docs/legacy/README.md` — legacy C++ source map, when comparing behavior with `fcitx5-vinput`.

Rules for agents:

- Communicate with the user in Chinese; keep code, comments, tests, and commit messages in English unless existing code requires otherwise.
- Refactor first. Do not add new backend features until the plan in `docs/plan/review-driven-refactor-plan.md` allows it or the user explicitly reprioritizes.
- Never manually track files under `docs/plan/`; it is local scratch and must remain ignored.
- Use `just` commands from `docs/development.md` for checks.
