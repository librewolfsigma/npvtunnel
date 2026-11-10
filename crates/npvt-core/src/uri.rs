//! Import sources: share URIs (vless/vmess/ss/trojan/hysteria2/socks/http)
//! and base64 `npvt://` links that carry a v2 container.
//!
//! Parsing is strict: length caps on the whole input and every field, unknown
//! schemes rejected, percent-decoding explicit, and every resulting profile
//! goes through [`Profile::validate`] before it can be stored.

use base64::Engine as _;
use serde_json::Value;
use url::Url;

use crate::error::{Error, Result};
use crate::profile::{
    new_profile_id, unix_now, validate_field, Credential, Profile, Protocol, Transport,
    MAX_FIELD_LEN, MAX_NAME_LEN,
};

/// Input length cap for a single share link.
pub const MAX_URI_LEN: usize = 16 * 1024;

/// What a share link or `npvt://` link contained.
#[derive(Debug, Clone)]
pub enum ImportSource {
    /// A plain profile (from share URIs and plaintext legacy JSON).
    Profile(Profile),
    /// An encoded v2 container (from `npvt://` links), ready for
    /// [`crate::npvt::v2::decode`].
    Container(Vec<u8>),
}

fn b64_flex(data: &str) -> Result<Vec<u8>> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
    for engine in [URL_SAFE_NO_PAD, URL_SAFE, STANDARD] {
        if let Ok(v) = engine.decode(data.trim()) {
            return Ok(v);
        }
    }
    Err(Error::InvalidConfig("invalid base64 payload".into()))
}

fn percent_decode_to_string(raw: &str) -> Result<String> {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map(|c| c.to_string())
        .map_err(|_| Error::InvalidConfig("invalid percent-encoding in link".into()))
}

fn clean_incoming(field: &str, what: &str) -> Result<String> {
    let v = field.trim().to_string();
    validate_field(&v, what)?;
    Ok(v)
}

fn clean_name(field: &str, default: &str) -> Result<String> {
    let v = if field.trim().is_empty() {
        default.to_string()
    } else {
        percent_decode_to_string(field.trim())?
    };
    let v = validate_field(&v, "profile name").map(|_| v)?;
    Ok(v.chars().take(MAX_NAME_LEN).collect())
}

type Params = std::collections::BTreeMap<String, String>;

fn query_map(url: &Url) -> Params {
    url.query_pairs()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
        .collect()
}

fn is_tls(params: &Params) -> bool {
    matches!(
        params
            .get("security")
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("tls") | Some("reality")
    )
}

fn insecure_flag(params: &Params) -> bool {
    matches!(
        params
            .get("allowinsecure")
            .or_else(|| params.get("insecure"))
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("1") | Some("true")
    )
}

fn udp_flag(params: &Params) -> bool {
    !matches!(
        params.get("udp").map(|s| s.to_ascii_lowercase()).as_deref(),
        Some("0") | Some("false")
    )
}

fn transport_from_params(params: &Params) -> Transport {
    let net = params
        .get("type")
        .or_else(|| params.get("net"))
        .map(|s| s.to_ascii_lowercase());
    match net.as_deref() {
        Some("ws") => Transport::Ws,
        Some("grpc") => Transport::Grpc,
        Some("h2") => Transport::H2,
        _ => Transport::Plain,
    }
}

fn clean_opt(value: Option<String>, what: &str) -> Result<Option<String>> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(Some(clean_incoming(&v, what)?)),
        _ => Ok(None),
    }
}

