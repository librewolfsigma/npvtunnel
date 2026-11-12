//! The engine bridge: spawn/stop the sing-box sidecar, watch its liveness
//! and poll the clash API traffic endpoint for live downlink/uplink stats.
//! All blocking work happens on worker threads; the UI thread only reads
//! atomics and drains an event channel.

use std::io::BufRead;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use npvt_core::profile::Profile;

pub const STATE_DISCONNECTED: u8 = 0;
pub const STATE_CONNECTING: u8 = 1;
pub const STATE_CONNECTED: u8 = 2;
pub const STATE_ERROR: u8 = 3;

pub struct Shared {
    pub state: AtomicU8,
    pub up_bps: AtomicU64,
    pub down_bps: AtomicU64,
    pub up_total: AtomicU64,
    pub down_total: AtomicU64,
    /// Monotonic counters that survive disconnects (monthly accounting).
    pub lifetime_up: AtomicU64,
    pub lifetime_down: AtomicU64,
    pub pid: AtomicU32,
    pub stop_traffic: AtomicBool,
    pub stop_watch: AtomicBool,
    pub error: std::sync::Mutex<Option<String>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            state: AtomicU8::new(STATE_DISCONNECTED),
            up_bps: AtomicU64::new(0),
            down_bps: AtomicU64::new(0),
            up_total: AtomicU64::new(0),
            down_total: AtomicU64::new(0),
            lifetime_up: AtomicU64::new(0),
            lifetime_down: AtomicU64::new(0),
            pid: AtomicU32::new(0),
            stop_traffic: AtomicBool::new(false),
            stop_watch: AtomicBool::new(false),
            error: std::sync::Mutex::new(None),
        }
    }
}

impl Shared {
    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Relaxed)
    }

    pub fn set_state(&self, s: u8) {
        self.state.store(s, Ordering::Relaxed);
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn set_error(&self, msg: Option<String>) {
        if let Ok(mut guard) = self.error.lock() {
            *guard = msg;
        }
    }

    pub fn reset_session(&self) {
        self.up_bps.store(0, Ordering::Relaxed);
        self.down_bps.store(0, Ordering::Relaxed);
        self.up_total.store(0, Ordering::Relaxed);
        self.down_total.store(0, Ordering::Relaxed);
    }
}

/// Commands the UI thread can send to the bridge.
pub enum Cmd {
    Start(StartSpec),
    Stop,
}

pub struct StartSpec {
    pub members: Vec<Profile>,
    pub active_id: String,
    pub socks_port: u16,
    pub clash_port: u16,
    pub dns_server: String,
    pub log_level: String,
    pub per_app_mode: crate::settings::PerAppMode,
    pub per_app: Vec<String>,
    pub engine_path: Option<String>,
}

/// Events flowing back to the UI.
#[derive(Debug)]
pub enum Ev {
    Started(u32),
    TrafficLive,
    Died {
        tail: String,
    },
    StartFailed(String),
    EngineMissing,
    Stopped,
    Ping {
        id: String,
        ms: Option<u128>,
    },
    AssetStep {
        which: usize,
        name: String,
    },
    AssetDone {
        which: usize,
        err: Option<String>,
    },
    SubscriptionDone {
        group: String,
        result: Result<(crate::net::SubInfoAlias, Vec<String>), String>,
    },
}

struct BridgeInner {
    tx: Sender<Cmd>,
    ev_tx: Sender<Ev>,
    /// Receiver wrapped in a Mutex so the handle is `Sync`.
    rx: std::sync::Mutex<Receiver<Ev>>,
    shared: Arc<Shared>,
}

