#!/usr/bin/env bash
set -euo pipefail

REPO="${WHISPERX_REPO:-krav4enkodm/whisperx}"
BIN_DIR="${HOME}/.local/bin"
BINARY="whisperx"

os="$(uname -s)"
arch="$(uname -m)"

if [ "${os}" != "Linux" ]; then
  echo "Unsupported OS: ${os}. This installer currently supports Linux x86_64 only." >&2
  exit 1
fi

case "${arch}" in
  x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
  *)
    echo "Unsupported architecture: ${arch}. This installer currently supports Linux x86_64 only." >&2
    exit 1
    ;;
esac

asset="${BINARY}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/latest/download/${asset}"

mkdir -p "${BIN_DIR}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

if ! curl -fsSL "${url}" -o "${tmpdir}/${asset}"; then
  echo "Failed to download ${asset} from latest release." >&2
  echo "Make sure a release exists with this asset: ${url}" >&2
  exit 1
fi
tar -xzf "${tmpdir}/${asset}" -C "${tmpdir}"
install -m 0755 "${tmpdir}/${BINARY}" "${BIN_DIR}/${BINARY}"
if [ -f "${tmpdir}/whisper-cli" ]; then
  install -m 0755 "${tmpdir}/whisper-cli" "${BIN_DIR}/whisper-cli"
else
  echo "Release artifact is missing bundled whisper-cli. Please open an issue." >&2
  exit 1
fi

for lib in "${tmpdir}"/libwhisper.so* "${tmpdir}"/libggml*.so*; do
  if [ -f "${lib}" ]; then
    install -m 0644 "${lib}" "${BIN_DIR}/$(basename "${lib}")"
  fi
done

if ! ls "${BIN_DIR}"/libwhisper.so* >/dev/null 2>&1; then
  echo "Release artifact is missing libwhisper runtime libraries. Please use a newer release." >&2
  exit 1
fi

if ! ls "${BIN_DIR}"/libggml*.so* >/dev/null 2>&1; then
  echo "Release artifact is missing libggml runtime libraries. Please use a newer release." >&2
  exit 1
fi

for action in daemon start stop toggle status shutdown; do
  helper="whisperx-mic-${action}"
  cat > "${tmpdir}/${helper}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
bin_dir="\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)"
exec "\${bin_dir}/whisperx" mic ${action} "\$@"
EOF
  install -m 0755 "${tmpdir}/${helper}" "${BIN_DIR}/${helper}"
done

echo "Installed ${BINARY} to ${BIN_DIR}/${BINARY}"
echo "Installed bundled whisper-cli to ${BIN_DIR}/whisper-cli"
echo "Installed mic helpers to ${BIN_DIR}/whisperx-mic-{daemon,start,stop,toggle,status,shutdown}"
case ":$PATH:" in
  *":${BIN_DIR}:"*) ;;
  *)
    echo "Add ${BIN_DIR} to PATH, then run: ${BINARY} --help"
    ;;
esac
