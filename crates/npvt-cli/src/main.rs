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

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Generator, Shell};
use npvt_core::engine::DEFAULT_SOCKS_PORT;
use npvt_core::error::ErrorClass;
use npvt_core::npvt::v2;
use npvt_core::{Error, Result};

mod doctor;
mod library;
mod session;
mod transfer;
mod tunnel;

use doctor::cmd_doctor;
use library::{cmd_list, cmd_remove, cmd_show};
use session::cmd_keygen;
use transfer::{cmd_export, cmd_import};
use tunnel::{cmd_connect, cmd_disconnect, cmd_status};

pub(crate) const B64URL: base64::engine::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Maximum accepted config file size on import.
pub(crate) const MAX_IMPORT_FILE: usize = v2::MAX_CONTAINER_LEN;

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
