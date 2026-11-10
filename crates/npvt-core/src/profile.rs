//! The tunnel profile model plus strict input validation.
//!
//! Every string that can end up inside an engine config passes through
//! [`Profile::validate`] rules: bounded length, no control characters,
//! non-empty. This is the first layer of the config-injection defense
//! (defect D3 in the concept blueprint): even a malicious config file cannot
//! smuggle newlines, control bytes or oversized blobs into engine configs.

use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

use crate::error::{Error, Result};

/// Hard cap for any single string field.
pub const MAX_FIELD_LEN: usize = 512;
/// Hard cap for display names.
pub const MAX_NAME_LEN: usize = 128;

/// Tunnel protocol families, mirroring the NPV Tunnel mobile feature set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// VLESS (Xray).
    Vless,
    /// VMess (Xray).
    Vmess,
    /// Trojan.
    Trojan,
    /// Shadowsocks (incl. 2022 ciphers).
    Shadowsocks,
    /// Hysteria2 (QUIC).
    Hysteria2,
    /// WireGuard.
    WireGuard,
    /// SSH (with udpgw-style UDP relay).
    Ssh,
    /// DNSTT (DNS tunnel).
    Dnstt,
    /// Plain SOCKS5 upstream.
    Socks,
    /// Plain HTTP(S) proxy upstream.
    Http,
}

impl Protocol {
    /// All protocols in display order.
    pub const ALL: [Protocol; 10] = [
        Protocol::Vless,
        Protocol::Vmess,
        Protocol::Trojan,
        Protocol::Shadowsocks,
        Protocol::Hysteria2,
        Protocol::WireGuard,
        Protocol::Ssh,
        Protocol::Dnstt,
        Protocol::Socks,
        Protocol::Http,
    ];

    /// Stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Vless => "vless",
            Protocol::Vmess => "vmess",
            Protocol::Trojan => "trojan",
            Protocol::Shadowsocks => "shadowsocks",
            Protocol::Hysteria2 => "hysteria2",
            Protocol::WireGuard => "wireguard",
            Protocol::Ssh => "ssh",
            Protocol::Dnstt => "dnstt",
            Protocol::Socks => "socks",
            Protocol::Http => "http",
        }
    }

    /// Parse from the stable lowercase name.
    pub fn parse(value: &str) -> Option<Self> {
        Protocol::ALL.into_iter().find(|p| p.as_str() == value)
    }

    /// Does this protocol require some credential to be present?
    pub fn requires_credential(self) -> bool {
        !matches!(self, Protocol::Socks | Protocol::Http)
    }

    /// Is this protocol already wired to an engine sidecar in this release?
    /// (WireGuard/SSH/DNSTT land with the engine matrix work, roadmap P1.5.)
    pub fn engine_supported(self) -> bool {
        matches!(
            self,
            Protocol::Vless
                | Protocol::Vmess
                | Protocol::Trojan
                | Protocol::Shadowsocks
                | Protocol::Hysteria2
                | Protocol::Socks
                | Protocol::Http
        )
    }
}

/// What authenticates the profile. Zeroized when dropped.
#[derive(Debug, Clone, PartialEq, Eq, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Credential {
    /// UUID-style credential (VLESS/VMess).
    Uuid(String),
    /// Password-style credential (Trojan, Hysteria2, SSH).
    Password(String),
    /// Shadowsocks method + password pair.
    SsMethod { method: String, password: String },
    /// Opaque token (SOCKS/HTTP auth, DNSTT key material).
    Token(String),
    /// No credential.
    None,
}

/// Transport/obfuscation wrapper on top of the protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// No wrapper.
    #[default]
    Plain,
    /// TLS (Reality profiles use this with SNI).
    Tls,
    /// WebSocket.
    Ws,
    /// gRPC.
    Grpc,
    /// HTTP/2.
    H2,
}

