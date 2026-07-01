#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

just addon-build
cargo build -q -p vinput-daemon --features pipewire-backend

bus_runner="dbus-run-session"
log_file="target/tmp/vinput-cpp-dbus-pipewire-live-smoke-daemon.log"
mkdir -p "$(dirname "${log_file}")"

${bus_runner} -- bash -euo pipefail <<'INNER'
log_file="target/tmp/vinput-cpp-dbus-pipewire-live-smoke-daemon.log"
target/debug/vinput-daemon --dbus --audio-backend pipewire >"${log_file}" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" >/dev/null 2>&1 || true
  wait "${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT
sleep 0.5
export VINPUT_DBUS_SMOKE_RECORD_MS=100

for _ in $(seq 1 50); do
  if target/cpp/fcitx5-addon/vinput_fcitx_bridge_dbus_smoke; then
    exit 0
  fi
  if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
    cat "${log_file}" >&2
    exit 1
  fi
  sleep 0.1
done

cat "${log_file}" >&2
exit 1
INNER
