//! Vault opening, prompt helpers, identity key helpers and `keygen`.

use std::io::{BufRead, Write};

use base64::Engine as _;
use ed25519_dalek::SigningKey;
use x25519_dalek::{PublicKey, StaticSecret};

use npvt_core::vault::{generate_identity, Identity, Vault, VaultData};
use npvt_core::{Error, Result};

use crate::{Cli, B64URL};

pub(crate) fn log(cli: &Cli, msg: &str) {
    if cli.verbose {
        eprintln!("[npvtunnel] {msg}");
    }
}

pub(crate) fn open_vault(cli: &Cli) -> Result<(Vault, VaultData)> {
    let vault = match &cli.vault {
        Some(p) => Vault::at(p.clone()),
        None => Vault::open()?,
    };
    let data = vault.load()?;
    Ok((vault, data))
}

pub(crate) fn read_passphrase(confirm: bool) -> Result<String> {
    let first = rpassword::prompt_password("Container passphrase: ").map_err(Error::Io)?;
    if first.len() < 8 {
        return Err(Error::InvalidConfig(
            "passphrase is too short (minimum 8 characters)".into(),
        ));
    }
    if confirm {
        let second = rpassword::prompt_password("Repeat passphrase: ").map_err(Error::Io)?;
        if first != second {
            return Err(Error::InvalidConfig("passphrases do not match".into()));
        }
    }
    Ok(first)
}

pub(crate) fn read_passphrase_stdin() -> Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(Error::Io)?;
    let passphrase = line.trim_end_matches(['\r', '\n']).to_string();
    if passphrase.len() < 8 {
        return Err(Error::InvalidConfig(
            "passphrase is too short (minimum 8 characters)".into(),
        ));
    }
    Ok(passphrase)
}

pub(crate) fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    std::io::stdout().flush().map_err(Error::Io)?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(Error::Io)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(crate) fn signing_key_of(identity: &Identity) -> SigningKey {
    SigningKey::from_bytes(&identity.ed25519_seed)
}

pub(crate) fn x25519_pub_of(identity: &Identity) -> [u8; 32] {
    let secret = StaticSecret::from(identity.x25519_seed);
    *PublicKey::from(&secret).as_bytes()
}

pub(crate) fn cmd_keygen(cli: &Cli, rotate: bool) -> Result<()> {
    let (vault, mut data) = open_vault(cli)?;
    if data.identity.is_some() && !rotate {
        return Err(Error::Vault(
            "an identity already exists; pass --rotate to replace it".into(),
        ));
    }
    let identity = generate_identity();
    println!("Creator identity generated.");
    println!(
        "  signing pubkey (Ed25519, base64url): {}",
        B64URL.encode(signing_key_of(&identity).verifying_key().as_bytes())
    );
    println!(
        "  recipient pubkey (X25519, base64url): {}",
        B64URL.encode(x25519_pub_of(&identity))
    );
    println!("Share your recipient pubkey so creators can lock files to you.");
    log(cli, "saving vault with new identity");
    data.identity = Some(identity);
    vault.save(&data)?;
    Ok(())
}
