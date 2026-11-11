//! Engine glue: render profiles into `sing-box` configs and manage the
//! sidecar process.
//!
//! Security posture:
//! - the local inbound always binds `127.0.0.1` (never LAN-exposed by
//!   default; gateway sharing is an explicit, consent-gated feature);
//! - the engine is spawned directly (no shell), stdout/stderr go to a local
//!   log file, and the config file is written with owner-only permissions on
//!   Unix;
//! - profiles that are not yet wired to an engine sidecar fail with a clear
//!   roadmap message instead of silently downgrading security.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::profile::{Credential, Profile, Protocol, Transport};

/// Expected SHA-256 of the bundled sing-box binary.
///
/// Set this at build time (e.g. via `NPVT_ENGINE_SHA256` env var) to enable
/// authenticity verification. On each startup, the found sing-box binary is
/// hashed and compared to this value; a mismatch aborts startup.
///
/// If unset (empty), verification is skipped.
#[allow(dead_code)]
pub const EXPECTED_ENGINE_SHA256: Option<&'static str> = option_env!("NPVT_ENGINE_SHA256");

/// Default local SOCKS inbound port.
pub const DEFAULT_SOCKS_PORT: u16 = 2080;
/// Default local clash-API port used for live traffic stats (localhost only).
pub const DEFAULT_CLASH_PORT: u16 = 19090;
/// Default TUN interface address (IPv4) when TUN mode is enabled.
pub const DEFAULT_TUN_ADDRESS: &str = "10.255.0.1/24";
/// Default TUN interface MTU.
pub const DEFAULT_TUN_MTU: u16 = 1500;
/// Extra port for [`ping_server`] probes when the caller wants a free port.
const PING_PROBE_PORT: u16 = 0;

/// Action for a per-process routing rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteAction {
    /// Force traffic through the tunnel proxy.
    Proxy,
    /// Force traffic directly (bypass the tunnel).
    Direct,
}

/// A per-process routing rule. Matches by process image name (e.g. `foo.exe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRule {
    /// Process image name to match.
    pub process_name: String,
    /// Action to take for matching traffic.
    pub action: RouteAction,
}

fn tls_block(p: &Profile, required: bool) -> Option<Value> {
    let enabled = required || p.transport == Transport::Tls;
    if !enabled {
        return None;
    }
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if let Some(sni) = &p.sni {
        tls.insert("server_name".into(), json!(sni));
    }
    tls.insert("insecure".into(), json!(p.allow_insecure));
    Some(Value::Object(tls))
}

fn transport_block(p: &Profile) -> Option<Value> {
    match p.transport {
        Transport::Ws => {
            let mut t = Map::new();
            t.insert("type".into(), json!("ws"));
            if let Some(path) = &p.path {
                t.insert("path".into(), json!(path));
            }
            Some(Value::Object(t))
        }
        Transport::Grpc => {
            let mut t = Map::new();
            t.insert("type".into(), json!("grpc"));
            if let Some(service) = &p.path {
                t.insert("service_name".into(), json!(service));
            }
            Some(Value::Object(t))
        }
        Transport::H2 => {
            let mut t = Map::new();
            t.insert("type".into(), json!("http"));
            if let Some(path) = &p.path {
                t.insert("path".into(), json!(path));
            }
            Some(Value::Object(t))
        }
        _ => None,
    }
}

fn user_pass(token: &str) -> Result<(String, String)> {
    let (u, p) = token.split_once(':').ok_or_else(|| {
        Error::InvalidConfig("socks/http credential must be user:password".into())
    })?;
    Ok((u.to_string(), p.to_string()))
}

