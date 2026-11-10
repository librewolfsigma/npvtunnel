# Changelog

All notable changes to npvtunnel are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versioning
follows semver.

## [0.1.1] - 2026-11-11

### Fixed

- Double-clicking `npvtunnel.exe` no longer looks like a crash: with no
  arguments the CLI now shows a welcome/quick-start screen and, on Windows,
  waits for Enter before the console window closes. Previously clap printed
  a usage error and the window vanished instantly, which read as "crashed
  and did not load".
- Windows binaries now statically link the C runtime (`+crt-static`), removing
  the `VCRUNTIME140.dll` dependency that could stop the exe from starting on
  machines without the VC++ redistributable.
- `connect` verifies the sing-box process is still alive 600 ms after spawn
  and, if it died, reports the last engine log lines and the config path
  instead of a silent "tunnel up" with nothing listening.
- Engine liveness on Windows now matches the pid as an exact quoted CSV field
  of `tasklist` output, so a substring of another column (for example a
  memory figure) can no longer make a stale pid file look like a running
  engine.
- Unexpected internal errors (panics) print a clean report message with a
  bug-tracker link instead of the raw panic dump; the double-click window
  stays open until dismissed.

### Changed

- Console output is ASCII-only (security-card rules, profile-name
  de-duplication separator) so legacy Windows code pages cannot garble it.
- An unknown container lock kind now returns a clean config error instead of
  an internal assertion.

## [0.1.0] - 2026-11-10

### Added

- Initial release: `npvt-core` (profiles, URI import, .npvt v2 container
  crypto with Argon2id + XChaCha20-Poly1305, X25519 key locks, Ed25519
  signatures, vault, sing-box engine glue) and the `npvtunnel` CLI
  (keygen, import, export, list, show, remove, connect, disconnect, status,
  doctor, completions).
- CI: WasewaseX/master-ci v2 gates (fmt, clippy, cargo-deny, cargo-machete,
  tests), CodeQL for Rust, gitleaks, zizmor, Dependabot, actions pinned by
  commit SHA.
- Tag-triggered `release.yml` that builds the Windows x64 binary, packages
  `npvtunnel-windows-x64-<tag>.zip` with `SHA256SUMS.txt` and publishes the
  GitHub Release via the preinstalled `gh` CLI.

## [0.2.0] - 2026-11-11

### Added

- Native Windows desktop GUI (`npvtunnel-gui.exe`, egui/eframe, single exe,
  no runtime dependencies): dark dashboard with one-click connect/disconnect,
  live status (Offline / Connecting / Protected / Error), uptime and SOCKS
  endpoint display, profile library with security-card import (file dialog or
  share link), passphrase-locked signed exports, engine diagnostics and an
  activity log.
- The GUI and the CLI share the same vault and the same sing-box sidecar;
  either surface picks up a tunnel the other started (shared pid file).
- Embedded application icon (shield + bolt) for window and taskbar.

### Changed

- Engine pid-file handling and expiry parsing moved into `npvt-core`
  (`engine::read/write_pid_file`, `profile::parse_expires`) so the CLI and
  the GUI share one implementation.
