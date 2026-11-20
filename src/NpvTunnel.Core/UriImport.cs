// Import sources: share URIs and base64 npvt:// / npvs:// links. Parsing is
// strict: length caps on the input and every field, unknown schemes rejected,
// and every profile goes through <see cref="Profile.Validate"/> before it
// can be stored.


using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>What a share link or npvt:// link contained.</summary>
public enum ImportSourceKind
{
    /// <summary>A plain profile (share URIs, plaintext legacy JSON).</summary>
    Profile,
    /// <summary>An encoded container (npvt:// / npvs:// links).</summary>
    Container,
}

/// <summary>Parse result of one import input; exactly one of Profile /
/// Container is populated, selected by Kind.</summary>
public sealed class ImportSource
{
    /// <summary>Which kind of source was parsed.</summary>
    public ImportSourceKind Kind { get; init; }

    /// <summary>The profile, when <see cref="Kind"/> is Profile.</summary>
    public Profile? Profile { get; init; }

    /// <summary>The container bytes, when <see cref="Kind"/> is Container.</summary>
    public byte[]? Container { get; init; }

    /// <summary>Wrap a parsed profile.</summary>
    public static ImportSource FromProfile(Profile profile) => new()
    {
        Kind = ImportSourceKind.Profile,
        Profile = profile,
    };

    /// <summary>Wrap container bytes.</summary>
    public static ImportSource FromContainer(byte[] bytes) => new()
    {
        Kind = ImportSourceKind.Container,
        Container = bytes,
    };
}

/// <summary>Shared string hygiene for every import path.</summary>
public static class ImportText
{
    /// <summary>Flexible base64 decode (url-safe padded/unpadded, standard).</summary>
    public static byte[] B64Flex(string data)
    {
        foreach (var variant in new[]
                    {
                        Base64UrlIgnorePad, Base64UrlPadded, StandardPadded,
                    })
        {
            try
            {
                return Convert.FromBase64String(NormalizeB64(variant, data.Trim()));
            }
            catch (FormatException)
            {
                // Try the next variant; all failing together means bad input.
            }
        }

        throw NpvException.InvalidConfig("invalid base64 payload");
    }

    private static string NormalizeB64(int variant, string data)
    {
        var normalized = variant switch
        {
            Base64UrlIgnorePad => data.Replace('-', '+').Replace('_', '/').TrimEnd('='),
            Base64UrlPadded => data.Replace('-', '+').Replace('_', '/'),
            _ => data,
        };
        // Convert.FromBase64String requires correct padding; the unpadded
        // link form needs it restored.
        return normalized.PadRight((normalized.Length + 3) / 4 * 4, '=');
    }

    private const int Base64UrlIgnorePad = 0;
    private const int Base64UrlPadded = 1;
    private const int StandardPadded = 2;

    /// <summary>Percent-decode a link component as UTF-8.</summary>
    public static string PercentDecode(string raw)
    {
        try
        {
            return Uri.UnescapeDataString(raw);
        }
        catch (UriFormatException)
        {
            throw NpvException.InvalidConfig("invalid percent-encoding in link");
        }
    }

    /// <summary>Trim and validate an incoming field.</summary>
    public static string CleanIncoming(string field, string what)
    {
        var v = field.Trim();
        ProfileValidation.ValidateField(v, what, Limits.MaxFieldLen);
        return v;
    }

    /// <summary>Percent-decode a display name, falling back to a default.</summary>
    public static string CleanName(string field, string defaultName)
    {
        var v = field.Trim().Length == 0
            ? defaultName
            : PercentDecode(field.Trim());
        ProfileValidation.ValidateField(v, "profile name", Limits.MaxFieldLen);
        return new string(v.Take(Limits.MaxNameLen).ToArray());
    }

    /// <summary>Clean an optional parameter value.</summary>
    public static string? CleanOpt(string? value, string what) =>
        string.IsNullOrWhiteSpace(value) ? null : CleanIncoming(value, what);
}

/// <summary>Parse share URIs and container links into import sources.</summary>
public static class UriImport
{
    /// <summary>Parse one share URI into a profile.</summary>
    public static Profile ParseUri(string input)
    {
        var trimmed = input.Trim();
        if (trimmed.Length > Limits.MaxUriLen)
        {
            throw NpvException.InvalidConfig("link is too long");
        }

        if (trimmed.Any(char.IsControl))
        {
            throw NpvException.InvalidConfig("link contains control characters");
        }

        var sep = trimmed.IndexOf("://", StringComparison.Ordinal);
        if (sep < 0)
        {
            throw NpvException.InvalidConfig("not a share link (missing ://)");
        }

        var scheme = trimmed[..sep].ToLowerInvariant();
        var rest = trimmed[(sep + 3)..];
        return scheme switch
        {
            "vmess" => ParseVmess(rest),
            "ss" => UriGeneric.ParseSs(rest),
            "vless" or "trojan" or "hysteria2" or "hy2" or "socks" or "socks5"
                or "socks5h" or "http" or "https" => UriGeneric.ParseGeneric(trimmed, scheme),
            _ => throw NpvException.InvalidConfig(
                $"unsupported link scheme '{scheme}'; supported: vless, vmess, "
                + "ss, trojan, hysteria2, socks, http(s), npvt"),
        };
    }

