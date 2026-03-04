#!/usr/bin/env bash
set -euo pipefail

DX_ARGS="${DX_ARGS:-}"

echo "Starting desktop development build..."

if [[ -z "${DX_ARGS}" ]]; then
  dx serve --platform desktop --no-default-features --features desktop
else
  dx serve --platform desktop --no-default-features --features desktop ${DX_ARGS}
fi