/// Parse one share URI into a profile.
pub fn parse_uri(input: &str) -> Result<Profile> {
    let trimmed = input.trim();
    if trimmed.len() > MAX_URI_LEN {
        return Err(Error::InvalidConfig("link is too long".into()));
    }
    if trimmed.contains(char::is_control) {
        return Err(Error::InvalidConfig(
            "link contains control characters".into(),
        ));
    }
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => {
            return Err(Error::InvalidConfig(
                "not a share link (missing ://)".into(),
            ))
        }
    };

    match scheme.as_str() {
        "vmess" => {
            let raw = b64_flex(rest)?;
            let text = std::str::from_utf8(&raw)
                .map_err(|_| Error::InvalidConfig("vmess payload is not valid UTF-8".into()))?;
            let json: Value = serde_json::from_str(text.trim()).map_err(|e| {
                Error::InvalidConfig(format!("vmess payload is not valid JSON: {e}"))
            })?;
            map_vmess_json(&json, "imported vmess profile")
        }
        "ss" => parse_ss(rest),
        "vless" | "trojan" | "hysteria2" | "hy2" | "socks" | "socks5" | "socks5h" | "http"
        | "https" => parse_generic(trimmed, &scheme),
        other => Err(Error::InvalidConfig(format!(
            "unsupported link scheme '{other}'; supported: vless, vmess, ss, trojan, hysteria2, \
             socks, http(s), npvt"
        ))),
    }
}

fn parse_generic(trimmed: &str, scheme: &str) -> Result<Profile> {
    let url =
        Url::parse(trimmed).map_err(|e| Error::InvalidConfig(format!("malformed link: {e}")))?;
    let params = query_map(&url);
    let server = clean_incoming(url.host_str().unwrap_or_default(), "server")?;
    if server.is_empty() {
        return Err(Error::InvalidConfig("link has no server host".into()));
    }
    let port = url.port().unwrap_or(match scheme {
        "vless" | "trojan" | "hysteria2" | "hy2" | "https" => 443,
        "http" => 8080,
        _ => 1080,
    });
    let name = match url.fragment() {
        Some(f) => clean_name(f, "imported profile")?,
        None => "imported profile".to_string(),
    };
    let userinfo = percent_decode_to_string(url.username())?;
    let password = url
        .password()
        .map(percent_decode_to_string)
        .transpose()?
        .unwrap_or_default();

    let base_transport = transport_from_params(&params);
    let tls = is_tls(&params);

    let (protocol, credential, transport) = match scheme {
        "vless" => (
            Protocol::Vless,
            Credential::Uuid(userinfo),
            if tls { Transport::Tls } else { base_transport },
        ),
        "trojan" => (
            Protocol::Trojan,
            Credential::Password(userinfo),
            if tls { Transport::Tls } else { base_transport },
        ),
        "hysteria2" | "hy2" => (
            Protocol::Hysteria2,
            Credential::Password(userinfo),
            Transport::Tls,
        ),
        "socks" | "socks5" | "socks5h" => (
            Protocol::Socks,
            if userinfo.is_empty() {
                Credential::None
            } else {
                Credential::Token(format!("{userinfo}:{password}"))
            },
            Transport::Plain,
        ),
        "http" => (
            Protocol::Http,
            if userinfo.is_empty() {
                Credential::None
            } else {
                Credential::Token(format!("{userinfo}:{password}"))
            },
            if tls {
                Transport::Tls
            } else {
                Transport::Plain
            },
        ),
        "https" => (
            Protocol::Http,
            if userinfo.is_empty() {
                Credential::None
            } else {
                Credential::Token(format!("{userinfo}:{password}"))
            },
            Transport::Tls,
        ),
        other => unreachable!("caller only dispatches known schemes, got {other}"),
    };

    let sni = clean_opt(
        params.get("sni").or_else(|| params.get("peer")).cloned(),
        "sni",
    )?;
    let path = clean_opt(
        params
            .get("path")
            .or_else(|| params.get("servicename"))
            .cloned(),
        "path",
    )?;

    let p = Profile {
        id: new_profile_id(),
        name,
        protocol,
        server,
        port,
        credential,
        transport,
        sni,
        path,
        allow_insecure: insecure_flag(&params),
        udp: udp_flag(&params),
        group: None,
        created_at: unix_now(),
    };
    p.validate()?;
    Ok(p)
}

