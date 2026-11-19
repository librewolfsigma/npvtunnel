// JSON converters that serialize enums exactly like the Rust serde
// lowercase names ("vless", "hysteria2", "wireguard", ...). The v2
// container payload and the vault file are byte-comparable between the
// Rust and C# implementations through these converters.

using System.Text.Json;
using System.Text.Json.Serialization;

namespace NpvTunnel.Core;

/// <summary>Serializes <see cref="Protocol"/> as the serde lowercase name.</summary>
public sealed class ProtocolJsonConverter : JsonConverter<Protocol>
{
    /// <inheritdoc />
    public override Protocol Read(ref Utf8JsonReader reader, Type t, JsonSerializerOptions o)
    {
        var text = reader.GetString()
            ?? throw NpvException.InvalidConfig("protocol must be a string");
        var parsed = ProtocolExtensions.ParseProtocol(text.ToLowerInvariant());
        return parsed
            ?? throw NpvException.InvalidConfig($"unknown protocol '{text}'");
    }

    /// <inheritdoc />
    public override void Write(Utf8JsonWriter writer, Protocol value, JsonSerializerOptions o) =>
        writer.WriteStringValue(value.AsStr());
}

/// <summary>Serializes <see cref="Transport"/> as the serde lowercase name.</summary>
public sealed class TransportJsonConverter : JsonConverter<Transport>
{
    /// <inheritdoc />
    public override Transport Read(ref Utf8JsonReader reader, Type t, JsonSerializerOptions o)
    {
        var text = reader.GetString()
            ?? throw NpvException.InvalidConfig("transport must be a string");
        return text.ToLowerInvariant() switch
        {
            "plain" => Transport.Plain,
            "tls" => Transport.Tls,
            "ws" => Transport.Ws,
            "grpc" => Transport.Grpc,
            "h2" => Transport.H2,
            _ => throw NpvException.InvalidConfig($"unknown transport '{text}'"),
        };
    }

    /// <inheritdoc />
    public override void Write(Utf8JsonWriter writer, Transport value, JsonSerializerOptions o) =>
        writer.WriteStringValue(value switch
        {
            Transport.Plain => "plain",
            Transport.Tls => "tls",
            Transport.Ws => "ws",
            Transport.Grpc => "grpc",
            Transport.H2 => "h2",
            _ => "plain",
        });
}

/// <summary>Serializes the externally-tagged credential enum (serde shape).</summary>
public sealed class CredentialJsonConverter : JsonConverter<Credential>
{
    /// <inheritdoc />
    public override Credential Read(
        ref Utf8JsonReader reader, Type t, JsonSerializerOptions o)
    {
        using var doc = JsonDocument.ParseValue(ref reader);
        var root = doc.RootElement;
        if (root.ValueKind != JsonValueKind.Object || root.EnumerateObject().Count() != 1)
        {
            throw NpvException.InvalidConfig("credential must be a single-key object");
        }

        var (variant, value) = root.EnumerateObject().First() is { } prop
            ? (prop.Name, prop.Value)
            : throw NpvException.InvalidConfig("credential object is empty");
        return variant switch
        {
            "uuid" => new UuidCredential(CredentialText(value, "uuid")),
            "password" => new PasswordCredential(CredentialText(value, "password")),
            "token" => new TokenCredential(CredentialText(value, "token")),
            "ssmethod" => ReadSsMethod(value),
            "none" => new NoneCredential(),
            _ => throw NpvException.InvalidConfig($"unknown credential variant '{variant}'"),
        };
    }

    /// <inheritdoc />
    public override void Write(Utf8JsonWriter writer, Credential value, JsonSerializerOptions o)
    {
        writer.WriteStartObject();
        switch (value)
        {
            case UuidCredential u:
                writer.WriteString("uuid", u.Value);
                break;
            case PasswordCredential p:
                writer.WriteString("password", p.Value);
                break;
            case SsMethodCredential s:
                writer.WritePropertyName("ssmethod");
                writer.WriteStartObject();
                writer.WriteString("method", s.Method);
                writer.WriteString("password", s.Password);
                writer.WriteEndObject();
                break;
            case TokenCredential t:
                writer.WriteString("token", t.Value);
                break;
            default:
                writer.WriteString("none", string.Empty);
                break;
        }

        writer.WriteEndObject();
    }

    private static SsMethodCredential ReadSsMethod(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            throw NpvException.InvalidConfig("ssmethod credential must be an object");
        }

        string? method = null;
        string? password = null;
        foreach (var field in value.EnumerateObject())
        {
            if (field.Name == "method")
            {
                method = field.Value.GetString();
            }
            else if (field.Name == "password")
            {
                password = field.Value.GetString();
            }
        }

        if (method is null || password is null)
        {
            throw NpvException.InvalidConfig("ssmethod credential missing fields");
        }

        return new SsMethodCredential(method, password);
    }

    private static string CredentialText(JsonElement value, string kind)
    {
        if (value.ValueKind != JsonValueKind.String)
        {
            throw NpvException.InvalidConfig($"{kind} credential must be a string");
        }

        return value.GetString()
            ?? throw NpvException.InvalidConfig($"{kind} credential is null");
    }
}