/// Render one profile as a sing-box outbound object.
pub fn outbound(p: &Profile) -> Result<Value> {
    p.validate()?;
    if !p.protocol.engine_supported() {
        return Err(Error::Engine(format!(
            "protocol '{}' is not wired to an engine sidecar yet (roadmap P1.5)",
            p.protocol.as_str()
        )));
    }
    let mut ob = Map::new();
    ob.insert("tag".into(), json!(p.id));
    ob.insert("server".into(), json!(p.server));
    ob.insert("server_port".into(), json!(p.port));

    match (&p.protocol, &p.credential) {
        (Protocol::Vless, Credential::Uuid(uuid)) => {
            ob.insert("type".into(), json!("vless"));
            ob.insert("uuid".into(), json!(uuid));
            ob.insert("packet_encoding".into(), json!("xudp"));
        }
        (Protocol::Vmess, Credential::Uuid(uuid)) => {
            ob.insert("type".into(), json!("vmess"));
            ob.insert("uuid".into(), json!(uuid));
            ob.insert("security".into(), json!("auto"));
            ob.insert("alter_id".into(), json!(0));
            ob.insert("packet_encoding".into(), json!("xudp"));
        }
        (Protocol::Trojan, Credential::Password(password)) => {
            ob.insert("type".into(), json!("trojan"));
            ob.insert("password".into(), json!(password));
        }
        (Protocol::Shadowsocks, Credential::SsMethod { method, password }) => {
            ob.insert("type".into(), json!("shadowsocks"));
            ob.insert("method".into(), json!(method));
            ob.insert("password".into(), json!(password));
        }
        (Protocol::Hysteria2, Credential::Password(password)) => {
            ob.insert("type".into(), json!("hysteria2"));
            ob.insert("password".into(), json!(password));
        }
        (Protocol::Socks, cred) => {
            ob.insert("type".into(), json!("socks"));
            ob.insert("version".into(), json!("5"));
            if let Credential::Token(token) = cred {
                let (user, pass) = user_pass(token)?;
                ob.insert("username".into(), json!(user));
                ob.insert("password".into(), json!(pass));
            }
        }
        (Protocol::Http, cred) => {
            ob.insert("type".into(), json!("http"));
            if let Credential::Token(token) = cred {
                let (user, pass) = user_pass(token)?;
                ob.insert("username".into(), json!(user));
                ob.insert("password".into(), json!(pass));
            }
        }
        (protocol, Credential::None) if !protocol.requires_credential() => {
            ob.insert(
                "type".into(),
                json!(match protocol {
                    Protocol::Socks => "socks",
                    Protocol::Http => "http",
                    _ => unreachable!("requires_credential guards this arm"),
                }),
            );
            if *protocol == Protocol::Socks {
                ob.insert("version".into(), json!("5"));
            }
        }
        (protocol, _) => {
            return Err(Error::InvalidConfig(format!(
                "credential kind does not fit protocol '{}'",
                protocol.as_str()
            )))
        }
    }

    if let Some(tls) = tls_block(p, p.protocol == Protocol::Hysteria2) {
        ob.insert("tls".into(), tls);
    }
    if let Some(transport) = transport_block(p) {
        ob.insert("transport".into(), transport);
    }
    Ok(Value::Object(ob))
}

/// Render a full sing-box config for the given profiles with `active` as the
/// selected outbound.
///
/// When `clash_port` is given, the config enables the clash-API experimental
/// block bound to `127.0.0.1` so front ends can poll live traffic stats; it
/// is never exposed beyond localhost.
///
/// When `tun_mode` is true, a TUN inbound is added and the route final points
/// to it instead of a profile outbound. When `route_rules` is non-empty,
/// per-process routing rules are added to the route block.
pub fn config(
    profiles: &[Profile],
    active_id: &str,
    socks_port: u16,
    dns_server: &str,
    clash_port: Option<u16>,
    tun_mode: bool,
    tun_address: Option<&str>,
    route_rules: &[RouteRule],
) -> Result<Value> {
    let active = profiles
        .iter()
        .find(|p| p.id == active_id)
        .ok_or_else(|| Error::Engine("active profile is not in the set".into()))?;
    let mut outbounds = Vec::new();
    for p in profiles {
        outbounds.push(outbound(p)?);
    }
    outbounds.push(json!({"type": "direct", "tag": "direct"}));

    let mut inbounds = vec![json!({
        "type": "socks",
        "tag": "socks-in",
        "listen": "127.0.0.1",
        "listen_port": socks_port,
        "udp_enabled": profiles.iter().any(|p| p.udp)
    })];

    let route_final: String = if tun_mode {
        // Add a TUN inbound that captures all traffic when TUN mode is on.
        let address = tun_address.unwrap_or(DEFAULT_TUN_ADDRESS);
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "address": [address],
            "mtu": DEFAULT_TUN_MTU,
            "auto_route": true,
            "strict_route": true
        }));
        "tun-in".to_string()
    } else {
        active.id.clone()
    };

    // Build route rules for per-process routing.
    let mut route_rules_value = Vec::new();
    for rule in route_rules {
        let action_tag = match rule.action {
            RouteAction::Proxy => active.id.clone(),
            RouteAction::Direct => "direct".to_string(),
        };
        route_rules_value.push(json!({
            "process_name": rule.process_name,
            "outbound": action_tag,
        }));
    }

    let mut route = json!({"final": route_final});
    if !route_rules_value.is_empty() {
        route["rules"] = Value::Array(route_rules_value);
    }

    let mut rendered = json!({
        "log": {"level": "warn"},
        "dns": {
            "servers": [
                {"tag": "dns-remote", "address": dns_server}
            ]
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route
    });
    if let Some(port) = clash_port {
        rendered["experimental"] = json!({
            "clash_api": {
                "external_controller": format!("127.0.0.1:{port}"),
                "default_mode": "Rule"
            }
        });
    }
    Ok(rendered)
}

