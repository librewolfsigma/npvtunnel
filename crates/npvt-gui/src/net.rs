//! Network helpers: subscription fetching (with traffic metadata), QR image
//! decoding and geo asset downloads. Plain blocking I/O on worker threads.

use base64::Engine as _;
use std::io::Read;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::settings::SubInfo;

const USER_AGENT: &str = concat!("NPVTunnel/", env!("CARGO_PKG_VERSION"));

/// Parse a `subscription-userinfo` header value.
pub fn parse_userinfo(value: &str) -> SubInfo {
    let mut info = SubInfo::default();
    for part in value.split(';') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            let num: u64 = val.trim().trim_end_matches("\r").parse().unwrap_or(0);
            match key.trim().to_ascii_lowercase().as_str() {
                "upload" => info.upload = num,
                "download" => info.download = num,
                "total" => info.total = num,
                "expire" => info.expire = num,
                _ => {}
            }
        }
    }
    info
}

fn client() -> Result<ureq::Agent, String> {
    Ok(ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build())
}

/// Fetch a subscription: returns (userinfo, body text).
pub fn fetch_subscription(url: &str) -> Result<(SubInfo, String), String> {
    let agent = client()?;
    let resp = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("subscription fetch failed: {e}"))?;
    let userinfo = resp
        .header("subscription-userinfo")
        .map(parse_userinfo)
        .unwrap_or_default();
    let body = resp
        .into_string()
        .map_err(|e| format!("subscription body read failed: {e}"))?;
    if body.len() > 8 * 1024 * 1024 {
        return Err("subscription body is implausibly large".to_owned());
    }
    Ok((userinfo, body))
}

/// Decode possible base64 subscription bodies into link lines.
fn has_scheme(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "vless://",
        "vmess://",
        "trojan://",
        "ss://",
        "hysteria2://",
        "hysteria://",
        "socks5://",
        "socks://",
        "http://",
        "https://",
        "npvt://",
        "npvs://",
        "tuic://",
        "juicity://",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
}

fn push_link(links: &mut Vec<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    if line.len() <= npvt_core::uri::MAX_URI_LEN {
        links.push(line.to_owned());
    }
}

pub fn extract_links(body: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    for line in body.lines() {
        if has_scheme(line) {
            push_link(&mut links, line);
        }
    }
    if links.is_empty() {
        // Whole body might be base64 of a link list.
        if let Ok(decoded) = base64_decode_flex(body) {
            let text = String::from_utf8_lossy(&decoded).to_string();
            for line in text.lines() {
                if has_scheme(line) {
                    push_link(&mut links, line);
                }
            }
        }
    }
    links
}

fn base64_decode_flex(data: &str) -> Result<Vec<u8>, String> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
    let cleaned: String = data
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    for engine in [URL_SAFE_NO_PAD, URL_SAFE, STANDARD] {
        if let Ok(v) = engine.decode(cleaned.as_bytes()) {
            return Ok(v);
        }
    }
    Err("invalid base64".to_owned())
}

/// Decode QR codes from an image file; returns all payload strings.
pub fn decode_qr_file(path: &std::path::Path) -> Result<Vec<String>, String> {
    let img = image::open(path).map_err(|e| format!("cannot open image: {e}"))?;
    let luma = img.to_luma8();
    let mut decoder = quircs::Quirc::default();
    let mut payloads = Vec::new();
    for code in decoder.identify(luma.width() as usize, luma.height() as usize, &luma) {
        let code = match code {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(data) = code.decode() {
            payloads.push(String::from_utf8_lossy(&data.payload).to_string());
        }
    }
    Ok(payloads)
}

/// Progress handle for asset downloads.
#[derive(Debug, Default, Clone)]
pub struct AssetProgress {
    pub stop: Arc<AtomicBool>,
    pub received: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
}

impl AssetProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.received.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Download one geo asset to `dest` with progress updates.
pub fn download_geo_asset(
    url: &str,
    dest: &std::path::Path,
    progress: &AssetProgress,
) -> Result<(), String> {
    let agent = client()?;
    let resp = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if let Some(len) = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
    {
        progress.total.store(len, Ordering::Relaxed);
    }
    let mut reader = resp.into_reader();
    let tmp = dest.with_extension("download");
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("cannot create file: {e}"))?;
    let mut buf = [0u8; 16 * 1024];
    let mut received: u64 = 0;
    loop {
        if progress.stop.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&tmp);
            return Err("download cancelled".to_owned());
        }
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                file.write_all(&buf[..n])
                    .map_err(|e| format!("write failed: {e}"))?;
                received += n as u64;
                progress.received.store(received, Ordering::Relaxed);
            }
            Err(e) => return Err(format!("download interrupted: {e}")),
        }
    }
    drop(file);
    std::fs::rename(&tmp, dest).map_err(|e| format!("cannot finalize download: {e}"))?;
    Ok(())
}

/// Render a byte count like the design's "20.3 GB / 93.1 GB".
pub fn gb_str(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 100.0 {
        format!("{gb:.0} GB")
    } else {
        format!("{gb:.1} GB")
    }
}

/// Alias so the engine bridge can reference the settings type without a
/// dependency cycle.
pub type SubInfoAlias = crate::settings::SubInfo;

pub const GEOIP_URL: &str =
    "https://github.com/SagerNet/sing-geoip/releases/latest/download/geoip.db";
pub const GEOSITE_URL: &str =
    "https://github.com/SagerNet/sing-geosite/releases/latest/download/geosite.db";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_userinfo_header() {
        let info =
            parse_userinfo("upload=12345; download=6789; total=107374182400; expire=1735689600");
        assert_eq!(info.upload, 12345);
        assert_eq!(info.download, 6789);
        assert_eq!(info.total, 107374182400);
        assert_eq!(info.expire, 1735689600);
    }

    #[test]
    fn userinfo_garbage_is_tolerated() {
        let info = parse_userinfo("garbage; total=abc");
        assert_eq!(info.total, 0);
    }

    #[test]
    fn extracts_scheme_lines_only() {
        let body = "# comment\nvless://uuid@h:1?a=1#n\nnot a link\nss://bmV4dA==@h:2#m\n";
        let links = extract_links(body);
        assert_eq!(links.len(), 2);
        assert!(links[0].starts_with("vless://"));
        assert!(links[1].starts_with("ss://"));
    }

    #[test]
    fn extracts_base64_bodies() {
        use base64::engine::general_purpose::STANDARD as B64;
        let inner = "vless://uuid@example.com:443?security=tls#shop";
        let body = B64.encode(inner);
        let links = extract_links(&body);
        assert_eq!(links, vec![inner.to_owned()]);
    }

    #[test]
    fn base64_flex_handles_padding_variants() {
        assert!(base64_decode_flex("aGVsbG8").is_ok());
        assert!(base64_decode_flex("aGVsbG8=").is_ok());
        assert!(base64_decode_flex("!!!").is_err());
    }

    #[test]
    fn gb_formatting() {
        assert_eq!(gb_str(20_300_000_000), "18.9 GB");
        assert_eq!(gb_str(107_374_182_400), "100 GB");
    }
}
