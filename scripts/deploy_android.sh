#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TARGET="${TARGET:-arm64-v8a}"
PROFILE="${PROFILE:-release}"
DX_ARGS="${DX_ARGS:-}"
APP_ID="${APP_ID:-com.fhenskens.hearbuds}"

if [[ "${PROFILE}" != "debug" && "${PROFILE}" != "release" && "${PROFILE}" != "minsize" ]]; then
  echo "Unsupported PROFILE: ${PROFILE}. Use debug, release, or minsize." >&2
  exit 1
fi

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

function ensure_manifest_permission() {
  local gradle_root="$1"
  local manifest="${gradle_root}/app/src/main/AndroidManifest.xml"
  if [[ ! -f "${manifest}" ]]; then
    return 1
  fi
  if grep -q "android.permission.RECORD_AUDIO" "${manifest}"; then
    return 1
  fi
  local insertion='    <uses-permission android:name="android.permission.RECORD_AUDIO" />'
  perl -0777 -i -pe "s|(<manifest[^>]*>)|$1\\n${insertion}|s" "${manifest}"
  return 0
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

function resolve_apk_path() {
  local base
  base="${PROJECT_ROOT}/target/dx/hear_buds"
  if [[ "${PROFILE}" == "release" || "${PROFILE}" == "minsize" ]]; then
    local release_apk="${base}/release/android/app/app/build/outputs/apk/release/app-release.apk"
    if [[ -f "${release_apk}" ]]; then
      echo "${release_apk}"
      return
    fi
  else
    local debug_apk="${base}/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
    if [[ -f "${debug_apk}" ]]; then
      echo "${debug_apk}"
      return
    fi
  fi
  find "${PROJECT_ROOT}/target/dx" -name "*.apk" -print0 2>/dev/null | xargs -0 ls -t | head -n 1
}

ensure_android_env

ADB="${ANDROID_HOME}/platform-tools/adb"
if [[ ! -x "${ADB}" ]]; then
  echo "adb not found at ${ADB}. Ensure Android SDK platform-tools are installed." >&2
  exit 1
fi

echo "Building Android APK..."
export TARGET PROFILE DX_ARGS
"${SCRIPT_DIR}/build_android.sh"

GRADLE_ROOT="$(find_gradle_root || true)"
if [[ -n "${GRADLE_ROOT}" ]]; then
  ensure_local_properties "${GRADLE_ROOT}"
  MANIFEST_CHANGED=0
  if ensure_manifest_permission "${GRADLE_ROOT}"; then
    MANIFEST_CHANGED=1
  fi
  if [[ "${MANIFEST_CHANGED}" == "1" ]]; then
    echo "Added RECORD_AUDIO permission. Rebuilding APK with Gradle..."
    TASK="assembleDebug"
    if [[ "${PROFILE}" == "release" ]]; then
      TASK="assembleRelease"
    fi
    (cd "${GRADLE_ROOT}" && ./gradlew "${TASK}")
  fi
fi

APK_PATH="$(resolve_apk_path)"
if [[ -z "${APK_PATH}" || ! -f "${APK_PATH}" ]]; then
  echo "APK not found after build." >&2
  exit 1
fi

echo "Installing APK: ${APK_PATH}"
"${ADB}" install -r "${APK_PATH}"

echo "Launching app..."
"${ADB}" shell am start -n "${APP_ID}/dev.dioxus.main.MainActivity" >/dev/null
echo "Done."

