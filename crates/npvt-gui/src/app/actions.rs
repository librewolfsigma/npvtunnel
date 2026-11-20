//! Connection, group and settings actions.

use npvt_core::profile::Profile;
use npvt_core::vault;

use crate::engine_bridge::{self, Cmd, StartSpec};
use crate::vault_actions;

use super::{App, Modal};

impl App {
    pub fn toggle_connection(&mut self) {
        let state = self.bridge.shared().state();
        if state == engine_bridge::STATE_CONNECTED || state == engine_bridge::STATE_CONNECTING {
            self.disconnect();
        } else {
            self.connect();
        }
    }

    pub fn connect(&mut self) {
        let Some(active_id) = self.settings.active_id.clone() else {
            self.toast_now("Select a config in CONFIGS first");
            return;
        };
        let Some(active) = self.vault.profiles.iter().find(|p| p.id == active_id) else {
            self.toast_now("The selected config no longer exists");
            return;
        };
        let label = vault_actions::group_label(active);
        let members: Vec<Profile> = self
            .members_of_group(&label)
            .into_iter()
            .filter(|p| p.protocol.engine_supported())
            .collect();
        if members.is_empty() {
            self.toast_now("No engine-compatible config in this group");
            return;
        }
        let spec = StartSpec {
            members,
            active_id,
            socks_port: self.settings.socks_port,
            clash_port: self.settings.clash_port,
            dns_server: self.settings.dns_server.clone(),
            log_level: self.settings.log_level.clone(),
            per_app_mode: self.settings.per_app_mode,
            per_app: self.settings.per_app.clone(),
            engine_path: self.settings.engine_path.clone(),
            tun_mode: self.settings.tun_mode,
            tun_address: self
                .settings
                .tun_mode
                .then(|| self.settings.tun_address.clone()),
            kill_switch: self.settings.kill_switch,
        };
        self.bridge.send(Cmd::Start(spec));
        self.push_log("INFO", &format!("connecting to '{label}'..."));
    }

    pub fn disconnect(&mut self) {
        self.bridge.send(Cmd::Stop);
        self.push_log("INFO", "disconnecting...");
    }

    pub fn ping_all(&mut self) {
        let ids: Vec<(String, String, u16)> = self
            .vault
            .profiles
            .iter()
            .map(|p| (p.id.clone(), p.server.clone(), p.port))
            .collect();
        let count = ids.len();
        for (id, _, _) in &ids {
            self.pings.insert(id.clone(), None);
        }
        self.last_ping_at = Some(self.time);
        engine_bridge::spawn_ping_worker(self.ev_tx.clone(), ids);
        self.push_log("INFO", &format!("pinging {count} configs..."));
    }

    pub fn ping_group(&mut self, label: &str) {
        let ids: Vec<(String, String, u16)> = self
            .members_of_group(label)
            .iter()
            .map(|p| (p.id.clone(), p.server.clone(), p.port))
            .collect();
        for (id, _, _) in &ids {
            self.pings.insert(id.clone(), None);
        }
        self.last_ping_at = Some(self.time);
        engine_bridge::spawn_ping_worker(self.ev_tx.clone(), ids);
    }

    pub fn activate_group(&mut self, label: &str) {
        let members = self.members_of_group(label);
        if members.is_empty() {
            self.toast_now("Group has no configs");
            return;
        }
        // Prefer the fastest member we know about.
        let best = members
            .iter()
            .filter_map(|p| {
                self.pings
                    .get(&p.id)
                    .and_then(|ping| ping.map(|ms| (ms, p.id.clone())))
            })
            .min_by_key(|(ms, _)| *ms)
            .map(|(_, id)| id)
            .unwrap_or_else(|| members[0].id.clone());
        self.set_active_profile(&best);
        self.toast_now(format!("Active group: {label}"));
        self.push_log("INFO", &format!("active group set to '{label}'"));
    }

    pub fn delete_profile(&mut self, id: &str) {
        match vault::remove_profile(&mut self.vault, id) {
            Ok(profile) => {
                if self.settings.active_id.as_deref() == Some(id) {
                    self.settings.active_id = None;
                }
                self.pings.remove(id);
                self.persist_vault();
                self.toast_now(format!("Removed {}", profile.name));
                self.push_log("INFO", &format!("removed '{}'", profile.name));
            }
            Err(e) => self.toast_now(format!("Remove failed: {e}")),
        }
    }

    pub fn delete_group(&mut self, label: &str) {
        let members = self.members_of_group(label);
        for member in &members {
            let _ = vault::remove_profile(&mut self.vault, &member.id);
            if self.settings.active_id.as_deref() == Some(&member.id) {
                self.settings.active_id = None;
            }
            self.pings.remove(&member.id);
        }
        self.settings.extra_groups.retain(|l| l != label);
        self.settings.subscriptions.remove(label);
        self.expanded.remove(label);
        self.persist_vault();
        self.settings.save();
        self.toast_now(format!("Deleted group '{label}'"));
        self.push_log(
            "INFO",
            &format!("deleted group '{label}' ({} configs)", members.len()),
        );
    }

    pub fn rename_group(&mut self, label: &str, new_name: &str) {
        for profile in &mut self.vault.profiles {
            if vault_actions::group_label(profile) == label {
                profile.group = if new_name == "Saved" {
                    None
                } else {
                    Some(new_name.to_owned())
                };
            }
        }
        if let Some(pos) = self.settings.extra_groups.iter().position(|l| l == label) {
            self.settings.extra_groups[pos] = new_name.to_owned();
        }
        if let Some(info) = self.settings.subscriptions.remove(label) {
            self.settings
                .subscriptions
                .insert(new_name.to_owned(), info);
        }
        if self.expanded.remove(label) {
            self.expanded.insert(new_name.to_owned());
        }
        self.persist_vault();
        self.settings.save();
        self.toast_now("Group renamed");
    }

