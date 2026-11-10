//! Legacy `.npvt` / `.npvs` v1 container support: detection and best-effort
//! import only.
//!
//! The v1 "lock" is client-side obfuscation, not cryptography: public
//! decrypters exist (see the concept blueprint, Section 2). NPVTunnel
//! deliberately does **not** reimplement that obfuscation. Instead:
//!
//! - unlocked v1 files that carry the mobile app's cross-platform JSON
//!   configuration format (introduced app-side in 2022) are imported;
//! - anything else is rejected with a clear, honest error and a pointer to
//!   the v2 format.
//!
//! This module is clean-room: it was written from public behavior
//! documentation only, never from AGPL source code.

use serde_json::Value;

use crate::error::{Error, Result};
use crate::profile::Profile;
use crate::uri;

/// Magic prefix of `.npvt` v1 files.
pub const V1_MAGIC: &[u8; 4] = b"NPVT";
/// Magic prefix of `.npvs` shared-file bundles (same family).
pub const V1_MAGIC_NPVS: &[u8; 4] = b"NPVS";

/// What we can say about a v1 file without decoding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V1Probe {
    /// Magic matches the v1 family.
    pub is_v1: bool,
    /// The body after the magic looks like the plaintext JSON config format.
    pub looks_plaintext_json: bool,
}

/// Probe a byte slice for the v1 family signature.
pub fn probe(bytes: &[u8]) -> V1Probe {
    let is_v1 = bytes.starts_with(V1_MAGIC) || bytes.starts_with(V1_MAGIC_NPVS);
    let body = if is_v1 { &bytes[4..] } else { bytes };
    let first_nonspace = body.iter().find(|b| !b.is_ascii_whitespace());
    let looks_plaintext_json = first_nonspace == Some(&b'{');
    V1Probe {
        is_v1,
        looks_plaintext_json,
    }
}

/// Best-effort v1 import: only plaintext JSON bodies are supported.
///
/// The mobile app's cross-platform configuration format is a JSON object
/// with VMess-shaped fields (`add`, `port`, `id`, `ps`, `net`, `type`, ...).
/// We reuse the tolerant VMess-JSON mapper from the URI module.
pub fn import_best_effort(bytes: &[u8]) -> Result<Profile> {
    let probe = probe(bytes);
    if !probe.is_v1 {
        return Err(Error::InvalidConfig(
            "not a .npvt/.npvs v1 container (magic mismatch)".into(),
        ));
    }
    if !probe.looks_plaintext_json {
        return Err(Error::V1Unsupported(
            "the file is locked with the legacy v1 obfuscation; locked v1 files are not decoded by NPVTunnel (the v1 lock is broken obfuscation) - re-export as a v2 container or ask the creator for an npvt:// link"
                .into(),
        ));
    }
    let body = &bytes[4..];
    let text = std::str::from_utf8(body)
        .map_err(|_| Error::V1Unsupported("v1 body is not valid UTF-8".into()))?;
    let value: Value = serde_json::from_str(text.trim())
        .map_err(|e| Error::V1Unsupported(format!("v1 body is not valid JSON: {e}")))?;
    uri::map_vmess_json(&value, "imported v1 config")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Protocol;

    #[test]
    fn detects_plain_json_v1() {
        let data = b"NPVT{\"add\":\"a.com\",\"port\":\"443\",\"id\":\"u1\",\"ps\":\"shop\"}";
        let p = probe(data);
        assert!(p.is_v1);
        assert!(p.looks_plaintext_json);
        let profile = import_best_effort(data).expect("import");
        assert_eq!(profile.protocol, Protocol::Vmess);
        assert_eq!(profile.server, "a.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.name, "shop");
    }

    #[test]
    fn rejects_locked_v1_honestly() {
        let data = b"NPVT\x01\x02\x03\x04not json at all";
        let p = probe(data);
        assert!(p.is_v1);
        assert!(!p.looks_plaintext_json);
        let err = import_best_effort(data).unwrap_err();
        assert!(matches!(err, Error::V1Unsupported(_)));
    }

    #[test]
    fn rejects_non_v1() {
        assert!(import_best_effort(b"NOPE").is_err());
    }

    #[test]
    fn probe_npvs_magic() {
        let p = probe(b"NPVS{}");
        assert!(p.is_v1);
    }

    #[test]
    fn transport_fields_flow_through() {
        use crate::profile::{Credential, Transport};
        let data = b"NPVT{\"add\":\"a.com\",\"port\":\"443\",\"id\":\"u1\",\"ps\":\"shop\",\
                    \"net\":\"ws\",\"path\":\"/ws\",\"tls\":\"tls\",\"sni\":\"s.com\"}";
        let profile = import_best_effort(data).expect("import");
        assert_eq!(profile.transport, Transport::Tls);
        assert_eq!(profile.path.as_deref(), Some("/ws"));
        let _ = Credential::None;
    }
}