    /// <summary>
    /// Parse any supported import source: a share URI, an npvt:// container
    /// link, or an npvs:// link.
    /// </summary>
    public static ImportSource ParseImport(string input)
    {
        var trimmed = input.Trim();
        if (trimmed.Length > Limits.MaxUriLen)
        {
            throw NpvException.InvalidConfig("input is too long");
        }

        var lower = trimmed.ToLowerInvariant();
        if (lower.StartsWith("npvt://", StringComparison.Ordinal)
            || lower.StartsWith("npvs://", StringComparison.Ordinal))
        {
            return ImportSource.FromContainer(ContainerFromLink(trimmed));
        }

        if (trimmed.Contains("://", StringComparison.Ordinal))
        {
            return ImportSource.FromProfile(ParseUri(trimmed));
        }

        throw NpvException.InvalidConfig(
            "input is neither an existing file nor a supported link");
    }

    private static byte[] ContainerFromLink(string trimmed)
    {
        var body = trimmed[(trimmed.IndexOf("://", StringComparison.Ordinal) + 3)..];
        var hash = body.IndexOf('#', StringComparison.Ordinal);
        if (hash >= 0)
        {
            body = body[..hash];
        }

        if (body.Length > V2Const.MaxContainerB64Len)
        {
            throw NpvException.InvalidConfig("container link is too large");
        }

        return ImportText.B64Flex(body);
    }

    private static Profile ParseVmess(string rest)
    {
        var raw = ImportText.B64Flex(rest);
        string text;
        try
        {
            text = Encoding.UTF8.GetString(raw);
        }
        catch (ArgumentException)
        {
            throw NpvException.InvalidConfig("vmess payload is not valid UTF-8");
        }

        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(text.Trim());
        }
        catch (JsonException e)
        {
            throw NpvException.InvalidConfig($"vmess payload is not valid JSON: {e.Message}");
        }

        using (doc)
        {
            return MapVmessJson(doc.RootElement, "imported vmess profile");
        }
    }

    /// <summary>
    /// Tolerant mapper for the VMess-style JSON configuration shape used by
    /// vmess:// payloads and plaintext legacy v1 files.
    /// </summary>
    public static Profile MapVmessJson(JsonElement obj, string defaultName)
    {
        if (obj.ValueKind != JsonValueKind.Object)
        {
            throw NpvException.InvalidConfig("config JSON must be an object");
        }

        var server = ImportText.CleanIncoming(FirstString(obj, "add", "address", "server", "host")
            ?? throw NpvException.InvalidConfig("config JSON missing server (add)"), "server");
        var portText = FirstString(obj, "port")
            ?? throw NpvException.InvalidConfig("config JSON missing port");
        var port = UriGeneric.ParsePort(portText);
        var uuid = FirstString(obj, "id", "uuid") ?? string.Empty;
        if (uuid.Length == 0)
        {
            throw NpvException.InvalidConfig("config JSON missing credential (id)");
        }

        var name = ImportText.CleanName(
            FirstString(obj, "ps", "name", "remark") ?? defaultName, defaultName);
        var net = (FirstString(obj, "net") ?? "tcp").ToLowerInvariant();
        var baseTransport = net switch
        {
            "ws" => Transport.Ws,
            "grpc" => Transport.Grpc,
            "h2" => Transport.H2,
            _ => Transport.Plain,
        };
        var tls = (FirstString(obj, "tls") ?? string.Empty)
            .Equals("tls", StringComparison.OrdinalIgnoreCase);
        var sni = ImportText.CleanOpt(FirstString(obj, "sni", "peer"), "sni");
        var path = ImportText.CleanOpt(FirstString(obj, "path"), "path");
        var profile = new Profile
        {
            Id = ProfileValidation.NewProfileId(),
            Name = name,
            Protocol = Protocol.Vmess,
            Server = server,
            Port = port,
            Credential = new UuidCredential(ImportText.CleanIncoming(uuid, "credential")),
            Transport = tls ? Transport.Tls : baseTransport,
            Sni = sni,
            Path = path,
            AllowInsecure = false,
            Udp = true,
            Group = null,
            CreatedAt = ProfileValidation.UnixNow(),
        };
        profile.Validate();
        return profile;
    }

    private static string? FirstString(JsonElement obj, params string[] keys)
    {
        foreach (var key in keys)
        {
            if (!obj.TryGetProperty(key, out var v))
            {
                continue;
            }

            if (v.ValueKind == JsonValueKind.String && v.GetString() is { } s && s.Length > 0)
            {
                return s;
            }

            if (v.ValueKind == JsonValueKind.Number)
            {
                return v.GetRawText();
            }
        }

        return null;
    }
}
