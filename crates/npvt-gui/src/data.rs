//! GUI-side state that is not the vault: settings, connection groups,
//! per-profile usage, and small Windows integrations (autostart registry,
//! dark-mode detection, opening URLs/folders without console flashes).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use npvt_core::profile::{new_profile_id, Profile};
use npvt_core::vault::VaultData;
use npvt_core::Result;
use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

// ---- Settings ------------------------------------------------------------

/// Per-app proxy configuration (sing-box `process_name` route rules).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerApp {
    /// "off" | "allow" | "deny".
    pub mode: String,
    /// Process names, e.g. "chrome.exe".
    pub processes: Vec<String>,
}

impl Default for PerApp {
    fn default() -> Self {
        PerApp {
            mode: "off".into(),
            processes: Vec::new(),
        }
    }
}

/// User settings persisted next to the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Appearance: Light / Dark / Auto (follows Windows).
    pub theme: ThemeMode,
    /// Local SOCKS5 inbound port.
    pub socks_port: u16,
    /// Bind the inbound on 0.0.0.0 instead of 127.0.0.1 (explicit consent).
    pub allow_lan: bool,
    /// Upstream DNS for the engine config.
    pub dns: String,
    /// sing-box clash API port for live traffic stats.
    pub api_port: u16,
    /// Launch NPVTunnel when Windows starts.
    pub autostart: bool,
    /// Connect the active group automatically at launch.
    pub autoconnect: bool,
    /// Per-app proxy rules.
    pub per_app: PerApp,
    /// Local points balance (earn from sponsor clips, spend on Premium).
    pub points: u64,
    /// Premium unlocked locally.
    pub premium: bool,
    /// Unix seconds of the last sponsor clip watch (cooldown gate).
    pub last_sponsor: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeMode::Dark,
            socks_port: npvt_core::engine::DEFAULT_SOCKS_PORT,
            allow_lan: false,
            dns: "1.1.1.1".into(),
            api_port: npvt_core::engine::DEFAULT_CLASH_PORT,
            autostart: false,
            autoconnect: false,
            per_app: PerApp::default(),
            points: 0,
            premium: false,
            last_sponsor: 0,
        }
    }
}

impl Settings {
    fn path() -> Result<PathBuf> {
        Ok(npvt_core::engine::work_dir()?.join("settings.json"))
    }

    pub fn load() -> Settings {
        match Settings::path().ok().and_then(|p| std::fs::read(p).ok()) {
            Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            None => Settings::default(),
        }
    }

    pub fn save(&self) {
        if let Ok(path) = Settings::path() {
            if let Ok(bytes) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }
}

// ---- Groups ----------------------------------------------------------------

/// How a group picks its active member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupMode {
    /// Lowest successful latency wins.
    Fastest,
    /// Rotate members on every connect.
    RoundRobin,
    /// The pinned member is used.
    Manual,
}

impl GroupMode {
    pub fn as_label(self) -> &'static str {
        match self {
            GroupMode::Fastest => "Fastest",
            GroupMode::RoundRobin => "Round robin",
            GroupMode::Manual => "Manual",
        }
    }
}

/// A user-defined connection group over profile ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub mode: GroupMode,
    pub members: Vec<String>,
    pub pinned: bool,
}

/// Persisted groups + selection + usage counters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupsStore {
    pub groups: Vec<Group>,
    pub active_group: Option<String>,
    /// Per-group manual member choice (group id -> profile id).
    pub manual_pick: BTreeMap<String, String>,
    /// Cumulative usage per profile id: (down, up) bytes.
    pub usage: BTreeMap<String, (u64, u64)>,
    /// Unix seconds of the last full ping sweep (LAST PING row).
    pub last_ping: Option<u64>,
    /// Round-robin rotation cursor.
    pub rr_cursor: BTreeMap<String, usize>,
    /// What engine is currently running (for re-adopting after restart).
    pub engine_note: Option<String>,
}

impl GroupsStore {
    fn path() -> Result<PathBuf> {
        Ok(npvt_core::engine::work_dir()?.join("groups.json"))
    }

    pub fn load() -> GroupsStore {
        let mut store = match GroupsStore::path().ok().and_then(|p| std::fs::read(p).ok()) {
            Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            None => GroupsStore::default(),
        };
        store.ensure_defaults();
        store
    }

