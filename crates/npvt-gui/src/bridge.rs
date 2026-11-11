//! The bridge between the UI thread and the blocking engine/vault work:
//! background threads communicate back through a single event channel, and
//! the UI drains it once per frame (`Bridge::poll`).
//!
//! Threading model:
//! - every blocking operation runs on its own short-lived `std::thread`;
//! - connection-scoped threads (monitor, traffic) carry a generation number
//!   and share the bridge cancel flag, so stale events from a previous
//!   connection are dropped and threads of a stopped connection wind down
//!   on their own;
//! - the egui Context is cloned into the threads so they can request a
//!   repaint when an event arrives.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use npvt_core::engine;
use npvt_core::session::{self, Session};

use crate::firewall;
use crate::store::{self, Incoming};

/// How often the monitor thread re-checks engine liveness.
const MONITOR_INTERVAL: Duration = Duration::from_millis(900);
/// Traffic polling cadence (clash API gives 1-second resolution).
const TRAFFIC_INTERVAL: Duration = Duration::from_millis(1000);

/// Sentinel error for cancelled operations (surfaced as a silent no-op).
const CANCELLED: &str = "cancelled";

/// A snapshot of live traffic from the engine's clash API.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Traffic {
    /// Downlink rate, bytes per second.
    pub down_bps: f64,
    /// Uplink rate, bytes per second.
    pub up_bps: f64,
    /// Session download total, bytes.
    pub down_total: u64,
    /// Session upload total, bytes.
    pub up_total: u64,
    /// Active proxied connections.
    pub conns: u32,
}

/// Everything the UI needs about the connected engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub pid: u32,
    pub profile_id: String,
    pub profile_name: String,
    pub socks_port: u16,
    pub started: u64,
}

impl From<Session> for Connected {
    fn from(s: Session) -> Self {
        Connected {
            pid: s.pid,
            profile_id: s.profile_id,
            profile_name: s.name,
            socks_port: s.socks_port,
            started: s.started,
        }
    }
}

/// The engine state machine as seen by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    Disconnected,
    Connecting,
    Connected(Connected),
    Stopping,
}

/// Events flowing from background threads to the UI.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // VaultData is a small struct of small strings in practice
enum Event {
    /// Fresh vault data (after load/import/export/delete).
    Vault(Result<npvt_core::vault::VaultData, String>),
    /// A decode finished; show the security card or surface the error.
    Decoded(Result<Incoming, String>, Option<PathBuf>),
    /// Import applied to the vault: (display name, replaced).
    Imported(Result<(String, bool), String>),
    /// Export finished: file path written.
    Exported(Result<PathBuf, String>),
    /// Identity generated (returns the recipient pubkey).
    IdentityReady(Result<String, String>),
    /// Engine connect finished.
    ConnectDone(Result<Connected, String>, u64),
    /// The engine died on its own (crash, kill); carries the log tail.
    EngineDied(String, u64),
    /// Disconnect finished.
    DisconnectDone(Result<(), String>, u64),
    /// Traffic snapshot for the current connection.
    Traffic(Traffic, u64),
    /// Ping result for a profile id, in milliseconds.
    Ping(String, Result<u16, String>),
}

/// Per-frame results the App reads after `poll`.
#[derive(Default)]
pub struct Pending {
    pub vault: Option<Result<npvt_core::vault::VaultData, String>>,
    pub decode: Option<Result<Incoming, String>>,
    pub import: Option<Result<(String, bool), String>>,
    pub export: Option<Result<PathBuf, String>>,
    pub identity: Option<Result<String, String>>,
}

/// What the UI can ask the bridge to do. All requests are non-blocking.
pub struct Bridge {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    pub state: EngineState,
    /// Last engine error (shown as a banner on Home).
    pub error: Option<String>,
    pub traffic: Option<Traffic>,
    /// (profile_id, result) of the most recent ping probe.
    pub last_ping: Option<(String, Result<u16, String>)>,
    pub ping_in_flight: bool,
    /// True while a vault/import/export operation runs (buttons disabled).
    pub busy: bool,
    /// Whether the loaded vault has a creator identity.
    pub has_identity: bool,
    generation: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    pending: Pending,
}

