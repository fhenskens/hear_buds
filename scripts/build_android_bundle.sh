#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TARGET="${TARGET:-arm64-v8a}"
DX_ARGS="${DX_ARGS:-}"

function resolve_triple() {
  case "$1" in
    arm64-v8a) echo "aarch64-linux-android" ;;
    armeabi-v7a) echo "armv7-linux-androideabi" ;;
    x86) echo "i686-linux-android" ;;
    x86_64) echo "x86_64-linux-android" ;;
    *) echo "Unsupported target: $1" >&2; exit 1 ;;
  esac
}

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

function resolve_latest_ndk() {
  local sdk_root="$1"
  local ndk_root="${sdk_root}/ndk"
  if [[ ! -d "${ndk_root}" ]]; then
    return
  fi
  ls -1 "${ndk_root}" | sort -V | tail -n 1
}

function ensure_android_env() {
  local sdk_root
  sdk_root="$(resolve_sdk_root || true)"
  if [[ -n "${sdk_root}" ]]; then
    export ANDROID_HOME="${ANDROID_HOME:-${sdk_root}}"
    export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${sdk_root}}"
  fi
  if [[ -z "${ANDROID_NDK_HOME:-}" && -n "${sdk_root}" ]]; then
    local ndk_version
    ndk_version="$(resolve_latest_ndk "${sdk_root}" || true)"
    if [[ -n "${ndk_version}" ]]; then
      export ANDROID_NDK_HOME="${sdk_root}/ndk/${ndk_version}"
    fi
  fi
  if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    echo "ANDROID_NDK_HOME is not set and no NDK could be found. Set ANDROID_NDK_HOME to your NDK root." >&2
    exit 1
  fi
  if [[ -z "${JAVA_HOME:-}" ]]; then
    if [[ -x "/usr/libexec/java_home" ]]; then
      export JAVA_HOME="$(/usr/libexec/java_home 2>/dev/null || true)"
    fi
  fi
  if [[ -z "${JAVA_HOME:-}" ]]; then
    if [[ -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
      export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
    elif [[ -d "${HOME}/android-studio/jbr" ]]; then
      export JAVA_HOME="${HOME}/android-studio/jbr"
    elif [[ -d "/opt/android-studio/jbr" ]]; then
      export JAVA_HOME="/opt/android-studio/jbr"
    fi
  fi
  if [[ -z "${JAVA_HOME:-}" ]]; then
    echo "Warning: JAVA_HOME is not set. Android builds will fail without a JDK. Set JAVA_HOME to your JDK or Android Studio JBR." >&2
  fi
}

function find_gradle_root() {
  local dx_root
  dx_root="${PROJECT_ROOT}/target/dx"
  if [[ ! -d "${dx_root}" ]]; then
    return
  fi
  local gradlew
  gradlew="$(find "${dx_root}" -name gradlew -type f 2>/dev/null | sort -r | head -n 1)"
  if [[ -n "${gradlew}" ]]; then
    dirname "${gradlew}"
  fi
}

function ensure_local_properties() {
  local gradle_root="$1"
  local sdk_root="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
  if [[ -z "${sdk_root}" ]]; then
    return
  fi
  local props="${gradle_root}/local.properties"
  local escaped
  escaped="$(echo "${sdk_root}" | sed 's/\\/\\\\/g')"
  echo "sdk.dir=${escaped}" > "${props}"
}

function find_bundle_path() {
  local bundle
  bundle="$(find "${PROJECT_ROOT}/target/dx" -path "*/build/outputs/bundle/release/*.aab" -type f 2>/dev/null | sort | tail -n 1 || true)"
  if [[ -n "${bundle}" ]]; then
    echo "${bundle}"
  fi
}

ensure_android_env
TRIPLE="$(resolve_triple "${TARGET}")"

echo "Running: dx build --platform android --release --no-default-features --features mobile ${DX_ARGS}"
if [[ -z "${DX_ARGS}" ]]; then
  dx build --platform android --target "${TRIPLE}" --release --no-default-features --features mobile
else
  dx build --platform android --target "${TRIPLE}" --release --no-default-features --features mobile ${DX_ARGS}
fi

GRADLE_ROOT="$(find_gradle_root || true)"
if [[ -z "${GRADLE_ROOT}" ]]; then
  echo "Could not find generated Gradle project under target/dx. Build cannot continue." >&2
  exit 1
fi

ensure_local_properties "${GRADLE_ROOT}"

echo "Running: ./gradlew bundleRelease"
(cd "${GRADLE_ROOT}" && ./gradlew bundleRelease)

BUNDLE_PATH="$(find_bundle_path || true)"
if [[ -z "${BUNDLE_PATH}" ]]; then
  echo "Release AAB not found after Gradle bundleRelease." >&2
  exit 1
fi

echo "AAB generated: ${BUNDLE_PATH}"

