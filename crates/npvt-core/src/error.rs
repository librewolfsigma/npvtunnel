//! Central error type. Error messages are user-facing: they never embed
//! credentials or raw secrets, only field names and reasons.

use crate::profile::Protocol;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A configuration failed validation (bad field, missing credential, ...).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    /// The input file is a legacy `.npvt`/`.npvs` v1 container that we cannot
    /// decode (locked v1 support is a roadmap item; the v1 lock is obfuscation
    /// and is deliberately not reimplemented).
    #[error("legacy v1 container cannot be imported: {0}")]
    V1Unsupported(String),
    /// Authentication failed or crypto parameters were wrong. Deliberately
    /// vague: callers cannot distinguish "wrong passphrase" from "tampered
    /// container".
    #[error("authentication failed: wrong passphrase/key or tampered container")]
    CryptoAuth,
    /// Cryptographic parameter or encoding problem (not authentication).
    #[error("cryptographic failure: {0}")]
    Crypto(String),
    /// Vault (local store) problem.
    #[error("vault error: {0}")]
    Vault(String),
    /// Engine (sing-box sidecar) problem.
    #[error("engine error: {0}")]
    Engine(String),
    /// I/O problem.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization problem.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Anything else.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Machine-readable class used for CLI exit codes (stable contract).
    pub fn class(&self) -> ErrorClass {
        match self {
            Error::InvalidConfig(_) | Error::V1Unsupported(_) | Error::Serde(_) => {
                ErrorClass::Config
            }
            Error::CryptoAuth | Error::Crypto(_) => ErrorClass::Crypto,
            Error::Vault(_) => ErrorClass::Permission,
            Error::Engine(_) => ErrorClass::Network,
            Error::Io(_) => ErrorClass::Io,
            Error::Other(_) => ErrorClass::Other,
        }
    }

    /// Whether this error means the protocol family is valid but not yet
    /// wired to an engine sidecar.
    pub fn is_protocol_unsupported(&self, protocol: Protocol) -> bool {
        matches!(self, Error::Engine(_)) && !protocol.engine_supported()
    }
}

/// Stable classification for CLI exit codes:
/// 1 usage, 2 config, 3 network/engine, 4 permission, 5 crypto, 6 io, 7 other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Bad configuration or bad input file.
    Config,
    /// Network or engine failure.
    Network,
    /// Permission or vault-access failure.
    Permission,
    /// Cryptographic failure.
    Crypto,
    /// Filesystem failure.
    Io,
    /// Anything else.
    Other,
}
