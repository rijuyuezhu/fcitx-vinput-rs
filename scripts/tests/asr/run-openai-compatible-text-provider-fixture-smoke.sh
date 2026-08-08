#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "${repo_root}"

fixture=scripts/fixtures/openai-compatible-text-provider-fixture.py
out_dir=target/tmp/openai-compatible-text-provider-fixture-smoke
ready_file="${out_dir}/ready.json"
trace_file="${out_dir}/trace.json"
error_file="${out_dir}/error.txt"
config_file="${out_dir}/config.json"
server_log="${out_dir}/server.log"
api_key='fixture-secret'
model='fixture-model'
input_text='fixture connectivity input'
server_pid=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  exit "${status}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
cargo build -q -p vinpst-cli --bin vinpst

python3 "${fixture}" \
  --ready-file "${ready_file}" \
  --trace-file "${trace_file}" \
  --error-file "${error_file}" \
  --api-key "${api_key}" \
  --model "${model}" \
  --response-prefix 'fixture-cli: ' \
  --allow-empty-selected \
  >"${server_log}" 2>&1 &
server_pid=$!

for _ in $(seq 1 100); do
  [[ -f "${ready_file}" ]] && break
  kill -0 "${server_pid}" 2>/dev/null || { cat "${server_log}" >&2; exit 1; }
  sleep 0.05
done
test -f "${ready_file}"
base_url="$(jq -r '.base_url' "${ready_file}")"

jq -n \
  --arg base_url "${base_url}" \
  --arg api_key "${api_key}" \
  --arg model "${model}" '
  {
    version: 1,
    llm: {providers: [{id: "fixture", base_url: $base_url, api_key: $api_key, model: $model, extra_body: {}}]},
    scenes: {active_scene: "raw", definitions: [{id: "raw", label: "Raw", candidate_count: 0}]}
  }
' >"${config_file}"

target/debug/vinpst llm test fixture \
  --config "${config_file}" --text "${input_text}" --json \
  >"${out_dir}/result.json"
wait "${server_pid}"
server_pid=""

test ! -e "${error_file}"
jq -e --arg expected "fixture-cli: ${input_text}" \
  '.ok and .called and .result.commit_text == $expected and .result.candidate_count == 1' \
  "${out_dir}/result.json" >/dev/null
jq -e --arg input "${input_text}" '
  .event == "request" and
  .path == "/v1/chat/completions" and
  .authorization_scheme == "Bearer" and
  .authorization_value_recorded == false and
  .model == "fixture-model" and
  .raw_asr_text == $input
' "${trace_file}" >/dev/null
! grep -R -F -- "${api_key}" "${out_dir}/result.json" "${trace_file}" "${ready_file}" >/dev/null

printf 'OpenAI-compatible text provider CLI smoke passed\n'
