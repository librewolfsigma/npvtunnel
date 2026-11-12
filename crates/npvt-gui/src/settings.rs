//! GUI-side persisted state: appearance, connection settings, traffic quota
//! accounting and subscription metadata. Stored as JSON next to the vault
//! (`%APPDATA%/npvtunnel/gui.json` on Windows).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

/// Traffic metadata captured from a subscription response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubInfo {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PerAppMode {
    #[default]
    Off,
    /// Listed apps bypass the tunnel (route directly).
    Block,
    /// Only listed apps use the tunnel.
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortMode {
    #[default]
    Name,
    Ping,
    Added,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub socks_port: u16,
    pub clash_port: u16,
    pub dns_server: String,
    pub engine_path: Option<String>,
    pub autostart: bool,
    pub connect_on_launch: bool,
    pub quota_gb: u32,
    pub active_id: Option<String>,
    pub per_app_mode: PerAppMode,
    pub per_app: Vec<String>,
    pub log_level: String,
    pub sort: SortMode,
    pub stats_month: String,
    pub monthly_down: u64,
    pub subscriptions: BTreeMap<String, SubInfo>,
    /// Empty connection groups the user created manually.
    pub extra_groups: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeMode::Dark,
            socks_port: npvt_core::engine::DEFAULT_SOCKS_PORT,
            clash_port: 19090,
            dns_server: "8.8.8.8".to_owned(),
            engine_path: None,
            autostart: false,
            connect_on_launch: false,
            quota_gb: 100,
            active_id: None,
            per_app_mode: PerAppMode::Off,
            per_app: Vec::new(),
            log_level: "warn".to_owned(),
            sort: SortMode::Name,
            stats_month: month_key(),
            monthly_down: 0,
            subscriptions: BTreeMap::new(),
            extra_groups: Vec::new(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join("npvtunnel").join("gui.json"))
}

fn month_key() -> String {
    // Compact YYYY-MM derived from unix time (UTC).
    let secs = npvt_core::profile::unix_now();
    let days = secs / 86_400;
    // Civil-from-days algorithm (Howard Hinnant), trimmed to what we need.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}")
}

impl Settings {
    pub fn load() -> Self {
        let path = match settings_path() {
            Some(p) => p,
            None => return Settings::default(),
        };
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        if let Some(path) = settings_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(bytes) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }

    /// Roll the monthly traffic counter when the calendar month changes.
    pub fn roll_month_if_needed(&mut self) {
        let now = month_key();
        if self.stats_month != now {
            self.stats_month = now;
            self.monthly_down = 0;
        }
    }

    pub fn quota_bytes(&self) -> u64 {
        u64::from(self.quota_gb) * 1024 * 1024 * 1024
    }
}

/// Windows: add/remove the app from the per-user Run key.
pub fn set_autostart(enable: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot resolve exe path: {e}"))?
            .to_string_lossy()
            .to_string();
        if enable {
            run_reg(&[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "NPVTunnel",
                "/t",
                "REG_SZ",
                "/d",
                &exe,
                "/f",
            ])
        } else {
            run_reg(&[
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "NPVTunnel",
                "/f",
            ])
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        Ok(())
    }
}

#[cfg(windows)]
fn run_reg(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run reg: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "reg failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_key_shape() {
        let key = month_key();
        assert_eq!(key.len(), 7);
        assert_eq!(key.as_bytes()[4], b'-');
        assert!(key.starts_with("20"));
    }

    #[test]
    fn roundtrip_default_settings() {
        let s = Settings::default();
        let bytes = serde_json::to_vec(&s).unwrap();
        let back: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.socks_port, s.socks_port);
        assert_eq!(back.theme, s.theme);
    }

    #[test]
    fn month_roll_resets_counter() {
        let mut s = Settings {
            stats_month: "1999-01".to_owned(),
            monthly_down: 42,
            ..Settings::default()
        };
        s.roll_month_if_needed();
        assert_eq!(s.monthly_down, 0);
        assert_eq!(s.stats_month, month_key());
    }
}