/// Parse `ss://` (SIP002-style) manually: the userinfo can be
/// `base64(method:password)` or plain `method:password`, or the whole body
/// after `ss://` can be `base64(method:password@host:port)`.
fn parse_ss(rest: &str) -> Result<Profile> {
    let (body, fragment) = match rest.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (rest, None),
    };
    let body = body.split('?').next().unwrap_or(body);
    let name = match fragment {
        Some(f) => clean_name(f, "imported ss profile")?,
        None => "imported ss profile".to_string(),
    };

    let (method, password, host, port) = if let Some((userinfo, hostport)) = body.rsplit_once('@') {
        let (m, p) = split_method_password(userinfo)?;
        let (h, port) = split_host_port(hostport)?;
        (m, p, h, port)
    } else {
        let decoded = b64_flex(body)?;
        let text = String::from_utf8(decoded)
            .map_err(|_| Error::InvalidConfig("ss payload is not valid UTF-8".into()))?;
        if text.len() > MAX_FIELD_LEN {
            return Err(Error::InvalidConfig("ss payload is too long".into()));
        }
        let (mp, hostport) = text
            .split_once('@')
            .ok_or_else(|| Error::InvalidConfig("ss payload missing @host:port".into()))?;
        let (m, p) = split_method_password(mp)?;
        let (h, port) = split_host_port(hostport)?;
        (m, p, h, port)
    };

    let p = Profile {
        id: new_profile_id(),
        name,
        protocol: Protocol::Shadowsocks,
        server: clean_incoming(&host, "server")?,
        port,
        credential: Credential::SsMethod {
            method: clean_incoming(&method, "shadowsocks method")?,
            password: clean_incoming(&password, "shadowsocks password")?,
        },
        transport: Transport::Plain,
        sni: None,
        path: None,
        allow_insecure: false,
        udp: true,
        group: None,
        created_at: unix_now(),
    };
    p.validate()?;
    Ok(p)
}

fn split_method_password(s: &str) -> Result<(String, String)> {
    // Either base64(method:password) or plain (percent-decoded) method:password.
    if s.contains(':') && !s.contains('+') && !s.contains('/') && !s.contains('=') {
        let decoded = percent_decode_to_string(s)?;
        return decoded
            .split_once(':')
            .map(|(m, p)| (m.to_string(), p.to_string()))
            .ok_or_else(|| Error::InvalidConfig("ss userinfo missing method:password".into()));
    }
    let decoded = b64_flex(s)?;
    let text = String::from_utf8(decoded)
        .map_err(|_| Error::InvalidConfig("ss userinfo is not valid UTF-8".into()))?;
    text.split_once(':')
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .ok_or_else(|| Error::InvalidConfig("ss userinfo missing method:password".into()))
}

fn split_host_port(s: &str) -> Result<(String, u16)> {
    if let Some(stripped) = s.strip_prefix('[') {
        let (host, tail) = stripped
            .split_once(']')
            .ok_or_else(|| Error::InvalidConfig("ss host: malformed IPv6 literal".into()))?;
        let port = tail
            .strip_prefix(':')
            .ok_or_else(|| Error::InvalidConfig("ss host: missing port".into()))?;
        return Ok((host.to_string(), parse_port(port)?));
    }
    let (host, port) = s
        .rsplit_once(':')
        .ok_or_else(|| Error::InvalidConfig("ss host: missing port".into()))?;
    Ok((host.to_string(), parse_port(port)?))
}

fn parse_port(s: &str) -> Result<u16> {
    let p: u16 = s
        .parse()
        .map_err(|_| Error::InvalidConfig("port must be 1-65535".into()))?;
    if p == 0 {
        return Err(Error::InvalidConfig("port must be 1-65535".into()));
    }
    Ok(p)
}

