//! Shared App helpers: modal lifecycle, logging, clipboard, vault IO.

use std::collections::BTreeSet;

use npvt_core::profile::Profile;
use npvt_core::vault;

use crate::vault_actions;
use crate::widgets;

use super::{App, Modal};

impl App {
    pub fn open_modal(&mut self, modal: Modal) {
        self.ms.sub_busy = false;
        self.ms.export.err = None;
        match &modal {
            Modal::Export { profile_id } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| &p.id == profile_id) {
                    self.ms.export.profile_id = profile_id.clone();
                    self.ms.export.filename = format!("{}.npvt", profile.name);
                    self.ms.export.lock = 0;
                    self.ms.export.deliver = 0;
                    self.ms.export.keys.clear();
                    self.ms.export.pass.clear();
                    self.ms.export.pass2.clear();
                    self.ms.export.secrecy = 0.34;
                }
            }
            Modal::EditProfile { id } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| &p.id == id) {
                    self.ms.edit_id = id.clone();
                    self.ms.edit_name = profile.name.clone();
                    self.ms.edit_group = profile.group.clone().unwrap_or_default();
                    self.ms.edit_udp = profile.udp;
                    self.ms.edit_insecure = profile.allow_insecure;
                }
            }
            Modal::RenameGroup { label } => {
                self.ms.group_name = label.clone();
            }
            Modal::Settings => {
                self.ms.set_socks = self.settings.socks_port.to_string();
                self.ms.set_clash = self.settings.clash_port.to_string();
                self.ms.set_dns = self.settings.dns_server.clone();
                self.ms.set_engine = self.settings.engine_path.clone().unwrap_or_default();
                self.ms.set_quota = self.settings.quota_gb.to_string();
                self.ms.set_autostart = self.settings.autostart;
                self.ms.set_launch = self.settings.connect_on_launch;
                self.ms.set_loglevel = ["warn", "info", "error", "debug"]
                    .iter()
                    .position(|l| *l == self.settings.log_level)
                    .unwrap_or(0);
            }
            Modal::PerApp => {
                self.ms.per_app_mode = self.settings.per_app_mode;
                self.ms.per_app_text = self.settings.per_app.join("\n");
            }
            Modal::PublicKey => {
                vault_actions::ensure_identity(&mut self.vault);
                self.persist_vault();
            }
            _ => {}
        }
        self.modal = Some(modal);
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    pub fn toast_now(&mut self, message: impl Into<String>) {
        self.toast = Some(widgets::Toast::new(message, self.time));
    }

    pub fn push_log(&mut self, level: &str, message: &str) {
        let secs = npvt_core::profile::unix_now();
        let stamp = format!(
            "[{:02}:{:02}:{:02}]",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60
        );
        self.event_log.push(format!("{stamp} {level} {message}"));
        if self.event_log.len() > 600 {
            self.event_log.remove(0);
        }
    }

    pub fn copy_to_clipboard(&mut self, text: String) {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())) {
            Ok(()) => self.toast_now("Copied to clipboard"),
            Err(_) => self.toast_now("Clipboard not available"),
        }
    }

    pub fn persist_vault(&mut self) {
        let path = vault::Vault::default_path();
        if let Ok(path) = path {
            let file = vault::Vault::at(path);
            if let Err(e) = file.save(&self.vault) {
                self.push_log("ERROR", &format!("vault save failed: {e}"));
            }
        }
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        let id = self.settings.active_id.as_deref()?;
        self.vault.profiles.iter().find(|p| p.id == id)
    }

    pub fn set_active_profile(&mut self, id: &str) {
        self.settings.active_id = Some(id.to_owned());
        self.settings.save();
    }

    pub fn members_of_group(&self, label: &str) -> Vec<Profile> {
        self.vault
            .profiles
            .iter()
            .filter(|p| vault_actions::group_label(p) == label)
            .cloned()
            .collect()
    }

    pub fn all_group_labels(&self) -> BTreeSet<String> {
        vault_actions::group_profiles(&self.vault)
            .into_iter()
            .map(|(l, _)| l)
            .chain(self.settings.extra_groups.iter().cloned())
            .collect()
    }

    pub fn last_ping_label(&self) -> String {
        match self.last_ping_at {
            None => "Never".to_owned(),
            Some(t) => {
                let elapsed = self.time - t;
                if elapsed < 60.0 {
                    format!("{}s ago", elapsed as u64)
                } else if elapsed < 3600.0 {
                    format!("{}m ago", (elapsed / 60.0) as u64)
                } else {
                    format!("{}h ago", (elapsed / 3600.0) as u64)
                }
            }
        }
    }
}