    pub fn save(&self) {
        if let Ok(path) = GroupsStore::path() {
            if let Ok(bytes) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }

    /// Guarantee at least the Smart Route group exists and the selection is
    /// valid.
    pub fn ensure_defaults(&mut self) {
        if !self.groups.iter().any(|g| g.name == "Smart Route") {
            self.groups.insert(
                0,
                Group {
                    id: new_profile_id(),
                    name: "Smart Route".into(),
                    mode: GroupMode::Fastest,
                    members: Vec::new(),
                    pinned: false,
                },
            );
        }
        if self.active_group.is_none() {
            self.active_group = self.groups.first().map(|g| g.id.clone());
        }
        if let Some(active) = self.active_group.clone() {
            if !self.groups.iter().any(|g| g.id == active) {
                self.active_group = self.groups.first().map(|g| g.id.clone());
            }
        }
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn group_mut(&mut self, id: &str) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// Pinned groups first, then the rest in insertion order.
    pub fn display_order(&self) -> Vec<&Group> {
        let mut pinned: Vec<&Group> = self.groups.iter().filter(|g| g.pinned).collect();
        let mut rest: Vec<&Group> = self.groups.iter().filter(|g| !g.pinned).collect();
        pinned.sort_by_key(|a| a.name.to_lowercase());
        rest.sort_by_key(|a| a.name.to_lowercase());
        pinned.extend(rest);
        pinned
    }

    /// Profiles that belong to no group ("Saved").
    pub fn saved_ids(&self, data: &VaultData) -> Vec<String> {
        data.profiles
            .iter()
            .filter(|p| !self.groups.iter().any(|g| g.members.contains(&p.id)))
            .map(|p| p.id.clone())
            .collect()
    }

    /// Every profile id ("Global Network").
    pub fn all_ids(&self, data: &VaultData) -> Vec<String> {
        data.profiles.iter().map(|p| p.id.clone()).collect()
    }

    /// Total usage across everything: (down, up).
    pub fn total_usage(&self) -> (u64, u64) {
        let mut down = 0u64;
        let mut up = 0u64;
        for (d, u) in self.usage.values() {
            down = down.saturating_add(*d);
            up = up.saturating_add(*u);
        }
        (down, up)
    }
}

// ---- Formatting helpers -------------------------------------------------

/// `20480` -> `"20.0 KB"`, `21800000000` -> `"21.8 GB"`.
pub fn format_bytes(n: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1000.0 && unit + 1 < units.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// Live rate: `47700` -> `"47.7 MB"`.
pub fn format_rate(bytes_per_sec: f64) -> String {
    format_bytes(bytes_per_sec.round() as u64)
}

/// Secrecy slider positions, matching the reference gradient.
pub const SECRECY_LEVELS: [&str; 5] = ["NONE", "LOW", "MODERATE", "HIGH", "MAXIMUM"];

/// Default suggested filename for an export.
pub fn suggest_filename(profile_name: &str) -> String {
    let mut name: String = profile_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "config.npvt".into()
    } else {
        format!("{name}.npvt")
    }
}

// ---- Import helpers -------------------------------------------------------

/// Result of a bulk import.
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub added: usize,
    pub replaced: usize,
    pub skipped: Vec<String>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        let mut s = format!("Added {}, updated {}", self.added, self.replaced);
        if !self.skipped.is_empty() {
            s.push_str(&format!(", skipped {} (invalid)", self.skipped.len()));
        }
        s
    }
}

/// Insert profiles into the vault with name dedup + suffixing.
pub fn import_profiles(data: &mut VaultData, mut incoming: Vec<Profile>) -> ImportReport {
    let mut report = ImportReport::default();
    for mut p in incoming.drain(..) {
        if p.validate().is_err() {
            report.skipped.push(p.name.clone());
            continue;
        }
        // Duplicate protection: same protocol + server + port already stored
        // counts as a replace, otherwise ensure a unique display name.
        if let Some(existing) = data
            .profiles
            .iter()
            .find(|e| e.protocol == p.protocol && e.server == p.server && e.port == p.port)
        {
            p.id = existing.id.clone();
            p.name = existing.name.clone();
            let name_for_report = p.name.clone();
            match npvt_core::vault::upsert_profile(data, p) {
                Ok(true) => report.replaced += 1,
                Ok(false) => report.added += 1,
                Err(_) => report.skipped.push(name_for_report),
            }
            continue;
        }
        if data.profiles.iter().any(|e| e.name == p.name) {
            p.name = format!("{} ({})", p.name, npvt_core::vault::short_suffix());
        }
        let name_for_report = p.name.clone();
        match npvt_core::vault::upsert_profile(data, p) {
            Ok(_) => report.added += 1,
            Err(_) => report.skipped.push(name_for_report),
        }
    }
    report
}

