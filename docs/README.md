# Documentation map

This directory is task-oriented. New agents should follow the reading order instead of scanning every file blindly.

## Required reading order

1. [`../AGENT.md`](../AGENT.md): short root instruction file for agents.
2. [`development.md`](development.md): project style, commit message style, and test commands.
3. [`migration/e2e-port-plan.md`](migration/e2e-port-plan.md): current tracked plan for accelerating the Rust port toward a usable E2E Fcitx vinput input method.
4. [`migration/agent-kickoff.md`](migration/agent-kickoff.md): copyable context for a fresh implementation agent.
5. [`architecture/README.md`](architecture/README.md): tracked architecture contract index. Read the contract document for the crate or subsystem you will touch.
6. [`legacy/README.md`](legacy/README.md) and [`legacy/source-annotations.md`](legacy/source-annotations.md): legacy C++ source analysis when behavior comparison is needed.
7. `plan/review-driven-refactor-plan.md`: ignored local scratch notes, when present. These are no longer the primary plan.

## Directory roles

- [`architecture/`](architecture/): tracked stable contracts for crate boundaries, bus, config, registry, ASR, audio, and text behavior.
- [`legacy/`](legacy/): tracked migration record for the original C++ source tree.
- [`migration/`](migration/): tracked execution plans and agent handoff prompts for the active Rust port.
- `plan/`: ignored local scratch. Do not manually track files under this directory.

## How to update docs

- Update `migration/e2e-port-plan.md` when the active migration strategy, next phase, or legacy/Rust gap list changes.
- Update `migration/agent-kickoff.md` when a new agent needs different startup context, checks, or first-task guidance.
- Update `development.md` when workflow, test commands, or commit conventions change.
- Update `architecture/*` when a public contract or compatibility rule changes.
- Update `legacy/*` only when source analysis of the original project changes.
