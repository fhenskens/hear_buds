# HearBuds

HearBuds is a Dioxus (0.7) app for Android and desktop.
It uses your mic, runs low-latency DSP, and plays audio to your output device.
You can run a calibration sweep, then fine-tune hearing correction with EQ controls.

## Features
- Mic -> DSP -> speaker pipeline (low latency).
- Threshold-seeking calibration sweep (per-band, adaptive).
- Multi-band compression tuned for speech clarity.
- Noise reduction with:
  - strength control
  - room-noise capture
  - Comfort / Strong profiles
  - artifact guard metrics in advanced view
- EQ display with user-unlocked editing.
- Safety limiter and safe mode.
- Profiles (save/load).
- Rust-first DSP core for performance-critical processing.

## Status
Active development (alpha).
The Dioxus UI, state flow, and native audio pipeline are working.
The app is usable for testing, but not yet production-ready.

Known gaps:
- Bluetooth headset/earbud microphone routing is not yet reliable across devices.
- Android runtime mic permission flow still needs broader device validation.
- Background audio behavior depends on Android system policy and device settings.

## Architecture
- `src/`: Dioxus UI, state, and Rust DSP core (`src/lib.rs`).
- `android/`: Android manifest/project files.
- Android audio runs through `trombone-audio` (AAudio/OpenSL ES fallback) with the Rust DSP core.

## Requirements
- Rust (`cargo`)
- Dioxus CLI (`dx`)
- Android SDK (with NDK tools)
- Java (JDK)
- `cargo-ndk` (`cargo install cargo-ndk`)
- `adb` (for device install)

## Quick Start (Desktop, cross-platform)
```bash
dx serve --platform desktop --no-default-features --features desktop
```

Windows PowerShell:
```powershell
.\scripts\dev_desktop.ps1
```

macOS / Linux:
```bash
./scripts/dev_desktop.sh
```

## Android Build (Rust DSP enabled)

Environment variables (required):
- `ANDROID_HOME` (or `ANDROID_SDK_ROOT`) -> Android SDK root
- `ANDROID_NDK_HOME` -> NDK root (for example: `...\\Sdk\\ndk\\29.0.14206865`)
- `JAVA_HOME` -> JDK/JBR path

Windows:
```powershell
.\scripts\build_android.ps1
```

macOS / Linux:
```bash
./scripts/build_android.sh
```

Both scripts:
- Run `dx build --platform android --target aarch64-linux-android`.

## Android Build + Install (one step)

Windows:
```powershell
.\scripts\deploy_android.ps1
```

macOS / Linux:
```bash
./scripts/deploy_android.sh
```

These scripts:
- Ensure Android SDK/NDK environment variables are set or inferred.
- Build the APK.
- Install it to the connected device.
- Launch the app.

Optional:
- Override the launch app ID with `APP_ID` (bash) or `-AppId` (PowerShell).

### Smallest APK build target

Use the `minsize` profile.
It enables release build and size-focused Rust settings.

Windows:
```powershell
.\scripts\build_android.ps1 -Profile minsize
.\scripts\deploy_android.ps1 -Profile minsize
```

macOS / Linux:
```bash
PROFILE=minsize ./scripts/build_android.sh
PROFILE=minsize ./scripts/deploy_android.sh
```

### Extra build options

Pass extra args to `dx`:

Windows:
```powershell
.\scripts\build_android.ps1 -DxArgs "--verbose"
```

macOS / Linux:
```bash
DX_ARGS="--verbose" ./scripts/build_android.sh
```

Target a different ABI (emulators, etc.):

Windows:
```powershell
.\scripts\build_android.ps1 -Target x86_64
```

macOS / Linux:
```bash
TARGET=x86_64 ./scripts/build_android.sh
```

## Install on Device
1. Build the Android artifact.
2. Find the APK path from the `dx build` output (typically `target/dx/hear_buds/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`).
3. Install:
```bash
adb install -r /path/to/your.apk
```

## Roadmap (short)
- Feedback suppression / notch filters.
- Refine calibration -> EQ mapping with real-world tests.
- On-device latency and CPU profiling.
- Solidify Bluetooth input device routing.
- Ensure smooth operation when the screen is locked

## Contributing
Issues and pull requests are welcome.

Please read:
- [CONTRIBUTING.md](CONTRIBUTING.md) for setup, coding standards, and required checks.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for expected behavior in the community.

Short version:
- Keep real-time audio paths fast and safe.
- Keep changes focused.
- Run `cargo fmt`, `cargo clippy`, and `cargo check` before opening a PR.

## CI
GitHub Actions runs:
- `cargo fmt --check`
- `cargo clippy --no-default-features --features desktop`
- `cargo check --no-default-features --features desktop`
- `cargo check --target aarch64-linux-android --no-default-features --features mobile`

## License
Apache-2.0. See `LICENSE`.
