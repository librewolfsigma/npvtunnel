//! Library commands: `list`, `show` and `remove`.

use npvt_core::npvt::v2::{self, Policy};
use npvt_core::profile::Credential;
use npvt_core::vault::{find_profile, remove_profile};
use npvt_core::Result;

use super::transfer::{print_security_card, DecodedIncoming};
use crate::session::{confirm, open_vault};
use crate::Cli;

pub(crate) fn cmd_list(cli: &Cli, json: bool, sort: &str) -> Result<()> {
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

pub(crate) fn cmd_show(cli: &Cli, target: &str, reveal: bool, json: bool) -> Result<()> {
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

pub(crate) fn cmd_remove(cli: &Cli, target: &str, yes: bool) -> Result<()> {
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
