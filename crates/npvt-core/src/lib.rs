//! npvt-core: the engine-agnostic heart of NPVTunnel.
//!
//! Everything here is deliberately free of I/O side effects except the vault
//! file store, so the whole surface is unit-testable and fuzzable. Security
//! posture:
//!
//! - `#![forbid(unsafe_code)]` — no unsafe anywhere in the workspace.
//! - All profile fields are length-capped and control-character sanitized
//!   before they can reach an engine config (config-injection defense).
//! - The `.npvt` v2 container uses Argon2id + XChaCha20-Poly1305 + Ed25519
//!   (see [`npvt::v2`]); authentication failures never leak which check
//!   failed beyond "authentication failed".
//! - Secrets are wrapped in zeroizing types and never formatted by the
//!   redaction helpers in [`redact`].

#![forbid(unsafe_code)]

pub mod engine;
pub mod error;
pub mod npvt;
pub mod profile;
pub mod redact;
pub mod session;
pub mod uri;
pub mod vault;

pub use error::{Error, Result};
