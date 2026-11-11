//! EXPORT / SHARE page: a faithful port of reference image 3. Exports the
//! active profile of a group as a signed .npvt v2 container - open,
//! passphrase-locked or key-locked for specific recipient keys.

use npvt_core::npvt::v2::{self, Lock, Policy};

use crate::app::App;
use crate::data::{suggest_filename, SECRECY_LEVELS};
use crate::theme;
use crate::widgets::{self, fill_button, radio_row, secrecy_slider, tracked_label};

/// Export page state.
#[derive(Clone)]
pub struct ExportState {
    /// Profile being exported.
    pub profile_id: String,
    /// Title shown at the top (group/profile name).
    pub label: String,
    /// Suggested + user-edited filename.
    pub filename: String,
    /// "Lock this config?" radio: true = Lock it, false = Keep it open.
    pub lock_it: bool,
    /// Secrecy slider position (0..4) -> policy expiry.
    pub secrecy: usize,
    /// Deliver it with: true = Specific people, false = Passphrase.
    pub specific_people: bool,
    /// Recipient public keys textarea (one per line).
    pub recipients: String,
    /// Passphrase + confirmation.
    pub passphrase: String,
    pub confirm: String,
}

impl ExportState {
    /// Prepare an export for the active profile of `group_id`.
    pub fn for_group(app: &mut App, group_id: &str) -> Option<ExportState> {
        let group = match group_id {
            "virtual:global" | "virtual:saved" => None,
            id => app.groups.group(id).cloned(),
        };
        let profile_id = match &group {
            Some(g) => app.resolve_profile_id(g)?,
            None => {
                // Virtual groups: export the currently selected member.
                app.groups
                    .active_group
                    .as_deref()
                    .and_then(|gid| app.groups.manual_pick.get(gid).cloned())
                    .or_else(|| app.data.profiles.first().map(|p| p.id.clone()))?
            }
        };
        let profile = app
            .data
            .profiles
            .iter()
            .find(|p| p.id == profile_id)?
            .clone();
        let label = group
            .map(|g| g.name)
            .unwrap_or_else(|| profile.name.clone());
        Some(ExportState {
            profile_id,
            label,
            filename: suggest_filename(&profile.name),
            lock_it: true,
            secrecy: 2,
            specific_people: true,
            recipients: String::new(),
            passphrase: String::new(),
            confirm: String::new(),
        })
    }

    /// Expiry days implied by the secrecy level.
    pub fn secrecy_days(&self) -> Option<u64> {
        match self.secrecy {
            0 => None,      // NONE: no expiry
            1 => Some(365), // LOW
            2 => Some(180), // MODERATE
            3 => Some(90),  // HIGH
            _ => Some(30),  // MAXIMUM
        }
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette;
    let Some(mut state) = app.export.clone() else {
        app.export_page = false;
        return;
    };

    let (back, _) = widgets::header(ui, &p, true, false);
    if back {
        app.export_page = false;
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down_justified(egui::Align::Center),
            |ui| {
                ui.set_max_width(430.0);
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::sans_med(state.label.clone(), 13.0, p.text_dim));
                });
                ui.add_space(6.0);

                // Export Configuration button (disabled until a filename exists).
                let ready = !state.filename.trim().is_empty();
                if fill_button(ui, &p, "Export Configuration", ready).clicked() {
                    export_now(app);
                }
                ui.add_space(10.0);

