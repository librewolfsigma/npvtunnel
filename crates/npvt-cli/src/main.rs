//! `npvt-cli` — the NPVTunnel command line client.
//!
//! Verb-first CLI (see the concept blueprint, Section 7). Security rules the
//! CLI enforces:
//!
//! - credentials are never printed unless `show --reveal` is used explicitly;
//! - passphrases are prompted hidden (or read from stdin with
//!   `--passphrase-stdin`, never from argv);
//! - expired containers are refused at import time, so expired state can
//!   never reach the engine;
//! - the engine inbound binds 127.0.0.1 only;
//! - exit codes are stable: 0 ok, 1 usage, 2 config, 3 network/engine,
//!   4 permission, 5 crypto, 6 io, 7 other.

#![forbid(unsafe_code)]

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use base64::Engine as _;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Generator, Shell};
use ed25519_dalek::SigningKey;
use npvt_core::engine::{self, DEFAULT_SOCKS_PORT};
use npvt_core::error::ErrorClass;
use npvt_core::npvt::v1;
use npvt_core::npvt::v2::{self, Lock, Policy, Unlock};
use npvt_core::profile::{parse_expires, unix_now, Credential, Profile};
use npvt_core::session::{self, Session};
use npvt_core::uri::{parse_import, ImportSource};
use npvt_core::vault::{
    clean_name_input, find_profile, generate_identity, remove_profile, short_suffix,
    upsert_profile, Identity, Vault, VaultData,
};
use npvt_core::{Error, Result};
use x25519_dalek::{PublicKey, StaticSecret};

const B64URL: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Maximum accepted config file size on import.
const MAX_IMPORT_FILE: usize = v2::MAX_CONTAINER_LEN;

#[derive(Parser)]
#[command(
    name = "npvt-cli",
    version,
    about = "Lightweight, security-hardened .npvt tunnel client for Windows",
    long_about = "NPVTunnel: a lightweight, security-hardened client for the .npvt config ecosystem. Import .npvt/.npvs files and share links, keep them in a local vault, export cryptographically-locked v2 containers, and run tunnels through a sing-box sidecar."
)]
struct Cli {
    /// Override the vault path (default: OS data directory).
    #[arg(long, global = true, value_name = "PATH")]
    vault: Option<PathBuf>,

