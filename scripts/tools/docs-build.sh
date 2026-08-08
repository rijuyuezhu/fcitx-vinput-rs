#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

if command -v mkdocs >/dev/null 2>&1; then
  exec mkdocs build --strict
fi

if command -v uv >/dev/null 2>&1; then
  exec uvx \
    --from 'mkdocs==1.6.1' \
    --with 'mkdocs-material==9.7.7' \
    mkdocs build --strict
fi

echo "MkDocs is unavailable. Install requirements-docs.txt or install uv." >&2
exit 1
