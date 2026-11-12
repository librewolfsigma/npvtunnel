//! Vault operations behind the UI: importing (clipboard, file, QR,
//! subscription, manual) and exporting (.npvt v2 containers). The GUI is a
//! thin wrapper over npvt-core, exactly like the CLI.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64U;
use base64::Engine as _;
use npvt_core::npvt::v1;
use npvt_core::npvt::v2::{self, Lock, Policy, Unlock};
use npvt_core::profile::{new_profile_id, Profile};
use npvt_core::uri::parse_import;
use npvt_core::vault::{self, VaultData};

/// Outcome of an import batch.
#[derive(Debug, Default, Clone)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub failed: Vec<String>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        if self.failed.is_empty() {
            format!(
                "Imported {} config{}",
                self.imported.len(),
                if self.imported.len() == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "Imported {}, failed {} ({})",
                self.imported.len(),
                self.failed.len(),
                self.failed.first().map(String::as_str).unwrap_or("")
            )
        }
    }
}

/// Import a single link/plain input. Returns the stored profile name.
pub fn import_one(
    data: &mut VaultData,
    input: &str,
    group: Option<&str>,
) -> Result<String, String> {
    let source = parse_import(input).map_err(|e| e.to_string())?;
    match source {
        npvt_core::uri::ImportSource::Profile(mut profile) => {
            if let Some(g) = group {
                profile.group = Some(g.to_owned());
            }
            profile.id = new_profile_id();
            let name = profile.name.clone();
            vault::upsert_profile(data, profile).map_err(|e| e.to_string())?;
            Ok(name)
        }
        npvt_core::uri::ImportSource::Container(bytes) => {
            let decoded = decode_container(&bytes, data)?;
            let mut profile = decoded.profile;
            profile.id = new_profile_id();
            if group.is_some() {
                profile.group = group.map(str::to_owned);
            } else if profile.group.is_none() {
                if let Some(hint) = decoded.policy.group_hint.clone() {
                    profile.group = Some(hint);
                }
            }
            let name = profile.name.clone();
            vault::upsert_profile(data, profile).map_err(|e| e.to_string())?;
            Ok(name)
        }
    }
}

/// Decode a v2 container trying the vault identity, open mode, or a caller
/// passphrase.
pub fn decode_container(bytes: &[u8], data: &VaultData) -> Result<v2::Decoded, String> {
    let mode = v2::peek_lock(bytes).map_err(|e| e.to_string())?;
    match mode {
        "open" => v2::decode(bytes, Unlock::Any).map_err(|e| e.to_string()),
        "keys" => {
            if let Some(identity) = &data.identity {
                let secrets = [identity.x25519_seed];
                v2::decode(bytes, Unlock::Keys(&secrets)).map_err(|e| e.to_string())
            } else {
                Err("this config is key-locked and this vault has no identity yet".to_owned())
            }
        }
        _ => Err(format!("this container needs a passphrase ({mode})")),
    }
}

/// Decode with an explicit passphrase (Passphrase-locked containers).
pub fn decode_container_passphrase(bytes: &[u8], passphrase: &str) -> Result<v2::Decoded, String> {
    v2::decode(bytes, Unlock::Passphrase(passphrase)).map_err(|e| e.to_string())
}

/// Import every link found in a text blob (clipboard / subscription body).
pub fn import_links(data: &mut VaultData, body: &str, group: Option<&str>) -> ImportReport {
    let mut report = ImportReport::default();
    for link in crate::net::extract_links(body) {
        match import_one(data, &link, group) {
            Ok(name) => report.imported.push(name),
            Err(e) => report.failed.push(e),
        }
    }
    report
}