/// Tolerant mapper for the VMess-style JSON configuration shape used by
/// `vmess://` payloads and plaintext legacy v1 files.
///
/// Accepted keys (values may be JSON strings or numbers): `add`/`address`,
/// `port`, `id`/`uuid`, `ps`/`name`/`remark`, `net`/`type`, `path`, `tls`,
/// `sni`/`peer`.
pub fn map_vmess_json(value: &Value, default_name: &str) -> Result<Profile> {
    let obj = value
        .as_object()
        .ok_or_else(|| Error::InvalidConfig("config JSON must be an object".into()))?;
    let get = |keys: &[&str]| -> Option<String> {
        for k in keys {
            match obj.get(*k) {
                Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
                Some(Value::Number(n)) => return Some(n.to_string()),
                _ => continue,
            }
        }
        None
    };

    let server = clean_incoming(
        &get(&["add", "address", "server", "host"])
            .ok_or_else(|| Error::InvalidConfig("config JSON missing server (add)".into()))?,
        "server",
    )?;
    let port: u16 = get(&["port"])
        .ok_or_else(|| Error::InvalidConfig("config JSON missing port".into()))?
        .parse()
        .map_err(|_| Error::InvalidConfig("config JSON port must be 1-65535".into()))?;
    let uuid = get(&["id", "uuid"]).unwrap_or_default();
    if uuid.is_empty() {
        return Err(Error::InvalidConfig(
            "config JSON missing credential (id)".into(),
        ));
    }
    let name = clean_name(
        &get(&["ps", "name", "remark"]).unwrap_or_else(|| default_name.to_string()),
        default_name,
    )?;
    let net = get(&["net"])
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".into());
    let base_transport = match net.as_str() {
        "ws" => Transport::Ws,
        "grpc" => Transport::Grpc,
        "h2" => Transport::H2,
        _ => Transport::Plain,
    };
    let tls = get(&["tls"])
        .map(|s| s.eq_ignore_ascii_case("tls"))
        .unwrap_or(false);

    let p = Profile {
        id: new_profile_id(),
        name,
        protocol: Protocol::Vmess,
        server,
        port,
        credential: Credential::Uuid(clean_incoming(&uuid, "credential")?),
        transport: if tls { Transport::Tls } else { base_transport },
        sni: clean_opt(get(&["sni", "peer"]), "sni")?,
        path: clean_opt(get(&["path"]), "path")?,
        allow_insecure: false,
        udp: true,
        group: None,
        created_at: unix_now(),
    };
    p.validate()?;
    Ok(p)
}