    pub fn confirm_delete_profile(&mut self, id: &str) {
        self.open_modal(Modal::ConfirmDeleteProfile { id: id.to_owned() });
    }

    pub fn confirm_delete_group(&mut self, label: &str) {
        self.open_modal(Modal::ConfirmDeleteGroup {
            label: label.to_owned(),
        });
    }

    pub fn apply_profile_edit(&mut self, id: &str) {
        let name = self.ms.edit_name.trim().to_owned();
        let group_raw = self.ms.edit_group.trim().to_owned();
        let group = match group_raw.as_str() {
            "" | "Saved" => None,
            g => Some(g.to_owned()),
        };
        if let Err(e) = vault::clean_name_input(&name) {
            self.toast_now(format!("Invalid name: {e}"));
            return;
        }
        if let Some(profile) = self.vault.profiles.iter_mut().find(|p| p.id == id) {
            profile.name = name;
            profile.group = group;
            profile.udp = self.ms.edit_udp;
            profile.allow_insecure = self.ms.edit_insecure;
            if let Err(e) = profile.validate() {
                self.toast_now(format!("Invalid profile: {e}"));
                return;
            }
        }
        self.persist_vault();
        self.toast_now("Saved");
    }

    pub fn apply_settings_dialog(&mut self) {
        let socks: u16 = self.ms.set_socks.trim().parse().unwrap_or(0);
        let clash: u16 = self.ms.set_clash.trim().parse().unwrap_or(0);
        let quota: u32 = self.ms.set_quota.trim().parse().unwrap_or(0);
        if !(1024..=65535).contains(&socks) || !(1024..=65535).contains(&clash) {
            self.toast_now("Ports must be 1024-65535");
            return;
        }
        let dns = self.ms.set_dns.trim().to_owned();
        if dns.is_empty() {
            self.toast_now("DNS server must not be empty");
            return;
        }
        let engine = self.ms.set_engine.trim().to_owned();
        if !engine.is_empty() && !std::path::Path::new(&engine).is_file() {
            self.toast_now("Engine path does not exist");
            return;
        }
        if self.settings.autostart != self.ms.set_autostart {
            if let Err(e) = crate::settings::set_autostart(self.ms.set_autostart) {
                self.toast_now(format!("Autostart failed: {e}"));
            }
        }
        self.settings.socks_port = socks;
        self.settings.clash_port = clash;
        self.settings.dns_server = dns;
        self.settings.quota_gb = quota;
        self.settings.engine_path = if engine.is_empty() {
            None
        } else {
            Some(engine)
        };
        self.settings.autostart = self.ms.set_autostart;
        self.settings.connect_on_launch = self.ms.set_launch;
        self.settings.log_level =
            ["warn", "info", "error", "debug"][self.ms.set_loglevel].to_owned();
        // TUN address validation: an invalid CIDR would render a config the
        // engine rejects at start, so catch it here where the user sees it.
        let tun_addr = self.ms.set_tun_addr.trim().to_owned();
        if let Err(msg) = validate_tun_address(&tun_addr) {
            self.toast_now(format!("TUN address: {msg}"));
            return;
        }
        self.settings.tun_mode = self.ms.set_tun;
        self.settings.tun_address = tun_addr;
        self.settings.kill_switch = self.ms.set_kill;
        // File association is applied immediately on Save: newly enabled
        // writes the per-user registry keys (skipped when the extensions
        // already point at this install), failures keep the old value.
        if self.ms.set_assoc && !self.settings.file_assoc && !crate::assoc::is_associated() {
            match crate::assoc::associate() {
                Ok(()) => {
                    self.settings.file_assoc = true;
                    self.push_log("INFO", ".npvt / .npvs files associated with NPVTunnel");
                }
                Err(e) => {
                    self.toast_now(format!("Association failed: {e}"));
                }
            }
        }
        self.settings.save();
        self.push_log("INFO", "settings saved");
        self.toast_now("Settings saved");
        self.close_modal();
    }
}

/// Check a TUN interface address in CIDR form (`a.b.c.d/prefix`).
fn validate_tun_address(value: &str) -> Result<(), String> {
    let (addr, prefix) = value
        .split_once('/')
        .ok_or_else(|| "expected IPv4 CIDR like 10.255.0.1/24".to_string())?;
    let octets: Vec<u8> = addr
        .split('.')
        .map(|o| o.parse::<u8>())
        .collect::<Result<_, _>>()
        .map_err(|_| "address must be four dot-separated numbers 0-255".to_string())?;
    if octets.len() != 4 {
        return Err("address must have exactly four octets".to_string());
    }
    let prefix: u8 = prefix
        .parse()
        .map_err(|_| "prefix must be 0-32".to_string())?;
    if prefix > 32 {
        return Err("prefix must be 0-32".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_tun_address;

    #[test]
    fn valid_cidr_addresses_pass() {
        assert_eq!(validate_tun_address("10.255.0.1/24"), Ok(()));
        assert_eq!(validate_tun_address("192.168.1.1/16"), Ok(()));
    }

    #[test]
    fn malformed_cidr_addresses_fail() {
        assert!(validate_tun_address("10.255.0.1").is_err());
        assert!(validate_tun_address("10.255.0.1/33").is_err());
        assert!(validate_tun_address("999.0.0.1/24").is_err());
        assert!(validate_tun_address("10.255.0.1.5/24").is_err());
    }
}
