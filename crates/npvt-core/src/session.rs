//! Shared engine session bookkeeping (the pid file).
//!
//! Every front end (CLI and GUI) reads and writes the same session file at
//! the npvtunnel data directory, so either one can see and stop the engine
//! started by the other. The JSON schema is stable: `pid`, `profile_id`,
//! `name`, `socks_port`, `started`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Snapshot of a running engine, persisted at `engine.pid`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// OS process id of the sing-box sidecar.
    pub pid: u32,
    /// Connected profile id.
    pub profile_id: String,
    /// Connected profile display name.
    pub name: String,
    /// Local SOCKS5 inbound port.
    pub socks_port: u16,
    /// Start time, unix seconds.
    pub started: u64,
}

/// Path of the session file.
pub fn session_file() -> Result<PathBuf> {
    Ok(crate::engine::work_dir()?.join("engine.pid"))
}

/// Load a session file at an explicit path. A corrupt file is an error,
/// matching the historical CLI contract; a missing file is `None`.
pub fn read_session_at(path: &Path) -> Result<Option<Session>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(Error::Io)?;
    let session: Session = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Engine(format!("corrupt pid file: {e}")))?;
    Ok(Some(session))
}

/// Persist a session file at an explicit path (parent dirs created).
pub fn write_session_at(path: &Path, session: &Session) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec(session)?).map_err(Error::Io)?;
    Ok(())
}

/// Remove a session file at an explicit path, if present.
pub fn clear_session_at(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path).map_err(Error::Io)?;
    }
    Ok(())
}

/// Read the default session file.
pub fn read_session() -> Result<Option<Session>> {
    read_session_at(&session_file()?)
}

/// Write the default session file.
pub fn write_session(session: &Session) -> Result<()> {
    write_session_at(&session_file()?, session)
}

/// Remove the default session file.
pub fn clear_session() -> Result<()> {
    clear_session_at(&session_file()?)
}

/// Live state of the engine as seen through a session file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    /// Session file present and the pid is alive.
    Running(Session),
    /// Session file present but the pid is gone (stale entry).
    Stale(Session),
    /// No session file.
    NotRunning,
}

/// Probe the engine state for a session file at an explicit path.
pub fn probe_at(path: &Path) -> Result<EngineStatus> {
    match read_session_at(path)? {
        Some(session) => {
            if crate::engine::alive(session.pid) {
                Ok(EngineStatus::Running(session))
            } else {
                Ok(EngineStatus::Stale(session))
            }
        }
        None => Ok(EngineStatus::NotRunning),
    }
}

/// Probe the default session file.
pub fn probe() -> Result<EngineStatus> {
    probe_at(&session_file()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session {
            pid: 4242,
            profile_id: "abc123".into(),
            name: "shop".into(),
            socks_port: 2080,
            started: 1_700_000_000,
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("npvt-session-{}-{}.json", tag, std::process::id()));
        p
    }

    #[test]
    fn session_roundtrip_and_clear() {
        let path = temp_path("roundtrip");
        write_session_at(&path, &sample()).expect("write");
        let loaded = read_session_at(&path).expect("read").expect("present");
        assert_eq!(loaded, sample());
        clear_session_at(&path).expect("clear");
        assert!(read_session_at(&path).expect("read").is_none());
        assert!(matches!(
            probe_at(&path).expect("probe"),
            EngineStatus::NotRunning
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_session_is_an_error() {
        let path = temp_path("corrupt");
        std::fs::write(&path, b"{not json").expect("write junk");
        assert!(read_session_at(&path).is_err());
        clear_session_at(&path).expect("clear");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn probe_reports_stale_for_dead_pid() {
        // A pid far above any real pid space; guaranteed not to be alive.
        let dead = Session {
            pid: u32::MAX - 1,
            ..sample()
        };
        let path = temp_path("stale");
        write_session_at(&path, &dead).expect("write");
        match probe_at(&path).expect("probe") {
            EngineStatus::Stale(_) => {}
            other => panic!("expected stale, got {other:?}"),
        }
        clear_session_at(&path).expect("clear");
        let _ = std::fs::remove_file(&path);
    }
}
