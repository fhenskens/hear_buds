# Privacy Policy for HearBuds

Effective date: March 12, 2026

HearBuds is a local-first audio processing application for Android and desktop. This Privacy Policy explains what information the app accesses, what information it stores, and how that information is used.

## Summary

- HearBuds uses your microphone to process audio in real time.
- HearBuds stores your settings and hearing-related tuning data on your device.
- HearBuds does not require an account.
- HearBuds does not include in-app advertising.
- HearBuds does not include in-app analytics or telemetry in the current codebase.

## Information HearBuds Accesses

### Microphone audio

HearBuds requests microphone access so it can capture live audio input, apply DSP processing, run calibration sweeps, and play processed audio through your selected output device.

In the current version of the app, microphone audio is used for real-time processing on your device. HearBuds is not designed to create a user account profile from your microphone audio.

### Settings and hearing-related data stored on your device

HearBuds stores app configuration data locally so your setup persists between sessions. Depending on how you use the app, this may include:

- EQ band values
- calibration results
- hearing threshold values
- noise reduction settings
- gain, limiter, AGC, and filter settings
- saved user profiles
- selected input and output device preferences

On Android, this data is stored in the app's internal files directory. On desktop builds, the current code stores settings in a local `hear_buds_settings.json` file.

### Technical and runtime information

The app may access device and runtime information needed to operate audio features correctly, such as available input and output devices, platform audio backend settings, and permission status.

Development and runtime logs may include limited technical details such as device names, route selection, sample rates, or audio errors. These logs are intended for debugging and diagnostics and are not used as in-app analytics.

## What HearBuds Does Not Currently Do

Based on the current project codebase:

- HearBuds does not require you to create an account.
- HearBuds does not include in-app ads.
- HearBuds does not include third-party analytics SDKs or telemetry collection.
- HearBuds does not intentionally upload your saved settings, calibration results, or hearing profile data to a HearBuds-operated server.

If future versions add cloud sync, crash reporting, analytics, account features, or other online services, this Privacy Policy should be updated before or when those features are released.

## Permissions

Depending on platform and version, HearBuds may request or use permissions including:

- microphone access, to capture live audio
- foreground service access on Android, so audio processing can continue while the app runs in the background
- wake lock related capability on Android, to help keep processing active during use
- internet permission on Android, which is declared in the current app manifest

The presence of a platform permission does not necessarily mean HearBuds uses it for tracking or advertising.

## Sharing of Information

HearBuds does not sell your personal information.

In the current codebase, HearBuds does not intentionally share your microphone audio, saved settings, calibration data, or hearing thresholds with third parties through an in-app service operated by the project.

Third-party platforms or distributors you use to obtain the app, such as app stores, code hosting services, or operating system vendors, may collect their own data independently under their own privacy policies.

## Third-Party Services and Links

The HearBuds project may be distributed or hosted through third-party services such as GitHub, Android platform tooling, or app distribution channels. Those services operate independently and have their own privacy practices.

If HearBuds links to third-party websites or resources, those sites are governed by their own privacy policies.

## Data Security

HearBuds is designed to keep core settings and processing local to your device. However, no method of storage or transmission is completely secure, and absolute security cannot be guaranteed.

You are responsible for the security of the device on which you use HearBuds.

## Your Choices

You can limit data use by:

- denying microphone permission, though core audio features may not work
- deleting the app's locally stored settings or profiles from your device
- uninstalling the app

## Changes to This Privacy Policy

This Privacy Policy may be updated from time to time. Any updated version should replace this file and use a new effective date.

## Contact

For questions about this Privacy Policy, please open an issue at:

https://github.com/fhenskens/hear_buds/issues
