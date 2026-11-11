//! Vault access and the import/export pipelines, adapted from the CLI flows
//! so the GUI shows the same security cards and enforces the same rules.
//!
//! Decoding containers is CPU-heavy (Argon2id), so the GUI calls these
//! functions from background threads (see [`crate::bridge`]).

use npvt_core::npvt::v1;
use npvt_core::npvt::v2::{self, Lock, Policy, Unlock};
use npvt_core::profile::{new_profile_id, unix_now, Profile};
use npvt_core::uri::{parse_import, ImportSource};
use npvt_core::vault::{
    clean_name_input, find_profile, generate_identity, label_from_filename, short_suffix,
    upsert_profile, Identity, Vault, VaultData,
};
use npvt_core::Error;

/// Sentinel returned when a container needs a passphrase the caller did not
/// supply; the GUI shows the unlock dialog in response.
pub const PASSPHRASE_REQUIRED: &str = "passphrase-required";

/// Maximum import file size (mirrors the CLI limit).
pub const MAX_IMPORT_FILE: usize = v2::MAX_CONTAINER_LEN;

const B64URL: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// What an import attempt decoded, ready for the security-card dialog.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub profile: Profile,
    pub policy: Policy,
    pub verification: v2::Verification,
    pub lock_kind: &'static str,
}

/// Open the default vault and load its data.
pub fn load() -> Result<(Vault, VaultData), String> {
    let vault = Vault::open().map_err(|e| e.to_string())?;
    let data = vault.load().map_err(|e| e.to_string())?;
    Ok((vault, data))
}

/// Ensure a creator identity exists; returns the (possibly new) identity.
pub fn ensure_identity(vault: &Vault, data: &mut VaultData) -> Result<Identity, String> {
    if data.identity.is_none() {
        data.identity = Some(generate_identity());
        vault.save(data).map_err(|e| e.to_string())?;
    }
    Ok(data.identity.clone().expect("identity just set"))
}

/// Recipient X25519 public key, base64url (this is what creators lock files
/// to).
pub fn recipient_pubkey(identity: &Identity) -> String {
    let secret = x25519_dalek::StaticSecret::from(identity.x25519_seed);
    let public = x25519_dalek::PublicKey::from(&secret);
    b64url(public.as_bytes())
}

/// Creator Ed25519 signing key for export signing.
pub fn signing_key_of(identity: &Identity) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&identity.ed25519_seed)
}

/// Peek the lock kind of a v2 container ("open" | "passphrase" | "keys").
pub fn peek_kind(bytes: &[u8]) -> Result<&'static str, String> {
    v2::peek_lock(bytes).map_err(|e| e.to_string())
}

/// Decode an incoming source:
/// - `Some(bytes)`: an `.npvt` file (v2 container or legacy v1);
/// - `None` with `Some(text)`: a share URI / `npvt://` link / legacy JSON.
///
/// Passphrase-locked containers without `passphrase` return the
/// [`PASSPHRASE_REQUIRED`] sentinel (as a String error).
pub fn decode_source(
    bytes: Option<&[u8]>,
    text: Option<&str>,
    identity: Option<&Identity>,
    passphrase: Option<String>,
) -> std::result::Result<Incoming, String> {
    decode_source_result(bytes, text, identity, passphrase).map_err(|e| match &e {
        Error::InvalidConfig(msg) if msg.contains("passphrase required") => {
            PASSPHRASE_REQUIRED.to_string()
        }
        _ => e.to_string(),
    })
}

