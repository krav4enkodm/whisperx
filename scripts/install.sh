#!/usr/bin/env bash
set -euo pipefail

REPO="${WHISPERX_REPO:-krav4enkodm/whisperx}"
BIN_DIR="${HOME}/.local/bin"
BINARY="whisperx"

os="$(uname -s)"
arch="$(uname -m)"

case "${os}" in
  Linux) platform="unknown-linux-gnu" ;;
  Darwin) platform="apple-darwin" ;;
  *)
    echo "Unsupported OS: ${os}" >&2
    exit 1
    ;;
esac

case "${arch}" in
  x86_64|amd64) cpu="x86_64" ;;
  arm64|aarch64) cpu="aarch64" ;;
  *)
    echo "Unsupported architecture: ${arch}" >&2
    exit 1
    ;;
esac

target="${cpu}-${platform}"
asset="${BINARY}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/latest/download/${asset}"

mkdir -p "${BIN_DIR}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

curl -fsSL "${url}" -o "${tmpdir}/${asset}"
tar -xzf "${tmpdir}/${asset}" -C "${tmpdir}"
install -m 0755 "${tmpdir}/${BINARY}" "${BIN_DIR}/${BINARY}"

echo "Installed ${BINARY} to ${BIN_DIR}/${BINARY}"
case ":$PATH:" in
  *":${BIN_DIR}:"*) ;;
  *)
    echo "Add ${BIN_DIR} to PATH, then run: ${BINARY} --help"
    ;;
esac
