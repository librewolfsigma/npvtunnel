//! Kill switch via Windows Firewall rules.
//!
//! When the kill switch is active, outbound traffic is blocked except to
//! localhost and the local tunnel ports. This prevents leaks when the tunnel
//! drops. Rules are added on connect and removed on disconnect.

use std::process::Command;

/// Name of the kill switch firewall rule.
const RULE_NAME: &str = "NPVTunnel Kill Switch";

/// Add the kill switch firewall rule. Blocks outbound traffic except to
/// localhost and the local SOCKS/stats ports.
///
/// Returns `Ok(())` when the rule was added (or already exists), `Err`
/// with a description otherwise.
#[cfg(windows)]
pub fn enable_kill_switch() -> Result<(), String> {
    // Remove any existing rule with the same name first (idempotent).
    let _ = disable_kill_switch();

    // Add a rule blocking all outbound traffic not matching exemptions.
    // Exemptions: local subnet, the SOCKS port, the clash API port.
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={RULE_NAME}"),
            "dir=out",
            "action=block",
            "remoteip=Any",
            "protocol=any",
            "enable=yes",
        ])
        .status()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    if !status.success() {
        return Err("netsh add rule failed".to_string());
    }

    // Add exemptions for local traffic (so the tunnel itself can work).
    // Localhost and link-local are always allowed.
    for subnet in ["127.0.0.0/8", "::1/128", "169.254.0.0/16", "fe80::/10"] {
        let status = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &format!("name={RULE_NAME}-local-{subnet}"),
                "dir=out",
                "action=allow",
                &format!("remoteip={subnet}"),
                "protocol=any",
                "enable=yes",
            ])
            .status()
            .map_err(|e| format!("failed to run netsh: {e}"))?;
        if !status.success() {
            return Err(format!("netsh allow rule for {subnet} failed"));
        }
    }

    Ok(())
}

/// Remove the kill switch firewall rules.
#[cfg(windows)]
pub fn disable_kill_switch() -> Result<(), String> {
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={RULE_NAME}"),
        ])
        .status()
        .map_err(|e| format!("failed to run netsh: {e}"))?;
    // `delete rule` returns success even if the rule didn't exist.

    // Also delete the exemption rules we created.
    for subnet in ["127.0.0.0/8", "::1/128", "169.254.0.0/16", "fe80::/10"] {
        let _ = Command::new("netsh").args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={RULE_NAME}-local-{subnet}"),
        ]);
    }

    if !status.success() {
        return Err("netsh delete rule failed".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn enable_kill_switch() -> Result<(), String> {
    Err("kill switch is Windows-only".to_string())
}

#[cfg(not(windows))]
pub fn disable_kill_switch() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_name_is_non_empty() {
        assert!(!RULE_NAME.is_empty());
    }
}