/// Thread-safe handle the UI owns.
#[derive(Clone)]
pub struct Bridge(Arc<BridgeInner>);

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    pub fn new() -> Self {
        let (tx, cmd_rx) = channel::<Cmd>();
        let (ev_tx, rx) = channel::<Ev>();
        let shared = Arc::new(Shared::default());
        let shared2 = shared.clone();
        let ev_tx2 = ev_tx.clone();
        std::thread::Builder::new()
            .name("npvt-engine-bridge".into())
            .spawn(move || worker(cmd_rx, ev_tx, shared2))
            .expect("spawn engine bridge");
        Bridge(Arc::new(BridgeInner {
            tx,
            ev_tx: ev_tx2,
            rx: std::sync::Mutex::new(rx),
            shared,
        }))
    }

    /// A sender handle so UI-side worker threads (pings, subscriptions,
    /// asset downloads) can push events into the same queue.
    pub fn event_sender(&self) -> Sender<Ev> {
        self.0.ev_tx.clone()
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.0.shared
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.0.tx.send(cmd);
    }

    /// Drain all pending events.
    pub fn drain(&self) -> Vec<Ev> {
        let mut out = Vec::new();
        if let Ok(rx) = self.0.rx.lock() {
            while let Ok(ev) = rx.try_recv() {
                out.push(ev);
            }
        }
        out
    }
}

fn worker(cmd_rx: Receiver<Cmd>, ev_tx: Sender<Ev>, shared: Arc<Shared>) {
    let mut running_pid: Option<u32> = None;
    for cmd in cmd_rx {
        match cmd {
            Cmd::Stop => {
                shared.stop_traffic.store(true, Ordering::Relaxed);
                shared.stop_watch.store(true, Ordering::Relaxed);
                if let Some(pid) = running_pid.take() {
                    let _ = npvt_core::engine::stop(pid);
                }
                shared.set_state(STATE_DISCONNECTED);
                shared.reset_session();
                shared.set_error(None);
                let _ = ev_tx.send(Ev::Stopped);
            }
            Cmd::Start(spec) => {
                // Stop any previous engine first.
                shared.stop_traffic.store(true, Ordering::Relaxed);
                shared.stop_watch.store(true, Ordering::Relaxed);
                if let Some(pid) = running_pid.take() {
                    let _ = npvt_core::engine::stop(pid);
                    std::thread::sleep(Duration::from_millis(400));
                }
                shared.reset_session();
                shared.set_error(None);
                shared.set_state(STATE_CONNECTING);

                let bin = npvt_core::engine::find_engine_binary(
                    spec.engine_path.as_deref().map(std::path::Path::new),
                );
                let Some(bin) = bin else {
                    shared.set_state(STATE_ERROR);
                    shared.set_error(Some(
                        "sing-box engine not found. Put sing-box.exe next to NPVTunnel.exe, in the npvtunnel data folder under bin\\, or on PATH. Downloads: https://github.com/SagerNet/sing-box/releases"
                            .to_owned(),
                    ));
                    let _ = ev_tx.send(Ev::EngineMissing);
                    continue;
                };

                match launch(&spec, bin) {
                    Ok(pid) => {
                        shared.pid.store(pid, Ordering::Relaxed);
                        running_pid = Some(pid);
                        shared.stop_watch.store(false, Ordering::Relaxed);
                        spawn_watcher(shared.clone(), ev_tx.clone(), pid);
                        spawn_traffic_poller(shared.clone(), ev_tx.clone(), spec.socks_port);
                        let _ = ev_tx.send(Ev::Started(pid));
                    }
                    Err(err) => {
                        shared.set_state(STATE_ERROR);
                        shared.set_error(Some(err.clone()));
                        let _ = ev_tx.send(Ev::StartFailed(err));
                    }
                }
            }
        }
    }
}

/// Build + write the sing-box config, then spawn it.
fn launch(spec: &StartSpec, bin: std::path::PathBuf) -> Result<u32, String> {
    let cfg = npvt_core::engine::config(
        &spec.members,
        &spec.active_id,
        spec.socks_port,
        &spec.dns_server,
        Some(spec.clash_port),
    )
    .map_err(|e| format!("config render failed: {e}"))?;
    let mut cfg = cfg;
    inject_extras(&mut cfg, spec);
    let dir = npvt_core::engine::work_dir().map_err(|e| format!("data dir: {e}"))?;
    let cfg_path = dir.join("npvtunnel-gui-engine.json");
    let log_path = dir.join("npvtunnel-gui-engine.log");
    npvt_core::engine::write_config(&cfg, &cfg_path)
        .map_err(|e| format!("config write failed: {e}"))?;
    let running = npvt_core::engine::start(&bin, &cfg_path, &log_path)
        .map_err(|e| format!("engine start failed: {e}"))?;
    Ok(running.pid)
}

