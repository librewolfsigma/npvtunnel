//! The local vault: profiles, the creator identity, and known creator keys.
//!
//! Storage model (v0.1): a single JSON file at the OS data directory
//! (`%APPDATA%/npvtunnel/vault.json` on Windows, `~/.local/share/...` on
//! Linux). Hardened to the extent possible without OS-specific key stores:
//!
//! - file permissions tightened to owner-only on Unix (0600), ACL inheritance
//!   relied upon on Windows (documented in SECURITY.md; DPAPI integration is
//!   a roadmap item);
//! - writes are atomic (temp file + rename) so a crash never truncates state;
//! - credential material is zeroized when profiles are dropped;
//! - nothing is ever logged (the CLI redacts through [`crate::redact`]).
//!
//! The `import` surface is deliberately a library API: the CLI is a thin
//! wrapper so tests can exercise the same paths.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};
use crate::profile::{Profile, MAX_NAME_LEN};

/// Current vault schema version.
pub const VAULT_VERSION: u32 = 1;

/// Creator identity stored in the vault (seeds are sensitive).
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
pub struct Identity {
    /// Ed25519 seed (32 bytes) for signing exports.
    pub ed25519_seed: [u8; 32],
    /// X25519 secret (32 bytes) for opening key-locked shares.
    pub x25519_seed: [u8; 32],
}

/// Persisted vault state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultData {
    /// Schema version.
    pub version: u32,
    /// Stored profiles.
    pub profiles: Vec<Profile>,
    /// The user's creator identity, once generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    /// Known creator signing keys (base64url pubkey -> label), trust-on-first-use.
    #[serde(default)]
    pub known_creators: BTreeMap<String, String>,
}

impl Default for VaultData {
    fn default() -> Self {
        VaultData {
            version: VAULT_VERSION,
            profiles: Vec::new(),
            identity: None,
            known_creators: BTreeMap::new(),
        }
    }
}

/// File-backed vault.
pub struct Vault {
    path: PathBuf,
}

impl Vault {
    /// The vault file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Vault at an explicit path (used by tests and `--vault`).
    pub fn at(path: PathBuf) -> Self {
        Vault { path }
    }

    /// Default vault path for the current user.
    pub fn default_path() -> Result<PathBuf> {
        let base = dirs::data_dir().ok_or_else(|| Error::Vault("no user data directory".into()))?;
        Ok(base.join("npvtunnel").join("vault.json"))
    }

    /// Vault at the default path.
    pub fn open() -> Result<Self> {
        Ok(Vault::at(Vault::default_path()?))
    }

    /// Load the vault, creating an empty one if the file does not exist.
    pub fn load(&self) -> Result<VaultData> {
        if !self.path.exists() {
            return Ok(VaultData::default());
        }
        let bytes = std::fs::read(&self.path)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::Vault("vault file is implausibly large".into()));
        }
        let data: VaultData = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Vault(format!("corrupt vault: {e}")))?;
        if data.version != VAULT_VERSION {
            return Err(Error::Vault(format!(
                "unsupported vault version {} (expected {VAULT_VERSION})",
                data.version
            )));
        }
        for profile in &data.profiles {
            profile
                .validate()
                .map_err(|e| Error::Vault(format!("stored profile failed validation: {e}")))?;
        }
        Ok(data)
    }

    /// Atomically persist the vault.
    pub fn save(&self, data: &VaultData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(data)?;
        let tmp = self.path.with_extension("json.tmp");
        {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&tmp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
            }
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&self.path) {
                if meta.permissions().mode() & 0o177 != 0 {
                    return Err(Error::Vault("vault file permissions are too open".into()));
                }
            }
        }
        Ok(())
    }
}

/// Look a profile up by exact id or unique case-insensitive name.
pub fn find_profile<'a>(data: &'a VaultData, key: &str) -> Result<&'a Profile> {
    if let Some(p) = data.profiles.iter().find(|p| p.id == key) {
        return Ok(p);
    }
    let matches: Vec<&Profile> = data
        .profiles
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(key))
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(Error::InvalidConfig(format!("no profile named '{key}'"))),
        _ => Err(Error::InvalidConfig(format!(
            "'{key}' is ambiguous, {} profiles share that name; use an id",
            matches.len()
        ))),
    }
}

/// Insert or replace a profile (matched by id). Returns true when replaced.
pub fn upsert_profile(data: &mut VaultData, profile: Profile) -> Result<bool> {
    profile.validate()?;
    if let Some(existing) = data.profiles.iter_mut().find(|p| p.id == profile.id) {
        let mut cleaned = profile;
        // Keep the original creation time of the replaced profile.
        cleaned.created_at = existing.created_at;
        *existing = cleaned;
        return Ok(true);
    }
    if data.profiles.len() >= 10_000 {
        return Err(Error::Vault("vault profile limit reached".into()));
    }
    data.profiles.push(profile);
    Ok(false)
}

