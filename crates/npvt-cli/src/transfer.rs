//! Import and export: container decode, the security card, name
//! de-duplication and the `import`/`export` commands.

use std::path::Path;

use base64::Engine as _;

use npvt_core::npvt::v1;
use npvt_core::npvt::v2::{self, Lock, Policy, Unlock};
use npvt_core::profile::{parse_expires, unix_now, Profile};
use npvt_core::uri::{parse_import, ImportSource};
use npvt_core::vault::{
    clean_name_input, find_profile, short_suffix, upsert_profile, Identity, VaultData,
};
use npvt_core::{Error, Result};

use crate::session::{
    confirm, log, open_vault, read_passphrase, read_passphrase_stdin, signing_key_of,
};
use crate::{Cli, B64URL, MAX_IMPORT_FILE};

pub(crate) struct DecodedIncoming {
    pub(crate) profile: Profile,
    pub(crate) policy: Policy,
    pub(crate) verification: v2::Verification,
    pub(crate) lock_kind: &'static str,
}

fn decode_container(
    bytes: &[u8],
    identity: Option<&Identity>,
    passphrase_stdin: bool,
) -> Result<DecodedIncoming> {
    let lock_kind = v2::peek_lock(bytes)?;
    let (decoded, verification) = match lock_kind {
        "open" => {
            let d = v2::decode(bytes, Unlock::Any)?;
            let v = d.verification.clone();
            (d, v)
        }
        "passphrase" => {
            let passphrase = if passphrase_stdin {
                read_passphrase_stdin()?
            } else {
                read_passphrase(false)?
            };
            let d = v2::decode(bytes, Unlock::Passphrase(&passphrase))?;
            let v = d.verification.clone();
            (d, v)
        }
        "keys" => {
            let seed = identity
                .as_ref()
                .ok_or_else(|| {
                    Error::Vault(
                        "container is key-locked but no local identity exists; run npvt-cli keygen first"
                            .into(),
                    )
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
    Ok(DecodedIncoming {
        profile: decoded.profile,
        policy: decoded.policy,
        verification,
        lock_kind,
    })
}

pub(crate) fn print_security_card(
    incoming: &DecodedIncoming,
    verification: &v2::Verification,
) -> Result<()> {
    let p = &incoming.profile;
    println!("---------------------------------------------");
    println!(" Security card");
    println!("---------------------------------------------");
    println!(" name        : {}", p.name);
    println!(
        " protocol    : {} ({})",
        p.protocol.as_str(),
        match p.transport {
            npvt_core::profile::Transport::Plain => "plain",
            npvt_core::profile::Transport::Tls => "tls",
            npvt_core::profile::Transport::Ws => "websocket",
            npvt_core::profile::Transport::Grpc => "grpc",
            npvt_core::profile::Transport::H2 => "h2",
        }
    );
    println!(" endpoint    : {}:{}", p.server, p.port);
    if let Some(sni) = &p.sni {
        println!(" sni         : {sni}");
    }
    println!(" udp         : {}", p.udp);
    println!(" lock        : {}", incoming.lock_kind);
    println!(
        " signature   : {}",
        if verification.signed {
            format!(
                "verified, creator {}",
                verification.creator.as_deref().unwrap_or("?")
            )
        } else {
            "UNSIGNED (origin unproven)".to_string()
        }
    );
    if let Some(exp) = incoming.policy.expires_at {
        println!(" expires     : unix {exp}");
    }
    if incoming.policy.metered_only {
        println!(" policy      : metered connections only");
    }
    if let Some(hint) = &incoming.policy.group_hint {
        println!(" group hint  : {hint}");
    }
    if p.allow_insecure {
        println!(" WARNING     : this config disables TLS verification (allow_insecure)");
    }
    println!("---------------------------------------------");
    Ok(())
}

fn unique_name(data: &VaultData, wanted: &str) -> String {
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

pub(crate) fn cmd_import(
    cli: &Cli,
    source: String,
    name_override: Option<String>,
    group: Option<String>,
    yes: bool,
    passphrase_stdin: bool,
) -> Result<()> {
    let (vault, mut data) = open_vault(cli)?;
    let incoming: DecodedIncoming;

    let path = Path::new(&source);
    if path.exists() {
        let meta = std::fs::metadata(path).map_err(Error::Io)?;
        if meta.len() as usize > MAX_IMPORT_FILE {
            return Err(Error::InvalidConfig("file is too large".into()));
        }
        let bytes = std::fs::read(path).map_err(Error::Io)?;
        if bytes.starts_with(v2::MAGIC) {
            log(cli, "detected npvt2 container");
            incoming = decode_container(&bytes, data.identity.as_ref(), passphrase_stdin)?;
        } else if v1::probe(&bytes).is_v1 {
            log(cli, "detected legacy v1 container");
            let profile = v1::import_best_effort(&bytes)?;
            incoming = DecodedIncoming {
                profile,
                policy: Policy::default(),
                verification: v2::Verification {
                    signed: false,
                    creator: None,
                    valid: false,
                },
                lock_kind: "legacy-v1",
            };
        } else {
            return Err(Error::InvalidConfig(
                "unrecognized file format (expected an npvt2 container or a legacy v1 file)".into(),
            ));
        }
    } else {
        match parse_import(&source)? {
            ImportSource::Container(bytes) => {
                log(cli, "detected npvt2 container link");
                incoming = decode_container(&bytes, data.identity.as_ref(), passphrase_stdin)?;
            }
            ImportSource::Profile(profile) => {
                incoming = DecodedIncoming {
                    profile,
                    policy: Policy::default(),
                    verification: v2::Verification {
                        signed: false,
                        creator: None,
                        valid: false,
                    },
                    lock_kind: "link",
                };
            }
        }
    }

    print_security_card(&incoming, &incoming.verification)?;
    if !yes && !confirm("Add this profile to the library?")? {
        println!("Cancelled - nothing was added.");
        return Ok(());
    }

    let mut profile = incoming.profile;
    profile.name = match &name_override {
        Some(n) => clean_name_input(n)?,
        None => unique_name(&data, &profile.name),
    };
    profile.group = match group {
        Some(g) => Some(clean_name_input(&g)?),
        None => incoming
            .policy
            .group_hint
            .as_deref()
            .map(clean_name_input)
            .transpose()?,
    };
    if let Some(creator) = &incoming.verification.creator {
        data.known_creators
            .entry(creator.clone())
            .or_insert_with(|| format!("creator {}", &creator[..8.min(creator.len())]));
    }
    let id = profile.id.clone();
    let replaced = upsert_profile(&mut data, profile.clone())?;
    vault.save(&data)?;
    println!(
        "{} profile '{}' ({} {}:{}) id={id}",
        if replaced { "Replaced" } else { "Added" },
        profile.name,
        profile.protocol.as_str(),
        profile.server,
        profile.port
    );
    println!(
        "Default action is add-to-library; run 'npvt-cli connect {}' to connect.",
        profile.name
    );
    Ok(())
}
fn parse_pubkey(spec: &str) -> Result<[u8; 32]> {
    let bytes = B64URL
        .decode(spec.trim())
        .map_err(|_| Error::InvalidConfig("recipient pubkey must be base64url".into()))?;
    bytes
        .try_into()
        .map_err(|_| Error::InvalidConfig("recipient pubkey must be 32 bytes".into()))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_export(
    cli: &Cli,
    target: &str,
    out: &Path,
    open: bool,
    for_key: &[String],
    expires: Option<String>,
    metered_only: bool,
    group_hint: Option<String>,
    unsigned: bool,
    passphrase_stdin: bool,
) -> Result<()> {
    let (vault, data) = open_vault(cli)?;
    let profile = find_profile(&data, target)?.clone();

    let signer = if unsigned {
        None
    } else {
        let identity = data.identity.as_ref().ok_or_else(|| {
            Error::Vault("export signing needs an identity; run 'npvt-cli keygen'".into())
        })?;
        Some(signing_key_of(identity))
    };

    let lock: Lock = if open {
        Lock::Open
    } else if !for_key.is_empty() {
        let mut pubs = Vec::new();
        for spec in for_key {
            pubs.push(parse_pubkey(spec)?);
        }
        Lock::Keys(pubs)
    } else if passphrase_stdin {
        Lock::Passphrase(read_passphrase_stdin()?)
    } else {
        Lock::Passphrase(read_passphrase(true)?)
    };

    let policy = Policy {
        expires_at: expires.as_deref().map(parse_expires).transpose()?,
        metered_only,
        group_hint: group_hint.map(|g| clean_name_input(&g)).transpose()?,
    };

    let bytes = v2::encode(&profile, &policy, &lock, signer.as_ref())?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    std::fs::write(out, &bytes).map_err(Error::Io)?;

    println!("Wrote {} ({} bytes)", out.display(), bytes.len());
    println!(
        "  profile   : {} ({} {}:{})",
        profile.name,
        profile.protocol.as_str(),
        profile.server,
        profile.port
    );
    println!(
        "  lock      : {}",
        if open {
            "open (plaintext payload)"
        } else if !for_key.is_empty() {
            "recipient keys"
        } else {
            "passphrase"
        }
    );
    match &signer {
        Some(sk) => println!(
            "  signature : ed25519 {}",
            B64URL.encode(sk.verifying_key().as_bytes())
        ),
        None => println!("  signature : NONE - recipients cannot verify the origin"),
    }
    if let Some(exp) = policy.expires_at {
        println!("  expires   : unix {exp}");
    }
    let _ = vault; // vault loaded for identity/lookup semantics
    Ok(())
}