/// Add clash_api (traffic stats), log level and per-app routing rules.
fn inject_extras(cfg: &mut serde_json::Value, spec: &StartSpec) {
    if let Some(obj) = cfg.as_object_mut() {
        // clash_api is injected by npvt_core::engine::config already.
        if let Some(log) = obj.get_mut("log").and_then(|v| v.as_object_mut()) {
            log.insert("level".to_owned(), serde_json::json!(spec.log_level));
        }
        match spec.per_app_mode {
            crate::settings::PerAppMode::Block if !spec.per_app.is_empty() => {
                obj["route"]["rules"] = serde_json::json!([
                    {"outbound": "direct", "process_name": spec.per_app}
                ]);
            }
            crate::settings::PerAppMode::Allow if !spec.per_app.is_empty() => {
                let active = &spec.active_id;
                obj["route"]["rules"] = serde_json::json!([
                    {"outbound": active, "process_name": spec.per_app}
                ]);
                obj["route"]["final"] = serde_json::json!("direct");
            }
            _ => {}
        }
    }
}

fn spawn_watcher(shared: Arc<Shared>, ev_tx: Sender<Ev>, pid: u32) {
    let log_path = npvt_core::engine::work_dir()
        .map(|d| d.join("npvtunnel-gui-engine.log"))
        .unwrap_or_else(|_| PathBuf::from("npvtunnel-engine.log"));
    std::thread::Builder::new()
        .name("npvt-engine-watch".into())
        .spawn(move || {
            let mut died_at: Option<Instant> = None;
            loop {
                if shared.stop_watch.load(Ordering::Relaxed) {
                    return;
                }
                if !npvt_core::engine::alive(pid) {
                    // Give the engine a moment: early in startup the process
                    // may still be initializing.
                    match died_at {
                        None => died_at = Some(Instant::now()),
                        Some(t) if t.elapsed() > Duration::from_millis(1200) => {
                            let tail = npvt_core::engine::log_tail(&log_path, 12);
                            if shared.state() != STATE_ERROR {
                                shared.set_state(STATE_ERROR);
                                shared.set_error(Some("engine exited unexpectedly".to_owned()));
                            }
                            let _ = ev_tx.send(Ev::Died { tail });
                            return;
                        }
                        Some(_) => {}
                    }
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                died_at = None;
                std::thread::sleep(Duration::from_millis(700));
            }
        })
        .ok();
}

fn spawn_traffic_poller(shared: Arc<Shared>, ev_tx: Sender<Ev>, socks_port: u16) {
    std::thread::Builder::new()
        .name("npvt-traffic".into())
        .spawn(move || {
            shared.stop_traffic.store(false, Ordering::Relaxed);
            // Wait for the clash API to come up (up to ~6s), probing the
            // SOCKS port too as a fallback connectivity signal.
            let deadline = Instant::now() + Duration::from_secs(6);
            let mut live = false;
            while !live && Instant::now() < deadline && !shared.stop_traffic.load(Ordering::Relaxed)
            {
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], socks_port));
                if TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok() {
                    live = true;
                }
                if !live {
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
            if !live || shared.stop_traffic.load(Ordering::Relaxed) {
                return;
            }
            let _ = ev_tx.send(Ev::TrafficLive);
            stream_traffic(&shared);
        })
        .ok();
}

