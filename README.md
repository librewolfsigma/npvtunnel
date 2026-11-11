# NPVTunnel (NPV TUNNEL)

A lightweight, security-hardened client for the `.npvt` configuration
ecosystem, built for Windows. Ships as a native desktop GUI (`NPVTunnel.exe`)
and a scriptable CLI (`npvt-cli.exe`). No ads, no telemetry, no console
windows on double-click.

- **Native Windows GUI:** dark, neon-green desktop app with the big connect
  power button, live traffic stats (down/up rates and session totals via the
  engine's local clash API), endpoint ping, config groups, import/export with
  the same security card as the CLI, and Dark / Light / Auto themes.
- **File experience:** `.npvt` / `.npvs` files and `npvt://` links import
  through a security card; the default action is *add to library*, never
  silent connect.
- **Container crypto (.npvt v2):** Argon2id + XChaCha20-Poly1305 passphrase
  locks, X25519 multi-recipient key locks, Ed25519 creator signatures, signed
  expiry/policy blocks. The legacy v1 "lock" is publicly broken obfuscation;
  NPVTunnel refuses to reimplement it and imports plaintext v1 files only.
- **Engine:** sing-box sidecar (bundled in the release zip), local SOCKS5 on
  `127.0.0.1`, never LAN-exposed by default.
- **Status:** Phase 1-2 build — core + CLI + native GUI. TUN mode, kill
  switch, per-process routing, tray and the file-association installer are
  the next milestones (see roadmap).

## Tech stack

| Layer | Choice |
|-------|--------|
| Language | Rust (edition 2021, MSRV 1.85), `#![forbid(unsafe_code)]` workspace-wide |
| GUI | `eframe`/`egui` 0.37 (wgpu renderer), Inter typeface, hand-drawn vector icon glyphs, `rfd` native file dialogs, `windows`-subsystem binary (no terminal on launch) |
| CLI | `clap` 4 (derive) verb-first interface, stable exit codes, `clap_complete` shell completions |
| Container crypto | Argon2id (64 MiB, t=3, p=4) KDF, XChaCha20-Poly1305 AEAD, X25519 multi-recipient locks, HKDF-SHA256, Ed25519 `verify_strict` signatures |
| Vault | JSON on disk, atomic writes, Unix `0600`, `zeroize` on drop |
| Engine | sing-box sidecar rendering vless / vmess / trojan / shadowsocks / hysteria2 / socks / http outbounds; SOCKS5 inbound bound to `127.0.0.1`; optional local clash API for live traffic stats |
| Windows binary | `x86_64-pc-windows-msvc`, statically linked CRT (`+crt-static`), thin LTO, stripped |
| CI / release | master-ci v2 (fmt, clippy, cargo-deny, cargo-machete, tests), CodeQL (rust), gitleaks, zizmor, Dependabot; tag-triggered `release.yml` publishes the Windows zip (GUI + CLI + bundled sing-box) + `SHA256SUMS.txt` via the `gh` CLI |

## Build

Requires Rust 1.85+ (stable).

```sh
cargo build --release
# binaries: target/release/NPVTunnel.exe (GUI) and target/release/npvt-cli.exe
```

CI builds and tests the Windows x64 release binaries on every push; pushing a
`v*` tag publishes a GitHub Release with `npvtunnel-windows-x64-<tag>.zip`
(GUI, CLI and the bundled sing-box engine) and a `SHA256SUMS.txt` checksum
file.

## Quick start (GUI)

1. Download the release zip and unpack it anywhere.
2. Double-click `NPVTunnel.exe` - a window opens, no terminal involved.
3. Configs -> Add config: import an `.npvt` file, paste a share link from the
   clipboard, or add one manually.
4. Press the power button. `[ CONNECTED ]` shows live down/up traffic and the
   endpoint ping; press it again to disconnect.
5. Logs shows the engine output; More holds theme, ports, identity and
   diagnostics.

The tunnel keeps running if you close the window while connected (the app
asks first); the CLI can stop it any time.

## Quick start (CLI)

```sh
# 1. Create your creator identity (signing + recipient keys)
npvt-cli keygen

# 2. Import from a share link, npvt:// link, or a .npvt/.npvs file
npvt-cli import "vless://...@example.com:443?security=tls#my-shop"
npvt-cli import "C:\\Users\\me\\Downloads\\shop.npvt"

# 3. Inspect the library (credentials are never printed here)
npvt-cli list
npvt-cli show my-shop

# 4. Export a locked, signed container
npvt-cli export my-shop --out shop.npvt2 --expires +30d

# 5. Connect through the sing-box sidecar (SOCKS5 on 127.0.0.1:2080)
npvt-cli connect my-shop
npvt-cli status
npvt-cli disconnect

# 6. Self-check
npvt-cli doctor
```

Exit codes are stable: `0` ok, `1` usage, `2` config, `3` network/engine,
`4` permission, `5` crypto, `6` io, `7` other.

## Security posture

- `#![forbid(unsafe_code)]` across the workspace.
- All profile fields are length-capped and control-character sanitized before
  they can reach an engine config (config-injection defense).
- Wrong passphrase and tampered container are indistinguishable
  (`authentication failed`); signatures verify with `verify_strict` and fail
  closed.
- Credentials live in the local vault, are zeroized when dropped, and are
  never printed unless `show --reveal` is used explicitly. Passphrases are
  prompted hidden or read from stdin — never from argv.
- Expired containers are refused at import; the engine inbound binds
  localhost only; the engine is spawned directly (no shell) with logs to a
  local file.
- CI: clippy `-D warnings`, `cargo-deny` advisories + license allow-list,
  cargo-machete, CodeQL, Dependabot, actions pinned by commit SHA, and the
  master-ci secrets scanner.

See [SECURITY.md](SECURITY.md) for reporting policy and hardening notes.

## Troubleshooting (Windows)

- **The GUI shows "ENGINE MISSING".** The release zip bundles `sing-box.exe`
  next to `NPVTunnel.exe`; keep the two together. Otherwise put
  `sing-box.exe` in `%APPDATA%\npvtunnel\bin\` or on `PATH`. The More page
  shows exactly what was detected, and `npvt-cli doctor` lists the checks.
- **Port already in use on connect.** The GUI and CLI share one engine
  session; if `npvt-cli connect` is already up, disconnect there first (the
  error names the running profile).
- **`VCRUNTIME140.dll was not found`.** Fixed since 0.1.1: the release
  binaries statically link the C runtime and have no redistributable
  dependency. Use the latest release zip.
- **SmartScreen warns about an unsigned binary.** Choose "More info" then
  "Run anyway", and verify the zip against `SHA256SUMS.txt`. Authenticode
  signing is on the P3 roadmap (see SECURITY.md).

## Legacy v1 files

Plaintext v1 JSON configurations import fine. Locked v1 files are rejected
with an honest error: the v1 lock is client-side obfuscation with public
decrypters, and security-by-obscurity is exactly what this project replaces.
Re-export such configs as v2 containers (creators: `npvt-cli export`).

## Workspace layout

```text
crates/npvt-core   profiles, URI import, .npvt v2 container, vault, engine glue
crates/npvt-cli    the npvt-cli binary (import/export/lock/share/list/connect)
crates/npvt-gui    the NPV TUNNEL desktop app (egui/eframe, windows-subsystem)
assets             app icon, Inter fonts (SIL OFL 1.1)
```

## Roadmap

| Phase | Scope |
|-------|-------|
| P1 (shipped) | Core + CLI: import/export/lock/share/list/connect, v2 crypto, vault, sing-box sidecar |
| P2 (shipped) | Native Windows GUI: power button, live traffic, ping, config groups, import/export, themes |
| P3 | Hardening: tray mode, .npvt file association installer, fuzzing, threat-model review, signed updater, AV pre-submissions |
| P4/P5 | TUN mode, kill switch, per-process routing, seller closed beta, GA channels (winget/MSI) |

## License

MIT — see [LICENSE](LICENSE).