/// Import a raw config file (.npvt v2 / .npvs v1 / plaintext JSON / links).
pub fn import_file_bytes(
    data: &mut VaultData,
    bytes: &[u8],
    fallback_name: &str,
) -> Result<String, FileImportError> {
    if bytes.starts_with(v2::MAGIC) {
        let decoded = decode_container(bytes, data).map_err(FileImportError::NeedsUnlock)?;
        let mut profile = decoded.profile;
        profile.id = new_profile_id();
        if profile.group.is_none() {
            if let Some(hint) = decoded.policy.group_hint.clone() {
                profile.group = Some(hint);
            }
        }
        let name = profile.name.clone();
        vault::upsert_profile(data, profile).map_err(|e| FileImportError::Other(e.to_string()))?;
        return Ok(name);
    }
    if bytes.starts_with(v1::V1_MAGIC) || bytes.starts_with(v1::V1_MAGIC_NPVS) {
        let profile =
            v1::import_best_effort(bytes).map_err(|e| FileImportError::Other(e.to_string()))?;
        let mut profile = profile;
        profile.id = new_profile_id();
        let name = profile.name.clone();
        vault::upsert_profile(data, profile).map_err(|e| FileImportError::Other(e.to_string()))?;
        return Ok(name);
    }
    // Maybe plaintext JSON (VMess-shaped) or a list of links.
    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('{') {
        let parsed: serde_json::Value = serde_json::from_str(text.trim())
            .map_err(|e| FileImportError::Other(format!("bad JSON: {e}")))?;
        let profile = npvt_core::uri::map_vmess_json(&parsed, fallback_name)
            .map_err(|e| FileImportError::Other(e.to_string()))?;
        let mut profile = profile;
        profile.id = new_profile_id();
        let name = profile.name.clone();
        vault::upsert_profile(data, profile).map_err(|e| FileImportError::Other(e.to_string()))?;
        return Ok(name);
    }
    let report = import_links(data, &text, None);
    if report.imported.is_empty() {
        return Err(FileImportError::Other(
            report
                .failed
                .first()
                .cloned()
                .unwrap_or_else(|| "no links found in file".to_owned()),
        ));
    }
    Ok(report.imported.join(", "))
}

#[derive(Debug)]
pub enum FileImportError {
    /// A v2 container that requires a passphrase; the UI should prompt.
    NeedsUnlock(String),
    Other(String),
}

/// Export one profile as a .npvt v2 container.
pub struct ExportSpec {
    pub profile: Profile,
    pub lock: ExportLock,
    /// Expiry: None / seconds from now.
    pub expires_in: Option<u64>,
    pub filename: String,
    pub recipients: Vec<[u8; 32]>,
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLock {
    Open,
    Keys,
    Passphrase,
}

pub fn export_container(data: &VaultData, spec: &ExportSpec) -> Result<Vec<u8>, String> {
    let signer = data
        .identity
        .as_ref()
        .map(|identity| ed25519_dalek::SigningKey::from_bytes(&identity.ed25519_seed));
    let policy = Policy {
        expires_at: spec
            .expires_in
            .map(|secs| npvt_core::profile::unix_now() + secs),
        metered_only: false,
        group_hint: spec.profile.group.clone(),
    };
    let lock = match spec.lock {
        ExportLock::Open => Lock::Open,
        ExportLock::Keys => {
            if spec.recipients.is_empty() {
                return Err("add at least one recipient public key".to_owned());
            }
            Lock::Keys(spec.recipients.clone())
        }
        ExportLock::Passphrase => {
            let Some(pass) = spec.passphrase.as_deref() else {
                return Err("passphrase is empty".to_owned());
            };
            if pass.len() < 8 {
                return Err("passphrase must be at least 8 characters".to_owned());
            }
            Lock::Passphrase(pass.to_owned())
        }
    };
    v2::encode(&spec.profile, &policy, &lock, signer.as_ref()).map_err(|e| e.to_string())
}

/// Parse recipient public keys (one base64url X25519 key per line).
pub fn parse_recipient_keys(text: &str) -> Result<Vec<[u8; 32]>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bytes = B64U
            .decode(line)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(line))
            .map_err(|_| format!("invalid recipient key: {line}"))?;
        if bytes.len() != 32 {
            return Err(format!("recipient key must be 32 bytes: {line}"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        out.push(key);
    }
    Ok(out)
}

/// The vault's X25519 public key, base64url (for the "My public key" page).
pub fn my_public_key(data: &VaultData) -> Option<String> {
    let identity = data.identity.as_ref()?;
    let secret = x25519_dalek::StaticSecret::from(identity.x25519_seed);
    let public = x25519_dalek::PublicKey::from(&secret);
    Some(B64U.encode(public.as_bytes()))
}

/// The vault's Ed25519 creator key, base64url.
pub fn my_creator_key(data: &VaultData) -> Option<String> {
    let identity = data.identity.as_ref()?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&identity.ed25519_seed);
    Some(B64U.encode(signing.verifying_key().to_bytes()))
}

