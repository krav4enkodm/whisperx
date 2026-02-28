#!/usr/bin/env bash
set -Eeuo pipefail

REPO="${REPO:-krav4enkodm/whisperx}"
BRANCH="${BRANCH:-main}"
INSTALL_URL="${INSTALL_URL:-https://raw.githubusercontent.com/${REPO}/${BRANCH}/scripts/install.sh}"
LOG_FILE="${LOG_FILE:-/tmp/whisperx-clean-test-$(date +%Y%m%d-%H%M%S).log}"
KEEP_TEMP="${KEEP_TEMP:-0}"

TMP_HOME="$(mktemp -d)"
MIN_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
LOCAL_PATH="${TMP_HOME}/.local/bin:${MIN_PATH}"

cleanup() {
  local exit_code=$?
  if [[ "${KEEP_TEMP}" == "1" || ${exit_code} -ne 0 ]]; then
    echo "Temporary HOME kept at: ${TMP_HOME}"
  else
    rm -rf "${TMP_HOME}"
    echo "Temporary HOME removed: ${TMP_HOME}"
  fi
  echo "Log file: ${LOG_FILE}"
}
trap cleanup EXIT

exec > >(tee -a "${LOG_FILE}") 2>&1

echo "== whisperx clean-install smoke test =="
echo "date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
echo "repo: ${REPO}"
echo "branch: ${BRANCH}"
echo "install_url: ${INSTALL_URL}"
echo "tmp_home: ${TMP_HOME}"
echo "log_file: ${LOG_FILE}"
echo "uname: $(uname -a)"

command -v curl >/dev/null
command -v ffmpeg >/dev/null

echo
echo "== install =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc "set -euo pipefail; curl -fsSL '${INSTALL_URL}' | bash"

echo
echo "== installed binaries =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc 'ls -l "$HOME/.local/bin/whisperx" "$HOME/.local/bin/whisper-cli"'

echo
echo "== installed mic helpers =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc 'ls -l "$HOME/.local/bin"/whisperx-mic-*'

echo
echo "== installed runtime libs =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc 'ls -l "$HOME/.local/bin"/libwhisper.so* "$HOME/.local/bin"/libggml*.so*'

echo
echo "== version/help/doctor =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc '"$HOME/.local/bin/whisperx" --version'
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc '"$HOME/.local/bin/whisperx" --help >/dev/null'
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc '"$HOME/.local/bin/whisperx" doctor'

echo
echo "== mic daemon controls (dry-run) =="
env HOME="${TMP_HOME}" PATH="${LOCAL_PATH}" bash -lc 'set -euo pipefail; socket="$HOME/.cache/whisperx/mic-test.sock"; whisperx-mic-daemon --dry-run --socket "$socket" >"$HOME/mic-daemon.log" 2>&1 & daemon_pid=$!; for _ in $(seq 1 50); do [ -S "$socket" ] && break; sleep 0.1; done; test -S "$socket"; whisperx-mic-status --socket "$socket"; whisperx-mic-shutdown --socket "$socket"; wait "$daemon_pid"'

echo
echo "== config + models =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc '"$HOME/.local/bin/whisperx" config init'
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc '"$HOME/.local/bin/whisperx" models list'

echo
echo "== end-to-end transcribe =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc 'set -euo pipefail; ffmpeg -f lavfi -i "sine=frequency=1000:duration=1" -ar 16000 -ac 1 "$HOME/test.wav" -y >/dev/null 2>&1; "$HOME/.local/bin/whisperx" transcribe "$HOME/test.wav" -- --no-timestamps > "$HOME/transcript.txt"'

echo
echo "== transcript output (first 20 lines) =="
env HOME="${TMP_HOME}" PATH="${MIN_PATH}" bash -lc 'sed -n "1,20p" "$HOME/transcript.txt"'

echo
echo "PASS: clean-install smoke test succeeded"
