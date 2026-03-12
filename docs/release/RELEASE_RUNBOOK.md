# Release Runbook

This runbook defines a repeatable release flow for HearBuds Android artifacts.

## 1) Prepare

1. Update version in `Cargo.toml`.
2. Update release notes/changelog text.
3. Confirm clean git working tree.
4. Run checks:
   - `cargo fmt --check`
   - `cargo clippy --no-default-features --features desktop -- -D warnings`
   - `cargo check --no-default-features --features desktop`
   - `cargo check --target aarch64-linux-android --no-default-features --features mobile`

## 2) Build Android Artifacts

Release APK:
- macOS/Linux: `PROFILE=release ./scripts/build_android.sh`
- Windows: `.\scripts\build_android.ps1 -Profile release`

Release App Bundle (Play upload):
- macOS/Linux: `./scripts/build_android_bundle.sh`
- Windows: `.\scripts\build_android_bundle.ps1`

The bundle script prints the `.aab` output path when complete.

## 3) Validate On Devices

1. Install release APK on test devices.
2. Run through `DEVICE_TEST_CHECKLIST.txt`.
3. Capture known issues and block release on critical regressions.

## 4) Compliance Pack

Before store submission:
1. Finalize privacy policy from `docs/release/PRIVACY_POLICY_TEMPLATE.md`.
2. Verify store descriptions avoid medical-device claims.
3. Confirm permission declaration text matches actual app behavior.

## 5) Tag And Push

1. Create release tag:
   - `git tag -a vX.Y.Z -m "vX.Y.Z"`
2. Push branch and tag:
   - `git push origin main`
   - `git push origin vX.Y.Z`

## 6) Submit

Google Play:
1. Upload `.aab`.
2. Complete Play declarations and release notes.
3. Roll out internal testing first, then production.

F-Droid:
1. Ensure source-release reproducibility.
2. Prepare fdroiddata metadata submission.
3. Submit and monitor build logs.

