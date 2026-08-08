#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"
source scripts/tests/dbus-session-common.sh

for command in dbus-run-session jq readlink timeout; do
  command -v "${command}" >/dev/null
done

stage_root="${repo_root}/target/tmp/daemon-handoff-diagnostics-smoke"
mismatch_root="${stage_root}/mismatch"
deleted_root="${stage_root}/deleted"
dbus_service_dir="${stage_root}/dbus-services"
dbus_config="${stage_root}/session.conf"
rm -rf "${stage_root}"
mkdir -p "${mismatch_root}/bin" "${mismatch_root}/config" \
  "${deleted_root}/bin" "${deleted_root}/config" "${dbus_service_dir}"
write_isolated_dbus_session_config "${dbus_config}" "${dbus_service_dir}"

cargo build -q -p vinpst-cli --bin vinpst -p vinpst-daemon --bin vinpst-daemon

install -Dm755 target/debug/vinpst-daemon "${mismatch_root}/bin/vinpst-daemon-old"
install -Dm755 target/debug/vinpst "${deleted_root}/bin/vinpst"
install -Dm755 target/debug/vinpst-daemon "${deleted_root}/bin/vinpst-daemon"

VINPST_HANDOFF_CLI="${repo_root}/target/debug/vinpst" \
VINPST_HANDOFF_DAEMON="${mismatch_root}/bin/vinpst-daemon-old" \
VINPST_HANDOFF_CONFIG_HOME="${mismatch_root}/config" \
VINPST_HANDOFF_STATUS="${mismatch_root}/status.json" \
  timeout 20s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
"${VINPST_HANDOFF_DAEMON}" --dbus >"${VINPST_HANDOFF_STATUS}.daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPST_HANDOFF_CONFIG_HOME}" \
    "${VINPST_HANDOFF_CLI}" daemon status --json >"${VINPST_HANDOFF_STATUS}" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1
INNER

expected_mismatch="$(readlink -f target/debug/vinpst-daemon)"
owner_mismatch="$(readlink -f "${mismatch_root}/bin/vinpst-daemon-old")"
jq -e \
  --arg expected "${expected_mismatch}" \
  --arg owner "${owner_mismatch}" \
  '.handoff.expected_executable == $expected
   and .handoff.owner_executable == $owner
   and .handoff.owner_executable_deleted == false
   and .handoff.path_matches == false
   and .handoff.restart_recommended == true
   and .handoff.reason == "owner-executable-path-mismatch"
   and .handoff.automatic_restart_performed == false
   and .handoff.next_step == "run vinpst daemon handoff"' \
  "${mismatch_root}/status.json" >/dev/null

VINPST_HANDOFF_CLI="${deleted_root}/bin/vinpst" \
VINPST_HANDOFF_DAEMON="${deleted_root}/bin/vinpst-daemon" \
VINPST_HANDOFF_REPLACEMENT="${repo_root}/target/debug/vinpst-daemon" \
VINPST_HANDOFF_CONFIG_HOME="${deleted_root}/config" \
VINPST_HANDOFF_STATUS="${deleted_root}/status.json" \
  timeout 20s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
"${VINPST_HANDOFF_DAEMON}" --dbus >"${VINPST_HANDOFF_STATUS}.daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPST_HANDOFF_CONFIG_HOME}" \
    "${VINPST_HANDOFF_CLI}" daemon status --json >"${VINPST_HANDOFF_STATUS}.before" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1
jq -e \
  '.handoff.path_matches == true
   and .handoff.owner_executable_deleted == false
   and .handoff.restart_recommended == false' \
  "${VINPST_HANDOFF_STATUS}.before" >/dev/null

rm -f "${VINPST_HANDOFF_DAEMON}"
install -Dm755 "${VINPST_HANDOFF_REPLACEMENT}" "${VINPST_HANDOFF_DAEMON}"
XDG_CONFIG_HOME="${VINPST_HANDOFF_CONFIG_HOME}" \
  "${VINPST_HANDOFF_CLI}" daemon status --json >"${VINPST_HANDOFF_STATUS}"
XDG_CONFIG_HOME="${VINPST_HANDOFF_CONFIG_HOME}" \
  "${VINPST_HANDOFF_CLI}" daemon status >"${VINPST_HANDOFF_STATUS}.txt"
INNER

expected_deleted="$(readlink -f "${deleted_root}/bin/vinpst-daemon")"
jq -e \
  --arg expected "${expected_deleted}" \
  '.handoff.expected_executable == $expected
   and .handoff.normalized_owner_executable == $expected
   and (.handoff.owner_executable | endswith(" (deleted)"))
   and .handoff.owner_executable_deleted == true
   and .handoff.path_matches == true
   and .handoff.restart_recommended == true
   and .handoff.reason == "owner-executable-deleted"
   and .handoff.automatic_restart_performed == false
   and .handoff.next_step == "run vinpst daemon handoff"' \
  "${deleted_root}/status.json" >/dev/null
grep -Fqx 'The running daemon belongs to an older installation.' "${deleted_root}/status.json.txt"
grep -Fqx 'Run `vinpst daemon handoff` to switch to the current daemon safely.' "${deleted_root}/status.json.txt"
if grep -Eq 'handoff_|owner_exe|path_matches|restart_recommended' "${deleted_root}/status.json.txt"; then
  echo "daemon status text leaked handoff internals" >&2
  exit 1
fi

echo "daemon handoff diagnostics smoke passed"
