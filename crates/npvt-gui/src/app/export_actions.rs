//! Export actions: container file and share link.

use crate::vault_actions::{self, ExportLock};

use super::App;

impl App {
    fn build_export_spec(&self) -> Result<vault_actions::ExportSpec, String> {
        let profile_id = self.ms.export.profile_id.clone();
        let Some(profile) = self.vault.profiles.iter().find(|p| p.id == profile_id) else {
            return Err("config not found".to_owned());
        };
        let lock = if self.ms.export.lock == 1 {
            ExportLock::Open
        } else if self.ms.export.deliver == 1 {
            ExportLock::Passphrase
        } else {
            ExportLock::Keys
        };
        let recipients = if lock == ExportLock::Keys {
            vault_actions::parse_recipient_keys(&self.ms.export.keys)?
        } else {
            Vec::new()
        };
        let passphrase = if lock == ExportLock::Passphrase {
            if self.ms.export.pass != self.ms.export.pass2 {
                return Err("passphrases do not match".to_owned());
            }
            Some(self.ms.export.pass.clone())
        } else {
            None
        };
        // Secrecy -> expiry: MINIMAL never expires, MODERATE 30 days,
        // MAXIMUM 24 hours.
        let expires_in = if self.ms.export.secrecy < 0.34 {
            None
        } else if self.ms.export.secrecy < 0.67 {
            Some(30u64 * 86_400)
        } else {
            Some(86_400)
        };
        Ok(vault_actions::ExportSpec {
            profile: profile.clone(),
            lock,
            expires_in,
            filename: self.ms.export.filename.clone(),
            recipients,
            passphrase,
        })
    }

    pub fn export_profile(&mut self) {
        let spec = match self.build_export_spec() {
            Ok(spec) => spec,
            Err(e) => {
                self.ms.export.err = Some(e);
                return;
            }
        };
        match vault_actions::export_container(&self.vault, &spec) {
            Ok(bytes) => {
                let filename = if spec.filename.trim().is_empty() {
                    "config.npvt".to_owned()
                } else {
                    spec.filename.trim().to_owned()
                };
                match super::save_export_file(&filename) {
                    Some(path) => match std::fs::write(&path, &bytes) {
                        Ok(()) => {
                            self.push_log(
                                "INFO",
                                &format!("exported '{}' to {}", filename, path.display()),
                            );
                            self.toast_now(format!("Exported {filename}"));
                            self.close_modal();
                        }
                        Err(e) => self.ms.export.err = Some(format!("write failed: {e}")),
                    },
                    None => {
                        self.ms.export.err = Some(
                            "no save location chosen (dialogs need the Windows build)".to_owned(),
                        );
                    }
                }
            }
            Err(e) => self.ms.export.err = Some(e),
        }
    }

    pub fn export_link_to_clipboard(&mut self) {
        let spec = match self.build_export_spec() {
            Ok(spec) => spec,
            Err(e) => {
                self.ms.export.err = Some(e);
                return;
            }
        };
        match vault_actions::export_container(&self.vault, &spec) {
            Ok(bytes) => {
                let link = vault_actions::container_to_link(&bytes);
                if link.len() > 60_000 {
                    self.ms.export.err = Some(
                        "container too large for a share link; save a file instead".to_owned(),
                    );
                    return;
                }
                self.copy_to_clipboard(link);
            }
            Err(e) => self.ms.export.err = Some(e),
        }
    }
}
