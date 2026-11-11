//! Engine control for the GUI: turning a group selection into a running
//! sing-box sidecar, TCP latency probes and live traffic sampling.
//!
//! Everything that can block runs on background threads and publishes
//! results into the lock-protected [`Shared`] state; the UI only reads.

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use egui::Context;
use serde_json::{json, Value};

use npvt_core::engine;
use npvt_core::profile::Profile;

use crate::data::{Group, GroupMode, GroupsStore, PerApp, Settings};

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What the power button currently shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Status {
    #[default]
    Disconnected,
    Connecting {
        label: String,
    },
    Connected {
        profile_id: String,
        label: String,
        since: u64,
    },
}

/// One latency probe result.
#[derive(Debug, Clone, PartialEq)]
pub enum PingOutcome {
    Ms(u16),
    Timeout,
    Failed(String),
}

impl PingOutcome {
    pub fn short(&self) -> String {
        match self {
            PingOutcome::Ms(ms) => format!("{ms} ms"),
            PingOutcome::Timeout => "timeout".into(),
            PingOutcome::Failed(e) => format!("error: {e}"),
        }
    }
    pub fn color(&self) -> (u8, u8, u8) {
        match self {
            PingOutcome::Ms(ms) if *ms < 120 => (63, 217, 127),
            PingOutcome::Ms(ms) if *ms < 300 => (245, 158, 11),
            PingOutcome::Ms(_) => (248, 113, 113),
            PingOutcome::Timeout => (146, 160, 154),
            PingOutcome::Failed(_) => (146, 160, 154),
        }
    }
    pub fn rank(&self) -> u32 {
        match self {
            PingOutcome::Ms(ms) => *ms as u32,
            PingOutcome::Timeout => 100_000,
            PingOutcome::Failed(_) => 200_000,
        }
    }
}

/// State shared between the UI thread and the background workers.
#[derive(Default)]
pub struct Shared {
    pub status: Mutex<Status>,
    pub engine_pid: Mutex<Option<u32>>,
    /// (down, up) bytes per second from the clash API.
    pub rates: Mutex<(f64, f64)>,
    /// Last engine error surfaced to the user.
    pub last_error: Mutex<Option<String>>,
    pub pings: Mutex<BTreeMap<String, PingOutcome>>,
    pub pinging: AtomicBool,
    pub traffic_alive: AtomicBool,
    /// Live engine log tail, refreshed on demand.
    pub log_tail: Mutex<String>,
}

impl Shared {
    pub fn new() -> Arc<Shared> {
        Arc::new(Shared::default())
    }

    pub fn set_status(&self, status: Status) {
        if let Ok(mut guard) = self.status.lock() {
            *guard = status;
        }
    }

    pub fn current_status(&self) -> Status {
        self.status
            .lock()
            .map(|g| g.clone())
            .unwrap_or(Status::Disconnected)
    }

    pub fn rates(&self) -> (f64, f64) {
        self.rates.lock().map(|g| *g).unwrap_or((0.0, 0.0))
    }

    pub fn pid(&self) -> Option<u32> {
        self.engine_pid.lock().map(|g| *g).unwrap_or(None)
    }
}

/// TCP-connect latency probe; this is what the PING buttons run.
pub fn probe_once(profile: &Profile) -> PingOutcome {
    use std::str::FromStr;
    let addr = std::net::SocketAddr::from_str(&format!("{}:{}", profile.server, profile.port));
    let addr = match addr {
        Ok(a) => a,
        Err(_) => {
            // Host name: resolve first (connect_timeout needs a socket addr).
            use std::net::ToSocketAddrs;
            match (profile.server.as_str(), profile.port).to_socket_addrs() {
                Ok(mut addrs) => match addrs.next() {
                    Some(a) => a,
                    None => return PingOutcome::Failed("resolve failed".into()),
                },
                Err(e) => return PingOutcome::Failed(format!("resolve: {e}")),
            }
        }
    };
    let started = Instant::now();
    match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
        Ok(_) => {
            let ms = started.elapsed().as_millis();
            u16::try_from(ms)
                .map(PingOutcome::Ms)
                .unwrap_or(PingOutcome::Ms(u16::MAX))
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => PingOutcome::Timeout,
        Err(_) => PingOutcome::Timeout,
    }
}