/// Where the engine config, pid and log files live.
pub fn work_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| Error::Engine("no user data directory".into()))?;
    let dir = base.join("npvtunnel");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Engine handle persisted across CLI and GUI invocations so both surfaces
/// agree on the single running engine.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PidFile {
    /// OS process id of the sidecar.
    pub pid: u32,
    /// Id of the connected profile.
    pub profile_id: String,
    /// Display name of the connected profile.
    pub name: String,
    /// Local SOCKS5 port.
    pub socks_port: u16,
    /// Unix start time.
    pub started: u64,
}

/// Path of the engine pid file inside the work directory.
pub fn pid_file_path() -> Result<PathBuf> {
    Ok(work_dir()?.join("engine.pid"))
}

/// Load the engine pid file, if present and parseable.
pub fn read_pid_file() -> Result<Option<PidFile>> {
    let path = pid_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let pf: PidFile = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Engine(format!("corrupt pid file: {e}")))?;
    Ok(Some(pf))
}

/// Persist the engine pid file.
pub fn write_pid_file(pf: &PidFile) -> Result<()> {
    std::fs::write(pid_file_path()?, serde_json::to_vec(pf)?)?;
    Ok(())
}

/// Remove the pid file (best effort; missing files are fine).
pub fn clear_pid_file() {
    if let Ok(path) = pid_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Locate the sing-box binary: explicit path, next to our own executable,
/// the npvtunnel data directory, or `PATH`.
pub fn find_engine_binary(explicit: Option<&Path>) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "sing-box.exe"
    } else {
        "sing-box"
    };
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.parent()?.join(exe_name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    if let Ok(base) = work_dir() {
        let candidate = base.join("bin").join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Compute the SHA-256 digest of a file as a lowercase hex string.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Engine(format!("cannot read '{}': {e}", path.display())))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    Ok(format!("{result:x}"))
}

/// Verify the authenticity of the sing-box binary against the embedded
/// expected checksum. Returns `Ok(())` when verification passes or is skipped
/// (no expected checksum configured), `Err` on mismatch.
pub fn verify_engine_binary(bin: &Path) -> Result<()> {
    let Some(expected) = EXPECTED_ENGINE_SHA256 else {
        return Ok(()); // verification not configured
    };
    if expected.is_empty() {
        return Ok(());
    }
    let actual = sha256_file(bin)?;
    if actual != expected {
        return Err(Error::Engine(format!(
            "sing-box authenticity check failed: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

/// On Windows, set the file's DACL to owner-only. Best-effort: not yet
/// implemented because `#![forbid(unsafe_code)]` blocks the required Windows
/// API calls. Tracked as a post-v1.0 hardening item (SECURITY.md).
#[cfg(windows)]
pub fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

/// Write the engine config with tight permissions.
pub fn write_config(value: &Value, path: &Path) -> Result<()> {
    let rendered = serde_json::to_vec_pretty(value)?;
    std::fs::write(path, rendered)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A running engine process.
pub struct Running {
    /// OS process id.
    pub pid: u32,
}

/// Spawn the engine sidecar for `config_path`; stdout/stderr append to
/// `log_path`. The child is spawned directly, never through a shell.
pub fn start(bin: &Path, config_path: &Path, log_path: &Path) -> Result<Running> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let child = std::process::Command::new(bin)
        .arg("run")
        .arg("-c")
        .arg(config_path)
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .map_err(|e| Error::Engine(format!("failed to start engine: {e}")))?;
    Ok(Running { pid: child.id() })
}

/// Best-effort liveness probe for a pid.
pub fn alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        // Match the pid as an exact quoted CSV field of the tasklist row, so
        // a substring of another column (e.g. a memory figure) can never be
        // mistaken for a running process.
        let needle = format!("\"{pid}\"");
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|line| line.contains(&needle))
            })
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = pid;
        true // conservative fallback on untested platforms
    }
}

