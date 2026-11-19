// The tunnel profile model. Property order matches the Rust struct exactly:
// the v2 container payload JSON is byte-comparable between the two
// implementations.


using System.Text.Json.Serialization;

namespace NpvTunnel.Core;

/// <summary>Tunnel protocol families.</summary>
public enum Protocol
{
    /// <summary>VLESS (Xray).</summary>
    Vless,
    /// <summary>VMess (Xray).</summary>
    Vmess,
    /// <summary>Trojan.</summary>
    Trojan,
    /// <summary>Shadowsocks (incl. 2022 ciphers).</summary>
    Shadowsocks,
    /// <summary>Hysteria2 (QUIC).</summary>
    Hysteria2,
    /// <summary>WireGuard.</summary>
    WireGuard,
    /// <summary>SSH (with udpgw-style UDP relay).</summary>
    Ssh,
    /// <summary>DNSTT (DNS tunnel).</summary>
    Dnstt,
    /// <summary>Plain SOCKS5 upstream.</summary>
    Socks,
    /// <summary>Plain HTTP(S) proxy upstream.</summary>
    Http,
}

/// <summary>Transport/obfuscation wrapper on top of the protocol.</summary>
public enum Transport
{
    /// <summary>No wrapper.</summary>
    Plain,
    /// <summary>TLS (Reality profiles use this with SNI).</summary>
    Tls,
    /// <summary>WebSocket.</summary>
    Ws,
    /// <summary>gRPC.</summary>
    Grpc,
    /// <summary>HTTP/2.</summary>
    H2,
}

/// <summary>Helpers mirroring the Rust Protocol impl.</summary>
public static class ProtocolExtensions
{
    private static readonly string[] Names =
    {
        "vless", "vmess", "trojan", "shadowsocks", "hysteria2",
        "wireguard", "ssh", "dnstt", "socks", "http",
    };

    /// <summary>All protocols in display order.</summary>
    public static readonly Protocol[] All =
    {
        Protocol.Vless, Protocol.Vmess, Protocol.Trojan, Protocol.Shadowsocks,
        Protocol.Hysteria2, Protocol.WireGuard, Protocol.Ssh, Protocol.Dnstt,
        Protocol.Socks, Protocol.Http,
    };

    /// <summary>Stable lowercase name (matches the Rust serde naming).</summary>
    public static string AsStr(this Protocol p) => Names[(int)p];

    /// <summary>Parse from the stable lowercase name.</summary>
    public static Protocol? ParseProtocol(string value)
    {
        foreach (var p in All)
        {
            if (p.AsStr() == value)
            {
                return p;
            }
        }

        return null;
    }

    /// <summary>Does this protocol require some credential to be present?</summary>
    public static bool RequiresCredential(this Protocol p) =>
        p is not (Protocol.Socks or Protocol.Http);

    /// <summary>Is this protocol wired to an engine sidecar in this release?</summary>
    public static bool EngineSupported(this Protocol p) =>
        p is Protocol.Vless or Protocol.Vmess or Protocol.Trojan
            or Protocol.Shadowsocks or Protocol.Hysteria2 or Protocol.Socks or Protocol.Http;
}

/// <summary>
/// A single connectable tunnel configuration: imported from URIs or .npvt
/// containers, stored in the vault, rendered to engine configs.
/// </summary>
public sealed class Profile : IEquatable<Profile>
{
    /// <summary>Stable random hex id (16 bytes).</summary>
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    /// <summary>Display name.</summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    /// <summary>Protocol family (lowercase, matching serde).</summary>
    [JsonPropertyName("protocol")]
    [JsonConverter(typeof(ProtocolJsonConverter))]
    public Protocol Protocol { get; set; }

    /// <summary>Server host or IP (never a URL).</summary>
    [JsonPropertyName("server")]
    public string Server { get; set; } = string.Empty;

    /// <summary>Server port.</summary>
    [JsonPropertyName("port")]
    public ushort Port { get; set; }

    /// <summary>Credential material.</summary>
    [JsonPropertyName("credential")]
    [JsonConverter(typeof(CredentialJsonConverter))]
    public Credential Credential { get; set; } = new NoneCredential();

    /// <summary>Transport wrapper (lowercase, matching serde).</summary>
    [JsonPropertyName("transport")]
    [JsonConverter(typeof(TransportJsonConverter))]
    public Transport Transport { get; set; }

    /// <summary>TLS SNI / Reality server name (null when unset).</summary>
    [JsonPropertyName("sni")]
    public string? Sni { get; set; }

    /// <summary>WS/gRPC path or service name (null when unset).</summary>
    [JsonPropertyName("path")]
    public string? Path { get; set; }

