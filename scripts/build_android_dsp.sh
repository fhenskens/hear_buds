#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TARGET="${TARGET:-arm64-v8a}"
PROFILE="${PROFILE:-release}"
OUT_DIR="${OUT_DIR:-}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found on PATH." >&2
  exit 1
fi

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "cargo-ndk not found. Install with: cargo install cargo-ndk" >&2
  exit 1
fi

echo "Building Rust DSP staticlib for target ${TARGET} (${PROFILE})..."
if [[ "${PROFILE}" == "debug" ]]; then
  cargo ndk -t "${TARGET}" build --lib --no-default-features
else
  cargo ndk -t "${TARGET}" build --profile "${PROFILE}" --lib --no-default-features
fi

if [[ "${PROFILE}" == "release" ]]; then
  PROFILE_DIR="release"
elif [[ "${PROFILE}" == "debug" ]]; then
  PROFILE_DIR="debug"
else
  PROFILE_DIR="${PROFILE}"
fi

case "${TARGET}" in
  arm64-v8a) TRIPLE="aarch64-linux-android" ;;
  armeabi-v7a) TRIPLE="armv7-linux-androideabi" ;;
  x86) TRIPLE="i686-linux-android" ;;
  x86_64) TRIPLE="x86_64-linux-android" ;;
  *) echo "Unsupported target: ${TARGET}" >&2; exit 1 ;;
esac

LIB_PATH="${PROJECT_ROOT}/target/${TRIPLE}/${PROFILE_DIR}/libhear_buds_dsp.a"

if [[ ! -f "${LIB_PATH}" ]]; then
  echo "Expected library not found: ${LIB_PATH}" >&2
  exit 1
fi

if [[ -z "${OUT_DIR}" ]]; then
  echo "Built: ${LIB_PATH}"
  exit 0
fi

mkdir -p "${OUT_DIR}"
DEST="${OUT_DIR}/libhear_buds_dsp.a"
cp "${LIB_PATH}" "${DEST}"

echo "Copied to: ${DEST}"