/// Ensure the vault has an identity, creating one when missing.
pub fn ensure_identity(data: &mut VaultData) {
    if data.identity.is_none() {
        data.identity = Some(vault::generate_identity());
    }
}

/// Render a container as an `npvt://` share link.
pub fn container_to_link(bytes: &[u8]) -> String {
    format!("npvt://{}", B64U.encode(bytes))
}

/// Group label for a profile (None -> the implicit "Saved" group).
pub fn group_label(profile: &Profile) -> String {
    profile.group.clone().unwrap_or_else(|| "Saved".to_owned())
}

/// Group profiles by their group label, preserving vault order.
pub fn group_profiles(data: &VaultData) -> Vec<(String, Vec<Profile>)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::BTreeMap<String, Vec<Profile>> = Default::default();
    for p in &data.profiles {
        let label = group_label(p);
        if !map.contains_key(&label) {
            order.push(label.clone());
        }
        map.entry(label).or_default().push(p.clone());
    }
    order
        .into_iter()
        .map(|label| {
            let members = map.remove(&label).unwrap_or_default();
            (label, members)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use npvt_core::profile::{Credential, Protocol, Transport};

    fn profile(name: &str, group: Option<&str>) -> Profile {
        Profile {
            id: new_profile_id(),
            name: name.into(),
            protocol: Protocol::Vless,
            server: "example.com".into(),
            port: 443,
            credential: Credential::Uuid("11111111-2222-3333-4444-555555555555".into()),
            transport: Transport::Tls,
            sni: None,
            path: None,
            allow_insecure: false,
            udp: true,
            group: group.map(str::to_owned),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn groups_preserve_order() {
        let mut data = VaultData::default();
        vault::upsert_profile(&mut data, profile("a", Some("Smart Route"))).unwrap();
        vault::upsert_profile(&mut data, profile("b", None)).unwrap();
        vault::upsert_profile(&mut data, profile("c", Some("Smart Route"))).unwrap();
        let groups = group_profiles(&data);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "Smart Route");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "Saved");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn recipient_keys_parse() {
        let (_, pk) = npvt_core::npvt::v2::generate_recipient_keypair();
        let text = B64U.encode(pk);
        let parsed = parse_recipient_keys(&text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], pk);
        assert!(parse_recipient_keys("nope").is_err());
    }

    #[test]
    fn container_link_roundtrip_via_import() {
        let mut data = VaultData::default();
        ensure_identity(&mut data);
        let p = profile("shop", None);
        let bytes = export_container(
            &data,
            &ExportSpec {
                profile: p.clone(),
                lock: ExportLock::Open,
                expires_in: None,
                filename: "shop.npvt".into(),
                recipients: vec![],
                passphrase: None,
            },
        )
        .unwrap();
        let link = container_to_link(&bytes);
        let mut target = VaultData::default();
        ensure_identity(&mut target);
        let name = import_one(&mut target, &link, None).unwrap();
        assert_eq!(name, "shop");
        assert_eq!(target.profiles[0].server, p.server);
    }

    #[test]
    fn passphrase_container_roundtrip() {
        let mut data = VaultData::default();
        ensure_identity(&mut data);
        let p = profile("locked", None);
        let bytes = export_container(
            &data,
            &ExportSpec {
                profile: p,
                lock: ExportLock::Passphrase,
                expires_in: Some(86_400),
                filename: "locked.npvt".into(),
                recipients: vec![],
                passphrase: Some("open sesame 42".into()),
            },
        )
        .unwrap();
        assert_eq!(v2::peek_lock(&bytes).unwrap(), "passphrase");
        let decoded = decode_container_passphrase(&bytes, "open sesame 42").unwrap();
        assert_eq!(decoded.profile.name, "locked");
        assert!(decoded.policy.expires_at.is_some());
        assert!(decoded.verification.valid);
    }

    #[test]
    fn identity_public_key_stable() {
        let mut data = VaultData::default();
        ensure_identity(&mut data);
        let first = my_public_key(&data).unwrap();
        let second = my_public_key(&data).unwrap();
        assert_eq!(first, second);
        assert!(my_creator_key(&data).is_some());
    }
}