fn decode_source_result(
    bytes: Option<&[u8]>,
    text: Option<&str>,
    identity: Option<&Identity>,
    passphrase: Option<String>,
) -> Result<Incoming, Error> {
    if let Some(bytes) = bytes {
        if bytes.len() > MAX_IMPORT_FILE {
            return Err(Error::InvalidConfig("file is too large".into()));
        }
        if bytes.starts_with(v2::MAGIC) {
            return decode_container(bytes, identity, passphrase);
        }
        if v1::probe(bytes).is_v1 {
            let profile = v1::import_best_effort(bytes)?;
            return Ok(Incoming {
                profile,
                policy: Policy::default(),
                verification: v2::Verification {
                    signed: false,
                    creator: None,
                    valid: false,
                },
                lock_kind: "legacy-v1",
            });
        }
        return Err(Error::InvalidConfig(
            "unrecognized file format (expected an npvt2 container or a legacy v1 file)".into(),
        ));
    }
    let text = text.ok_or_else(|| Error::InvalidConfig("empty import source".into()))?;
    match parse_import(text)? {
        ImportSource::Container(bytes) => decode_container(&bytes, identity, passphrase),
        ImportSource::Profile(profile) => Ok(Incoming {
            profile,
            policy: Policy::default(),
            verification: v2::Verification {
                signed: false,
                creator: None,
                valid: false,
            },
            lock_kind: "link",
        }),
    }
}

fn decode_container(
    bytes: &[u8],
    identity: Option<&Identity>,
    passphrase: Option<String>,
) -> Result<Incoming, Error> {
    let lock_kind = v2::peek_lock(bytes)?;
    let (decoded, verification) = match lock_kind {
        "open" => {
            let d = v2::decode(bytes, Unlock::Any)?;
            let v = d.verification.clone();
            (d, v)
        }
        "passphrase" => {
            let passphrase = passphrase.ok_or_else(|| {
                Error::InvalidConfig("passphrase required to open this config".into())
            })?;
            let d = v2::decode(bytes, Unlock::Passphrase(&passphrase))?;
            let v = d.verification.clone();
            (d, v)
        }
        "keys" => {
            let seed = identity
                .as_ref()
                .ok_or_else(|| {
                    Error::Vault("container is key-locked but no local identity exists yet".into())
                })?
                .x25519_seed;
            let d = v2::decode(bytes, Unlock::Keys(&[seed]))?;
            let v = d.verification.clone();
            (d, v)
        }
        other => {
            return Err(Error::InvalidConfig(format!(
                "container uses an unsupported lock kind '{other}'"
            )))
        }
    };
    if decoded.policy.is_expired(unix_now()) {
        return Err(Error::InvalidConfig(
            "container policy is expired; expired configs are refused by default".into(),
        ));
    }
    Ok(Incoming {
        profile: decoded.profile,
        policy: decoded.policy,
        verification,
        lock_kind,
    })
}

/// Pick a unique display name for an imported profile.
pub fn unique_name(data: &VaultData, wanted: &str) -> String {
    let clash = |n: &str| data.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(n));
    if !clash(wanted) {
        return wanted.to_string();
    }
    loop {
        let candidate = format!("{wanted} - {}", short_suffix());
        if !clash(&candidate) {
            return candidate;
        }
    }
}

/// Add a decoded incoming profile to the vault (with the security-card
/// decisions: optional name/group override). Returns `(name, replaced)`.
pub fn apply_import(
    vault: &Vault,
    data: &mut VaultData,
    incoming: &Incoming,
    name_override: Option<String>,
    group_override: Option<String>,
) -> Result<(String, bool), String> {
    let mut profile = incoming.profile.clone();
    profile.name = match &name_override {
        Some(n) => clean_name_input(n).map_err(|e| e.to_string())?,
        None => unique_name(data, &profile.name),
    };
    profile.group = match group_override {
        Some(g) => Some(clean_name_input(&g).map_err(|e| e.to_string())?),
        None => incoming
            .policy
            .group_hint
            .as_deref()
            .map(clean_name_input)
            .transpose()
            .map_err(|e| e.to_string())?,
    };
    if let Some(creator) = &incoming.verification.creator {
        data.known_creators
            .entry(creator.clone())
            .or_insert_with(|| format!("creator {}", &creator[..8.min(creator.len())]));
    }
    let profile_id = profile.id.clone();
    let replaced = upsert_profile(data, profile).map_err(|e| e.to_string())?;
    vault.save(data).map_err(|e| e.to_string())?;
    let name = find_profile(data, &profile_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| profile_id);
    Ok((name, replaced))
}

