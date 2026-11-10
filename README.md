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
- **Status:** Phase 1 build — core + CLI. GUI, TUN mode, kill switch,
  per-process routing and the file-association installer are the next
  milestones (see roadmap).

## Build

Requires Rust 1.85+ (stable).

```sh
cargo build --release
# binary: target/release/npvtunnel(.exe)
```

CI builds and tests the Windows x64 release binary on every push; download it
from the workflow run artifacts (`npvtunnel-windows-x64`).

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

## Legacy v1 files

Plaintext v1 JSON configurations import fine. Locked v1 files are rejected
with an honest error: the v1 lock is client-side obfuscation with public
decrypters, and security-by-obscurity is exactly what this project replaces.
Re-export such configs as v2 containers ( creators: `npvtunnel export`).

## Workspace layout

```
crates/npvt-core   profiles, URI import, .npvt v2 container, vault, engine glue
crates/npvt-cli    the npvtunnel binary
```

## Roadmap

| Phase | Scope |
|-------|-------|
| P1 (this) | Core + CLI: import/export/lock/share/list/connect, v2 crypto, vault, sing-box sidecar |
| P2 | Tray-first GUI, wintun TUN mode, kill switch, per-process routing, .npvt file association |
| P3 | Hardening: fuzzing, threat-model review, signed updater, AV pre-submissions |
| P4/P5 | Seller closed beta, performance budgets, GA channels (winget/MSI) |

## License

MIT — see [LICENSE](LICENSE).
