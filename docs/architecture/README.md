# Architecture notes

This directory tracks the stable architecture contracts used while porting the daemon/backend from the original C++ project to Rust.

Start with:

- [`target-architecture.md`](target-architecture.md): target crate boundaries, runtime actors, compatibility contracts, and migration order.
- [`bootstrap.md`](bootstrap.md): current bootstrap workspace shape and early local commands.

Contract references:

- [`dbus-service.md`](dbus-service.md): legacy D-Bus service facade and test expectations.
- [`config-contract.md`](config-contract.md): committed default config fixture and diagnostics behavior.
- [`registry-contract.md`](registry-contract.md): registry metadata shape, planning, and sample fixture ids.
- [`asr-contract.md`](asr-contract.md): ASR backend/session seams and diagnostic payloads.
- [`audio-contract.md`](audio-contract.md): PCM layout, raw byte, WAV decode, and processing order contracts.
- [`text-contract.md`](text-contract.md): text adapter and scene post-processing seams.

Keep these documents aligned with committed fixtures, smoke checks, and integration tests. Planning scratch notes live under `docs/plan/` and are intentionally not part of this public architecture index.