/// A share link for one profile, ready to be written to disk.
pub struct ExportRequest {
    pub profile_id: String,
    pub out_path: std::path::PathBuf,
    pub open_lock: bool,
    pub passphrase: Option<String>,
    pub expires_at: Option<u64>,
    pub metered_only: bool,
    pub group_hint: Option<String>,
    pub sign: bool,
}

/// Encode and write an `.npvt` container; returns the byte count.
pub fn export_file(_vault: &Vault, data: &VaultData, req: &ExportRequest) -> Result<usize, String> {
    let profile = find_profile(data, &req.profile_id)
        .map_err(|e| e.to_string())?
        .clone();

    let signer = if req.sign {
        let identity = data.identity.as_ref().ok_or_else(|| {
            "export signing needs an identity; generate one in the More page".to_string()
        })?;
        Some(signing_key_of(identity))
    } else {
        None
    };

    let lock: Lock = if req.open_lock {
        Lock::Open
    } else {
        let pass = req
            .passphrase
            .clone()
            .ok_or_else(|| "passphrase required for a passphrase-locked export".to_string())?;
        if pass.chars().count() < 8 {
            return Err("passphrase is too short (minimum 8 characters)".to_string());
        }
        Lock::Passphrase(pass)
    };

    let policy = Policy {
        expires_at: req.expires_at,
        metered_only: req.metered_only,
        group_hint: req
            .group_hint
            .as_deref()
            .map(clean_name_input)
            .transpose()
            .map_err(|e| e.to_string())?,
    };

    let bytes = v2::encode(&profile, &policy, &lock, signer.as_ref()).map_err(|e| e.to_string())?;
    if let Some(parent) = req.out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let len = bytes.len();
    std::fs::write(&req.out_path, &bytes).map_err(|e| e.to_string())?;
    Ok(len)
}

/// Build a profile from the manual-add / edit form.
#[allow(clippy::too_many_arguments)]
pub fn manual_profile(
    editing_id: Option<&str>,
    name: &str,
    protocol: npvt_core::profile::Protocol,
    server: &str,
    port: u16,
    credential: npvt_core::profile::Credential,
    transport: npvt_core::profile::Transport,
    sni: Option<String>,
    path: Option<String>,
    udp: bool,
    group: Option<String>,
) -> Result<Profile, String> {
    let name = clean_name_input(name).map_err(|e| e.to_string())?;
    let profile = Profile {
        id: editing_id
            .map(str::to_string)
            .unwrap_or_else(new_profile_id),
        name,
        protocol,
        server: server.trim().to_string(),
        port,
        credential,
        transport,
        sni,
        path,
        allow_insecure: false,
        udp,
        group,
        created_at: unix_now(),
    };
    profile.validate().map_err(|e| e.to_string())?;
    Ok(profile)
}

/// Delete a profile and persist.
pub fn delete_profile(vault: &Vault, data: &mut VaultData, id: &str) -> Result<String, String> {
    let removed = npvt_core::vault::remove_profile(data, id).map_err(|e| e.to_string())?;
    vault.save(data).map_err(|e| e.to_string())?;
    Ok(removed.name)
}

/// A label for an import source, used in the security card dialog.
pub fn label_from_bytes_or_text(path: Option<&std::path::Path>, text: Option<&str>) -> String {
    match path {
        Some(p) => label_from_filename(p),
        None => {
            let t = text.unwrap_or("").trim();
            let head: String = t.chars().take(42).collect();
            if t.len() > 42 {
                format!("{head}...")
            } else {
                head
            }
        }
    }
}

