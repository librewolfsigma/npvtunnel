//! The GUI application state and frame orchestration.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::Context;

use npvt_core::profile::Profile;
use npvt_core::vault::{Vault, VaultData};

use crate::data::{GroupsStore, Settings};
use crate::engine_ctrl::{self, Shared, Status};
use crate::net::DownloadState;
use crate::theme::{self, Palette, ThemeMode};
use crate::widgets::{nav_bar, Page, Toasts};

// ---- Dialog state ----------------------------------------------------------

/// Draft for "Add config manually".
#[derive(Debug, Clone, Default)]
pub struct ManualDraft {
    pub name: String,
    pub protocol_idx: usize,
    pub server: String,
    pub port: String,
    pub cred: String,
    pub ss_method: String,
    pub transport_idx: usize,
    pub sni: String,
    pub path: String,
    pub udp: bool,
    pub insecure: bool,
}

impl ManualDraft {
    pub fn new() -> Self {
        ManualDraft {
            port: "443".into(),
            ..ManualDraft::default()
        }
    }
}

/// Which kind of row a kebab menu was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KebabKind {
    User,
    Saved,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Ping,
    Created,
}

/// Every modal the app can show.
#[derive(Default)]
pub enum Dialog {
    #[default]
    Closed,
    AddMenu,
    AddSubscription {
        url: String,
        busy: bool,
    },
    Manual(ManualDraft),
    NewGroup {
        name: String,
        mode: usize,
    },
    EditGroup {
        group_id: String,
        name: String,
        mode: usize,
        members: Vec<String>,
    },
    Rename {
        group_id: String,
        name: String,
    },
    ConfirmDeleteGroup {
        group_id: String,
    },
    ConfirmDeleteProfile {
        profile_id: String,
    },
    EditProfile {
        profile_id: String,
        draft: ManualDraft,
    },
    ImportFile {
        path: String,
        lock_kind: String,
        passphrase: String,
        error: Option<String>,
    },
    Premium,
    Points {
        watching: Option<Instant>,
    },
    SettingsDlg,
    PerApp {
        mode: usize,
        processes: String,
    },
    PublicKey,
    UserAssets {
        geoip: Option<Arc<Mutex<DownloadState>>>,
        geosite: Option<Arc<Mutex<DownloadState>>>,
        geoip_cancel: Arc<AtomicBool>,
        geosite_cancel: Arc<AtomicBool>,
    },
    Kebab {
        group_id: String,
        kind: KebabKind,
    },
    SortMenu,
    EngineMissing {
        message: String,
    },
}