/// Decode a subscription body: base64 blob or plain text with one link per
/// line. Returns the raw lines for the caller to parse.
pub fn decode_subscription(body: &str) -> Vec<String> {
    use base64::Engine as _;
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() > 24 {
        for engine_name in ["urlsafe-nopad", "urlsafe", "standard"] {
            let decoded = match engine_name {
                "urlsafe-nopad" => {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(compact.as_bytes())
                }
                "urlsafe" => base64::engine::general_purpose::URL_SAFE.decode(compact.as_bytes()),
                _ => base64::engine::general_purpose::STANDARD.decode(compact.as_bytes()),
            };
            if let Ok(bytes) = decoded {
                if let Ok(text) = String::from_utf8(bytes) {
                    if text.contains("://") {
                        return text
                            .lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty() && l.contains("://"))
                            .map(ToOwned::to_owned)
                            .collect();
                    }
                }
            }
        }
    }
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains("://"))
        .map(ToOwned::to_owned)
        .collect()
}

// ---- System integration ---------------------------------------------------

/// Run `reg query` / `reg add` / `reg delete` without ever flashing a
/// console (npvt-core::engine::hide_console applies to engine spawns; these
/// helpers spawn reg.exe directly, so they set the flag themselves).
fn reg(args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("reg");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "NPVTunnel";

/// Is the "start with Windows" registry entry present?
pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE]).is_some()
}

/// Enable or remove the "start with Windows" registry entry.
pub fn set_autostart(enable: bool) -> Result<()> {
    if enable {
        let exe = std::env::current_exe()
            .map_err(|e| npvt_core::Error::Io(std::io::Error::other(e.to_string())))?;
        let quoted = format!("\"{}\"", exe.display());
        reg(&[
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &quoted, "/f",
        ]);
    } else {
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    }
    Ok(())
}

/// Windows "Apps use light theme" preference, for Auto appearance.
pub fn system_prefers_light() -> bool {
    reg(&[
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "/v",
        "AppsUseLightTheme",
    ])
    .and_then(|out| {
        out.lines()
            .find_map(|line| {
                let rest = line.split_whitespace().last()?;
                if rest.starts_with("0x") {
                    Some(u32::from_str_radix(rest.trim_start_matches("0x"), 16).ok()?)
                } else {
                    None
                }
            })
            .map(|v| v != 0)
    })
    .unwrap_or(false)
}

/// Open a URL in the default browser (explorer.exe handles http(s) URLs and
/// never opens a console).
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

/// Reveal a path in Explorer.
pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use npvt_core::profile::{Credential, Protocol, Transport};

    fn profile(name: &str, server: &str) -> Profile {
        Profile {
            id: new_profile_id(),
            name: name.into(),
            protocol: Protocol::Trojan,
            server: server.into(),
            port: 443,
            credential: Credential::Password("pw".into()),
            transport: Transport::Tls,
            sni: None,
            path: None,
            allow_insecure: false,
            udp: true,
            group: None,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(20_300_000_000), "20.3 GB");
        assert_eq!(format_bytes(93_100_000_000), "93.1 GB");
    }

    #[test]
    fn import_dedupes_and_suffixes() {
        let mut data = VaultData::default();
        // a and a2 share protocol+server+port, so a2 replaces a (keeping a's
        // name); b is a different server and gets a name suffix.
        let a = profile("shop", "a.example");
        let b = profile("shop", "b.example");
        let a2 = profile("other", "a.example");
        let rep = import_profiles(&mut data, vec![a, b, a2]);
        assert_eq!(rep.added, 2);
        assert_eq!(rep.replaced, 1);
        assert_eq!(data.profiles.len(), 2);
        let again = import_profiles(&mut data, vec![profile("x", "a.example")]);
        assert_eq!(again.replaced, 1);
        assert_eq!(data.profiles.len(), 2);
        let third = import_profiles(&mut data, vec![profile("shop", "c.example")]);
        assert_eq!(third.added, 1);
        assert!(data.profiles[2].name.starts_with("shop ("));
    }

    #[test]
    fn subscription_decode_plain_and_b64() {
        let plain = decode_subscription("vless://a@h:1\nvless://b@h:2");
        assert_eq!(plain.len(), 2);
        use base64::Engine as _;
        let blob = base64::engine::general_purpose::STANDARD.encode("trojan://c@d:3?x=1\n#n");
        let decoded = decode_subscription(&blob);
        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].starts_with("trojan://"));
    }

    #[test]
    fn tracked_spaces() {
        assert_eq!(crate::theme::tracked("AB"), "A B");
        assert_eq!(crate::theme::tracked("A B"), "A   B");
    }

    #[test]
    fn suggest_filename_sane() {
        assert_eq!(suggest_filename("My Shop!!"), "My-Shop.npvt");
        assert_eq!(suggest_filename("///"), "config.npvt");
    }

    #[test]
    fn settings_roundtrip() {
        let s = Settings::default();
        let bytes = serde_json::to_vec(&s).expect("serialize");
        let back: Settings = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(s, back);
    }
}
