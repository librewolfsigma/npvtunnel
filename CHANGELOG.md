# Changelog

All notable changes to npvtunnel are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versioning
follows semver.

## [0.4.0] - 2026-11-12

### Changed

- GUI rebuilt line-by-line against the reference design with full visual
  verification (rendered under a headless compositor and compared to the
  mockups): exact palette, spaced mono headers, gold premium badge, pulsing
  power button, connection group card, group cards with pins/pings/trash,
  floating add toolbar, bottom nav pills and the add-config sheet.
- Every control in the mockups is now wired to real behavior:
  - CONFIGS: expand/collapse-all, search, sort (name / ping / recently
    added), per-group ping, pin-to-active, rename, delete confirmations,
    member rows with per-config ping values, share/edit/remove actions and
    subscription or monthly-quota traffic bars.
  - HOME: PING action, live downlink/uplink (clash API stream), error card
    with engine-missing guidance, Fastest member strategy on group connect.
  - Share sheet: secrecy slider maps to container expiry (never / 30 days /
    24 hours), passphrases require 8+ chars and confirmation, recipients
    parse as base64url X25519 keys, exports are Ed25519-signed, plus a
    copy-as-`npvt://`-link action.
  - MORE: appearance (light/dark/auto), my public key (X25519 + Ed25519),
    per-app proxy (block or allow-only via sing-box `process_name` rules),
    settings (ports, DNS, engine path, log level, autostart Run-key entry,
    connect-on-launch, monthly quota), user assets downloader, about/quit.
- Traffic accounting adds lifetime counters so the monthly quota survives
  disconnects; the monthly counter rolls automatically.
- GUI binary renamed to `NPVTunnel.exe` (double-click product name).

### Fixed

- Locked-container import prompts for a passphrase instead of failing;
  wrong passphrases show inline errors and can be retried.
- Modal dialogs hug their content and scroll instead of stretching
  full-height; Esc and click-outside close them.
- Clipboard/QR/file import paths share one dedup + validation flow.

## [0.2.0] - 2026-11-11

### Added

- **Native Windows GUI (`NPVTunnel.exe`, new `npvt-gui` crate):** a dark,
  neon-green desktop app in the NPV TUNNEL brand style - the signature power
  button with a breathing glow, `[ CONNECTED ]` status, active-connection
  card, live traffic stats (down/up rates plus session totals from the
  engine's local clash API), endpoint ping with colored latency badges,
  grouped config cards, and a left sidebar (Home / Configs / Logs / More).
- The release zip now bundles `sing-box.exe` next to the app, so connect
  works out of the box; the More page shows the detected engine path.
- Import flows in the GUI: `.npvt`/`.npvs` file picker, clipboard share
  links, a passphrase unlock dialog for locked containers, and the security
  card (lock kind, signature, expiry, allow_insecure warnings) before
  anything enters the vault.
- Export dialog with lock-it (passphrase) / keep-it-open modes, optional
  expiry in days, group hints and creator-key signing; manual add/edit form
  for all engine-supported protocols; delete confirmation.
- More page: Dark / Light / Auto themes (Auto follows the Windows
  personalization setting), recipient public key with copy, engine ports and
  DNS, auto-ping / auto-connect options, data-folder shortcut.
- Shared engine session (`npvt-core::session`): the GUI and the CLI both see
  and can stop the same tunnel through one pid file; closing the GUI while
  connected asks whether to stop the tunnel or leave it up.
- App icon (dark rounded square, neon power glyph) embedded into the exe and
  the window; Inter typeface bundled (SIL OFL 1.1); third-party license
  notices ship in the zip.

### Changed

- The CLI binary is renamed `npvtunnel` -> `npvt-cli` so it cannot collide
  with `NPVTunnel.exe` on case-insensitive Windows filesystems.
- `engine::config` takes an optional local clash-API port so front ends can
  poll live traffic; the CLI keeps its previous behavior by passing `None`.
- Release zip now contains `NPVTunnel.exe`, `npvt-cli.exe`, `sing-box.exe`,
  a plain-text quick start, license notices and `SHA256SUMS.txt`.

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