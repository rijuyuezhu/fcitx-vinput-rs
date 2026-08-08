#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

scripts/tools/format.sh --check
scripts/tests/scripts-lint.sh
export PYTHONDONTWRITEBYTECODE=1
scripts/tests/lint.sh
python3 -B scripts/tests/check-upstream-inventory.py
scripts/tests/test.sh
python3 -B scripts/tests/check_fcitx_ffi_abi.py
scripts/tests/addon-install-smoke.sh
scripts/tests/cpp/run-cpp-dbus-smoke.sh
scripts/tests/cpp/run-cpp-dbus-asr-menu-smoke.sh
scripts/tests/cpp/run-cpp-dbus-configured-activation-smoke.sh
scripts/tools/build-toolkit-probes.sh

scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-deb-package.sh
scripts/release/check-flatpak-manifest.sh
scripts/release/check-nix-flake.sh
scripts/release/check-rpm-spec.sh
scripts/release/check-source-archive.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-metadata.sh
release_version="$(scripts/release/check-release-metadata.sh --print-version)"
scripts/release/check-release-metadata.sh --tag "v${release_version}"
if scripts/release/check-release-metadata.sh --tag "v${release_version}.mismatch" \
  >target/tmp/release-metadata-mismatch.out \
  2>target/tmp/release-metadata-mismatch.err; then
  echo "release metadata checker accepted a mismatched tag" >&2
  exit 1
fi
scripts/release/check-release-signature.sh
scripts/release/check-github-release-publish.sh
scripts/release/check-arch-release-candidate.sh

scripts/tests/asr/run-command-asr-wav-helper-smoke.sh
scripts/tests/asr/run-legacy-command-asr-wav-bridge-smoke.sh
scripts/tests/asr/run-openai-compatible-asr-fixture-smoke.sh
scripts/tests/asr/run-openai-compatible-asr-network-smoke.sh
scripts/tests/asr/run-openai-compatible-text-provider-fixture-smoke.sh
scripts/tests/asr/run-provider-ca-rotation-smoke.sh
scripts/tests/asr/run-capture-cold-start-smoke.sh

scripts/tests/daemon/run-daemon-default-config-smoke.sh
scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh
scripts/tests/daemon/run-daemon-handoff-smoke.sh
scripts/tests/daemon/run-daemon-removal-handoff-smoke.sh
scripts/tests/daemon/run-package-upgrade-handoff-smoke.sh
scripts/tests/daemon/run-package-remove-handoff-smoke.sh
scripts/tests/daemon/run-direct-activation-upgrade-smoke.sh
scripts/tests/daemon/run-daemon-unavailable-asr-smoke.sh
scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh
scripts/tests/daemon/run-remote-text-external-collector-smoke.sh

scripts/tests/install/run-user-ime-activation-owner-smoke.sh
scripts/tests/install/run-user-ime-real-command-asr-wav-smoke.sh
scripts/tests/install/run-user-ime-sherpa-sense-voice-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-command-smoke.sh