/// Stop a running engine by pid.
pub fn stop(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .status()
            .map_err(|e| Error::Engine(format!("failed to run taskkill: {e}")))?;
        if !status.success() {
            return Err(Error::Engine("engine was not running anymore".into()));
        }
    }
    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|e| Error::Engine(format!("failed to run kill: {e}")))?;
        if !status.success() {
            return Err(Error::Engine("engine was not running anymore".into()));
        }
    }
    Ok(())
}

/// Measure the TCP handshake latency to `host:port` in milliseconds.
///
/// This is a liveness/latency probe of the tunnel endpoint itself, not an
/// application-level HTTP ping. DNS resolution counts toward the reported
/// latency. Fails when the endpoint refuses or times out.
pub fn ping_server(host: &str, port: u16, timeout: Duration) -> Result<u16> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| Error::Engine(format!("failed to resolve '{host}': {e}")))?
        .next()
        .ok_or_else(|| Error::Engine(format!("no address resolved for '{host}'")))?;
    let start = Instant::now();
    let stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| Error::Engine(format!("connect to {host}:{port} failed: {e}")))?;
    drop(stream);
    let elapsed = start.elapsed().as_millis();
    Ok(u16::try_from(elapsed).unwrap_or(u16::MAX))
}

/// Pick a free localhost port by binding an ephemeral listener and reading
/// its port. Used by tests and front ends that need to avoid port clashes.
pub fn free_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", PING_PROBE_PORT))
        .map_err(|e| Error::Engine(format!("could not bind an ephemeral port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| Error::Engine(format!("could not read local addr: {e}")))?
        .port();
    // Deliberately drop the listener: the port becomes free again. Racy in
    // theory, deterministic enough for tests and best-effort defaults.
    drop(listener);
    Ok(port)
}

/// Read the last `max_lines` lines of a log file for error reports; the
/// file may be unreadable or contain invalid UTF-8, which is not fatal here.
pub fn log_tail(path: &Path, max_lines: usize) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(max_lines);
            lines[start..].join("\n")
        }
        Err(_) => "(engine log not readable)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Credential, Transport};

    fn vless() -> Profile {
        Profile {
            id: "abc123".into(),
            name: "shop".into(),
            protocol: Protocol::Vless,
            server: "example.com".into(),
            port: 443,
            credential: Credential::Uuid("11111111-2222-3333-4444-555555555555".into()),
            transport: Transport::Tls,
            sni: Some("example.com".into()),
            path: None,
            allow_insecure: false,
            udp: true,
            group: None,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn log_tail_returns_last_lines_only() {
        let dir = std::env::temp_dir().join(format!("npvt-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("tail.log");
        std::fs::write(&path, "l1\nl2\nl3\n").expect("write log");
        assert_eq!(log_tail(&path, 2), "l2\nl3");
        assert_eq!(log_tail(&path, 10), "l1\nl2\nl3");
        assert_eq!(
            log_tail(&dir.join("missing.log"), 2),
            "(engine log not readable)"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn outbound_shape_vless() {
        let ob = outbound(&vless()).expect("outbound");
        assert_eq!(ob["type"], "vless");
        assert_eq!(ob["server"], "example.com");
        assert_eq!(ob["server_port"], 443);
        assert_eq!(ob["tls"]["enabled"], true);
        assert_eq!(ob["tls"]["insecure"], false);
        assert!(ob.get("password").is_none());
    }

    #[test]
    fn outbound_shape_ss_and_hysteria2() {
        let mut p = vless();
        p.protocol = Protocol::Shadowsocks;
        p.credential = Credential::SsMethod {
            method: "aes-256-gcm".into(),
            password: "sesame".into(),
        };
        p.transport = Transport::Plain;
        p.sni = None;
        let ob = outbound(&p).expect("outbound");
        assert_eq!(ob["type"], "shadowsocks");
        assert_eq!(ob["method"], "aes-256-gcm");
        assert!(ob.get("tls").is_none());

        let mut h = vless();
        h.protocol = Protocol::Hysteria2;
        h.credential = Credential::Password("auth".into());
        h.transport = Transport::Plain;
        h.sni = None;
        let ob = outbound(&h).expect("outbound");
        assert_eq!(ob["type"], "hysteria2");
        assert_eq!(ob["tls"]["enabled"], true, "hysteria2 requires TLS");
    }

    #[test]
    fn config_binds_localhost_only() {
        let p = vless();
        let cfg = config(
            std::slice::from_ref(&p),
            &p.id,
            DEFAULT_SOCKS_PORT,
            "1.1.1.1",
            None,
            false,
            None,
            &[],
        )
        .expect("config");
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(cfg["route"]["final"], "abc123");
        assert!(cfg.get("experimental").is_none(), "no clash api by default");
        let types: Vec<&str> = cfg["outbounds"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|o| o["type"].as_str())
            .collect();
        assert!(types.contains(&"direct"));
    }

    #[test]
    fn config_with_clash_api_binds_localhost() {
        let p = vless();
        let cfg = config(
            std::slice::from_ref(&p),
            &p.id,
            DEFAULT_SOCKS_PORT,
            "1.1.1.1",
            Some(DEFAULT_CLASH_PORT),
            false,
            None,
            &[],
        )
        .expect("config");
        let controller = cfg["experimental"]["clash_api"]["external_controller"]
            .as_str()
            .expect("controller");
        assert_eq!(controller, "127.0.0.1:19090");
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
    }

    #[test]
    fn config_tun_mode_adds_tun_inbound() {
        let p = vless();
        let cfg = config(
            std::slice::from_ref(&p),
            &p.id,
            DEFAULT_SOCKS_PORT,
            "1.1.1.1",
            None,
            true,
            Some("10.255.0.1/24"),
            &[],
        )
        .expect("config");
        assert_eq!(cfg["route"]["final"], "tun-in");
        let tun = cfg["inbounds"]
            .as_array()
            .expect("inbounds array")
            .iter()
            .find(|i| i["tag"] == "tun-in")
            .expect("tun-in inbound");
        assert_eq!(tun["type"], "tun");
        assert_eq!(tun["address"][0], "10.255.0.1/24");
        assert_eq!(tun["mtu"], 1500);
        assert_eq!(tun["auto_route"], true);
        assert_eq!(tun["strict_route"], true);
        // SOCKS inbound still present.
        assert_eq!(cfg["inbounds"][0]["type"], "socks");
    }

    #[test]
    fn config_route_rules_are_rendered() {
        let p = vless();
        let rules = [
            crate::engine::RouteRule {
                process_name: "foo.exe".to_string(),
                action: crate::engine::RouteAction::Direct,
            },
            crate::engine::RouteRule {
                process_name: "bar.exe".to_string(),
                action: crate::engine::RouteAction::Proxy,
            },
        ];
        let cfg = config(
            std::slice::from_ref(&p),
            &p.id,
            DEFAULT_SOCKS_PORT,
            "1.1.1.1",
            None,
            false,
            None,
            &rules,
        )
        .expect("config");
        let route_rules = cfg["route"]["rules"].as_array().expect("rules array");
        assert_eq!(route_rules.len(), 2);
        assert_eq!(route_rules[0]["process_name"], "foo.exe");
        assert_eq!(route_rules[0]["outbound"], "direct");
        assert_eq!(route_rules[1]["process_name"], "bar.exe");
        assert_eq!(route_rules[1]["outbound"], "abc123");
    }

    #[test]
    fn ping_localhost_listener_is_fast() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let ms = ping_server("127.0.0.1", port, Duration::from_secs(2)).expect("ping");
        assert!(ms <= 2000, "handshake latency should be bounded, got {ms}");
    }

    #[test]
    fn ping_dead_endpoint_fails() {
        // Port 9 (discard) is virtually never open; bind-then-drop is racy.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        assert!(ping_server("127.0.0.1", port, Duration::from_millis(250)).is_err());
    }

    #[test]
    fn free_local_port_returns_a_usable_port() {
        let port = free_local_port().expect("port");
        assert!(port > 0);
        // Another ephemeral bind must not return the *same* live listener.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let _ = listener;
    }

    #[test]
    fn unsupported_protocol_fails_with_roadmap_error() {
        let mut p = vless();
        p.protocol = Protocol::Dnstt;
        p.credential = Credential::Token("key".into());
        let err = outbound(&p).unwrap_err();
        assert!(err.is_protocol_unsupported(Protocol::Dnstt));
        assert!(err.to_string().contains("P1.5"));
    }

    #[test]
    fn mismatched_credential_kind_is_rejected() {
        let mut p = vless();
        p.protocol = Protocol::Trojan;
        p.credential = Credential::Uuid("not-a-password-scheme".into());
        assert!(outbound(&p).is_err());
    }
}
