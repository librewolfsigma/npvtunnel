# NPVTunnel (npvtunnel)

A lightweight, security-hardened client for the `.npvt` configuration
ecosystem, built for Windows. Native GUI, no ads, no telemetry.

- **GUI (`NPVTunnel.exe`):** double-click and go - a console-free,
  windows-subsystem binary with the dark green NPV Tunnel interface:
  HOME (power button, connection status, active group card, live
  downlink/uplink), CONFIGS (groups, pings, usage, import), LOGS
  (live engine output) and MORE (appearance, keys, per-app proxy,
  points, settings, geo assets).
- **CLI (`npvt-cli.exe`):** the same vault from a terminal - import, export,
  lock, share, list, connect.
- **File experience:** `.npvt` / `.npvs` files and `npvt://` links import
  through a security card; the default action is *add to library*, never
  silent connect.
- **Container crypto (.npvt v2):** Argon2id + XChaCha20-Poly1305 passphrase
  locks, X25519 multi-recipient key locks, Ed25519 creator signatures, signed
  expiry/policy blocks. The legacy v1 "lock" is publicly broken obfuscation;
  NPVTunnel refuses to reimplement it and imports plaintext v1 files only.
- **Engine:** sing-box sidecar (local SOCKS5 on `127.0.0.1`, never
  LAN-exposed by default).
- **Status:** Phase 2 build - core + CLI + native GUI. TUN mode, kill switch
  and the file-association installer are the next milestones (see roadmap).

## Tech stack

| Layer | Choice |
| ------ | ------ |
| Language | Rust (edition 2021, MSRV 1.85), no unsafe |
| GUI | egui/eframe 0.29, glow backend, painter-drawn icons |
| GUI fonts | Bundled Inter + JetBrains Mono (OFL, see THIRD-PARTY) |
| GUI binary | `windows` subsystem - no console on double-click |
| CLI | clap 4 verb-first, stable exit codes, shell completions |
| Crypto | Argon2id, XChaCha20-Poly1305, X25519 locks, Ed25519 sigs |
| Vault | JSON on disk, atomic writes, Unix 0600, zeroize |
| Engine | sing-box sidecar, SOCKS5 bound to 127.0.0.1 |
| Windows | msvc, static CRT, thin LTO, stripped |
| CI | master-ci v2, CodeQL, gitleaks, tag releases via gh |
| Engine | sing-box sidecar; SOCKS5 inbound bound to 127.0.0.1 |
| Engine protocols | vless, vmess, trojan, shadowsocks, hysteria2, socks, http |
| Windows binary | msvc target, static CRT (`+crt-static`), thin LTO, stripped |
| CI / release | master-ci v2, CodeQL (rust), gitleaks, zizmor, Dependabot |
| Release artifacts | release.yml publishes zip + SHA256SUMS via gh |

## Build

Requires Rust 1.85+ (stable).

```sh
cargo build --release
# GUI binary:  target/release/NPVTunnel(.exe)
# CLI binary:  target/release/npvtunnel(.exe)
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

## GUI quick start

1. Download `npvtunnel-windows-x64-<tag>.zip` from the Releases page and
   unzip it anywhere - `sing-box.exe` ships inside the zip next to
   `NPVTunnel.exe`, so it works out of the box.
2. Double-click `NPVTunnel.exe`. No console window appears - the binary is a
   GUI-subsystem executable and every helper process it spawns is created
   with `CREATE_NO_WINDOW`.
3. **CONFIGS > +** imports configs: subscription URL, clipboard, QR code
   image, `.npvt`/`.npvs` files, or manual entry. **New connection group**
   builds Smart Route / Round robin / Manual groups.
4. Press the power button on HOME. `[ CONNECTED ]` appears and the card shows
   live downlink/uplink from the engine's clash API.
5. Group rows support pinning, per-group ping sweeps (speed icon), editing,
   deletion and *Export / share* (signed `.npvt` v2 containers: open,
   passphrase-locked or key-locked to recipient keys).
6. Closing the window leaves the tunnel running; the next launch re-adopts
   the engine. Stop it from HOME or the CLI with `npvtunnel disconnect`.

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
```

## Roadmap

| Phase | Scope |
| ------ | ------ |
| P1 (done) | Core + CLI: import/export/lock/share/list/connect, v2 crypto |
| P2 (done) | Native GUI: vault, groups, engine control, sheets |
| P3 | wintun TUN mode, kill switch, per-process routing, file association |
| P4 | Hardening: fuzzing, threat-model review, signed updater, AV |
| P5 | Beta + GA channels (winget/MSI) |

## License

MIT — see [LICENSE](LICENSE).