    /// Log actions to stderr (secrets are never logged).
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate the local creator identity (signing + recipient keys).
    Keygen {
        /// Replace an existing identity.
        #[arg(long)]
        rotate: bool,
    },
    /// Import from a .npvt/.npvs file, a share URI or an npvt:// link.
    Import {
        /// File path or link.
        source: String,
        /// Override the profile name.
        #[arg(long)]
        name: Option<String>,
        /// Assign a group label.
        #[arg(long)]
        group: Option<String>,
        /// Accept the import security card without prompting (scripts).
        #[arg(long)]
        yes: bool,
        /// Read the container passphrase from stdin (scripts).
        #[arg(long, default_value_t = false)]
        passphrase_stdin: bool,
    },
    /// Export a profile as a .npvt2 container with a lock and signature.
    Export {
        /// Profile name or id.
        target: String,
        /// Output file path.
        #[arg(long)]
        out: PathBuf,
        /// Share openly ("anyone with the app") - plaintext payload.
        #[arg(long, default_value_t = false)]
        open: bool,
        /// Lock to a recipient public key (base64url X25519; repeatable).
        #[arg(long, value_name = "PUBKEY")]
        for_key: Vec<String>,
        /// Expiry: YYYY-MM-DD or +Nd days.
        #[arg(long)]
        expires: Option<String>,
        /// Policy: metered connections only.
        #[arg(long, default_value_t = false)]
        metered_only: bool,
        /// Suggest a group for importers.
        #[arg(long)]
        group_hint: Option<String>,
        /// Skip signing (strongly discouraged).
        #[arg(long, default_value_t = false)]
        unsigned: bool,
        /// Read the passphrase from stdin instead of the hidden prompt.
        #[arg(long, default_value_t = false)]
        passphrase_stdin: bool,
    },
    /// List profiles (credentials are never shown).
    List {
        /// JSON output.
        #[arg(long)]
        json: bool,
        /// Sort by name (default) or most recent.
        #[arg(long, default_value = "name")]
        sort: String,
    },
    /// Show one profile's security card.
    Show {
        target: String,
        /// Include credentials. Use sparingly.
        #[arg(long, default_value_t = false)]
        reveal: bool,
        /// JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Remove a profile from the vault.
    Remove {
        target: String,
        /// Do not ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Connect a profile through the sing-box sidecar (SOCKS5 on localhost).
    Connect {
        target: String,
        /// Local SOCKS5 port.
        #[arg(long, default_value_t = DEFAULT_SOCKS_PORT)]
        socks_port: u16,
        /// Explicit path to the sing-box binary.
        #[arg(long)]
        engine: Option<PathBuf>,
        /// DNS server for the engine.
        #[arg(long, default_value = "1.1.1.1")]
        dns: String,
    },
    /// Stop a running engine.
    Disconnect,
    /// Show engine status.
    Status {
        /// JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Self-check: vault, identity, engine binary, profile hygiene.
    Doctor {
        /// JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Emit shell completions.
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> ExitCode {
    install_panic_hook();
    // Double-click UX: with no arguments clap would print a usage error and
    // exit instantly, so a Windows console window would flash and disappear
    // and the launch would look like a crash. Show a friendly quick-start
    // screen instead and keep the window open.
    if interactive_no_args() {
        print_welcome();
        if cfg!(windows) {
            pause_for_window_close();
        }
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(exit_code_for(&err))
        }
    }
}

/// True when the process was launched with no arguments from an interactive
/// console - the Windows Explorer double-click scenario.
fn interactive_no_args() -> bool {
    std::env::args_os().count() == 1
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

/// Print a plain ASCII quick-start screen for double-click launches.
fn print_welcome() {
    println!(
        "NPVTunnel v{} - security-hardened .npvt tunnel client",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("Nothing is wrong - npvtunnel is a command-line tool. Windows opens");
    println!("a console for it and closes the window as soon as it finishes.");
    println!("To use it, open PowerShell or Windows Terminal and run:");
    println!();
    println!("  npvt-cli doctor                     check your setup");
    println!("  npvt-cli import <file-or-link>      add a profile (security card first)");
    println!("  npvtunnel list                       show the library");
    println!("  npvt-cli connect <name>             connect (SOCKS5 127.0.0.1:2080)");
    println!("  npvtunnel --help                     all commands");
    println!();
    println!("The connect command needs the sing-box engine (sing-box.exe), either");
    println!("next to npvtunnel.exe, in the npvtunnel data folder under bin\\, or");
    println!("on PATH. Downloads: https://github.com/SagerNet/sing-box/releases");
}

/// Keep a double-clicked console window open long enough to be read.
fn pause_for_window_close() {
    let _ = std::io::stdout().flush();
    eprintln!();
    eprint!("Press Enter to close this window...");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

/// Replace the default panic output with a clean, reportable message; keep
/// the window open when the panic can only have been seen by double-click.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        eprintln!();
        eprintln!("npvtunnel hit an unexpected internal error and must close.");
        eprintln!("  what : {message}");
        eprintln!("  where: {location}");
        eprintln!("This is a bug, not a problem with your configs. Please report it:");
        eprintln!("  https://github.com/librewolfsigma/npvtunnel/issues");
        if interactive_no_args() && cfg!(windows) {
            pause_for_window_close();
        }
    }));
}

fn exit_code_for(err: &Error) -> u8 {
    match err.class() {
        ErrorClass::Config => 2,
        ErrorClass::Network => 3,
        ErrorClass::Permission => 4,
        ErrorClass::Crypto => 5,
        ErrorClass::Io => 6,
        ErrorClass::Other => 7,
    }
}

fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Keygen { rotate } => cmd_keygen(cli, *rotate),
        Command::Import {
            source,
            name,
            group,
            yes,
            passphrase_stdin,
        } => cmd_import(
            cli,
            source.clone(),
            name.clone(),
            group.clone(),
            *yes,
            *passphrase_stdin,
        ),
        Command::Export {
            target,
            out,
            open,
            for_key,
            expires,
            metered_only,
            group_hint,
            unsigned,
            passphrase_stdin,
        } => cmd_export(
            cli,
            target,
            out,
            *open,
            for_key,
            expires.clone(),
            *metered_only,
            group_hint.clone(),
            *unsigned,
            *passphrase_stdin,
        ),
        Command::List { json, sort } => cmd_list(cli, *json, sort),
        Command::Show {
            target,
            reveal,
            json,
        } => cmd_show(cli, target, *reveal, *json),
        Command::Remove { target, yes } => cmd_remove(cli, target, *yes),
        Command::Connect {
            target,
            socks_port,
            engine,
            dns,
        } => cmd_connect(cli, target, *socks_port, engine.as_deref(), dns),
        Command::Disconnect => cmd_disconnect(cli),
        Command::Status { json } => cmd_status(cli, *json),
        Command::Doctor { json } => cmd_doctor(cli, *json),
        Command::Completions { shell } => {
            let cmd = Cli::command();
            shell.generate(&cmd, &mut std::io::stdout());
            Ok(())
        }
    }
}

fn log(cli: &Cli, msg: &str) {
    if cli.verbose {
        eprintln!("[npvtunnel] {msg}");
    }
}

fn open_vault(cli: &Cli) -> Result<(Vault, VaultData)> {
    let vault = match &cli.vault {
        Some(p) => Vault::at(p.clone()),
        None => Vault::open()?,
    };
    let data = vault.load()?;
    Ok((vault, data))
}

// ── passphrase handling ─────────────────────────────────────────────────────

fn read_passphrase(confirm: bool) -> Result<String> {
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

fn read_passphrase_stdin() -> Result<String> {
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

fn confirm(prompt: &str) -> Result<bool> {
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

// ── identity helpers ────────────────────────────────────────────────────────

fn signing_key_of(identity: &Identity) -> SigningKey {
    SigningKey::from_bytes(&identity.ed25519_seed)
}

fn x25519_pub_of(identity: &Identity) -> [u8; 32] {
    let secret = StaticSecret::from(identity.x25519_seed);
    *PublicKey::from(&secret).as_bytes()
}

// ── keygen ──────────────────────────────────────────────────────────────────

fn cmd_keygen(cli: &Cli, rotate: bool) -> Result<()> {
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

// ── import ──────────────────────────────────────────────────────────────────

struct DecodedIncoming {
    profile: Profile,
    policy: Policy,
    verification: v2::Verification,
    lock_kind: &'static str,
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

fn print_security_card(incoming: &DecodedIncoming, verification: &v2::Verification) -> Result<()> {
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

fn cmd_import(
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

// ── export ──────────────────────────────────────────────────────────────────

fn parse_pubkey(spec: &str) -> Result<[u8; 32]> {
    let bytes = B64URL
        .decode(spec.trim())
        .map_err(|_| Error::InvalidConfig("recipient pubkey must be base64url".into()))?;
    bytes
        .try_into()
        .map_err(|_| Error::InvalidConfig("recipient pubkey must be 32 bytes".into()))
}

#[allow(clippy::too_many_arguments)]
fn cmd_export(
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

// ── list / show / remove ────────────────────────────────────────────────────

fn cmd_list(cli: &Cli, json: bool, sort: &str) -> Result<()> {
    let (_, data) = open_vault(cli)?;
    let mut profiles = data.profiles.clone();
    match sort {
        "recent" => {
            profiles.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.name.cmp(&b.name)))
        }
        _ => profiles.sort_by_key(|p| p.name.to_lowercase()),
    }
    if json {
        let items: Vec<serde_json::Value> = profiles
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id, "name": p.name, "protocol": p.protocol.as_str(),
                    "server": p.server, "port": p.port, "group": p.group,
                    "udp": p.udp, "allow_insecure": p.allow_insecure,
                    "created_at": p.created_at,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }
    if profiles.is_empty() {
        println!("Library is empty. Try: npvt-cli import <file-or-link>");
        return Ok(());
    }
    for p in profiles {
        println!(
            "{:<24} {:<12} {:>21}:{:<5} {}",
            p.name,
            p.protocol.as_str(),
            p.server,
            p.port.to_string(),
            p.group.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn cmd_show(cli: &Cli, target: &str, reveal: bool, json: bool) -> Result<()> {
    let (_, data) = open_vault(cli)?;
    let p = find_profile(&data, target)?;
    if json {
        let mut obj = serde_json::json!({
            "id": p.id, "name": p.name, "protocol": p.protocol.as_str(),
            "server": p.server, "port": p.port, "transport": format!("{:?}", p.transport).to_lowercase(),
            "sni": p.sni, "path": p.path, "udp": p.udp,
            "allow_insecure": p.allow_insecure, "group": p.group,
            "created_at": p.created_at,
        });
        if reveal {
            obj["credential"] = serde_json::json!(credential_label(&p.credential));
        }
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
    }
    print_security_card(
        &DecodedIncoming {
            profile: p.clone(),
            policy: Policy::default(),
            verification: v2::Verification {
                signed: false,
                creator: None,
                valid: false,
            },
            lock_kind: "vault",
        },
        &v2::Verification {
            signed: false,
            creator: None,
            valid: false,
        },
    )?;
    if reveal {
        println!(" credential : {}", credential_label(&p.credential));
    } else {
        println!("(credentials hidden; pass --reveal to include them)");
    }
    Ok(())
}

fn credential_label(cred: &Credential) -> String {
    match cred {
        Credential::Uuid(v) => format!("uuid {v}"),
        Credential::Password(v) => format!("password {v}"),
        Credential::SsMethod { method, password } => format!("{method}:{password}"),
        Credential::Token(v) => format!("token {v}"),
        Credential::None => "none".to_string(),
    }
}

fn cmd_remove(cli: &Cli, target: &str, yes: bool) -> Result<()> {
    let (vault, mut data) = open_vault(cli)?;
    let probe = find_profile(&data, target)?;
    if !yes && !confirm(&format!("Remove '{}' from the library?", probe.name))? {
        println!("Cancelled.");
        return Ok(());
    }
    let removed = remove_profile(&mut data, target)?;
    vault.save(&data)?;
    println!("Removed '{}'.", removed.name);
    Ok(())
}

// ── connect / disconnect / status ───────────────────────────────────────────

fn cmd_connect(
    cli: &Cli,
    target: &str,
    socks_port: u16,
    engine_path: Option<&Path>,
    dns: &str,
) -> Result<()> {
    let (_, data) = open_vault(cli)?;
    let profile = find_profile(&data, target)?.clone();

    if let Some(existing) = session::read_session()? {
        if engine::alive(existing.pid) {
            return Err(Error::Engine(format!(
                "an engine is already running (pid {}, profile '{}'); disconnect first",
                existing.pid, existing.name
            )));
        }
    }

    let workdir = engine::work_dir()?;
    let cfg = engine::config(
        std::slice::from_ref(&profile),
        &profile.id,
        socks_port,
        dns,
        None,
    )?;
    let cfg_path = workdir.join("engine.json");
    engine::write_config(&cfg, &cfg_path)?;
    log(
        cli,
        &format!("engine config written to {}", cfg_path.display()),
    );

    let bin = engine::find_engine_binary(engine_path).ok_or_else(|| {
        Error::Engine(
            "sing-box binary not found; place it next to npvt-cli, in the npvtunnel data directory under bin/, or on PATH (npvt-cli doctor shows what was checked)"
                .into(),
        )
    })?;

    let running = engine::start(&bin, &cfg_path, &workdir.join("engine.log"))?;
    // Give the engine a moment, then verify it is still alive: a malformed
    // config or a port conflict would otherwise die silently right after a
    // "tunnel up" message.
    std::thread::sleep(Duration::from_millis(600));
    if !engine::alive(running.pid) {
        let tail = engine::log_tail(&workdir.join("engine.log"), 20);
        return Err(Error::Engine(format!(
            "sing-box exited immediately after start; last engine log lines:\n{tail}\n(engine config: {})\n(run 'npvt-cli doctor' to check the engine setup)",
            cfg_path.display()
        )));
    }
    session::write_session(&Session {
        pid: running.pid,
        profile_id: profile.id.clone(),
        name: profile.name.clone(),
        socks_port,
        started: unix_now(),
    })?;

    println!(
        "Tunnel up: '{}' ({} {}:{}) - sing-box pid {}",
        profile.name,
        profile.protocol.as_str(),
        profile.server,
        profile.port,
        running.pid
    );
    println!("SOCKS5 proxy listening on 127.0.0.1:{socks_port} (localhost only).");
    Ok(())
}

fn cmd_disconnect(cli: &Cli) -> Result<()> {
    match session::read_session()? {
        Some(pf) if engine::alive(pf.pid) => {
            engine::stop(pf.pid)?;
            session::clear_session()?;
            println!("Engine stopped (pid {}).", pf.pid);
            Ok(())
        }
        Some(pf) => {
            session::clear_session()?;
            println!("Stale pid file removed (pid {} was not running).", pf.pid);
            Ok(())
        }
        None => {
            log(cli, "no pid file; nothing to do");
            println!("Engine is not running.");
            Ok(())
        }
    }
}

fn cmd_status(_cli: &Cli, json: bool) -> Result<()> {
    let pf = session::read_session()?;
    let running = pf.as_ref().is_some_and(|p| engine::alive(p.pid));
    if json {
        let value = match (&pf, running) {
            (Some(p), true) => serde_json::json!({
                "running": true, "pid": p.pid, "profile": p.name,
                "profile_id": p.profile_id, "socks_port": p.socks_port,
                "started": p.started,
            }),
            (Some(p), false) => serde_json::json!({
                "running": false, "stale_pid": p.pid, "profile": p.name,
            }),
            (None, _) => serde_json::json!({"running": false}),
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    match (pf, running) {
        (Some(p), true) => println!(
            "running: pid {} profile '{}' socks 127.0.0.1:{} since unix {}",
            p.pid, p.name, p.socks_port, p.started
        ),
        (Some(p), false) => println!(
            "not running (stale pid {} from profile '{}')",
            p.pid, p.name
        ),
        (None, _) => println!("not running"),
    }
    Ok(())
}

// ── doctor ──────────────────────────────────────────────────────────────────

struct Check {
    status: &'static str, // pass | warn | fail
    name: &'static str,
    detail: String,
}

fn cmd_doctor(cli: &Cli, json: bool) -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();

    let (_vault, data) = match open_vault(cli) {
        Ok((vault, loaded)) => {
            checks.push(Check {
                status: "pass",
                name: "vault",
                detail: format!("readable at {}", vault.path().display()),
            });
            (vault, loaded)
        }
        Err(e) => {
            checks.push(Check {
                status: "fail",
                name: "vault",
                detail: e.to_string(),
            });
            emit_doctor(checks, json)?;
            return Err(Error::Other("doctor found failures".into()));
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(_vault.path()) {
            Ok(meta) => {
                let mode = meta.permissions().mode();
                if mode & 0o077 == 0 {
                    checks.push(Check {
                        status: "pass",
                        name: "vault-permissions",
                        detail: format!("mode {mode:o}"),
                    });
                } else {
                    checks.push(Check {
                        status: "fail",
                        name: "vault-permissions",
                        detail: format!("mode {mode:o} is too open (want 0600)"),
                    });
                }
            }
            Err(e) => checks.push(Check {
                status: "warn",
                name: "vault-permissions",
                detail: format!("could not stat vault: {e}"),
            }),
        }
    }

    checks.push(Check {
        status: if data.identity.is_some() {
            "pass"
        } else {
            "warn"
        },
        name: "identity",
        detail: if data.identity.is_some() {
            "creator identity present".into()
        } else {
            "no identity; run 'npvt-cli keygen' to sign exports and open key-locked files".into()
        },
    });

    let insecure = data.profiles.iter().filter(|p| p.allow_insecure).count();
    checks.push(Check {
        status: if insecure == 0 { "pass" } else { "warn" },
        name: "profile-hygiene",
        detail: format!(
            "{} profiles, {insecure} with TLS verification disabled",
            data.profiles.len()
        ),
    });

    match engine::find_engine_binary(None) {
        Some(bin) => checks.push(Check {
            status: "pass",
            name: "engine-binary",
            detail: bin.display().to_string(),
        }),
        None => checks.push(Check {
            status: "warn",
            name: "engine-binary",
            detail: "sing-box not found; connect needs it (see 'npvt-cli connect --help')".into(),
        }),
    }

    match session::read_session() {
        Ok(Some(pf)) if engine::alive(pf.pid) => checks.push(Check {
            status: "pass",
            name: "engine",
            detail: format!("running, pid {} profile '{}'", pf.pid, pf.name),
        }),
        Ok(Some(pf)) => checks.push(Check {
            status: "warn",
            name: "engine",
            detail: format!("stale pid file (pid {} not running)", pf.pid),
        }),
        Ok(None) => checks.push(Check {
            status: "pass",
            name: "engine",
            detail: "not running".into(),
        }),
        Err(e) => checks.push(Check {
            status: "fail",
            name: "engine",
            detail: e.to_string(),
        }),
    }

    let failed = checks.iter().any(|c| c.status == "fail");
    emit_doctor(checks, json)?;
    if failed {
        Err(Error::Other("doctor found failures".into()))
    } else {
        Ok(())
    }
}

fn emit_doctor(checks: Vec<Check>, json: bool) -> Result<()> {
    if json {
        let items: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| serde_json::json!({"check": c.name, "status": c.status, "detail": c.detail}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        for c in &checks {
            println!("{:<4} {:<20} {}", c.status.to_uppercase(), c.name, c.detail);
        }
    }
    Ok(())
}
