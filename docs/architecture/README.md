# Architecture contracts

This directory contains tracked architecture and compatibility contracts for the Rust rewrite. Read [`../development.md`](../development.md) and [`../migration/e2e-port-plan.md`](../migration/e2e-port-plan.md) first, then use this index to choose the subsystem document relevant to the task.

## Reading order

1. [`target-architecture.md`](target-architecture.md): crate boundaries, runtime actors, state machine target, and migration principles.
2. Subsystem contract for the area being changed:
   - [`dbus-service.md`](dbus-service.md): legacy D-Bus service facade, diagnostic extension, and compatibility rules.
   - [`config-contract.md`](config-contract.md): default config fixture, parsing, validation, and diagnostics behavior.
   - [`registry-contract.md`](registry-contract.md): registry metadata, dry-run planning, and sample fixture contracts.
   - [`asr-contract.md`](asr-contract.md): ASR backend/session seams, command ASR behavior, and diagnostics.
   - [`audio-contract.md`](audio-contract.md): PCM layout, WAV/raw byte policy, recorder lifecycle, and PipeWire scaffold.
   - [`text-contract.md`](text-contract.md): text post-processing, prompt/context cache, command adapters, and OpenAI-compatible seams.
3. [`../migration/e2e-port-plan.md`](../migration/e2e-port-plan.md), for active E2E migration direction.
4. `../plan/review-driven-refactor-plan.md`, when present locally, for ignored scratch notes only.

## Maintenance rules

- These files are tracked and should describe stable contracts or explicit compatibility targets.
- Do not use these files as scratch planning space; use ignored `docs/plan/` for that.
- Delete stale review snapshots after consolidating their conclusions into `docs/plan/` or these contract docs.
- Keep statements precise: distinguish `implemented`, `mock/seam only`, `configured behind an explicit flag`, and `future work`.
