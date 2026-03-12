# Publish Readiness Checklist

This checklist is for shipping HearBuds to:
- Google Play
- F-Droid

Use it as a gate. If any required item is incomplete, do not publish.

## 1) Product And Compliance Positioning (Required)

- [ ] Product copy avoids medical-device claims.
- [ ] No UI/docs text calls the app a "hearing aid" or implies diagnosis/treatment.
- [ ] Store listing language uses non-medical wording (for example: sound enhancement, sound profile, listening support).
- [ ] Legal review completed for target markets.

## 2) Stability And Device Validation (Required)

- [ ] Android mic permission flow tested on multiple Android versions/devices.
- [ ] Bluetooth input/output routing verified on representative headsets/earbuds.
- [ ] Background behavior validated (screen off, app switch, call interruptions).
- [ ] No severe audio glitches, clipping spikes, or stuck-audio states.
- [ ] Manual device test checklist completed for release candidate.

Reference:
- `DEVICE_TEST_CHECKLIST.txt`

## 3) Release Build Artifacts (Required)

- [ ] Signed release App Bundle (`.aab`) generated for Play.
- [ ] Signed release APK(s) generated for F-Droid workflow/testing.
- [ ] Version updated (`Cargo.toml` and release notes).
- [ ] Release tag created and pushed.
- [ ] Reproducible build steps documented.

References:
- `scripts/build_android.sh`
- `scripts/build_android_bundle.sh`
- `scripts/build_android_bundle.ps1`
- `docs/release/RELEASE_RUNBOOK.md`

## 4) Google Play Specific (Required For Play)

- [ ] Google Play Console app created.
- [ ] App category/content rating completed.
- [ ] Data safety form completed.
- [ ] Permission declarations completed (microphone + foreground service usage).
- [ ] Privacy policy URL published and entered in Play Console.
- [ ] Store listing assets complete (icons, screenshots, short/full descriptions).
- [ ] Internal testing rollout passes on target devices.

## 5) F-Droid Specific (Required For F-Droid)

- [ ] App builds from source in a clean environment.
- [ ] No proprietary binaries embedded in source releases.
- [ ] License and copyright metadata are complete.
- [ ] Fast-changing network dependencies are pinned and auditable.
- [ ] F-Droid metadata prepared (`metadata/*.yml` in fdroiddata submission).
- [ ] Build recipe tested before requesting inclusion.

## 6) Security And Privacy (Required)

- [ ] Privacy policy written, reviewed, and publicly hosted.
- [ ] Data collection behavior documented (including "none", if true).
- [ ] Crash/log handling reviewed to ensure no unintended sensitive audio data retention.
- [ ] Network behavior documented and justified.

Reference:
- `docs/release/PRIVACY_POLICY_TEMPLATE.md`

## 7) Release Decision

Go/No-Go:
- Publish only when all required items are checked.

