//! Persisted GUI settings (ports, theme, last profile), stored next to the
//! vault in the npvtunnel data directory.

use serde::{Deserialize, Serialize};

use crate::theme;

/// Theme preference shown in the More page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the Windows personalization setting.
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
        }
    }
}

/// All user-tweakable GUI state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub socks_port: u16,
    pub clash_port: u16,
    pub dns_server: String,
    pub last_profile_id: Option<String>,
    pub auto_ping_on_connect: bool,
    pub auto_connect_on_launch: bool,
    pub confirm_disconnect_on_exit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeMode::Auto,
            socks_port: npvt_core::engine::DEFAULT_SOCKS_PORT,
            clash_port: npvt_core::engine::DEFAULT_CLASH_PORT,
            dns_server: "1.1.1.1".to_string(),
            last_profile_id: None,
            auto_ping_on_connect: true,
            auto_connect_on_launch: false,
            confirm_disconnect_on_exit: true,
        }
    }
}

impl Settings {
    fn path() -> Result<std::path::PathBuf, String> {
        npvt_core::engine::work_dir()
            .map(|dir| dir.join("gui-settings.json"))
            .map_err(|e| e.to_string())
    }

    /// Load settings, falling back to defaults on any error (fresh installs,
    /// corrupt files) so the app always starts.
    pub fn load() -> Settings {
        match Self::path() {
            Ok(path) => std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    /// Best-effort save; failures surface in the UI as a toast, not a crash.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())
    }

    /// Resolve the palette for the current mode. `Auto` reads the Windows
    /// dark-mode registry key (defaults to dark elsewhere).
    pub fn palette(&self) -> (&'static theme::Palette, bool) {
        let light = match self.theme {
            ThemeMode::Dark => false,
            ThemeMode::Light => true,
            ThemeMode::Auto => system_prefers_light(),
        };
        if light {
            (&theme::LIGHT, false)
        } else {
            (&theme::DARK, true)
        }
    }
}

/// Windows: `HKCU\...\Themes\Personalize\AppsUseLightTheme == 1`.
#[cfg(windows)]
fn system_prefers_light() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn system_prefers_light() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_roundtrip() {
        let s = Settings::default();
        assert_eq!(s.socks_port, 2080);
        assert_eq!(s.clash_port, 19090);
        let json = serde_json::to_vec(&s).expect("serialize");
        let back: Settings = serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(back.socks_port, s.socks_port);
        assert_eq!(back.theme, s.theme);
        // Partial JSON (older schema) must merge with defaults.
        let partial: Settings =
            serde_json::from_slice(br#"{"socks_port": 3000}"#).expect("partial");
        assert_eq!(partial.socks_port, 3000);
        assert_eq!(partial.theme, ThemeMode::Auto);
    }
}
