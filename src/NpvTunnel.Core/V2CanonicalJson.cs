// Canonical JSON for the v2 container header: the bytes that feed the AEAD
// AAD and the Ed25519 signature. Written by hand so the output is
// byte-identical to serde_json in the Rust implementation (same field
// order, same skip rules, same escaping behavior).

using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Canonical header JSON writing and parsing.</summary>
public static class V2CanonicalJson
{
    private static readonly JsonWriterOptions WriterOptions = new()
    {
        // Matches serde_json: only quotes, backslashes and control bytes are
        // escaped; UTF-8 is emitted as-is.
        Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        Indented = false,
        SkipValidation = false,
    };

    /// <summary>Serialize a header into its canonical JSON bytes.</summary>
    public static byte[] Write(V2Header header)
    {
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream, WriterOptions))
        {
            WriteHeader(writer, header);
        }

        return stream.ToArray();
    }

    /// <summary>Parse canonical header JSON with unknown-field rejection.</summary>
    public static V2Header Parse(byte[] bytes)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(bytes);
        }
        catch (JsonException e)
        {
            throw NpvException.InvalidConfig($"container header is invalid: {e.Message}");
        }

        using (doc)
        {
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                throw NpvException.InvalidConfig("container header must be an object");
            }

            return ReadHeader(root);
        }
    }

    private static void WriteHeader(Utf8JsonWriter w, V2Header h)
    {
        w.WriteStartObject();
        w.WriteString("format", h.Format);
        w.WriteNumber("version", h.Version);
        w.WriteNumber("created", h.Created);
        WritePolicy(w, h.Policy);
        WriteCrypto(w, h.Crypto);
        w.WriteStartObject("payload");
        w.WriteString("alg", h.Payload.Alg);
        w.WriteString("encoding", h.Payload.Encoding);
        w.WriteEndObject();
        if (h.Sig is { } sig)
        {
            w.WriteStartObject("sig");
            w.WriteString("alg", sig.Alg);
            w.WriteString("pubkey", sig.Pubkey);
            w.WriteString("value", sig.Value);
            w.WriteEndObject();
        }

        w.WriteEndObject();
    }

    private static V2KdfParams RequireKdf(V2CryptoHeader c) =>
        c.Kdf ?? throw NpvException.Crypto(
            "passphrase lock is missing its KDF parameters");

    private static List<V2RecipientSlot> RequireRecipients(V2CryptoHeader c) =>
        c.Recipients ?? throw NpvException.Crypto("key lock has no recipient slots");

    private static void WritePolicy(Utf8JsonWriter w, V2Policy p)
    {
        w.WriteStartObject("policy");
        if (p.ExpiresAt is { } expires)
        {
            w.WriteNumber("expires_at", expires);
        }

        if (p.MeteredOnly)
        {
            w.WriteBoolean("metered_only", true);
        }

        if (p.GroupHint is { } hint)
        {
            w.WriteString("group_hint", hint);
        }

        w.WriteEndObject();
    }

    private static void WriteCrypto(Utf8JsonWriter w, V2CryptoHeader c)
    {
        w.WriteStartObject("crypto");
        w.WriteString("mode", c.Mode);
        switch (c.Mode)
        {
            case "passphrase":
                var kdf = RequireKdf(c);
                w.WriteStartObject("kdf");
                w.WriteString("alg", kdf.Alg);
                w.WriteNumber("m_kib", kdf.MKib);
                w.WriteNumber("t", kdf.T);
                w.WriteNumber("p", kdf.P);
                w.WriteEndObject();
                w.WriteString("salt", c.Salt);
                w.WriteString("nonce", c.Nonce);
                break;
            case "keys":
                var slots = RequireRecipients(c);
                w.WriteStartArray("recipients");
                foreach (var slot in slots)
                {
                    w.WriteStartObject();
                    w.WriteString("recipient_pub", slot.RecipientPub);
                    w.WriteString("eph_pub", slot.EphPub);
                    w.WriteString("salt", slot.Salt);
                    w.WriteString("nonce", slot.Nonce);
                    w.WriteString("ct", slot.Ct);
                    w.WriteEndObject();
                }

                w.WriteEndArray();
                break;
        }

        w.WriteEndObject();
    }

    private static V2Header ReadHeader(JsonElement root)
    {
        var header = new V2Header();
        foreach (var field in root.EnumerateObject())
        {
            switch (field.Name)
            {
                case "format": header.Format = RequireString(field.Value, "format"); break;
                case "version": header.Version = RequireU16(field.Value); break;
                case "created": header.Created = RequireU64(field.Value, "created"); break;
                case "policy": header.Policy = ReadPolicy(field.Value); break;
                case "crypto": header.Crypto = ReadCrypto(field.Value); break;
                case "payload": header.Payload = ReadPayload(field.Value); break;
                case "sig": header.Sig = ReadSig(field.Value); break;
                default:
                    throw NpvException.InvalidConfig(
                        $"container header is invalid: unknown field '{field.Name}'");
            }
        }

        return header;
    }

    private static V2Policy ReadPolicy(JsonElement e)
    {
        var policy = new V2Policy();
        foreach (var f in e.EnumerateObject())
        {
            switch (f.Name)
            {
                case "expires_at":
                    policy.ExpiresAt = f.Value.ValueKind == JsonValueKind.Null
                        ? null : RequireU64(f.Value, "expires_at");
                    break;
                case "metered_only": policy.MeteredOnly = RequireBool(f.Value); break;
                case "group_hint": policy.GroupHint = RequireString(f.Value, "group_hint"); break;
            }
        }

        return policy;
    }

    private static V2CryptoHeader ReadCrypto(JsonElement e)
    {
        var crypto = new V2CryptoHeader();
        foreach (var f in e.EnumerateObject())
        {
            switch (f.Name)
            {
                case "mode": crypto.Mode = RequireString(f.Value, "mode"); break;
                case "kdf": crypto.Kdf = ReadKdf(f.Value); break;
                case "salt": crypto.Salt = RequireString(f.Value, "salt"); break;
                case "nonce": crypto.Nonce = RequireString(f.Value, "nonce"); break;
                case "recipients": crypto.Recipients = ReadRecipients(f.Value); break;
                default:
                    throw NpvException.InvalidConfig(
                        $"container header is invalid: unknown field 'crypto.{f.Name}'");
            }
        }

        return crypto;
    }

    private static V2KdfParams ReadKdf(JsonElement e)
    {
        var kdf = new V2KdfParams();
        foreach (var f in e.EnumerateObject())
        {
            switch (f.Name)
            {
                case "alg": kdf.Alg = RequireString(f.Value, "alg"); break;
                case "m_kib": kdf.MKib = RequireU32(f.Value); break;
                case "t": kdf.T = RequireU32(f.Value); break;
                case "p": kdf.P = RequireU32(f.Value); break;
            }
        }

        return kdf;
    }

    private static List<V2RecipientSlot> ReadRecipients(JsonElement e) => e
        .EnumerateArray()
        .Select(slot => new V2RecipientSlot
        {
            RecipientPub = PropOrEmpty(slot, "recipient_pub"),
            EphPub = PropOrEmpty(slot, "eph_pub"),
            Salt = PropOrEmpty(slot, "salt"),
            Nonce = PropOrEmpty(slot, "nonce"),
            Ct = PropOrEmpty(slot, "ct"),
        })
        .ToList();

    private static V2PayloadDescr ReadPayload(JsonElement e)
    {
        var payload = new V2PayloadDescr();
        foreach (var f in e.EnumerateObject())
        {
            switch (f.Name)
            {
                case "alg": payload.Alg = RequireString(f.Value, "alg"); break;
                case "encoding": payload.Encoding = RequireString(f.Value, "encoding"); break;
            }
        }

        return payload;
    }

    private static V2SignatureBlock ReadSig(JsonElement e)
    {
        var sig = new V2SignatureBlock();
        foreach (var f in e.EnumerateObject())
        {
            switch (f.Name)
            {
                case "alg": sig.Alg = RequireString(f.Value, "alg"); break;
                case "pubkey": sig.Pubkey = RequireString(f.Value, "pubkey"); break;
                case "value": sig.Value = RequireString(f.Value, "value"); break;
            }
        }

        return sig;
    }

    private static string RequireString(JsonElement v, string what) =>
        v.ValueKind == JsonValueKind.String
            ? v.GetString() ?? throw NpvException.InvalidConfig($"{what} is null")
            : throw NpvException.InvalidConfig($"{what} must be a string");

    private static string PropOrEmpty(JsonElement obj, string name) =>
        obj.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String
            ? v.GetString() ?? string.Empty
            : string.Empty;

    private static bool RequireBool(JsonElement v) => v.ValueKind switch
    {
        JsonValueKind.True => true,
        JsonValueKind.False => false,
        _ => throw NpvException.InvalidConfig("header metered_only must be a bool"),
    };

    private static ushort RequireU16(JsonElement v) =>
        v.ValueKind == JsonValueKind.Number && v.TryGetUInt16(out var u16)
            ? u16
            : throw NpvException.InvalidConfig("header version must be u16");

    private static uint RequireU32(JsonElement v) =>
        v.ValueKind == JsonValueKind.Number && v.TryGetUInt32(out var u32)
            ? u32
            : throw NpvException.InvalidConfig("header number must be u32");

    private static ulong RequireU64(JsonElement v, string what) =>
        v.ValueKind == JsonValueKind.Number && v.TryGetUInt64(out var u64)
            ? u64
            : throw NpvException.InvalidConfig($"header {what} must be u64");
}
