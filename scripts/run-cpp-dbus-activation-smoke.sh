#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-addon-dbus-activation"
stage_dir="target/tmp/vinput-cpp-dbus-activation-smoke"
daemon_path="${repo_root}/target/debug/vinput-daemon"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
service_file="${repo_root}/${stage_dir}/share/dbus-1/services/org.fcitx.Vinput.service"

cargo build -q -p vinput-daemon
cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_DAEMON_ARGS=--dbus
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
rm -rf "${stage_dir}"
cmake --install "${build_dir}" --prefix "${stage_dir}"

grep -qx "Name=org.fcitx.Vinput" "${service_file}"
grep -qx "Exec=${daemon_path} --dbus" "${service_file}"

XDG_DATA_DIRS="${repo_root}/${stage_dir}/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
  dbus-run-session -- "${smoke_bin}"
