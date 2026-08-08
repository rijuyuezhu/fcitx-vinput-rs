#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

mapfile -t addon_sources < <(
  find cpp/fcitx5-addon -type f \( -name '*.cpp' -o -name '*.h' \) -print | sort
)

if [[ "${1:-}" == "--check" ]]; then
  clang-format --dry-run --Werror "${addon_sources[@]}"
  cargo fmt --all -- --check
else
  clang-format -i "${addon_sources[@]}"
  cargo fmt --all
fi
