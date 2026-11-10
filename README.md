# NPVTunnel (npvtunnel)

A lightweight, security-hardened client for the `.npvt` configuration
ecosystem, built for Windows. CLI-first, native, no ads, no telemetry.

- **Command:** `npvtunnel` — import, export, lock, share, list, connect.
- **File experience:** `.npvt` / `.npvs` files and `npvt://` links import
  through a security card; the default action is *add to library*, never
  silent connect.
- **Container crypto (.npvt v2):** Argon2id + XChaCha20-Poly1305 passphrase
  locks, X25519 multi-recipient key locks, Ed25519 creator signatures, signed
  expiry/policy blocks. The legacy v1 "lock" is publicly broken obfuscation;
  NPVTunnel refuses to reimplement it and imports plaintext v1 files only.
- **Engine:** sing-box sidecar (local SOCKS5 on `127.0.0.1`, never
  LAN-exposed by default).
- **Status:** Phase 2 build — core + CLI + **native desktop GUI**. TUN mode,
  kill switch, per-process routing, tray icon and the file-association
  installer are the next milestones (see roadmap).

## Desktop GUI (0.2.0+)

`npvtunnel-gui.exe` is the native Windows desktop client: a single exe with
no runtime dependencies, built with egui/eframe.

- Dark dashboard: one-click **Connect / Disconnect**, live status
  (Offline / Connecting / Protected / Error), uptime and SOCKS endpoint.
- Profile library with import via **file dialog or share link** — every
  import shows the security card (lock type, signature, expiry, TLS
  warnings) before anything is added.
- Passphrase-locked, signed **exports** straight from the library.
- Engine diagnostics (vault, identity, sing-box location) and an activity log.
- The GUI and the CLI share the same vault and the same sing-box sidecar:
  either surface can pick up a tunnel the other started.

## Tech stack

| Layer | Choice |
|-------|--------|
| Language | Rust (edition 2021, MSRV 1.85), `#![forbid(unsafe_code)]` workspace-wide |
| CLI | `clap` 4 (derive) verb-first interface, stable exit codes, `clap_complete` shell completions |
| Container crypto | Argon2id (64 MiB, t=3, p=4) KDF, XChaCha20-Poly1305 AEAD, X25519 multi-recipient locks, HKDF-SHA256, Ed25519 `verify_strict` signatures |
| Vault | JSON on disk, atomic writes, Unix `0600`, `zeroize` on drop |
| Engine | sing-box sidecar rendering vless / vmess / trojan / shadowsocks / hysteria2 / socks / http outbounds; SOCKS5 inbound bound to `127.0.0.1` |
| Windows binary | `x86_64-pc-windows-msvc`, statically linked CRT (`+crt-static`), thin LTO, stripped |
| CI / release | master-ci v2 (fmt, clippy, cargo-deny, cargo-machete, tests), CodeQL (rust), gitleaks, zizmor, Dependabot; tag-triggered `release.yml` publishes the Windows zip + `SHA256SUMS.txt` via the `gh` CLI |

## Build

Requires Rust 1.85+ (stable).

```sh
cargo build --release
# binary: target/release/npvtunnel(.exe)
```

CI builds and tests the Windows x64 release binary on every push; pushing a
`v*` tag publishes a GitHub Release with `npvtunnel-windows-x64-<tag>.zip`
and a `SHA256SUMS.txt` checksum file.

## Quick start

```sh
# 1. Create your creator identity (signing + recipient keys)
npvtunnel keygen

# 2. Import from a share link, npvt:// link, or a .npvt/.npvs file
npvtunnel import "vless://...@example.com:443?security=tls#my-shop"
npvtunnel import "C:\\Users\\me\\Downloads\\shop.npvt"

# 3. Inspect the library (credentials are never printed here)
npvtunnel list
npvtunnel show my-shop

# 4. Export a locked, signed container
npvtunnel export my-shop --out shop.npvt2 --expires +30d

# 5. Connect through the sing-box sidecar (SOCKS5 on 127.0.0.1:2080)
npvtunnel connect my-shop
npvtunnel status
npvtunnel disconnect

# 6. Self-check
npvtunnel doctor
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

- **A console window flashes and disappears when I double-click
  `npvtunnel.exe`.** That is not a crash: npvtunnel is a command-line tool and
  the window closes as soon as the program finishes. Since 0.1.1 a
  double-click shows a quick-start screen and waits for Enter. Run the
  commands from the quick start above in PowerShell or Windows Terminal.
- **`VCRUNTIME140.dll was not found`.** Fixed since 0.1.1: the release
  binary statically links the C runtime and has no redistributable
  dependency. Use the latest release zip.
- **`connect` says the sing-box binary was not found.** Put `sing-box.exe`
  next to `npvtunnel.exe`, or in `%APPDATA%\npvtunnel\bin\`, or on `PATH`.
  `npvtunnel doctor` shows exactly what was checked.
- **SmartScreen warns about an unsigned binary.** Choose "More info" then
  "Run anyway", and verify the zip against `SHA256SUMS.txt`. Authenticode
  signing is on the P3 roadmap (see SECURITY.md).

## Legacy v1 files

Plaintext v1 JSON configurations import fine. Locked v1 files are rejected
with an honest error: the v1 lock is client-side obfuscation with public
decrypters, and security-by-obscurity is exactly what this project replaces.
Re-export such configs as v2 containers ( creators: `npvtunnel export`).

## Workspace layout

```text
crates/npvt-core   profiles, URI import, .npvt v2 container, vault, engine glue
crates/npvt-cli    the npvtunnel binary
crates/npvt-gui    the npvtunnel-gui desktop client (egui/eframe)
```

## Roadmap

| Phase | Scope |
|-------|-------|
| P1 (done) | Core + CLI: import/export/lock/share/list/connect, v2 crypto, vault, sing-box sidecar |
| P2 (this) | Native Windows GUI: dashboard, library, import/export, diagnostics |
| P2.5 | Tray icon, .npvt file association, autostart |
| P3 | TUN mode, kill switch, per-process routing |

## License

MIT — see [LICENSE](LICENSE).