/// Spawn a sequential sweep over `ids`; results stream into `shared.pings`.
pub fn ping_sweep(
    ids: Vec<String>,
    profiles: BTreeMap<String, Profile>,
    shared: Arc<Shared>,
    ctx: Context,
) {
    if shared.pinging.swap(true, Ordering::SeqCst) {
        return; // a sweep is already running
    }
    std::thread::spawn(move || {
        for id in ids {
            if let Some(p) = profiles.get(&id) {
                let outcome = probe_once(p);
                if let Ok(mut guard) = shared.pings.lock() {
                    guard.insert(id.clone(), outcome);
                }
                ctx.request_repaint();
            }
        }
        shared.pinging.store(false, Ordering::SeqCst);
        ctx.request_repaint();
    });
}

/// Pick the profile a group currently resolves to.
pub fn resolve_active(
    group: &Group,
    store: &GroupsStore,
    _profiles: &BTreeMap<String, Profile>,
    pings: &BTreeMap<String, PingOutcome>,
) -> Option<String> {
    match group.mode {
        GroupMode::Manual => store
            .manual_pick
            .get(&group.id)
            .cloned()
            .or_else(|| group.members.first().cloned()),
        GroupMode::Fastest => group
            .members
            .iter()
            .filter_map(|id| pings.get(id).map(|o| (o.rank(), id)))
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, id)| id.clone())
            .or_else(|| group.members.first().cloned()),
        GroupMode::RoundRobin => {
            let len = group.members.len();
            if len == 0 {
                return None;
            }
            let idx = store.rr_cursor.get(&group.id).copied().unwrap_or(0) % len;
            group.members.get(idx).cloned()
        }
    }
}

/// Build the final engine config for `active_id`: the core rendering plus
/// the GUI's clash API and per-app rules.
pub fn build_engine_config(
    profiles: &[Profile],
    active_id: &str,
    settings: &Settings,
) -> npvt_core::Result<Value> {
    let mut cfg = engine::config(profiles, active_id, settings.socks_port, &settings.dns)?;
    // Live traffic: expose the clash API on loopback only.
    cfg["experimental"] = json!({
        "clash_api": {
            "external_controller": format!("127.0.0.1:{}", settings.api_port),
            "default_mode": "rule"
        },
        "cache_file": { "enabled": false }
    });
    apply_per_app(&mut cfg, &settings.per_app, active_id, profiles);
    if settings.allow_lan {
        if let Some(inbound) = cfg["inbounds"].as_array_mut().and_then(|a| a.first_mut()) {
            inbound["listen"] = json!("0.0.0.0");
        }
    }
    Ok(cfg)
}

/// Translate the per-app proxy setting into sing-box route rules.
fn apply_per_app(cfg: &mut Value, per_app: &PerApp, active_id: &str, profiles: &[Profile]) {
    if per_app.mode == "off" || per_app.processes.is_empty() {
        return;
    }
    let names: Vec<String> = per_app
        .processes
        .iter()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if names.is_empty() {
        return;
    }
    let rules = match per_app.mode.as_str() {
        // Only the listed apps are tunneled; everything else goes direct.
        "allow" => vec![json!({
            "process_name": names,
            "outbound": active_id,
        })],
        // The listed apps bypass the tunnel.
        "deny" => vec![json!({
            "process_name": names,
            "outbound": "direct",
        })],
        _ => return,
    };
    if let Some(route) = cfg["route"].as_object_mut() {
        route.insert("rules".into(), Value::Array(rules));
        if per_app.mode == "allow" {
            // With an allow-list the default becomes direct.
            route.insert("final".into(), json!("direct"));
            // Keep a selector-style path so the tunneled apps can still reach
            // every configured outbound.
            let _ = profiles;
        }
    }
}

/// Adopt an engine left running by the CLI (pid file) so the GUI never
/// silently double-starts.
pub fn adopt_running(shared: &Shared) -> bool {
    let pid_path = match engine::work_dir() {
        Ok(dir) => dir.join("engine.pid"),
        Err(_) => return false,
    };
    let Ok(text) = std::fs::read_to_string(&pid_path) else {
        return false;
    };
    let pid: u32 = match text.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if engine::alive(pid) {
        if let Ok(mut guard) = shared.engine_pid.lock() {
            *guard = Some(pid);
        }
        shared.set_status(Status::Connected {
            profile_id: String::new(),
            label: "CLI engine".into(),
            since: 0,
        });
        true
    } else {
        let _ = std::fs::remove_file(&pid_path);
        false
    }
}