/// Read the clash API NDJSON traffic stream until stopped.
fn stream_traffic(shared: &Arc<Shared>) {
    // The poller re-derives the controller URL from the engine config that
    // was written on disk, so the port never has to be threaded through.
    let Ok(cfg_path) = npvt_core::engine::work_dir() else {
        return;
    };
    let cfg_path = cfg_path.join("npvtunnel-gui-engine.json");
    let Ok(bytes) = std::fs::read(&cfg_path) else {
        return;
    };
    let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let Some(url) = cfg
        .pointer("/experimental/clash_api/external_controller")
        .and_then(|v| v.as_str())
        .map(|c| format!("http://{c}/traffic"))
    else {
        return;
    };
    while !shared.stop_traffic.load(Ordering::Relaxed) {
        match ureq::get(&url).call() {
            Ok(resp) => {
                let reader = std::io::BufReader::new(resp.into_reader());
                for line in reader.lines() {
                    if shared.stop_traffic.load(Ordering::Relaxed) {
                        return;
                    }
                    let Ok(line) = line else { return };
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                        let up = v.get("up").and_then(|x| x.as_u64()).unwrap_or(0);
                        let down = v.get("down").and_then(|x| x.as_u64()).unwrap_or(0);
                        shared.up_bps.store(up, Ordering::Relaxed);
                        shared.down_bps.store(down, Ordering::Relaxed);
                        shared.up_total.fetch_add(up, Ordering::Relaxed);
                        shared.down_total.fetch_add(down, Ordering::Relaxed);
                        shared.lifetime_up.fetch_add(up, Ordering::Relaxed);
                        shared.lifetime_down.fetch_add(down, Ordering::Relaxed);
                    }
                }
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(1500));
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Ping a set of profiles sequentially on a worker thread.
pub fn spawn_ping_worker(ev_tx: Sender<Ev>, ids: Vec<(String, String, u16)>) {
    std::thread::Builder::new()
        .name("npvt-ping".into())
        .spawn(move || {
            for (id, server, port) in ids {
                let ms = ping_host(&server, port);
                let _ = ev_tx.send(Ev::Ping { id, ms });
            }
        })
        .ok();
}

fn ping_host(server: &str, port: u16) -> Option<u128> {
    use std::net::ToSocketAddrs;
    let start = Instant::now();
    let addrs: Vec<_> = (server, port).to_socket_addrs().ok()?.collect();
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok() {
            return Some(start.elapsed().as_millis());
        }
    }
    None
}

/// Format a byte rate like the design: "46.7 MB".
pub fn fmt_mb(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.1} MB")
    }
}

/// Format a rate with a per-second suffix.
pub fn fmt_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", fmt_mb(bytes_per_sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_mb() {
        assert_eq!(fmt_mb(0), "0.0 MB");
        assert_eq!(fmt_mb(46_700_000), "44.5 MB");
        assert_eq!(fmt_mb(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn formats_rate() {
        assert_eq!(fmt_rate(1024 * 1024), "1.0 MB/s");
    }

    #[test]
    fn inject_extras_sets_log_level() {
        let spec = StartSpec {
            members: vec![],
            active_id: "abc".into(),
            socks_port: 2080,
            clash_port: 19090,
            dns_server: "8.8.8.8".into(),
            log_level: "info".into(),
            per_app_mode: crate::settings::PerAppMode::Off,
            per_app: vec![],
            engine_path: None,
        };
        let mut cfg = serde_json::json!({
            "log": {"level": "warn"},
            "route": {"final": "abc"}
        });
        inject_extras(&mut cfg, &spec);
        // clash_api itself is injected by npvt_core::engine::config.
        assert_eq!(cfg.pointer("/log/level"), Some(&serde_json::json!("info")));
    }

    #[test]
    fn inject_extras_block_mode() {
        let spec = StartSpec {
            members: vec![],
            active_id: "abc".into(),
            socks_port: 2080,
            clash_port: 19090,
            dns_server: "8.8.8.8".into(),
            log_level: "warn".into(),
            per_app_mode: crate::settings::PerAppMode::Block,
            per_app: vec!["steam.exe".into()],
            engine_path: None,
        };
        let mut cfg = serde_json::json!({"route": {"final": "abc"}});
        inject_extras(&mut cfg, &spec);
        assert_eq!(
            cfg.pointer("/route/rules/0/outbound"),
            Some(&serde_json::json!("direct"))
        );
    }
}
