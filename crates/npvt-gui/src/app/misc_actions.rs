//! Asset downloads, log views, event pump and background ticks.

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::engine_bridge::{self, Ev};
use crate::net;
use crate::theme;

use super::App;

impl App {
    pub fn start_asset_download(&mut self, which: usize, name: &str) {
        let Ok(dir) = npvt_core::engine::work_dir() else {
            self.toast_now("Data directory unavailable");
            return;
        };
        let name = name.to_owned();
        self.asset_busy = true;
        self.asset_which = which;
        self.asset_status = format!("downloading {name}...");
        let progress = self.asset_progress.clone();
        let tx = self.ev_tx.clone();
        let url = if which == 0 {
            net::GEOIP_URL
        } else {
            net::GEOSITE_URL
        }
        .to_owned();
        let dest = dir.join(&name);
        std::thread::Builder::new()
            .name("npvt-asset".into())
            .spawn(move || {
                let _ = tx.send(Ev::AssetStep {
                    which,
                    name: name.to_owned(),
                });
                let err = net::download_geo_asset(&url, &dest, &progress).err();
                let _ = tx.send(Ev::AssetDone { which, err });
            })
            .ok();
    }

    pub fn open_log_folder(&mut self) {
        if let Ok(dir) = npvt_core::engine::work_dir() {
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("explorer")
                    .arg(dir.to_string_lossy().to_string())
                    .spawn();
            }
            #[cfg(not(windows))]
            {
                self.toast_now(dir.to_string_lossy().to_string());
            }
        }
    }

    pub fn refresh_engine_tail(&mut self) {
        if let Ok(dir) = npvt_core::engine::work_dir() {
            let log = dir.join("npvtunnel-gui-engine.log");
            let tail = npvt_core::engine::log_tail(&log, 120);
            self.engine_tail = tail
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| format!("[engine] {l}"))
                .collect();
        }
    }

    pub fn visible_log_lines(&self) -> Vec<String> {
        let filter = self.log_filter.to_lowercase();
        let mut lines: Vec<String> = self.event_log.clone();
        lines.extend(self.engine_tail.iter().cloned());
        if filter.is_empty() {
            lines
        } else {
            lines
                .into_iter()
                .filter(|l| l.to_lowercase().contains(&filter))
                .collect()
        }
    }

    pub fn handle_event(&mut self, ev: Ev) {
        match ev {
            Ev::Started(pid) => {
                self.connecting_since = Some(self.time);
                self.push_log("INFO", &format!("engine started (pid {pid})"));
            }
            Ev::TrafficLive => {
                self.connecting_since = None;
                self.bridge
                    .shared()
                    .set_state(engine_bridge::STATE_CONNECTED);
                self.push_log("INFO", "tunnel established (SOCKS5 127.0.0.1)");
            }
            Ev::Died { tail } => {
                self.connecting_since = None;
                self.push_log("ERROR", "engine exited unexpectedly");
                for line in tail.lines().take(12) {
                    self.push_log("ENGINE", line);
                }
            }
            Ev::StartFailed(err) => {
                self.connecting_since = None;
                self.push_log("ERROR", &format!("start failed: {err}"));
                self.toast_now("Connect failed - see LOGS");
            }
            Ev::EngineMissing => {
                self.connecting_since = None;
                self.push_log("ERROR", "sing-box engine not found");
                self.toast_now("sing-box engine not found - see LOGS");
            }
            Ev::Stopped => {
                self.connecting_since = None;
                self.push_log("INFO", "disconnected");
            }
            Ev::Ping { id, ms } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| p.id == id) {
                    let label = match ms {
                        Some(v) => format!("{} ms", v),
                        None => "timeout".to_owned(),
                    };
                    self.push_log("INFO", &format!("ping '{}': {label}", profile.name));
                }
                self.pings.insert(id, ms);
            }
            Ev::SubscriptionDone { group, result } => {
                self.handle_subscription_done(group, result);
            }
            Ev::AssetStep { which, name } => {
                self.asset_busy = true;
                self.asset_which = which;
                self.asset_status = format!("downloading {name}...");
            }
            Ev::AssetDone { which, err } => {
                self.asset_busy = false;
                match err {
                    None => {
                        self.asset_status = format!(
                            "{} saved to the npvtunnel data folder",
                            if which == 0 { "geoip.db" } else { "geosite.db" }
                        );
                        self.push_log("INFO", &self.asset_status.clone());
                        self.toast_now("Asset downloaded");
                    }
                    Some(e) => {
                        self.asset_status = e.clone();
                        self.push_log("ERROR", &format!("asset download failed: {e}"));
                    }
                }
            }
        }
    }

    pub(crate) fn poll_events(&mut self) {
        for ev in self.bridge.drain() {
            self.handle_event(ev);
        }
        // Fallback: connecting for >6s with a live engine counts as connected.
        if let Some(since) = self.connecting_since {
            if self.time - since > 6.0
                && self.bridge.shared().state() == engine_bridge::STATE_CONNECTING
            {
                self.connecting_since = None;
                self.bridge
                    .shared()
                    .set_state(engine_bridge::STATE_CONNECTED);
                self.push_log("INFO", "tunnel assumed established");
            }
        }
    }

    pub(crate) fn persist_traffic_tick(&mut self) {
        if self.time - self.last_persist < 12.0 {
            return;
        }
        self.last_persist = self.time;
        let lifetime = self.bridge.shared().lifetime_down.load(Ordering::Relaxed);
        let delta = lifetime.saturating_sub(self.persisted_lifetime_down);
        if delta > 0 {
            self.persisted_lifetime_down = lifetime;
            self.settings.roll_month_if_needed();
            self.settings.monthly_down += delta;
            self.settings.save();
        }
    }

    pub(crate) fn theme_tick(&mut self, ctx: &egui::Context) {
        let resolved = theme::resolve(self.settings.theme);
        // Re-apply only when something changed (cheap equality check).
        if resolved.bg != self.palette.bg || resolved.accent != self.palette.accent {
            self.palette = resolved;
            theme::apply(ctx, self.settings.theme);
        }
    }
}
