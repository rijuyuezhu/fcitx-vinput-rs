# Documentation map

This directory is intentionally small and task-oriented. New agents should not read every file blindly; follow the reading order below.

## Required reading order

1. [`../AGENT.md`](../AGENT.md): short root instruction file for agents.
2. [`development.md`](development.md): project style, commit message style, and `just`-based checks.
3. [`architecture/README.md`](architecture/README.md): tracked architecture contract index. Read the contract document for the crate or subsystem you will touch.
4. `plan/review-driven-refactor-plan.md`: local ignored planning scratch and current single source of truth for the next refactor phase, when present in this working tree.
5. [`legacy/README.md`](legacy/README.md): legacy C++ source analysis index, when a task requires behavior comparison.

## Directory roles

- [`architecture/`](architecture/): tracked stable contracts for crate boundaries, D-Bus, config, registry, ASR, audio, and text behavior. These files should describe committed behavior or explicit compatibility targets.
- [`legacy/`](legacy/): tracked migration record for the original C++ source tree. Use this for source-to-target mapping and behavior lookup.
- `plan/`: ignored local scratch. This is where the current refactor plan lives, but files under this directory must not be manually tracked.

There is no tracked `docs/review/` directory anymore. Stale review snapshots should be deleted after their conclusions are consolidated into `docs/plan/` or the stable contract documents.

## How to update docs

- Update `development.md` when workflow, test commands, or commit conventions change.
- Update `architecture/*` when a public contract or compatibility rule changes.
- Update `legacy/*` only when source analysis of the original project changes.
- Keep next-step planning in `docs/plan/`; do not rely on old review snapshots.
