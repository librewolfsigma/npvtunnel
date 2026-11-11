# Changelog

All notable changes to npvtunnel are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versioning
follows semver.

## [0.2.0] - 2026-11-12

### Added

- Native Windows GUI crate `npvt-gui` shipping `NPVTunnel.exe`: a
  GUI-subsystem binary (`#![windows_subsystem = "windows"]`) so double-click
  never opens a terminal, styled after the NPV Tunnel mobile design and
  optimized for the desktop window.
  - HOME: circular power connect/disconnect with animated connecting state,
    `[ CONNECTED ]` / `[ CONNECTING ]` / `[ DISCONNECTED ]` status, the active
    connection group card, PING button and live DOWNLINK/UPLINK readouts.
  - CONFIGS: expandable groups (Smart Route / Saved / Global Network) with
    inline ping, edit, delete, pin and kebab actions, per-profile latency
    chips, the Global Network usage bar, a search field, sort menu and the
    floating pill toolbar with the green FAB.
  - Add-config menu: subscription URL fetch (base64 or plain), clipboard
    import, QR-code image import (rqrr), `.npvt`/`.npvs` file import with
    passphrase/key unlock, manual config entry and connection-group editor.
  - LOGS: live sing-box engine log with filter, copy, clear and reveal.
  - MORE: Premium, Light/Dark/Auto appearance (Auto follows the Windows
    theme via the registry), My public key (X25519 + Ed25519, copyable),
    Per-app Proxy (allow/deny process rules), Points, Settings (SOCKS port,
    clash API port, DNS, autoconnect, start-with-Windows) and User assets
    (geoip/geosite downloads from the official sing-geoip/sing-geosite
    releases).
  - Export/share page: Config Filename, Lock it / Keep it open, the Secrecy
    slider (maps to signed expiry policy), Specific people (recipient X25519
    keys) or Passphrase delivery, producing signed `.npvt` v2 containers.
- Connection groups, per-profile traffic usage, settings and the points
  ledger persist in the npvtunnel data directory (`groups.json`,
  `settings.json`).
- Engine control: adopt a running CLI-started engine on launch, background
  TCP latency sweeps, live traffic sampling from the sing-box clash API, and
  usage accounting per profile.

### Changed

- The release zip now contains `NPVTunnel.exe` (GUI, headline binary) and
  `npvtunnel.exe` (CLI) plus the docs.
- `npvt-core::engine` spawns every child process with `CREATE_NO_WINDOW` on
  Windows: the GUI no longer risks console flashes when launching sing-box,
  `tasklist` or `taskkill`.
- `.npvt` v2 gained key-helper APIs (`signing_key_from_seed`,
  `x25519_public_from_secret`, `key_to_b64`, `parse_recipient_key`) to serve
  the GUI's share/export flows.

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
