// Shared byte-level primitives for the v2 container: payload envelopes,
// signature input framing, HKDF-SHA-256 and the little-endian base64 glue.

using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Byte-level helpers shared by encode and decode (partial class).</summary>
public static partial class V2Container
{
    private static byte[] PayloadJson(Profile profile)
    {
        using var stream = new MemoryStream();
        var options = new JsonSerializerOptions
        {
            Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };
        JsonSerializer.Serialize(stream, new PayloadEnvelope { Profile = profile }, options);
        return stream.ToArray();
    }

    private static Profile ParsePayload(byte[] plaintext)
    {
        ProfileEnvelope envelope;
        try
        {
            envelope = JsonSerializer.Deserialize<ProfileEnvelope>(plaintext)
                ?? throw NpvException.InvalidConfig("container payload is invalid: null");
        }
        catch (JsonException e)
        {
            throw NpvException.InvalidConfig($"container payload is invalid: {e.Message}");
        }

        envelope.Profile.Validate();
        return envelope.Profile;
    }

    private static byte[] SignInput(byte[] aad, byte[] payload)
    {
        using var sha256 = SHA256.Create();
        var aadHash = sha256.ComputeHash(aad);
        var payloadHash = sha256.ComputeHash(payload);
        var buf = new byte[V2Const.SignDomain.Length + 32 + 4 + 32];
        Array.Copy(V2Const.SignDomain, buf, V2Const.SignDomain.Length);
        Array.Copy(aadHash, 0, buf, V2Const.SignDomain.Length, 32);
        WriteU32Le(buf, V2Const.SignDomain.Length + 32, payload.Length);
        Array.Copy(payloadHash, 0, buf, V2Const.SignDomain.Length + 36, 32);
        return buf;
    }

    /// <summary>HKDF-SHA-256 expand with the container's recipient info label.</summary>
    private static byte[] HkdfSha256(byte[] ikm, byte[] salt)
    {
        var prk = HmacSha256(salt.Length == 0 ? new byte[32] : salt, ikm);
        var okm = new byte[32];
        var t = new byte[0];
        var counter = 1;
        var filled = 0;
        while (filled < okm.Length)
        {
            using var hmac = new HMACSHA256();
            hmac.Key = prk;
            var input = new byte[t.Length + V2Const.KdfInfoRecipient.Length + 1];
            Array.Copy(t, input, t.Length);
            Array.Copy(
                V2Const.KdfInfoRecipient, 0, input, t.Length,
                V2Const.KdfInfoRecipient.Length);
            input[^1] = (byte)counter;
            t = hmac.ComputeHash(input);
            var take = Math.Min(32, okm.Length - filled);
            Array.Copy(t, 0, okm, filled, take);
            filled += take;
            counter++;
        }

        return okm;
    }

    private static byte[] HmacSha256(byte[] key, byte[] data)
    {
        using var hmac = new HMACSHA256();
        hmac.Key = key;
        return hmac.ComputeHash(data);
    }

    private static byte[] RandomBytes(int n)
    {
        var b = new byte[n];
        RandomNumberGenerator.Fill(b);
        return b;
    }

    private static string B64(byte[] data) =>
        Convert.ToBase64String(data).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    private static byte[] Unb64(string data)
    {
        try
        {
            // The container uses unpadded base64url; re-pad for Convert.
            var normalized = data.Replace('-', '+').Replace('_', '/');
            normalized = normalized.PadRight((normalized.Length + 3) / 4 * 4, '=');
            return Convert.FromBase64String(normalized);
        }
        catch (FormatException)
        {
            throw NpvException.Crypto("invalid base64 field in container");
        }
    }

    private static uint ReadU32Le(byte[] b, int offset) =>
        (uint)(b[offset] | (b[offset + 1] << 8) | (b[offset + 2] << 16) | (b[offset + 3] << 24));

    private static void WriteU32Le(byte[] b, int offset, int value)
    {
        b[offset] = (byte)value;
        b[offset + 1] = (byte)(value >> 8);
        b[offset + 2] = (byte)(value >> 16);
        b[offset + 3] = (byte)(value >> 24);
    }

    private static void WriteU16Le(byte[] b, int offset, ushort value)
    {
        b[offset] = (byte)value;
        b[offset + 1] = (byte)(value >> 8);
    }

    /// <summary>Payload envelope: {"profile": {...}} (serde field name).</summary>
    private sealed class PayloadEnvelope
    {
        /// <summary>The profile, under the serde field name.</summary>
        [System.Text.Json.Serialization.JsonPropertyName("profile")]
        public Profile Profile { get; set; } = new();
    }

    /// <summary>Envelope used on decode (serde field name).</summary>
    private sealed class ProfileEnvelope
    {
        /// <summary>The profile, under the serde field name.</summary>
        [System.Text.Json.Serialization.JsonPropertyName("profile")]
        public Profile Profile { get; set; } = new();
    }
}
