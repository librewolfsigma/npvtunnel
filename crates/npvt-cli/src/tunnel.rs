//! Engine session commands: `connect`, `disconnect` and `status`.

use std::path::Path;
use std::time::Duration;

use npvt_core::engine;
use npvt_core::profile::unix_now;
use npvt_core::session::{self, Session};
use npvt_core::vault::find_profile;
use npvt_core::{Error, Result};

use crate::session::{log, open_vault};
use crate::Cli;

pub(crate) fn cmd_connect(
    cli: &Cli,
    target: &str,
    socks_port: u16,
    engine_path: Option<&Path>,
    dns: &str,
) -> Result<()> {
    let (_, data) = open_vault(cli)?;
    let profile = find_profile(&data, target)?.clone();

    if let Some(existing) = session::read_session()? {
        if engine::alive(existing.pid) {
            return Err(Error::Engine(format!(
                "an engine is already running (pid {}, profile '{}'); disconnect first",
                existing.pid, existing.name
            )));
        }
    }

    let workdir = engine::work_dir()?;
    let cfg = engine::config(
        std::slice::from_ref(&profile),
        &profile.id,
        socks_port,
        dns,
        None,
    )?;
    let cfg_path = workdir.join("engine.json");
    engine::write_config(&cfg, &cfg_path)?;
    log(
        cli,
        &format!("engine config written to {}", cfg_path.display()),
    );

    let bin = engine::find_engine_binary(engine_path).ok_or_else(|| {
        Error::Engine(
            "sing-box binary not found; place it next to npvt-cli, in the npvtunnel data directory under bin/, or on PATH (npvt-cli doctor shows what was checked)"
                .into(),
        )
    })?;

    let running = engine::start(&bin, &cfg_path, &workdir.join("engine.log"))?;
    // Give the engine a moment, then verify it is still alive: a malformed
    // config or a port conflict would otherwise die silently right after a
    // "tunnel up" message.
    std::thread::sleep(Duration::from_millis(600));
    if !engine::alive(running.pid) {
        let tail = engine::log_tail(&workdir.join("engine.log"), 20);
        return Err(Error::Engine(format!(
            "sing-box exited immediately after start; last engine log lines:\n{tail}\n(engine config: {})\n(run 'npvt-cli doctor' to check the engine setup)",
            cfg_path.display()
        )));
    }
    session::write_session(&Session {
        pid: running.pid,
        profile_id: profile.id.clone(),
        name: profile.name.clone(),
        socks_port,
        started: unix_now(),
    })?;

    println!(
        "Tunnel up: '{}' ({} {}:{}) - sing-box pid {}",
        profile.name,
        profile.protocol.as_str(),
        profile.server,
        profile.port,
        running.pid
    );
    println!("SOCKS5 proxy listening on 127.0.0.1:{socks_port} (localhost only).");
    Ok(())
}

pub(crate) fn cmd_disconnect(cli: &Cli) -> Result<()> {
    match session::read_session()? {
        Some(pf) if engine::alive(pf.pid) => {
            engine::stop(pf.pid)?;
            session::clear_session()?;
            println!("Engine stopped (pid {}).", pf.pid);
            Ok(())
        }
        Some(pf) => {
            session::clear_session()?;
            println!("Stale pid file removed (pid {} was not running).", pf.pid);
            Ok(())
        }
        None => {
            log(cli, "no pid file; nothing to do");
            println!("Engine is not running.");
            Ok(())
        }
    }
}

pub(crate) fn cmd_status(_cli: &Cli, json: bool) -> Result<()> {
    let pf = session::read_session()?;
    let running = pf.as_ref().is_some_and(|p| engine::alive(p.pid));
    if json {
        let value = match (&pf, running) {
            (Some(p), true) => serde_json::json!({
                "running": true, "pid": p.pid, "profile": p.name,
                "profile_id": p.profile_id, "socks_port": p.socks_port,
                "started": p.started,
            }),
            (Some(p), false) => serde_json::json!({
                "running": false, "stale_pid": p.pid, "profile": p.name,
            }),
            (None, _) => serde_json::json!({"running": false}),
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    match (pf, running) {
        (Some(p), true) => println!(
            "running: pid {} profile '{}' socks 127.0.0.1:{} since unix {}",
            p.pid, p.name, p.socks_port, p.started
        ),
        (Some(p), false) => println!(
            "not running (stale pid {} from profile '{}')",
            p.pid, p.name
        ),
        (None, _) => println!("not running"),
    }
    Ok(())
}