/// Start the engine for `active_id`. Returns Ok(label) on spawn success.
pub fn start_engine(
    shared: &Arc<Shared>,
    profiles: &[Profile],
    active_id: &str,
    settings: &Settings,
    active_label: &str,
) -> Result<(), String> {
    let profile: Profile = profiles
        .iter()
        .find(|p| p.id == active_id)
        .cloned()
        .ok_or_else(|| "the selected profile no longer exists".to_string())?;

    let bin = engine::find_engine_binary(None).ok_or_else(|| {
        "sing-box engine not found. Put sing-box.exe next to NPVTunnel.exe, in the \
         npvtunnel data folder under bin\\, or on PATH. Download: \
         https://github.com/SagerNet/sing-box/releases"
            .to_string()
    })?;

    shared.set_status(Status::Connecting {
        label: profile.name.clone(),
    });

    let cfg = build_engine_config(profiles, active_id, settings).map_err(|e| e.to_string())?;
    let workdir = engine::work_dir().map_err(|e| e.to_string())?;
    let cfg_path = workdir.join("config.json");
    let log_path = workdir.join("engine.log");
    engine::write_config(&cfg, &cfg_path).map_err(|e| e.to_string())?;

    match engine::start(&bin, &cfg_path, &log_path) {
        Ok(running) => {
            let pid = running.pid;
            let _ = std::fs::write(workdir.join("engine.pid"), pid.to_string());
            if let Ok(mut guard) = shared.engine_pid.lock() {
                *guard = Some(pid);
            }
            // Give the sidecar a beat, then make sure it did not die on a
            // bad config.
            std::thread::sleep(Duration::from_millis(700));
            if !engine::alive(pid) {
                let tail = engine::log_tail(&log_path, 12);
                shared.set_status(Status::Disconnected);
                if let Ok(mut guard) = shared.engine_pid.lock() {
                    *guard = None;
                }
                return Err(format!(
                    "engine exited immediately. Last log lines:\n{tail}"
                ));
            }
            spawn_traffic_poller(Arc::clone(shared), settings.api_port);
            shared.set_status(Status::Connected {
                profile_id: profile.id.clone(),
                label: active_label.to_string(),
                since: unix_now(),
            });
            if let Ok(mut guard) = shared.last_error.lock() {
                *guard = None;
            }
            Ok(())
        }
        Err(e) => {
            shared.set_status(Status::Disconnected);
            Err(format!("failed to start engine: {e}"))
        }
    }
}

/// Stop the engine (if any) and clean up the pid file.
pub fn stop_engine(shared: &Shared) -> Result<(), String> {
    let pid = shared.pid();
    shared.traffic_alive.store(false, Ordering::SeqCst);
    shared.set_status(Status::Disconnected);
    if let Some(pid) = pid {
        let result = engine::stop(pid);
        if let Ok(mut guard) = shared.engine_pid.lock() {
            *guard = None;
        }
        if let Ok(dir) = engine::work_dir() {
            let _ = std::fs::remove_file(dir.join("engine.pid"));
        }
        result.map_err(|e| e.to_string())?;
    }
    if let Ok(mut rates) = shared.rates.lock() {
        *rates = (0.0, 0.0);
    }
    Ok(())
}

/// Background sampler for the clash `/traffic` endpoint; also folds session
/// bytes into the group usage totals.
pub fn spawn_traffic_poller(shared: Arc<Shared>, api_port: u16) {
    if shared.traffic_alive.swap(true, Ordering::SeqCst) {
        return;
    }
    let url = format!("http://127.0.0.1:{api_port}/traffic");
    std::thread::spawn(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(900))
            .build();
        while shared.traffic_alive.load(Ordering::SeqCst) {
            let current = shared.current_status();
            if let Status::Connected { .. } = current {
                if let Ok(resp) = agent.get(&url).call() {
                    if let Ok(body) = resp.into_string() {
                        if let Ok(v) = serde_json::from_str::<Value>(&body) {
                            let down = v["down"].as_f64().unwrap_or(0.0);
                            let up = v["up"].as_f64().unwrap_or(0.0);
                            if let Ok(mut rates) = shared.rates.lock() {
                                *rates = (down, up);
                            }
                        }
                    }
                }
            } else {
                if let Ok(mut rates) = shared.rates.lock() {
                    *rates = (0.0, 0.0);
                }
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
    });
}

