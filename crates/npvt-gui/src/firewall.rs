//! Kill switch via Windows Firewall rules.
//!
//! When the kill switch is active, all outbound traffic is blocked except
//! loopback and link-local destinations, so nothing leaks past a dead tunnel.
//! The rules are added when the engine starts with the kill switch enabled
//! and removed when the user disconnects. Everything runs through `netsh`,
//! which needs no elevation for per-user rules on default Windows installs.

#[cfg(windows)]
use std::process::Command;

/// Name of the kill switch firewall rule (exemption rules get `-local-<subnet>`
/// suffixes of this name so they can all be found again on removal). Windows
/// builds only: the rules are installed through netsh.
#[cfg(any(windows, test))]
const RULE_NAME: &str = "NPVTunnel Kill Switch";

/// Destinations that stay reachable while the kill switch is active:
/// IPv4/IPv6 loopback and the two link-local ranges the tunnel and the
/// clash API may use.
#[cfg(any(windows, test))]
const EXEMPT_SUBNETS: [&str; 4] = ["127.0.0.0/8", "::1/128", "169.254.0.0/16", "fe80::/10"];

/// `netsh` arguments for the rule that blocks all outbound traffic.
#[cfg(any(windows, test))]
fn block_rule_args() -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        format!("name={RULE_NAME}"),
        "dir=out".into(),
        "action=block".into(),
        "remoteip=Any".into(),
        "protocol=any".into(),
        "enable=yes".into(),
    ]
}

/// `netsh` arguments for one exemption rule (loopback / link-local traffic).
#[cfg(any(windows, test))]
fn allow_rule_args(subnet: &str) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        format!("name={RULE_NAME}-local-{subnet}"),
        "dir=out".into(),
        "action=allow".into(),
        format!("remoteip={subnet}"),
        "protocol=any".into(),
        "enable=yes".into(),
    ]
}

/// `netsh` arguments that delete one rule by name (`delete` succeeds even
/// when the rule does not exist, which makes removal idempotent).
#[cfg(any(windows, test))]
fn delete_rule_args(rule_name: &str) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "delete".into(),
        "rule".into(),
        format!("name={rule_name}"),
    ]
}

/// Add the kill switch rules: block all outbound traffic, then punch the
/// exemption holes for loopback and link-local. Idempotent - any rules from
/// a previous run are removed first.
#[cfg(windows)]
pub fn enable_kill_switch() -> Result<(), String> {
    let _ = disable_kill_switch();
    run_netsh(block_rule_args())?;
    for subnet in EXEMPT_SUBNETS {
        run_netsh(allow_rule_args(subnet))?;
    }
    Ok(())
}

/// Remove the kill switch rules. Best-effort for the exemption rules; the
/// result reflects the block rule deletion.
#[cfg(windows)]
pub fn disable_kill_switch() -> Result<(), String> {
    let res = run_netsh(delete_rule_args(RULE_NAME));
    for subnet in EXEMPT_SUBNETS {
        let _ = run_netsh(delete_rule_args(&format!("{RULE_NAME}-local-{subnet}")));
    }
    res
}

#[cfg(windows)]
fn run_netsh(args: Vec<String>) -> Result<(), String> {
    let status = Command::new("netsh")
        .args(&args)
        .status()
        .map_err(|e| format!("failed to run netsh: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("netsh {} failed", args.join(" ")))
    }
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
    fn block_rule_blocks_all_outbound_traffic() {
        let args = block_rule_args();
        assert!(args.contains(&"dir=out".to_owned()));
        assert!(args.contains(&"action=block".to_owned()));
        assert!(args.contains(&"remoteip=Any".to_owned()));
        assert!(args.contains(&format!("name={RULE_NAME}")));
    }

    #[test]
    fn every_exempt_subnet_gets_an_allow_rule() {
        for subnet in EXEMPT_SUBNETS {
            let args = allow_rule_args(subnet);
            assert!(args.contains(&"action=allow".to_owned()));
            assert!(args.contains(&format!("remoteip={subnet}")));
            // The exemption rule name must be derivable from the block rule
            // name, otherwise disable_kill_switch would leave it behind.
            assert!(args
                .iter()
                .any(|a| a.starts_with(&format!("name={RULE_NAME}-local-"))));
        }
    }

    #[test]
    fn delete_args_cover_block_and_exemption_rules() {
        let block = delete_rule_args(RULE_NAME);
        assert!(block.contains(&format!("name={RULE_NAME}")));
        for subnet in EXEMPT_SUBNETS {
            let exempt = delete_rule_args(&format!("{RULE_NAME}-local-{subnet}"));
            assert!(exempt.contains(&format!("name={RULE_NAME}-local-{subnet}")));
        }
    }
}
