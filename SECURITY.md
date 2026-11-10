# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | yes       |

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository (Security tab
-> Report a vulnerability). Please do not open public issues for security
reports. Reports get a response within 7 days and a fix target within 30 days
for critical findings.

## Hardening commitments

These are enforced in code and CI; regressions are treated as security bugs.

1. `#![forbid(unsafe_code)]` in every crate.
2. Strict input validation for every field that reaches an engine config:
   bounded length, no control characters, no URL-as-server confusion.
3. The `.npvt` v2 container fails closed: wrong passphrase, tampered payload
   and invalid creator signatures are indistinguishable authentication
   failures. Signature verification uses Ed25519 `verify_strict`.
4. Credentials and passphrases are zeroized when dropped, never written to
   logs, and never accepted on the command line (hidden prompt or stdin).
5. The engine sidecar is spawned without a shell; the local inbound binds
   `127.0.0.1` only; engine configs are written owner-only on Unix.
6. The legacy v1 lock is not reimplemented. Locked v1 files are rejected with
   an honest error (the v1 scheme is broken client-side obfuscation).
7. CI runs clippy with `-D warnings`, cargo-deny (RustSec advisories +
   license allow-list), cargo-machete, CodeQL, a secrets scanner (via
   master-ci) and Dependabot; all third-party GitHub Actions are pinned by
   commit SHA.
8. Expired containers are refused at import; expiry lives inside the signed
   policy block, not in mutable client state.

## Known limitations (0.1.x)

- The vault is a permission-tightened JSON file. OS key-store integration
  (DPAPI on Windows) is planned; until then, file permissions and process
  isolation are the protection boundary.
- TUN mode, the kill switch and per-process routing land with the P2
  milestones; the current engine surface is a localhost SOCKS5 proxy.
- Locked legacy v1 containers are not decoded by design.