/// Encode a byte slice as base64url (used by the public-key display).
pub fn b64url(data: &[u8]) -> String {
    B64URL.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use npvt_core::profile::{Credential, Protocol, Transport};

    fn vless() -> Profile {
        Profile {
            id: new_profile_id(),
            name: "shop".into(),
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

    fn tmp_vault() -> Vault {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "npvt-gui-{}-{}.json",
            std::process::id(),
            new_profile_id()
        ));
        Vault::at(path)
    }

    #[test]
    fn export_import_open_roundtrip() {
        let vault = tmp_vault();
        let mut data = VaultData::default();
        let p = vless();
        upsert_profile(&mut data, p.clone()).expect("upsert");
        vault.save(&data).expect("save");

        let out = std::env::temp_dir().join(format!("npvt-gui-export-{}.npvt", std::process::id()));
        let req = ExportRequest {
            profile_id: p.id.clone(),
            out_path: out.clone(),
            open_lock: true,
            passphrase: None,
            expires_at: None,
            metered_only: false,
            group_hint: None,
            sign: false,
        };
        let len = export_file(&vault, &data, &req).expect("export");
        assert!(len > 0);

        let bytes = std::fs::read(&out).expect("read back");
        assert_eq!(peek_kind(&bytes).expect("peek"), "open");
        let incoming = decode_source(Some(&bytes), None, None, None).expect("decode");
        assert_eq!(incoming.profile.id, p.id);
        assert_eq!(incoming.profile.server, "example.com");
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(vault.path());
    }

    #[test]
    fn passphrase_lock_roundtrip_and_sentinel() {
        let vault = tmp_vault();
        let mut data = VaultData::default();
        let p = vless();
        upsert_profile(&mut data, p.clone()).expect("upsert");
        vault.save(&data).expect("save");

        let out = std::env::temp_dir().join(format!("npvt-gui-lock-{}.npvt", std::process::id()));
        let req = ExportRequest {
            profile_id: p.id.clone(),
            out_path: out.clone(),
            open_lock: false,
            passphrase: Some("correct horse".into()),
            expires_at: None,
            metered_only: false,
            group_hint: None,
            sign: false,
        };
        export_file(&vault, &data, &req).expect("export");
        let bytes = std::fs::read(&out).expect("read back");
        assert_eq!(peek_kind(&bytes).expect("peek"), "passphrase");

        let missing = decode_source(Some(&bytes), None, None, None);
        assert_eq!(missing.expect_err("sentinel"), PASSPHRASE_REQUIRED);

        let wrong = decode_source(Some(&bytes), None, None, Some("wrong-pass".into()));
        assert!(wrong.is_err());

        let good =
            decode_source(Some(&bytes), None, None, Some("correct horse".into())).expect("decode");
        assert_eq!(good.profile.id, p.id);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(vault.path());
    }

    #[test]
    fn manual_profile_validates() {
        let ok = manual_profile(
            None,
            "My Shop",
            Protocol::Trojan,
            "example.com",
            443,
            Credential::Password("hunter2".into()),
            Transport::Tls,
            Some("example.com".into()),
            None,
            true,
            None,
        )
        .expect("profile");
        assert_eq!(ok.name, "My Shop");

        assert!(manual_profile(
            None,
            "",
            Protocol::Trojan,
            "example.com",
            443,
            Credential::Password("x".into()),
            Transport::Tls,
            None,
            None,
            false,
            None,
        )
        .is_err());

        assert!(manual_profile(
            None,
            "bad url",
            Protocol::Trojan,
            "https://evil.example/u",
            443,
            Credential::Password("x".into()),
            Transport::Tls,
            None,
            None,
            false,
            None,
        )
        .is_err());
    }

    #[test]
    fn unique_name_dedups() {
        let mut data = VaultData::default();
        data.profiles.push(vless());
        data.profiles[0].name = "shop".into();
        assert_eq!(unique_name(&data, "other"), "other");
        let deduped = unique_name(&data, "shop");
        assert_ne!(deduped, "shop");
        assert!(deduped.starts_with("shop"));
    }
}