    /// <summary>Skip TLS verification. Security-relevant: doctor flags it.</summary>
    [JsonPropertyName("allow_insecure")]
    public bool AllowInsecure { get; set; }

    /// <summary>UDP relay enabled.</summary>
    [JsonPropertyName("udp")]
    public bool Udp { get; set; }

    /// <summary>Optional group label for smart groups.</summary>
    [JsonPropertyName("group")]
    public string? Group { get; set; }

    /// <summary>Creation time, unix seconds.</summary>
    [JsonPropertyName("created_at")]
    public ulong CreatedAt { get; set; }

    /// <summary>Validate all fields (import, vault load, config render).</summary>
    public void Validate()
    {
        ProfileValidation.ValidateId(Id);
        ProfileValidation.ValidateField(Name, "profile name", Limits.MaxNameLen);
        ProfileValidation.ValidateField(Server, "server", Limits.MaxFieldLen);
        if (Server.Contains('/') || Server.Contains(' '))
        {
            throw NpvException.InvalidConfig("server must be a bare host or IP, not a URL");
        }

        ProfileValidation.ValidateOptional(Sni, "sni");
        ProfileValidation.ValidateOptional(Path, "path");
        ProfileValidation.ValidateOptional(Group, "group");
        ProfileValidation.ValidateCredential(Credential);
        if (Protocol.RequiresCredential() && Credential.IsNone)
        {
            throw NpvException.InvalidConfig(
                $"protocol {Protocol.AsStr()} requires a credential");
        }
    }

    /// <inheritdoc />
    public bool Equals(Profile? other) => other is not null
        && Id == other.Id
        && Name == other.Name
        && Protocol == other.Protocol
        && Server == other.Server
        && Port == other.Port
        && Credential.Equals(other.Credential)
        && Transport == other.Transport
        && Sni == other.Sni
        && Path == other.Path
        && AllowInsecure == other.AllowInsecure
        && Udp == other.Udp
        && Group == other.Group
        && CreatedAt == other.CreatedAt;

    /// <inheritdoc />
    public override bool Equals(object? obj) => obj is Profile p && Equals(p);

    /// <inheritdoc />
    public override int GetHashCode() => HashCode.Combine(
        Id, Name, Protocol, Server, Port, Credential, Transport, CreatedAt);
}

/// <summary>Validation helpers shared by the profile and import layers.</summary>
public static class ProfileValidation
{
    /// <summary>Validate one incoming field: non-empty, bounded, no controls.</summary>
    public static void ValidateField(string value, string what, int max)
    {
        if (value.Length == 0)
        {
            throw NpvException.InvalidConfig($"{what} must not be empty");
        }

        if (value.Length > max)
        {
            throw NpvException.InvalidConfig($"{what} is too long (>{max} bytes)");
        }

        if (value.Any(char.IsControl))
        {
            throw NpvException.InvalidConfig(
                $"{what} must not contain control characters");
        }

        if (string.IsNullOrWhiteSpace(value))
        {
            throw NpvException.InvalidConfig($"{what} must not be blank");
        }
    }

    /// <summary>Validate an optional field when it is present.</summary>
    public static void ValidateOptional(string? value, string what)
    {
        if (value is not null)
        {
            ValidateField(value, what, Limits.MaxFieldLen);
        }
    }

    /// <summary>Validate the credential text against the standard cap.</summary>
    public static void ValidateCredential(Credential cred)
    {
        switch (cred)
        {
            case UuidCredential u:
                ValidateField(u.Value, "credential", Limits.MaxFieldLen);
                break;
            case PasswordCredential p:
                ValidateField(p.Value, "credential", Limits.MaxFieldLen);
                break;
            case TokenCredential t:
                ValidateField(t.Value, "credential", Limits.MaxFieldLen);
                break;
            case SsMethodCredential s:
                ValidateField(s.Method, "shadowsocks method", 128);
                ValidateField(s.Password, "shadowsocks password", Limits.MaxFieldLen);
                break;
        }
    }

    /// <summary>Validate that the id is bounded hex.</summary>
    public static void ValidateId(string id)
    {
        ValidateField(id, "profile id", 64);
        if (!id.All(c => Uri.IsHexDigit(c)))
        {
            throw NpvException.InvalidConfig("profile id must be hex");
        }
    }

    /// <summary>Current unix time in seconds (0 if the clock is before the epoch).</summary>
    public static ulong UnixNow()
    {
        var span = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        return span < 0 ? 0UL : (ulong)span;
    }

    /// <summary>New random 16-byte hex id for a profile.</summary>
    public static string NewProfileId()
    {
        Span<byte> bytes = stackalloc byte[16];
        System.Security.Cryptography.RandomNumberGenerator.Fill(bytes);
        return Convert.ToHexString(bytes).ToLowerInvariant();
    }
}