/// A single connectable tunnel configuration. This is the unit the whole
/// product works with: imported from URIs or `.npvt` containers, stored in
/// the vault, rendered to engine configs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable random hex id (16 bytes).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Protocol family.
    pub protocol: Protocol,
    /// Server host or IP (never a URL).
    pub server: String,
    /// Server port.
    pub port: u16,
    /// Credential material (zeroized on drop).
    pub credential: Credential,
    /// Transport wrapper.
    pub transport: Transport,
    /// TLS SNI / Reality server name.
    pub sni: Option<String>,
    /// WS/gRPC path or service name.
    pub path: Option<String>,
    /// Skip TLS verification. Security-relevant: doctor flags it, defaults off.
    pub allow_insecure: bool,
    /// UDP relay enabled.
    pub udp: bool,
    /// Optional group label for smart groups.
    pub group: Option<String>,
    /// Creation time, unix seconds.
    pub created_at: u64,
}

fn clean_field(value: &str, what: &str, max: usize) -> Result<()> {
    if value.is_empty() {
        return Err(Error::InvalidConfig(format!("{what} must not be empty")));
    }
    if value.len() > max {
        return Err(Error::InvalidConfig(format!(
            "{what} is too long (>{max} bytes)"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::InvalidConfig(format!(
            "{what} must not contain control characters"
        )));
    }
    if value.trim().is_empty() {
        return Err(Error::InvalidConfig(format!("{what} must not be blank")));
    }
    Ok(())
}

fn clean_optional(value: &Option<String>, what: &str) -> Result<()> {
    if let Some(v) = value {
        clean_field(v, what, MAX_FIELD_LEN)?;
    }
    Ok(())
}

/// Validate an already-trimmed incoming field against the standard cap.
pub(crate) fn validate_field(field: &str, what: &str) -> Result<()> {
    clean_field(field, what, MAX_FIELD_LEN)
}

/// Current unix time in seconds (0 if the clock is before the epoch).
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// New random 16-byte hex id for a profile.
pub fn new_profile_id() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn clean_credential(cred: &Credential) -> Result<()> {
    match cred {
        Credential::Uuid(v) | Credential::Password(v) | Credential::Token(v) => {
            clean_field(v, "credential", MAX_FIELD_LEN)
        }
        Credential::SsMethod { method, password } => {
            clean_field(method, "shadowsocks method", 128)?;
            clean_field(password, "shadowsocks password", MAX_FIELD_LEN)
        }
        Credential::None => Ok(()),
    }
}

impl Profile {
    /// Validate all fields. Called on every import, on vault load and before
    /// any engine config is rendered.
    pub fn validate(&self) -> Result<()> {
        clean_field(&self.id, "profile id", 64)?;
        if !self.id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::InvalidConfig("profile id must be hex".into()));
        }
        clean_field(&self.name, "profile name", MAX_NAME_LEN)?;
        clean_field(&self.server, "server", MAX_FIELD_LEN)?;
        if self.server.contains('/') || self.server.contains(' ') {
            return Err(Error::InvalidConfig(
                "server must be a bare host or IP, not a URL".into(),
            ));
        }
        clean_optional(&self.sni, "sni")?;
        clean_optional(&self.path, "path")?;
        clean_optional(&self.group, "group")?;
        clean_credential(&self.credential)?;
        if self.protocol.requires_credential() && matches!(self.credential, Credential::None) {
            return Err(Error::InvalidConfig(format!(
                "protocol {} requires a credential",
                self.protocol.as_str()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Profile {
        Profile {
            id: "0123abcd0123abcd".into(),
            name: "shop".into(),
            protocol: Protocol::Trojan,
            server: "example.com".into(),
            port: 443,
            credential: Credential::Password("hunter2".into()),
            transport: Transport::Tls,
            sni: Some("example.com".into()),
            path: None,
            allow_insecure: false,
            udp: true,
            group: Some("work".into()),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn accepts_well_formed_profile() {
        assert!(good().validate().is_ok());
    }

    #[test]
    fn rejects_control_characters_in_server() {
        let mut p = good();
        p.server = "example.com\n\"extra\":1".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn rejects_url_as_server() {
        let mut p = good();
        p.server = "https://evil.example/u".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn rejects_missing_credential_for_trojan() {
        let mut p = good();
        p.credential = Credential::None;
        assert!(p.validate().is_err());
    }

    #[test]
    fn allows_credential_free_socks() {
        let mut p = good();
        p.protocol = Protocol::Socks;
        p.credential = Credential::None;
        p.transport = Transport::Plain;
        p.sni = None;
        assert!(p.validate().is_ok());
    }
}