impl Bridge {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Bridge {
            tx,
            rx,
            state: EngineState::Disconnected,
            error: None,
            traffic: None,
            last_ping: None,
            ping_in_flight: false,
            busy: false,
            has_identity: false,
            generation: Arc::new(AtomicU64::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
            pending: Pending::default(),
        }
    }

    /// Drain pending events; call once per frame.
    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        let current_gen = self.generation.load(Ordering::SeqCst);
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Vault(data) => {
                    self.busy = false;
                    if let Err(err) = &data {
                        self.error = Some(err.clone());
                    }
                    self.pending.vault = Some(data);
                }
                Event::Decoded(res, _path) => {
                    self.busy = false;
                    self.pending.decode = Some(res);
                }
                Event::Imported(res) => {
                    self.busy = false;
                    self.pending.import = Some(res);
                }
                Event::Exported(res) => {
                    self.busy = false;
                    self.pending.export = Some(res);
                }
                Event::IdentityReady(res) => {
                    self.busy = false;
                    self.pending.identity = Some(res);
                }
                Event::ConnectDone(res, gen) => {
                    if gen != current_gen {
                        continue;
                    }
                    match res {
                        Ok(info) => {
                            self.state = EngineState::Connected(info);
                            self.error = None;
                        }
                        Err(err) if err == CANCELLED => {
                            self.state = EngineState::Disconnected;
                        }
                        Err(err) => {
                            self.state = EngineState::Disconnected;
                            self.error = Some(err);
                        }
                    }
                }
                Event::EngineDied(tail, gen) => {
                    if gen != current_gen {
                        continue;
                    }
                    self.state = EngineState::Disconnected;
                    self.traffic = None;
                    self.error = Some(format!("the engine stopped unexpectedly.\n\n{tail}"));
                    let _ = session::clear_session();
                }
                Event::DisconnectDone(res, gen) => {
                    if gen != current_gen {
                        continue;
                    }
                    self.state = EngineState::Disconnected;
                    self.traffic = None;
                    if let Err(err) = res {
                        self.error = Some(err);
                    }
                }
                Event::Traffic(t, gen) => {
                    if gen != current_gen {
                        continue;
                    }
                    self.traffic = Some(t);
                }
                Event::Ping(profile_id, res) => {
                    self.ping_in_flight = false;
                    self.last_ping = Some((profile_id, res));
                }
            }
            ctx.request_repaint();
        }
    }

    /// Take the results accumulated by `poll` this frame.
    pub fn take_pending(&mut self) -> Pending {
        std::mem::take(&mut self.pending)
    }

    fn next_generation(&self) -> u64 {
        let gen = self.generation.load(Ordering::SeqCst) + 1;
        self.generation.store(gen, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);
        gen
    }

    /// Connect to a profile by id. Runs the CLI-equivalent flow on a thread.
    pub fn connect(
        &mut self,
        ctx: &eframe::egui::Context,
        profile_id: String,
        socks_port: u16,
        clash_port: u16,
        dns: &str,
        tun_mode: bool,
        tun_address: Option<String>,
        route_rules: Vec<npvt_core::engine::RouteRule>,
        kill_switch: bool,
    ) {
        if matches!(
            self.state,
            EngineState::Connecting | EngineState::Stopping | EngineState::Connected(_)
        ) {
            return;
        }
        let gen = self.next_generation();
        self.state = EngineState::Connecting;
        self.error = None;
        self.traffic = None;

        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let cancel = self.cancel.clone();
        let dns = dns.to_string();
        std::thread::spawn(move || {
            let res = connect_flow(
                &profile_id,
                socks_port,
                clash_port,
                &dns,
                tun_mode,
                tun_address.as_deref(),
                &route_rules,
                kill_switch,
                &cancel,
            );
            match &res {
                Ok(info) => {
                    spawn_monitor_thread(tx.clone(), ctx.clone(), gen, cancel.clone());
                    spawn_traffic_thread(tx.clone(), ctx.clone(), gen, clash_port, cancel.clone());
                    let _ = tx.send(Event::ConnectDone(Ok(info.clone().into()), gen));
                }
                Err(err) => {
                    let _ = tx.send(Event::ConnectDone(Err(err.clone()), gen));
                }
            }
        });
    }

    /// Stop the running engine (whichever front end started it).
    pub fn disconnect(&mut self) {
        match &self.state {
            EngineState::Connected(_) => {}
            EngineState::Connecting => {}
            _ => {
                // Not connected: still clear a stale session file if present.
                let _ = session::clear_session();
                self.state = EngineState::Disconnected;
                self.traffic = None;
                return;
            }
        }
        let gen = self.generation.load(Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
        self.state = EngineState::Stopping;
        self.traffic = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = disconnect_flow();
            let _ = tx.send(Event::DisconnectDone(res, gen));
        });
    }

    /// TCP-handshake ping of a profile's endpoint.
    pub fn ping(&mut self, profile_id: String, server: String, port: u16) {
        if self.ping_in_flight {
            return;
        }
        self.ping_in_flight = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = engine::ping_server(&server, port, Duration::from_secs(3))
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Ping(profile_id, res));
        });
    }

    /// Reload the vault from disk.
    pub fn load_vault(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Vault(store::load().map(|(_, data)| data)));
        });
    }

    /// Decode an import source on a thread. File bytes and clipboard text
    /// are mutually exclusive (`bytes` wins).
    pub fn decode_import(
        &mut self,
        bytes: Option<Vec<u8>>,
        text: Option<String>,
        passphrase: Option<String>,
        path: Option<PathBuf>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let identity = store::load().ok().and_then(|(_, data)| data.identity);
            let res = match (&bytes, &text) {
                (Some(b), _) => store::decode_source(Some(b), None, identity.as_ref(), passphrase),
                (None, Some(t)) => {
                    store::decode_source(None, Some(t), identity.as_ref(), passphrase)
                }
                (None, None) => Err("empty import source".to_string()),
            };
            let _ = tx.send(Event::Decoded(res, path));
        });
    }

    /// Apply a previously decoded import (security card confirmed).
    pub fn apply_import(
        &mut self,
        incoming: store::Incoming,
        name_override: Option<String>,
        group_override: Option<String>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = store::load().and_then(|(vault, mut data)| {
                store::apply_import(&vault, &mut data, &incoming, name_override, group_override)
            });
            let _ = tx.send(Event::Imported(res));
        });
    }

    /// Export a profile to a file on a thread.
    pub fn export(&mut self, req: store::ExportRequest) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        let out = req.out_path.clone();
        std::thread::spawn(move || {
            let res = store::load()
                .and_then(|(vault, data)| store::export_file(&vault, &data, &req))
                .map(|_| out.clone());
            let _ = tx.send(Event::Exported(res));
        });
    }

    /// Generate a creator identity if none exists.
    pub fn generate_identity(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = store::load().and_then(|(vault, mut data)| {
                let identity = store::ensure_identity(&vault, &mut data)?;
                Ok(store::recipient_pubkey(&identity))
            });
            let _ = tx.send(Event::IdentityReady(res));
        });
    }
}

