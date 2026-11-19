// Shared fixtures: the sample profile all tests build on.

using System.Text.Json;

namespace NpvTunnel.Core.Tests;

/// <summary>Factory for the canonical sample profile.</summary>
public static class Fixtures
{
    /// <summary>The sample profile (fake credentials only).</summary>
    public static Profile Sample() => new()
    {
        Id = "0123abcd0123abcd",
        Name = "shop bundle",
        Protocol = Protocol.Vless,
        Server = "shop.example.com",
        Port = 443,
        Credential = new UuidCredential("11111111-2222-3333-4444-555555555555"),
        Transport = Transport.Tls,
        Sni = "shop.example.com",
        Path = null,
        AllowInsecure = false,
        Udp = true,
        Group = null,
        CreatedAt = 1_700_000_000,
    };

    /// <summary>The committed cross-language fixture directory.</summary>
    public static string VectorDir => Path.Combine(
        AppContext.BaseDirectory, "Vectors");

    /// <summary>Read a committed vector file.</summary>
    public static byte[] Vector(string name) =>
        File.ReadAllBytes(Path.Combine(VectorDir, name));

    /// <summary>Signer seed used by every committed fixture.</summary>
    public static byte[] SignerSeed() => Enumerable.Repeat((byte)7, 32).ToArray();

    /// <summary>Recipient secret used by the keys fixture.</summary>
    public static byte[] RecipientSecret() => Enumerable.Repeat((byte)9, 32).ToArray();

    /// <summary>Parse JSON text into a JsonDocument.</summary>
    public static JsonDocument Json(string text) =>
        JsonDocument.Parse(text);
}