                // Config Filename.
                let mut filename = state.filename.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut filename)
                        .hint_text("Config Filename")
                        .desired_width(ui.available_width())
                        .font(egui::TextStyle::Monospace),
                );
                state.filename = filename;

                ui.add_space(8.0);

                // Lock this config?
                ui.label(theme::sans_med("Lock this config?", 14.0, p.text));
                if radio_row(
                    ui,
                    &p,
                    state.lock_it,
                    "Lock it",
                    "Recipients can connect, but cannot view or edit the config.",
                )
                .clicked()
                {
                    state.lock_it = true;
                }
                if radio_row(
                    ui,
                    &p,
                    !state.lock_it,
                    "Keep it open",
                    "Recipients can view, edit and share the config.",
                )
                .clicked()
                {
                    state.lock_it = false;
                }

                ui.add_space(8.0);

                // Secrecy.
                ui.horizontal(|ui| {
                    ui.label(theme::sans_med("Secrecy", 14.0, p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let level = SECRECY_LEVELS[state.secrecy.min(4)];
                        let color = match state.secrecy {
                            0 | 1 => p.red,
                            2 => p.orange,
                            _ => p.green,
                        };
                        ui.label(theme::mono_med(level, 11.5, color));
                    });
                });
                secrecy_slider(ui, &p, &mut state.secrecy);
                let expires = match state.secrecy_days() {
                    Some(days) => format!("Expires {days} days after the recipients import it."),
                    None => "Never expires.".to_string(),
                };
                ui.label(theme::sans(expires, 11.5, p.text_dim));

                ui.add_space(10.0);

                // Deliver it with.
                ui.label(theme::sans_med("Deliver it with", 14.0, p.text));
                ui.add_space(2.0);
                ui.label(theme::sans_med(
                    "Serverless: saves a file",
                    11.5,
                    p.text_dim,
                ));
                if radio_row(
                    ui,
                    &p,
                    state.specific_people,
                    "Specific people",
                    "Enter one or more recipient public keys.",
                )
                .clicked()
                {
                    state.specific_people = true;
                }
                if radio_row(
                    ui,
                    &p,
                    !state.specific_people,
                    "Passphrase",
                    "Post the file anywhere; share the passphrase to open it.",
                )
                .clicked()
                {
                    state.specific_people = false;
                }
                ui.add_space(2.0);
                ui.label(theme::sans_med(
                    "Your server: gives a link",
                    11.5,
                    p.text_dim,
                ));
                // Share link stays disabled exactly like the reference.
                ui.horizontal(|ui| {
                    ui.label(theme::sans_med("Share link", 14.0, p.text_faint));
                    ui.label(theme::sans(
                        "Share link handles one config at a time.",
                        11.0,
                        p.text_faint,
                    ));
                });

                // Recipient keys / passphrase inputs.
                ui.add_space(8.0);
                if state.specific_people {
                    tracked_label(ui, &p, "RECIPIENT PUBLIC KEYS (ONE PER LINE)");
                    let mut recipients = state.recipients.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut recipients)
                            .hint_text("base64url X25519 public key")
                            .desired_rows(3)
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace),
                    );
                    state.recipients = recipients;
                } else if state.lock_it {
                    let mut pass = state.passphrase.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut pass)
                            .password(true)
                            .hint_text("Passphrase")
                            .desired_width(ui.available_width()),
                    );
                    state.passphrase = pass;
                    let mut confirm = state.confirm.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut confirm)
                            .password(true)
                            .hint_text("Repeat passphrase")
                            .desired_width(ui.available_width()),
                    );
                    state.confirm = confirm;
                } else {
                    ui.label(theme::sans(
                        "Open exports are signed but not encrypted.",
                        11.5,
                        p.text_dim,
                    ));
                }
                ui.add_space(24.0);
            },
        );
    });

    // Write the edited draft back.
    app.export = Some(state);
}

/// Build the lock + policy and write the container via a save dialog.
fn export_now(app: &mut App) {
    let Some(state) = app.export.clone() else {
        return;
    };
    let Some(profile) = app
        .data
        .profiles
        .iter()
        .find(|p| p.id == state.profile_id)
        .cloned()
    else {
        app.toasts.error("The profile no longer exists");
        return;
    };

    // Identity for signing.
    if app.data.identity.is_none() {
        app.data.identity = Some(npvt_core::vault::generate_identity());
    }
    let seed = app.data.identity.as_ref().expect("just set").ed25519_seed;
    let signer = v2::signing_key_from_seed(&seed);

    // Policy from the secrecy slider.
    let policy = Policy {
        expires_at: state
            .secrecy_days()
            .map(|days| npvt_core::profile::unix_now() + days * 86400),
        metered_only: false,
        group_hint: Some(state.label.clone()),
    };

    // Lock.
    let lock = if !state.lock_it {
        Lock::Open
    } else if state.specific_people {
        let mut keys = Vec::new();
        for line in state.recipients.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match v2::parse_recipient_key(line) {
                Ok(key) => keys.push(key),
                Err(e) => {
                    app.toasts.error(format!("Recipient key invalid: {e}"));
                    return;
                }
            }
        }
        if keys.is_empty() {
            app.toasts.error("Enter at least one recipient public key");
            return;
        }
        Lock::Keys(keys)
    } else {
        if state.passphrase.len() < 4 {
            app.toasts.error("Passphrase must be at least 4 characters");
            return;
        }
        if state.passphrase != state.confirm {
            app.toasts.error("Passphrases do not match");
            return;
        }
        Lock::Passphrase(state.passphrase.clone())
    };

    match v2::encode(&profile, &policy, &lock, Some(&signer)) {
        Ok(bytes) => {
            let filename = if state.filename.ends_with(".npvt") {
                state.filename.clone()
            } else {
                format!("{}.npvt", state.filename.trim())
            };
            let Some(path) = crate::pick::save_config(&filename) else {
                return;
            };
            match std::fs::write(&path, &bytes) {
                Ok(()) => {
                    app.toasts.show(format!("Exported {}", path.display()));
                    app.export_page = false;
                }
                Err(e) => app.toasts.error(format!("Write failed: {e}")),
            }
        }
        Err(e) => app.toasts.error(format!("Export failed: {e}")),
    }
}
