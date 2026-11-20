//! Import entry points: clipboard, file, QR and subscriptions.

use npvt_core::vault;

use crate::engine_bridge::Ev;
use crate::net;
use crate::vault_actions;

use super::{App, Modal};

impl App {
    pub fn import_clipboard(&mut self) {
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) => t,
            Err(_) => {
                self.toast_now("Clipboard not available");
                return;
            }
        };
        if text.trim().is_empty() {
            self.toast_now("Clipboard is empty");
            return;
        }
        let report = vault_actions::import_links(&mut self.vault, &text, None);
        self.persist_vault();
        self.push_log(
            "INFO",
            &format!(
                "clipboard import: {} ok, {} failed",
                report.imported.len(),
                report.failed.len()
            ),
        );
        self.toast_now(report.summary());
    }

    pub fn import_file_dialog(&mut self) {
        match super::pick_config_file() {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => {
                    let fallback = vault::label_from_filename(&path);
                    match vault_actions::import_file_bytes(&mut self.vault, &bytes, &fallback) {
                        Ok(name) => {
                            self.persist_vault();
                            self.push_log(
                                "INFO",
                                &format!("imported file '{fallback}' as '{name}'"),
                            );
                            self.toast_now(format!("Imported {name}"));
                        }
                        Err(vault_actions::FileImportError::NeedsUnlock(msg)) => {
                            self.push_log("INFO", &msg);
                            self.ms.pending_locked = Some(bytes);
                            self.ms.pass_entry.clear();
                            self.ms.pass_err = None;
                            self.open_modal(Modal::Passphrase);
                        }
                        Err(vault_actions::FileImportError::Other(e)) => {
                            self.toast_now(format!("Import failed: {e}"));
                        }
                    }
                }
                Err(e) => self.toast_now(format!("Cannot read file: {e}")),
            },
            None => {
                self.toast_now(
                    "No file selected (dialogs need the Windows build; paste a link instead)",
                );
            }
        }
    }

    pub fn import_qr(&mut self) {
        match super::pick_image_file() {
            Some(path) => match net::decode_qr_file(&path) {
                Ok(payloads) if payloads.is_empty() => {
                    self.toast_now("No QR code found in the image");
                }
                Ok(payloads) => {
                    let mut report = vault_actions::ImportReport::default();
                    for payload in payloads {
                        let sub = vault_actions::import_links(&mut self.vault, &payload, None);
                        report.imported.extend(sub.imported);
                        report.failed.extend(sub.failed);
                    }
                    self.persist_vault();
                    self.toast_now(report.summary());
                }
                Err(e) => self.toast_now(format!("QR decode failed: {e}")),
            },
            None => {
                self.toast_now(
                    "No image selected (dialogs need the Windows build; paste a link instead)",
                );
            }
        }
    }

    pub fn fetch_subscription_async(&mut self, url: String, group: String) {
        let tx = self.ev_tx.clone();
        std::thread::Builder::new()
            .name("npvt-subscription".into())
            .spawn(move || {
                let result = net::fetch_subscription(&url)
                    .map(|(info, body)| (info, net::extract_links(&body)));
                let _ = tx.send(Ev::SubscriptionDone { group, result });
            })
            .ok();
    }

    pub fn handle_subscription_done(
        &mut self,
        group: String,
        result: Result<(net::SubInfoAlias, Vec<String>), String>,
    ) {
        self.ms.sub_busy = false;
        match result {
            Ok((info, links)) => {
                if links.is_empty() {
                    self.toast_now("Subscription returned no links");
                    return;
                }
                let mut imported = 0;
                let mut failed = 0;
                for link in &links {
                    match vault_actions::import_one(&mut self.vault, link, Some(&group)) {
                        Ok(_) => imported += 1,
                        Err(_) => failed += 1,
                    }
                }
                self.persist_vault();
                if imported > 0 {
                    let sub = crate::settings::SubInfo {
                        upload: info.upload,
                        download: info.download,
                        total: info.total,
                        expire: info.expire,
                    };
                    self.settings.subscriptions.insert(group.clone(), sub);
                    if !self.settings.extra_groups.contains(&group) {
                        self.settings.extra_groups.push(group.clone());
                    }
                    self.settings.save();
                }
                self.push_log(
                    "INFO",
                    &format!("subscription '{group}': {imported} imported, {failed} failed"),
                );
                self.toast_now(format!("Subscription '{group}': {imported} configs"));
                self.close_modal();
            }
            Err(e) => {
                self.toast_now(format!("Subscription failed: {e}"));
            }
        }
    }
}