/// Parse any supported import source: a share URI or an `npvt://` container
/// link.
pub fn parse_import(input: &str) -> Result<ImportSource> {
    let trimmed = input.trim();
    if trimmed.len() > MAX_URI_LEN {
        return Err(Error::InvalidConfig("input is too long".into()));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("npvt://") || lower.starts_with("npvs://") {
        let body = trimmed.split_once("://").expect("prefix checked").1;
        let body = body.split('#').next().unwrap_or(body);
        if body.len() > crate::npvt::v2::MAX_CONTAINER_B64_LEN {
            return Err(Error::InvalidConfig("container link is too large".into()));
        }
        return Ok(ImportSource::Container(b64_flex(body)?));
    }
    if trimmed.contains("://") {
        return Ok(ImportSource::Profile(parse_uri(trimmed)?));
    }
    Err(Error::InvalidConfig(
        "input is neither an existing file nor a supported link".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64U;

    #[test]
    fn parses_vless_with_tls_and_ws() {
        let p = parse_uri(
            "vless://11111111-2222-3333-4444-555555555555@example.com:443?security=tls&type=ws&path=%2Fws&sni=cdn.example.com&udp=1#My%20Shop",
        )
        .expect("parse");
        assert_eq!(p.protocol, Protocol::Vless);
        assert_eq!(p.server, "example.com");
        assert_eq!(p.port, 443);
        assert_eq!(p.transport, Transport::Tls);
        assert_eq!(p.path.as_deref(), Some("/ws"));
        assert_eq!(p.sni.as_deref(), Some("cdn.example.com"));
        assert_eq!(p.name, "My Shop");
        assert!(p.validate().is_ok());
    }

    #[test]
    fn parses_trojan_with_insecure_flag() {
        let p = parse_uri("trojan://passw0rd@server.io:8443?allowInsecure=1#t").expect("parse");
        assert_eq!(p.protocol, Protocol::Trojan);
        assert!(p.allow_insecure);
        assert_eq!(p.port, 8443);
    }

    #[test]
    fn parses_vmess_base64_json() {
        let json = serde_json::json!({
            "v": "2", "ps": "shop", "add": "vm.example.com", "port": "8443",
            "id": "11111111-2222-3333-4444-555555555555", "aid": "0",
            "net": "ws", "path": "/v", "tls": "tls", "sni": "s.example.com"
        });
        let link = format!("vmess://{}", B64.encode(json.to_string()));
        let p = parse_uri(&link).expect("parse");
        assert_eq!(p.protocol, Protocol::Vmess);
        assert_eq!(p.server, "vm.example.com");
        assert_eq!(p.port, 8443);
        assert_eq!(p.transport, Transport::Tls);
        assert_eq!(p.sni.as_deref(), Some("s.example.com"));
    }

    #[test]
    fn parses_ss_base64_userinfo() {
        let userinfo = B64.encode("aes-256-gcm:sesame");
        let p = parse_uri(&format!("ss://{userinfo}@1.2.3.4:8388#enc")).expect("parse");
        assert_eq!(p.protocol, Protocol::Shadowsocks);
        match &p.credential {
            Credential::SsMethod { method, password } => {
                assert_eq!(method, "aes-256-gcm");
                assert_eq!(password, "sesame");
            }
            other => panic!("unexpected credential: {other:?}"),
        }
    }

    #[test]
    fn parses_ss_whole_body_base64() {
        let body = B64.encode("aes-128-gcm:pw@[2001:db8::1]:8388");
        let p = parse_uri(&format!("ss://{body}#v6")).expect("parse");
        assert_eq!(p.server, "2001:db8::1");
        assert_eq!(p.port, 8388);
    }

    #[test]
    fn parses_hysteria2() {
        let p =
            parse_uri("hysteria2://auth-token@h.example.com:443?sni=h.example.com&insecure=1#hy2")
                .expect("parse");
        assert_eq!(p.protocol, Protocol::Hysteria2);
        assert!(p.allow_insecure);
        assert_eq!(p.transport, Transport::Tls);
        match &p.credential {
            Credential::Password(t) => assert_eq!(t, "auth-token"),
            other => panic!("unexpected credential: {other:?}"),
        }
    }

    #[test]
    fn parses_socks_and_https() {
        let p = parse_uri("socks5://user:secret@gw.local:1080#lan").expect("parse");
        assert_eq!(p.protocol, Protocol::Socks);
        assert_eq!(p.transport, Transport::Plain);
        match &p.credential {
            Credential::Token(t) => assert_eq!(t, "user:secret"),
            other => panic!("unexpected credential: {other:?}"),
        }
        let p = parse_uri("https://u:p@proxy.example.net#hp").expect("parse");
        assert_eq!(p.protocol, Protocol::Http);
        assert_eq!(p.transport, Transport::Tls);
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse_uri("gopher://x").is_err());
        assert!(parse_uri("javascript:alert(1)").is_err());
    }

    #[test]
    fn npvt_link_yields_container() {
        let payload = b"NPVT2\0xx";
        let link = format!("npvt://{}", B64U.encode(payload));
        match parse_import(&link).expect("parse") {
            ImportSource::Container(b) => assert_eq!(&b[..], payload),
            other => panic!("unexpected source: {other:?}"),
        }
    }

    #[test]
    fn rejects_oversized_input() {
        let big = "vless://a@b:1#".to_string() + &"x".repeat(MAX_URI_LEN + 1);
        assert!(parse_import(&big).is_err());
    }
}
