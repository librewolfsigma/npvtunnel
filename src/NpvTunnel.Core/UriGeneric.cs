// Generic (vless/trojan/hysteria2/socks/http) and ss:// share-link parsing.
// Parsed manually (not via System.Uri) so exotic schemes behave exactly
// like the Rust `url` crate paths they were ported from.

using System.Text;

namespace NpvTunnel.Core;

/// <summary>Parsers for the authority-style share links.</summary>
public static class UriGeneric
{
    /// <summary>Parse vless/trojan/hysteria2/socks/http(s) links.</summary>
    public static Profile ParseGeneric(string trimmed, string scheme)
    {
        var (authority, query, fragment) = SplitLink(trimmed);
        var (userinfo, hostport, linkPassword) = SplitUserInfo(authority);
        var (host, explicitPort) = SplitHostPortOptional(hostport);
        var server = ImportText.CleanIncoming(host, "server");
        if (server.Length == 0)
        {
            throw NpvException.InvalidConfig("link has no server host");
        }

        var port = explicitPort ?? DefaultPort(scheme);
        var name = fragment is null
            ? "imported profile"
            : ImportText.CleanName(fragment, "imported profile");
        var user = ImportText.PercentDecode(userinfo);
        var password = linkPassword is null
            ? string.Empty
            : ImportText.PercentDecode(linkPassword);
        var parameters = QueryMap(query);

        var (protocol, credential, transport) = MapScheme(
            scheme, user, password, parameters);
        var sni = ImportText.CleanOpt(parameters.Get("sni") ?? parameters.Get("peer"), "sni");
        var path = ImportText.CleanOpt(
            parameters.Get("path") ?? parameters.Get("servicename"), "path");
        var profile = new Profile
        {
            Id = ProfileValidation.NewProfileId(),
            Name = name,
            Protocol = protocol,
            Server = server,
            Port = port,
            Credential = credential,
            Transport = transport,
            Sni = sni,
            Path = path,
            AllowInsecure = InsecureFlag(parameters),
            Udp = UdpFlag(parameters),
            Group = null,
            CreatedAt = ProfileValidation.UnixNow(),
        };
        profile.Validate();
        return profile;
    }

    /// <summary>Parse ss:// (SIP002-style): base64 or plain userinfo forms.</summary>
    public static Profile ParseSs(string rest)
    {
        var (body, fragment) = CutOnce(rest, '#');
        var q = body.IndexOf('?', StringComparison.Ordinal);
        if (q >= 0)
        {
            body = body[..q];
        }

        var name = fragment is null
            ? "imported ss profile"
            : ImportText.CleanName(fragment, "imported ss profile");
        var (method, password, host, port) = SplitSsBody(body);
        var profile = new Profile
        {
            Id = ProfileValidation.NewProfileId(),
            Name = name,
            Protocol = Protocol.Shadowsocks,
            Server = ImportText.CleanIncoming(host, "server"),
            Port = port,
            Credential = new SsMethodCredential(
                ImportText.CleanIncoming(method, "shadowsocks method"),
                ImportText.CleanIncoming(password, "shadowsocks password")),
            Transport = Transport.Plain,
            Sni = null,
            Path = null,
            AllowInsecure = false,
            Udp = true,
            Group = null,
            CreatedAt = ProfileValidation.UnixNow(),
        };
        profile.Validate();
        return profile;
    }

    private static (string Method, string Password, string Host, ushort Port)
        SplitSsBody(string body)
    {
        if (body.TryCutLast('@', out var userinfo, out var hostport))
        {
            var (m, p) = SplitMethodPassword(userinfo);
            var (h, port) = SplitHostPort(hostport);
            return (m, p, h, port);
        }

        var decoded = Encoding.UTF8.GetString(ImportText.B64Flex(body));
        if (decoded.Length > Limits.MaxFieldLen)
        {
            throw NpvException.InvalidConfig("ss payload is too long");
        }

        if (!decoded.TryCut('@', out var mp, out var hp))
        {
            throw NpvException.InvalidConfig("ss payload missing @host:port");
        }

        var (m2, p2) = SplitMethodPassword(mp);
        var (h2, port2) = SplitHostPort(hp);
        return (m2, p2, h2, port2);
    }

    private static (string, string) SplitMethodPassword(string s)
    {
        if (s.Contains(':') && !s.Contains('+') && !s.Contains('/')
            && !s.Contains('='))
        {
            var decoded = ImportText.PercentDecode(s);
            return decoded.TryCut(':', out var m, out var p)
                ? (m, p)
                : throw NpvException.InvalidConfig("ss userinfo missing method:password");
        }

        var text = Encoding.UTF8.GetString(ImportText.B64Flex(s));
        return text.TryCut(':', out var m2, out var p2)
            ? (m2, p2)
            : throw NpvException.InvalidConfig("ss userinfo missing method:password");
    }

    private static (string Host, ushort Port) SplitHostPort(string s)
    {
        var (host, port) = SplitHostPortOptional(s);
        if (port is null)
        {
            throw NpvException.InvalidConfig("host: missing port");
        }

        return (host, port.Value);
    }

    private static (string Host, ushort?) SplitHostPortOptional(string s)
    {
        if (s.StartsWith("[", StringComparison.Ordinal))
        {
            if (!s[1..].TryCut(']', out var host, out var tail))
            {
                throw NpvException.InvalidConfig("malformed IPv6 literal");
            }

            if (!tail.StartsWith(':'))
            {
                return (host, null);
            }

            return (host, ParsePort(tail[1..]));
        }

        return s.TryCutLast(':', out var h, out var p)
            ? ushort.TryParse(p, out var parsed) && parsed != 0
                ? (h, parsed)
                : throw NpvException.InvalidConfig("port must be 1-65535")
            : (s, null);
    }

