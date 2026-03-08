# Contributing to HearBuds

Thanks for helping with HearBuds.
This file explains how to contribute.

## Prerequisites

- Rust `1.85` or newer (MSRV).
- `rustfmt` and `clippy` components installed.

## Setup

```bash
git clone <your-fork-or-repo-url>
cd hear_buds
cargo check --no-default-features --features desktop
```

## Development Standards

- Keep real-time audio code safe in callback paths:
  no allocation, no blocking locks, and no syscalls in the hot path.
- Keep PRs small and focused.
- Update docs when public APIs change.
- Add tests for behavior changes.

## Required Checks

Run these before opening a PR:

```bash
cargo fmt --check
cargo clippy --no-default-features --features desktop -- -D warnings
cargo check --no-default-features --features desktop
cargo check --target aarch64-linux-android --no-default-features --features mobile
```

If you have an Android device connected, you can also do a manual install:

```bash
./scripts/deploy_android.sh
```

## Pull Requests

- Link related issues.
- Explain what changed and which platforms are affected.
- Note any latency, glitch, or recovery impact.
- List follow-up work if something is intentionally left for later.

## Commit Guidance

- Use clear commit messages in imperative voice.
- Keep unrelated refactors out of feature/fix commits.

## Reporting Issues

When reporting a bug, include:

- Platform and device details.
- Steps to reproduce.
- Expected vs actual behavior.
- Logs and error codes when possible.

## Code of Conduct

By participating, you agree to follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
