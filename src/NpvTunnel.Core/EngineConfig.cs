// Render profiles into sing-box configs. Security posture: the local
// inbound always binds 127.0.0.1, and unsupported protocols fail with a
// roadmap message instead of silently downgrading security.


using System.Text.Json;
using System.Text.Json.Nodes;

namespace NpvTunnel.Core;

/// <summary>Renders profiles into sing-box configuration objects.</summary>
public static class EngineConfig
{
    /// <summary>Default local SOCKS inbound port.</summary>
    public const int DefaultSocksPort = 2080;

    /// <summary>Render one profile as a sing-box outbound object.</summary>
    public static JsonObject Outbound(Profile p)
    {
        p.Validate();
        if (!p.Protocol.EngineSupported())
        {
            throw NpvException.EngineError(
                $"protocol '{p.Protocol.AsStr()}' is not wired to an engine "
                + "sidecar yet (roadmap P1.5)");
        }

        var ob = new JsonObject
        {
            ["tag"] = p.Id,
            ["server"] = p.Server,
            ["server_port"] = (int)p.Port,
        };
        FillProtocolFields(ob, p);
        var tls = TlsBlock(p);
        if (tls is not null)
        {
            ob["tls"] = tls;
        }

        var transport = TransportBlock(p);
        if (transport is not null)
        {
            ob["transport"] = transport;
        }

        return ob;
    }

    /// <summary>Render a full sing-box config with active as the final route.</summary>
    public static JsonObject Config(
        IReadOnlyList<Profile> profiles, string activeId, int socksPort, string dnsServer)
    {
        var active = profiles.FirstOrDefault(p => p.Id == activeId)
            ?? throw NpvException.EngineError("active profile is not in the set");
        var outbounds = new JsonArray();
        foreach (var profile in profiles)
        {
            outbounds.Add(Outbound(profile));
        }

        outbounds.Add(new JsonObject { ["type"] = "direct", ["tag"] = "direct" });
        return new JsonObject
        {
            ["log"] = new JsonObject { ["level"] = "warn" },
            ["dns"] = new JsonObject
            {
                ["servers"] = new JsonArray(
                    new JsonObject
                    {
                        ["tag"] = "dns-remote",
                        ["address"] = dnsServer,
                    }),
            },
            ["inbounds"] = new JsonArray(
                new JsonObject
                {
                    ["type"] = "socks",
                    ["tag"] = "socks-in",
                    ["listen"] = "127.0.0.1",
                    ["listen_port"] = socksPort,
                    ["udp_enabled"] = profiles.Any(p => p.Udp),
                }),
            ["outbounds"] = outbounds,
            ["route"] = new JsonObject { ["final"] = active.Id },
        };
    }

    /// <summary>Serialize a rendered config with tight, stable formatting.</summary>
    public static byte[] Render(JsonObject config)
    {
        var options = new JsonSerializerOptions
        {
            WriteIndented = true,
            Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };
        return JsonSerializer.SerializeToUtf8Bytes(config, options);
    }

    private static void FillProtocolFields(JsonObject ob, Profile p)
    {
        switch (p.Protocol, p.Credential)
        {
            case (Protocol.Vless, UuidCredential u):
                ob["type"] = "vless";
                ob["uuid"] = u.Value;
                ob["packet_encoding"] = "xudp";
                break;
            case (Protocol.Vmess, UuidCredential u):
                ob["type"] = "vmess";
                ob["uuid"] = u.Value;
                ob["security"] = "auto";
                ob["alter_id"] = 0;
                ob["packet_encoding"] = "xudp";
                break;
            case (Protocol.Trojan, PasswordCredential pw):
                ob["type"] = "trojan";
                ob["password"] = pw.Value;
                break;
            case (Protocol.Shadowsocks, SsMethodCredential ss):
                ob["type"] = "shadowsocks";
                ob["method"] = ss.Method;
                ob["password"] = ss.Password;
                break;
            case (Protocol.Hysteria2, PasswordCredential pw2):
                ob["type"] = "hysteria2";
                ob["password"] = pw2.Value;
                break;
            case (Protocol.Socks, _) or (Protocol.Http, _):
                FillProxyFields(ob, p);
                break;
            default:
                throw NpvException.InvalidConfig(
                    $"credential kind does not fit protocol '{p.Protocol.AsStr()}'");
        }
    }