    /// <summary>Parse a 1-65535 port number.</summary>
    public static ushort ParsePort(string s)
    {
        if (!ushort.TryParse(s, out var p) || p == 0)
        {
            throw NpvException.InvalidConfig("port must be 1-65535");
        }

        return p;
    }

    private static ushort DefaultPort(string scheme) => scheme switch
    {
        "vless" or "trojan" or "hysteria2" or "hy2" or "https" => 443,
        "http" => 8080,
        _ => 1080,
    };

    private static (Protocol, Credential, Transport) MapScheme(
        string scheme, string user, string password, Dictionary<string, string> parameters)
    {
        var baseTransport = TransportFromParams(parameters);
        var tls = IsTls(parameters);
        var token = new TokenCredential($"{user}:{password}");
        return scheme switch
        {
            "vless" => (Protocol.Vless, new UuidCredential(user),
                tls ? Transport.Tls : baseTransport),
            "trojan" => (Protocol.Trojan, new PasswordCredential(user),
                tls ? Transport.Tls : baseTransport),
            "hysteria2" or "hy2" => (Protocol.Hysteria2, new PasswordCredential(user),
                Transport.Tls),
            "socks" or "socks5" or "socks5h" => (Protocol.Socks,
                user.Length == 0 ? new NoneCredential() : token, Transport.Plain),
            "http" => (Protocol.Http,
                user.Length == 0 ? new NoneCredential() : token,
                tls ? Transport.Tls : Transport.Plain),
            "https" => (Protocol.Http,
                user.Length == 0 ? new NoneCredential() : token, Transport.Tls),
            _ => throw NpvException.InvalidConfig($"unsupported scheme '{scheme}'"),
        };
    }

    private static Transport TransportFromParams(Dictionary<string, string> p)
    {
        var net = (p.Get("type") ?? p.Get("net") ?? string.Empty).ToLowerInvariant();
        return net switch
        {
            "ws" => Transport.Ws,
            "grpc" => Transport.Grpc,
            "h2" => Transport.H2,
            _ => Transport.Plain,
        };
    }

    private static bool IsTls(Dictionary<string, string> p)
    {
        var security = (p.Get("security") ?? string.Empty).ToLowerInvariant();
        return security is "tls" or "reality";
    }

    private static bool InsecureFlag(Dictionary<string, string> p)
    {
        var raw = (p.Get("allowinsecure") ?? p.Get("insecure") ?? string.Empty)
            .ToLowerInvariant();
        return raw is "1" or "true";
    }

    private static bool UdpFlag(Dictionary<string, string> p)
    {
        var raw = (p.Get("udp") ?? string.Empty).ToLowerInvariant();
        return raw is not ("0" or "false");
    }

    private static Dictionary<string, string> QueryMap(string? query)
    {
        var map = new Dictionary<string, string>(StringComparer.Ordinal);
        if (query is null)
        {
            return map;
        }

        foreach (var pair in query.Split('&'))
        {
            if (pair.Length == 0)
            {
                continue;
            }

            var eq = pair.IndexOf('=');
            var key = (eq < 0 ? pair : pair[..eq]).ToLowerInvariant();
            var value = eq < 0 ? string.Empty : ImportText.PercentDecode(pair[(eq + 1)..]);
            map[key] = value;
        }

        return map;
    }

    private static (string Authority, string? Query, string? Fragment) SplitLink(string link)
    {
        var (head, fragment) = CutOnce(link, '#');
        var schemeEnd = head.IndexOf("://", StringComparison.Ordinal) + 3;
        var rest = head[schemeEnd..];
        var q = rest.IndexOf('?');
        var authority = q < 0 ? rest : rest[..q];
        var query = q < 0 ? null : rest[(q + 1)..];
        return (authority, query, fragment);
    }

    private static (string User, string HostPort, string? Password) SplitUserInfo(
        string authority)
    {
        var at = authority.LastIndexOf('@');
        if (at < 0)
        {
            return (string.Empty, authority, null);
        }

        var userinfo = authority[..at];
        var hostport = authority[(at + 1)..];
        var colon = userinfo.IndexOf(':');
        return colon < 0
            ? (userinfo, hostport, null)
            : (userinfo[..colon], hostport, userinfo[(colon + 1)..]);
    }

    private static (string, string?) CutOnce(string s, char c)
    {
        var i = s.IndexOf(c);
        return i < 0 ? (s, null) : (s[..i], s[(i + 1)..]);
    }
}

/// <summary>Small string-splitting helpers shared by the URI parsers.</summary>
public static class StringCutExtensions
{
    /// <summary>Split at the first occurrence of c.</summary>
    public static bool TryCut(this string s, char c, out string head, out string tail)
    {
        var i = s.IndexOf(c);
        (head, tail) = i < 0 ? (s, string.Empty) : (s[..i], s[(i + 1)..]);
        return i >= 0;
    }

    /// <summary>Split at the last occurrence of c.</summary>
    public static bool TryCutLast(this string s, char c, out string head, out string tail)
    {
        var i = s.LastIndexOf(c);
        (head, tail) = i < 0 ? (s, string.Empty) : (s[..i], s[(i + 1)..]);
        return i >= 0;
    }

    /// <summary>Dictionary getter with a null default.</summary>
    public static string? Get(this Dictionary<string, string> map, string key) =>
        map.TryGetValue(key, out var v) ? v : null;
}
