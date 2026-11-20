//! The `doctor` self-check: vault readability, unix permissions,
//! identity presence, profile hygiene, engine binary and session state.

use std::path::Path;

use npvt_core::engine;
use npvt_core::session;
use npvt_core::vault::{Vault, VaultData};
use npvt_core::{Error, Result};

use crate::session::open_vault;
use crate::Cli;

struct Check {
    status: &'static str, // pass | warn | fail
    name: &'static str,
    detail: String,
}

pub(crate) fn cmd_doctor(cli: &Cli, json: bool) -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();
    let (vault, data) = match collect_vault_checks(cli, &mut checks) {
        Ok(ok) => ok,
        Err(e) => {
            emit_doctor(checks, json)?;
            return Err(e);
        }
    };
    check_vault_permissions(vault.path(), &mut checks);
    collect_profile_checks(&data, &mut checks);
    collect_engine_checks(&mut checks);

    let failed = checks.iter().any(|c| c.status == "fail");
    emit_doctor(checks, json)?;
    if failed {
        Err(Error::Other("doctor found failures".into()))
    } else {
        Ok(())
    }
}

/// Open the vault and record the vault check; a failure also pushes the
/// failing check so the report still renders before the error returns.
fn collect_vault_checks(cli: &Cli, checks: &mut Vec<Check>) -> Result<(Vault, VaultData)> {
    match open_vault(cli) {
        Ok((vault, loaded)) => {
            checks.push(Check {
                status: "pass",
                name: "vault",
                detail: format!("readable at {}", vault.path().display()),
            });
            Ok((vault, loaded))
        }
        Err(e) => {
            checks.push(Check {
                status: "fail",
                name: "vault",
                detail: e.to_string(),
            });
            Err(Error::Other("doctor found failures".into()))
        }
    }
}

/// On unix, flag a vault file that other users can read.
fn check_vault_permissions(path: &Path, checks: &mut Vec<Check>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
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
    #[cfg(not(unix))]
    {
        let _ = (path, checks);
    }
}

/// Identity presence and profile TLS-hygiene.
fn collect_profile_checks(data: &VaultData, checks: &mut Vec<Check>) {
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
}

/// Engine binary discovery and current session liveness.
fn collect_engine_checks(checks: &mut Vec<Check>) {
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