    private static void FillProxyFields(JsonObject ob, Profile p)
    {
        ob["type"] = p.Protocol == Protocol.Socks ? "socks" : "http";
        if (p.Protocol == Protocol.Socks)
        {
            ob["version"] = "5";
        }

        if (p.Credential is TokenCredential t)
        {
            var (user, pass) = UserPass(t.Value);
            ob["username"] = user;
            ob["password"] = pass;
        }
        else if (!p.Credential.IsNone)
        {
            throw NpvException.InvalidConfig(
                $"credential kind does not fit protocol '{p.Protocol.AsStr()}'");
        }
    }

    private static (string, string) UserPass(string token)
    {
        if (!token.TryCut(':', out var user, out var pass))
        {
            throw NpvException.InvalidConfig(
                "socks/http credential must be user:password");
        }

        return (user, pass);
    }

    private static JsonObject? TlsBlock(Profile p)
    {
        var required = p.Protocol == Protocol.Hysteria2;
        if (!required && p.Transport != Transport.Tls)
        {
            return null;
        }

        var tls = new JsonObject { ["enabled"] = true };
        if (p.Sni is { } sni)
        {
            tls["server_name"] = sni;
        }

        tls["insecure"] = p.AllowInsecure;
        return tls;
    }

    private static JsonObject? TransportBlock(Profile p) => p.Transport switch
    {
        Transport.Ws => new JsonObject
        {
            ["type"] = "ws",
            ["path"] = p.Path ?? string.Empty,
        },
        Transport.Grpc => new JsonObject
        {
            ["type"] = "grpc",
            ["service_name"] = p.Path ?? string.Empty,
        },
        Transport.H2 => new JsonObject
        {
            ["type"] = "http",
            ["path"] = p.Path ?? string.Empty,
        },
        _ => null,
    };
}

/// <summary>Where the engine config, pid and log files live.</summary>
public static class EnginePaths
{
    /// <summary>The npvtunnel data directory (created when missing).</summary>
    public static string WorkDir()
    {
        var dir = System.IO.Path.Combine(Vault.BaseDir(), "npvtunnel");
        Directory.CreateDirectory(dir);
        return dir;
    }

    /// <summary>engine.log inside the npvtunnel data directory; written
    /// by the sing-box process itself, not by LogService.</summary>
    public static string LogPath() => System.IO.Path.Combine(WorkDir(), "engine.log");

    /// <summary>engine.pid inside the data directory; written at spawn,
    /// cleared on stop and used by EngineManager.ReconnectSession after
    /// an app restart.</summary>
    public static string PidPath() => System.IO.Path.Combine(WorkDir(), "engine.pid");

    /// <summary>engine.config.json inside the data directory; rewritten
    /// before every engine spawn with the one-session profile set.</summary>
    public static string ConfigPath() =>
        System.IO.Path.Combine(WorkDir(), "engine.config.json");

    /// <summary>
    /// Locate the sing-box binary: explicit path, next to our own
    /// executable, the npvtunnel data directory, or PATH.
    /// </summary>
    public static string? FindEngineBinary(string? explicitPath)
    {
        var exeName = OperatingSystem.IsWindows() ? "sing-box.exe" : "sing-box";
        if (!string.IsNullOrEmpty(explicitPath))
        {
            return File.Exists(explicitPath) ? explicitPath : null;
        }

        var selfDir = System.IO.Path.GetDirectoryName(
            Environment.ProcessPath ?? string.Empty);
        if (!string.IsNullOrEmpty(selfDir))
        {
            var sibling = System.IO.Path.Combine(selfDir, exeName);
            if (File.Exists(sibling))
            {
                return sibling;
            }
        }

        var workCandidate = System.IO.Path.Combine(WorkDir(), "bin", exeName);
        if (File.Exists(workCandidate))
        {
            return workCandidate;
        }

        var pathVar = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
        return pathVar
            .Split(System.IO.Path.PathSeparator)
            .Select(dir => System.IO.Path.Combine(dir.Trim(), exeName))
            .FirstOrDefault(File.Exists);
    }
}
