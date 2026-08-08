# Developer tools

This directory contains short, functional developer helpers. A tool may be called
from `just` or a deterministic gate when its function is itself useful outside a
test harness; test orchestration and component regression scenarios stay under
`../tests/`.

Prefer moving reusable behavior into Rust crates and testing it there instead of
growing shell/Python test frameworks. In particular, management-GUI semantics
belong in crate-internal Rust tests below the Iced window/widget boundary, while
real GUI interaction is validated manually.

- `format.sh` formats or checks Rust/C++ sources.
- `docs-build.sh` performs a strict MkDocs build.
- `build-toolkit-probes.sh` compile-checks the small live toolkit probe programs.
- `bench-capture-cold-start.sh` summarizes structured capture-start timing from
  journal or saved log input; it is diagnostic evidence and does not replace the
  deterministic analyzer smoke under `../tests/asr/`.
