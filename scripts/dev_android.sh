#!/usr/bin/env bash
set -euo pipefail

TARGET="${TARGET:-arm64-v8a}"
PROFILE="${PROFILE:-debug}"
DX_ARGS="${DX_ARGS:-}"
NO_LOGCAT="${NO_LOGCAT:-0}"

function resolve_sdk_root() {
  if [[ -n "${ANDROID_HOME:-}" ]]; then
    echo "${ANDROID_HOME}"
    return
  fi
  if [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    echo "${ANDROID_SDK_ROOT}"
    return
  fi
  if [[ -d "${HOME}/Library/Android/sdk" ]]; then
    echo "${HOME}/Library/Android/sdk"
    return
  fi
  if [[ -d "${HOME}/Android/Sdk" ]]; then
    echo "${HOME}/Android/Sdk"
    return
  fi
  if [[ -d "${HOME}/Android/sdk" ]]; then
    echo "${HOME}/Android/sdk"
    return
  fi
}

echo "Building + deploying Android app..."
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export TARGET PROFILE DX_ARGS
"${SCRIPT_DIR}/deploy_android.sh"

if [[ "${NO_LOGCAT}" == "1" ]]; then
  exit 0
fi

SDK_ROOT="$(resolve_sdk_root || true)"
if [[ -z "${SDK_ROOT}" ]]; then
  echo "ANDROID_HOME/ANDROID_SDK_ROOT not set. Skipping logcat." >&2
  exit 0
fi

ADB="${SDK_ROOT}/platform-tools/adb"
if [[ ! -x "${ADB}" ]]; then
  echo "adb not found at ${ADB}. Skipping logcat." >&2
  exit 0
fi

echo "Tailing logcat (filter: HearBuds). Press Ctrl+C to stop."
"${ADB}" logcat -v time | grep --line-buffered HearBuds

