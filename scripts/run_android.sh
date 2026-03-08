#!/usr/bin/env bash
set -euo pipefail

TARGET="${TARGET:-arm64-v8a}"
PROFILE="${PROFILE:-debug}"

"$(cd "$(dirname "$0")" && pwd)/deploy_android.sh"
