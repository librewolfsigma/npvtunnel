//! Small network helpers: subscription fetches and geo-asset downloads.
//!
//! All calls happen on background threads (never on the UI thread) and
//! report progress through plain shared handles.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Download progress snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadState {
    /// "running" | "done" | "error".
    pub phase: String,
    pub got: u64,
    pub total: u64,
    pub error: Option<String>,
}

/// Official sing-geoip / sing-geosite release URLs.
pub const GEOIP_URL: &str =
    "https://github.com/SagerNet/sing-geoip/releases/latest/download/geoip.db";
pub const GEOSITE_URL: &str =
    "https://github.com/SagerNet/sing-geosite/releases/latest/download/geosite.db";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent("npvtunnel-gui")
        .build()
}

/// GET a small text document (subscription bodies, etc.).
pub fn fetch_text(url: &str) -> Result<String, String> {
    let resp = agent().get(url).call().map_err(|e| format!("{e}"))?;
    let text = resp.into_string().map_err(|e| format!("{e}"))?;
    if text.len() > 4 * 1024 * 1024 {
        return Err("subscription body is implausibly large".into());
    }
    Ok(text)
}

/// Download `url` to `dest`, streaming into `state`; honors `cancel`.
pub fn download_to(url: &str, dest: &Path, state: &Arc<Mutex<DownloadState>>, cancel: &AtomicBool) {
    let set = |phase: &str, got: u64, total: u64, error: Option<String>| {
        if let Ok(mut guard) = state.lock() {
            *guard = DownloadState {
                phase: phase.into(),
                got,
                total,
                error,
            };
        }
    };
    set("running", 0, 0, None);
    let resp = match agent().get(url).call() {
        Ok(r) => r,
        Err(e) => {
            set("error", 0, 0, Some(format!("{e}")));
            return;
        }
    };
    let total = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = match std::fs::File::create(dest) {
        Ok(f) => f,
        Err(e) => {
            set("error", 0, 0, Some(format!("cannot write file: {e}")));
            return;
        }
    };
    use std::io::Read as _;
    use std::io::Write as _;
    let mut got = 0u64;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            set("error", got, total, Some("cancelled".into()));
            let _ = std::fs::remove_file(dest);
            return;
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if file.write_all(&chunk[..n]).is_err() {
                    set("error", got, total, Some("disk write failed".into()));
                    return;
                }
                got += n as u64;
                set("running", got, total, None);
            }
            Err(e) => {
                set("error", got, total, Some(format!("{e}")));
                return;
            }
        }
    }
    set("done", got, total, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_state_defaults() {
        let s = DownloadState::default();
        assert_eq!(s.phase, "");
        assert!(s.error.is_none());
    }
}