/// The blocking connect flow, mirroring the CLI's `cmd_connect`.
fn connect_flow(
    profile_id: &str,
    socks_port: u16,
    clash_port: u16,
    dns: &str,
    tun_mode: bool,
    tun_address: Option<&str>,
    route_rules: &[npvt_core::engine::RouteRule],
    kill_switch: bool,
    cancel: &AtomicBool,
) -> Result<Session, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    if let Some(existing) = session::read_session().map_err(|e| e.to_string())? {
        if engine::alive(existing.pid) {
            return Err(format!(
                "an engine is already running (pid {}, profile '{}'); disconnect first",
                existing.pid, existing.name
            ));
        }
    }
    let (_, data) = store::load()?;
    let profile = npvt_core::vault::find_profile(&data, profile_id)
        .map_err(|e| e.to_string())?
        .clone();

    let workdir = engine::work_dir().map_err(|e| e.to_string())?;
    let cfg = engine::config(
        std::slice::from_ref(&profile),
        &profile.id,
        socks_port,
        dns,
        Some(clash_port),
        tun_mode,
        tun_address,
        route_rules,
    )
    .map_err(|e| e.to_string())?;
    let cfg_path = workdir.join("engine.json");
    engine::write_config(&cfg, &cfg_path).map_err(|e| e.to_string())?;

    let bin = engine::find_engine_binary(None).ok_or_else(|| {
        "sing-box engine not found. Place sing-box.exe next to NPVTunnel.exe (the release zip already bundles it), in the npvtunnel data directory under bin\\, or on PATH."
            .to_string()
    })?;

    let running =
        engine::start(&bin, &cfg_path, &workdir.join("engine.log")).map_err(|e| e.to_string())?;
    // Give the engine a moment, then verify it is still alive: a malformed
    // config or a port conflict dies silently right after start.
    std::thread::sleep(Duration::from_millis(600));
    if cancel.load(Ordering::SeqCst) {
        let _ = engine::stop(running.pid);
        return Err(CANCELLED.to_string());
    }
    if !engine::alive(running.pid) {
        let tail = engine::log_tail(&workdir.join("engine.log"), 20);
        return Err(format!(
            "sing-box exited immediately after start; last engine log lines:\n{tail}"
        ));
    }
    let pf = Session {
        pid: running.pid,
        profile_id: profile.id.clone(),
        name: profile.name.clone(),
        socks_port,
        started: npvt_core::profile::unix_now(),
    };
    session::write_session(&pf).map_err(|e| e.to_string())?;
    if kill_switch {
        firewall::enable_kill_switch().map_err(|e| format!("kill switch failed: {e}"))?;
    }
    Ok(pf)
}