/// Refresh the shared log tail (called while LOGS is visible or connected).
pub fn refresh_log_tail(shared: &Shared) {
    if let Ok(dir) = engine::work_dir() {
        let text = engine::log_tail(&dir.join("engine.log"), 500);
        if let Ok(mut guard) = shared.log_tail.lock() {
            *guard = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Group, GroupMode, GroupsStore};
    use npvt_core::profile::{Credential, Protocol, Transport};

    fn profile(id: &str) -> Profile {
        Profile {
            id: id.into(),
            name: id.into(),
            protocol: Protocol::Vless,
            server: "example.com".into(),
            port: 443,
            credential: Credential::Uuid("11111111-2222-3333-4444-555555555555".into()),
            transport: Transport::Tls,
            sni: None,
            path: None,
            allow_insecure: false,
            udp: true,
            group: None,
            created_at: 1_700_000_000,
        }
    }

    fn group() -> Group {
        Group {
            id: "g1".into(),
            name: "Smart Route".into(),
            mode: GroupMode::Fastest,
            members: vec!["a".into(), "b".into()],
            pinned: false,
        }
    }

    #[test]
    fn fastest_prefers_lowest_ping() {
        let g = group();
        let store = GroupsStore::default();
        let profiles = BTreeMap::new();
        let mut pings = BTreeMap::new();
        pings.insert("a".to_string(), PingOutcome::Ms(180));
        pings.insert("b".to_string(), PingOutcome::Ms(45));
        assert_eq!(
            resolve_active(&g, &store, &profiles, &pings).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn fastest_falls_back_to_first_member() {
        let g = group();
        let store = GroupsStore::default();
        let profiles = BTreeMap::new();
        let pings = BTreeMap::new();
        assert_eq!(
            resolve_active(&g, &store, &profiles, &pings).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn manual_uses_pick() {
        let mut g = group();
        g.mode = GroupMode::Manual;
        let mut store = GroupsStore::default();
        store.manual_pick.insert("g1".into(), "b".into());
        let profiles = BTreeMap::new();
        let pings = BTreeMap::new();
        assert_eq!(
            resolve_active(&g, &store, &profiles, &pings).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn roundrobin_rotates() {
        let mut g = group();
        g.mode = GroupMode::RoundRobin;
        let mut store = GroupsStore::default();
        let profiles = BTreeMap::new();
        let pings = BTreeMap::new();
        assert_eq!(
            resolve_active(&g, &store, &profiles, &pings).as_deref(),
            Some("a")
        );
        store.rr_cursor.insert("g1".into(), 1);
        assert_eq!(
            resolve_active(&g, &store, &profiles, &pings).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn engine_config_has_clash_api_and_lan_guard() {
        let p = profile("a");
        let mut settings = Settings::default();
        let cfg = build_engine_config(std::slice::from_ref(&p), "a", &settings).expect("cfg");
        assert_eq!(
            cfg["experimental"]["clash_api"]["external_controller"],
            "127.0.0.1:9090"
        );
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        settings.allow_lan = true;
        let cfg = build_engine_config(std::slice::from_ref(&p), "a", &settings).expect("cfg2");
        assert_eq!(cfg["inbounds"][0]["listen"], "0.0.0.0");
    }

    #[test]
    fn per_app_deny_and_allow() {
        let p = profile("a");
        let settings = Settings {
            per_app: PerApp {
                mode: "deny".into(),
                processes: vec!["Chrome.exe".to_string(), " ".to_string()],
            },
            ..Settings::default()
        };
        let cfg = build_engine_config(std::slice::from_ref(&p), "a", &settings).expect("cfg");
        assert_eq!(cfg["route"]["rules"][0]["outbound"], "direct");
        assert_eq!(cfg["route"]["rules"][0]["process_name"][0], "chrome.exe");
        assert_eq!(cfg["route"]["final"], "a");

        let settings = Settings {
            per_app: PerApp {
                mode: "allow".into(),
                processes: vec!["chrome.exe".into()],
            },
            ..Settings::default()
        };
        let cfg = build_engine_config(std::slice::from_ref(&p), "a", &settings).expect("cfg2");
        assert_eq!(cfg["route"]["final"], "direct");
    }
}