/// Remove a profile by id or unique name; returns the removed profile.
pub fn remove_profile(data: &mut VaultData, key: &str) -> Result<Profile> {
    let idx = data
        .profiles
        .iter()
        .position(|p| p.id == key)
        .or_else(|| {
            let mut hit: Option<usize> = None;
            let mut count = 0;
            for (i, p) in data.profiles.iter().enumerate() {
                if p.name.eq_ignore_ascii_case(key) {
                    count += 1;
                    hit = Some(i);
                }
            }
            if count == 1 {
                hit
            } else {
                None
            }
        })
        .ok_or_else(|| Error::InvalidConfig(format!("no profile named '{key}'")))?;
    Ok(data.profiles.remove(idx))
}

/// Sanitize a user-supplied display name for storage.
pub fn clean_name_input(name: &str) -> Result<String> {
    let v = name.trim();
    if v.is_empty() {
        return Err(Error::InvalidConfig("name must not be empty".into()));
    }
    if v.chars().any(char::is_control) {
        return Err(Error::InvalidConfig(
            "name must not contain control characters".into(),
        ));
    }
    Ok(v.chars().take(MAX_NAME_LEN).collect())
}

/// Generate a fresh creator identity (both keypairs).
pub fn generate_identity() -> Identity {
    use crate::npvt::v2::{generate_recipient_keypair, generate_signing_key};
    let signing = generate_signing_key();
    let (x25519_seed, _) = generate_recipient_keypair();
    Identity {
        ed25519_seed: signing.to_bytes(),
        x25519_seed,
    }
}

/// Zeroize an identity buffer that is no longer needed.
pub fn zeroize_bytes(buf: &mut [u8]) {
    buf.zeroize();
}

/// Sanitize a filename-derived label (for imported files without a name).
pub fn label_from_filename(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported".to_string())
        .chars()
        .take(MAX_NAME_LEN)
        .collect()
}

/// Random 8-hex-digit suffix for deduplicating imported names.
pub fn short_suffix() -> String {
    let mut b = [0u8; 4];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::new_profile_id;
    use crate::profile::{Credential, Protocol, Transport};

    fn profile(name: &str) -> Profile {
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
            group: None,
            created_at: 1_700_000_000,
        }
    }

    fn tmp_vault(tag: &str) -> Vault {
        let mut path = std::env::temp_dir();
        path.push(format!("npvt-test-{tag}-{}-vault.json", std::process::id()));
        Vault::at(path)
    }

    #[test]
    fn save_load_roundtrip_and_upsert() {
        let vault = tmp_vault("roundtrip");
        let mut data = VaultData::default();
        upsert_profile(&mut data, profile("shop")).expect("upsert");
        let p2 = profile("shop");
        let replaced = upsert_profile(&mut data, p2).expect("upsert2");
        assert!(!replaced, "fresh random id must insert, not replace");
        vault.save(&data).expect("save");
        let loaded = vault.load().expect("load");
        assert_eq!(loaded.profiles.len(), 2);
        assert!(vault.path.exists());
        let _ = std::fs::remove_file(vault.path);
    }

    #[test]
    fn find_by_id_and_unique_name() {
        let mut data = VaultData::default();
        let p = profile("alpha");
        let id = p.id.clone();
        data.profiles.push(p);
        data.profiles.push(profile("beta"));
        assert_eq!(find_profile(&data, &id).expect("by id").name, "alpha");
        assert_eq!(find_profile(&data, "ALPHA").expect("by name").name, "alpha");
        assert!(find_profile(&data, "gamma").is_err());
    }

    #[test]
    fn ambiguous_name_is_rejected() {
        let mut data = VaultData::default();
        data.profiles.push(profile("dup"));
        data.profiles.push(profile("dup"));
        assert!(find_profile(&data, "dup").is_err());
    }

    #[test]
    fn remove_by_unique_name() {
        let mut data = VaultData::default();
        data.profiles.push(profile("solo"));
        let removed = remove_profile(&mut data, "solo").expect("remove");
        assert_eq!(removed.name, "solo");
        assert!(data.profiles.is_empty());
    }

    #[test]
    fn identity_zeroizes_on_drop() {
        let mut id = generate_identity();
        let seed_copy = id.ed25519_seed;
        id.zeroize();
        assert_ne!(id.ed25519_seed, seed_copy);
    }
}