fn disconnect_flow() -> Result<(), String> {
    let res = match session::read_session().map_err(|e| e.to_string())? {
        Some(pf) if engine::alive(pf.pid) => {
            engine::stop(pf.pid).map_err(|e| e.to_string())?;
            session::clear_session().map_err(|e| e.to_string())?;
            Ok(())
        }
        Some(_) => {
            session::clear_session().map_err(|e| e.to_string())?;
            Ok(())
        }
        None => Ok(()),
    };
    // Best-effort: remove kill switch rules if any were added.
    let _ = firewall::disable_kill_switch();
    res
}

fn spawn_monitor_thread(
    tx: Sender<Event>,
    ctx: eframe::egui::Context,
    gen: u64,
    cancel: Arc<AtomicBool>,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(MONITOR_INTERVAL);
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        match session::probe() {
            Ok(npvt_core::session::EngineStatus::Running(_)) => {}
            _ => {
                let tail = engine::work_dir()
                    .ok()
                    .map(|dir| engine::log_tail(&dir.join("engine.log"), 12))
                    .unwrap_or_default();
                let _ = tx.send(Event::EngineDied(tail, gen));
                ctx.request_repaint();
                return;
            }
        }
    });
}

fn spawn_traffic_thread(
    tx: Sender<Event>,
    ctx: eframe::egui::Context,
    gen: u64,
    clash_port: u16,
    cancel: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let url = format!("http://127.0.0.1:{clash_port}/connections");
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(500)))
            .build()
            .into();
        let mut last: Option<(u64, u64, std::time::Instant)> = None;
        let mut consecutive_errors = 0u32;
        loop {
            std::thread::sleep(TRAFFIC_INTERVAL);
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let snapshot = (|| -> Option<Traffic> {
                let mut res = agent.get(&url).call().ok()?;
                let value: serde_json::Value = res.body_mut().read_json().ok()?;
                let down_total = value.get("downloadTotal").and_then(|v| v.as_u64())?;
                let up_total = value.get("uploadTotal").and_then(|v| v.as_u64())?;
                let conns = value
                    .get("connections")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len() as u32)
                    .unwrap_or(0);
                let now = std::time::Instant::now();
                let (down_bps, up_bps) = match last {
                    Some((ld, lu, at)) => {
                        let dt = now.duration_since(at).as_secs_f64();
                        if dt > 0.05 {
                            (
                                down_total.saturating_sub(ld) as f64 / dt,
                                up_total.saturating_sub(lu) as f64 / dt,
                            )
                        } else {
                            (0.0, 0.0)
                        }
                    }
                    None => (0.0, 0.0),
                };
                last = Some((down_total, up_total, now));
                Some(Traffic {
                    down_bps,
                    up_bps,
                    down_total,
                    up_total,
                    conns,
                })
            })();
            match snapshot {
                Some(t) => {
                    consecutive_errors = 0;
                    let _ = tx.send(Event::Traffic(t, gen));
                }
                None => {
                    consecutive_errors += 1;
                    if consecutive_errors >= 3 {
                        // Engine is likely gone; the monitor thread reports it.
                        return;
                    }
                }
            }
            ctx.request_repaint();
        }
    });
}
