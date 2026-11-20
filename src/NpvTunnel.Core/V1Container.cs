// Legacy .npvt / .npvs v1 support: detection and best-effort import. The v1
// "lock" is client-side obfuscation, not cryptography - unlocked plaintext
// JSON is imported, anything else is rejected honestly. Clean-room module.


using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>What we can say about a v1 file without decoding it.</summary>
public sealed class V1Probe
{
    /// <summary>Magic matches the v1 family (NPVT or NPVS).</summary>
    public bool IsV1 { get; init; }

    /// <summary>The magic identified ("NPVT" or "NPVS" or null).</summary>
    public string? Magic { get; init; }

    /// <summary>Heuristic only, never a security signal: the v1 lock is
    /// obfuscation rather than encryption, so a plaintext body just means
    /// the JSON is importable.</summary>
    public bool LooksPlaintextJson { get; init; }
}

/// <summary>V1 family probing and best-effort single-profile import.</summary>
public static class V1Container
{
    /// <summary>Magic prefix of .npvt v1 files.</summary>
    public static readonly byte[] V1Magic = { (byte)'N', (byte)'P', (byte)'V', (byte)'T' };

    /// <summary>Magic prefix of .npvs shared-file bundles (same family).</summary>
    public static readonly byte[] V1MagicNpvs = { (byte)'N', (byte)'P', (byte)'V', (byte)'S' };

    /// <summary>Probe a byte span for the v1 family signature.</summary>
    public static V1Probe Probe(ReadOnlySpan<byte> bytes)
    {
        string? magic = null;
        var isV2 = bytes.Length >= 6 && bytes[4] == (byte)'2' && bytes[5] == 0;
        if (isV2)
        {
            // NPVT2\0 is the v2 container frame, not a v1 file.
            return new V1Probe { IsV1 = false, Magic = null, LooksPlaintextJson = false };
        }

        if (bytes.StartsWith(V1Magic))
        {
            magic = "NPVT";
        }
        else if (bytes.StartsWith(V1MagicNpvs))
        {
            magic = "NPVS";
        }

        var body = magic is null ? bytes : bytes[4..];
        // The v1 family treats ASCII whitespace as padding before the JSON.
        byte[] whitespace = { (byte)' ', (byte)'\t', (byte)'\r', (byte)'\n', (byte)'\f', (byte)'\v' };
        var firstNonSpace = body.IndexOfAnyExcept(whitespace);
        var looksJson = firstNonSpace >= 0 && body[firstNonSpace] == (byte)'{';
        return new V1Probe
        {
            IsV1 = magic is not null,
            Magic = magic,
            LooksPlaintextJson = looksJson,
        };
    }

    /// <summary>
    /// Best-effort v1 import: the mobile app's VMess-shaped JSON only.
    /// </summary>
    public static Profile ImportBestEffort(byte[] bytes)
    {
        var probe = Probe(bytes);
        if (!probe.IsV1)
        {
            throw NpvException.InvalidConfig(
                "not a .npvt/.npvs v1 container (magic mismatch)");
        }

        if (!probe.LooksPlaintextJson)
        {
            throw NpvException.V1Unsupported(
                "the file is locked with the legacy v1 obfuscation; locked v1 "
                + "files are not decoded by NPVTunnel (the v1 lock is broken "
                + "obfuscation) - re-export as a v2 container or ask the "
                + "creator for an npvt:// link");
        }

        var body = System.Text.Encoding.UTF8.GetString(bytes, 4, bytes.Length - 4);
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(body.Trim());
        }
        catch (JsonException e)
        {
            throw NpvException.V1Unsupported($"v1 body is not valid JSON: {e.Message}");
        }

        using (doc)
        {
            return UriImport.MapVmessJson(doc.RootElement, "imported v1 config");
        }
    }
}