/// The eframe application.
pub struct App {
    pub page: Page,
    pub vault: Vault,
    pub data: VaultData,
    pub groups: GroupsStore,
    pub settings: Settings,
    pub shared: Arc<Shared>,
    pub toasts: Toasts,
    pub palette: Palette,
    pub applied_theme: Option<ThemeMode>,
    pub dialog: Dialog,
    pub export: Option<crate::export::ExportState>,
    pub export_page: bool,
    pub search: String,
    pub search_open: bool,
    pub expanded: HashSet<String>,
    pub sort: SortKey,
    pub last_log_poll: Instant,
    pub last_persist: Instant,
    pub last_traffic_tick: Instant,
    pub adopted: bool,
    pub connecting: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);

        // Bottom navigation first so the content area shrinks to fit.
        egui::TopBottomPanel::bottom("nav")
            .frame(
                egui::Frame::default()
                    .fill(self.palette.window_bg)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| {
                nav_bar(ui, &self.palette, &mut self.page);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(self.palette.window_bg)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| {
                if self.export_page {
                    crate::export::show(self, ui);
                } else {
                    match self.page {
                        Page::Home => crate::pages::home::show(self, ui),
                        Page::Configs => crate::pages::configs::show(self, ui),
                        Page::Logs => crate::pages::logs::show(self, ui),
                        Page::More => crate::pages::more::show(self, ui),
                    }
                }
            });

        // Modals + toasts on top of everything.
        crate::dialogs::show(self, ctx);
        self.toasts.paint(ctx, &self.palette);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // The tunnel keeps running in the background by design (it is
        // re-adopted on the next launch); persist everything we changed.
        self.groups.save();
        self.settings.save();
        let _ = npvt_core::vault::Vault::open().and_then(|v| v.save(&self.data));
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        theme::set_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let vault =
            Vault::open().unwrap_or_else(|_| Vault::at(PathBuf::from("npvtunnel-vault.json")));
        let data = vault.load().unwrap_or_default();
        let groups = GroupsStore::load();
        let shared = Shared::new();

        let mut app = App {
            page: Page::Home,
            vault,
            data,
            groups,
            settings,
            shared,
            toasts: Toasts::default(),
            palette: theme::dark(),
            applied_theme: None,
            dialog: Dialog::Closed,
            export: None,
            export_page: false,
            search: String::new(),
            search_open: false,
            expanded: HashSet::new(),
            sort: SortKey::Name,
            last_log_poll: Instant::now() - Duration::from_secs(2),
            last_persist: Instant::now(),
            last_traffic_tick: Instant::now(),
            adopted: false,
            connecting: false,
        };
        // Re-adopt an engine left running by the CLI or a previous session.
        if engine_ctrl::adopt_running(&app.shared) {
            app.adopted = true;
            let note = app
                .groups
                .engine_note
                .clone()
                .unwrap_or_else(|| "Existing engine".to_string());
            app.shared.set_status(Status::Connected {
                profile_id: String::new(),
                label: note,
                since: 0,
            });
            engine_ctrl::spawn_traffic_poller(Arc::clone(&app.shared), app.settings.api_port);
        }
        if app.settings.autoconnect && !app.adopted {
            app.connect_active(&cc.egui_ctx);
        }
        app
    }

    /// Per-frame housekeeping: theme, liveness, usage accounting, log tail.
    fn tick(&mut self, ctx: &Context) {
        // 1. Resolve the wanted theme.
        let wanted = match self.settings.theme {
            ThemeMode::Auto => {
                if crate::data::system_prefers_light() {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                }
            }
            other => other,
        };
        if self.applied_theme != Some(wanted) {
            self.palette = match wanted {
                ThemeMode::Light => theme::light(),
                _ => theme::dark(),
            };
            theme::set_visuals(ctx, &self.palette);
            self.applied_theme = Some(wanted);
        }

        // 2. Engine liveness check while connected.
        let status = self.shared.current_status();
        if let Status::Connected { .. } = status {
            if let Some(pid) = self.shared.pid() {
                if !npvt_core::engine::alive(pid) {
                    let _ = engine_ctrl::stop_engine(&self.shared);
                    self.groups.engine_note = None;
                    self.toasts.error("Engine process died. Connection closed.");
                }
            }
        }

        // 2b. Surface engine-start failures (e.g. missing sing-box) as a
        // dedicated dialog.
        if let Ok(mut guard) = self.shared.last_error.lock() {
            if let Some(err) = guard.take() {
                if err.contains("not found") || err.contains("Download:") {
                    self.dialog = crate::app::Dialog::EngineMissing { message: err };
                } else {
                    self.toasts.error(err);
                }
            }
        }

        // 3. Usage accounting from live rates.
        if let Status::Connected { profile_id, .. } = self.shared.current_status() {
            if !profile_id.is_empty() {
                let dt = self.last_traffic_tick.elapsed().as_secs_f64();
                self.last_traffic_tick = Instant::now();
                let (down, up) = self.shared.rates();
                if dt > 0.0 && (down > 0.0 || up > 0.0) {
                    let entry = self
                        .groups
                        .usage
                        .entry(profile_id.clone())
                        .or_insert((0, 0));
                    entry.0 += (down * dt) as u64;
                    entry.1 += (up * dt) as u64;
                }
            }
        } else {
            self.last_traffic_tick = Instant::now();
        }

        // 4. Periodic persistence.
        if self.last_persist.elapsed() > Duration::from_secs(10) {
            self.last_persist = Instant::now();
            self.groups.save();
            self.settings.save();
        }

        // 5. Refresh the log tail while LOGS is visible or connected.
        if self.page == Page::Logs && self.last_log_poll.elapsed() > Duration::from_secs(1) {
            self.last_log_poll = Instant::now();
            engine_ctrl::refresh_log_tail(&self.shared);
        }

        // 6. Keep the loop alive at a modest rate (animations, toasts).
        ctx.request_repaint_after(Duration::from_millis(900));
    }

    pub fn save_all(&mut self) {
        if let Ok(()) = self.vault.save(&self.data) {
            self.groups.save();
        } else {
            self.toasts.error("Could not save the vault file");
        }
    }

    pub fn profile_map(&self) -> std::collections::BTreeMap<String, Profile> {
        self.data
            .profiles
            .iter()
            .map(|p| (p.id.clone(), p.clone()))
            .collect()
    }

    /// The group the app currently targets (user selection or first group).
    pub fn active_group(&self) -> Option<&crate::data::Group> {
        let id = self.groups.active_group.clone()?;
        self.groups.group(&id)
    }

    /// Resolve the profile a group currently points at.
    pub fn resolve_profile_id(&self, group: &crate::data::Group) -> Option<String> {
        let pings = self
            .shared
            .pings
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let profiles = self.profile_map();
        engine_ctrl::resolve_active(group, &self.groups, &profiles, &pings)
    }

    /// Connect the active group (background thread; UI stays responsive).
    pub fn connect_active(&mut self, ctx: &Context) {
        if self.connecting {
            return;
        }
        let Some(group) = self.active_group().cloned() else {
            self.toasts.error("Create a group first (CONFIGS > +)");
            return;
        };
        let members: Vec<String> = if group.id == "virtual:saved" || group.id == "virtual:global" {
            match group.id.as_str() {
                "virtual:global" => self.groups.all_ids(&self.data),
                _ => self.groups.saved_ids(&self.data),
            }
        } else {
            group.members.clone()
        };
        if members.is_empty() {
            self.toasts
                .error("The group has no members. Import a config first.");
            return;
        }
        let Some(active_id) = self.resolve_profile_id(&group) else {
            self.toasts.error("No profile to connect to");
            return;
        };
        let profiles: Vec<Profile> = members
            .iter()
            .filter_map(|id| self.data.profiles.iter().find(|p| &p.id == id))
            .cloned()
            .collect();
        let label = format!(
            "{} · {}",
            group.name,
            self.data
                .profiles
                .iter()
                .find(|p| p.id == active_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "?".into())
        );
        let settings = self.settings.clone();
        let shared = Arc::clone(&self.shared);
        self.connecting = true;
        let rr_group = if group.mode == crate::data::GroupMode::RoundRobin {
            Some(group.id.clone())
        } else {
            None
        };
        let ctx2 = ctx.clone();
        let label_for_thread = label.clone();
        std::thread::spawn(move || {
            if let Err(err) = engine_ctrl::start_engine(
                &shared,
                &profiles,
                &active_id,
                &settings,
                &label_for_thread,
            ) {
                if let Ok(mut guard) = shared.last_error.lock() {
                    *guard = Some(err);
                }
            }
            ctx2.request_repaint();
        });
        // Rotate Round robin cursor now (next connect uses the next member).
        if let Some(gid) = rr_group {
            let len = group.members.len();
            if len > 0 {
                let next = (self.groups.rr_cursor.get(&gid).copied().unwrap_or(0) + 1) % len;
                self.groups.rr_cursor.insert(gid, next);
            }
        }
        self.groups.engine_note = Some(label.clone());
        self.groups.save();
    }

    /// Disconnect and clean up.
    pub fn disconnect(&mut self) {
        if let Err(err) = engine_ctrl::stop_engine(&self.shared) {
            self.toasts.error(err);
        }
        self.connecting = false;
        self.groups.engine_note = None;
        self.groups.save();
    }

    /// Toggle the power button.
    pub fn toggle_power(&mut self, ctx: &Context) {
        match self.shared.current_status() {
            Status::Disconnected => self.connect_active(ctx),
            Status::Connecting { .. } => {}
            Status::Connected { .. } => self.disconnect(),
        }
    }

    /// Run a ping sweep over the given profile ids.
    pub fn ping_ids(&mut self, ids: Vec<String>, ctx: &Context) {
        if ids.is_empty() {
            self.toasts
                .error("Nothing to ping yet: import a config first");
            return;
        }
        let map = self.profile_map();
        engine_ctrl::ping_sweep(ids, map, Arc::clone(&self.shared), ctx.clone());
        self.groups.last_ping = Some(engine_ctrl::unix_now());
        self.groups.save();
    }

    /// Ping the members of a group (virtual groups included).
    pub fn ping_group(&mut self, group_id: &str, ctx: &Context) {
        let ids: Vec<String> = match group_id {
            "virtual:global" => self.groups.all_ids(&self.data),
            "virtual:saved" => self.groups.saved_ids(&self.data),
            id => self
                .groups
                .group(id)
                .map(|g| g.members.clone())
                .unwrap_or_default(),
        };
        self.ping_ids(ids, ctx);
    }
}
