// .npvs reading compatibility: v1 NPVS containers, npvs:// links and
// subscription text (plain or base64-wrapped URI lists). Every profile
// passes <see cref="Profile.Validate"/> before it is returned.


using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>One entry imported from a .npvs source.</summary>
public sealed class NpvsEntry
{
    /// <summary>Create one entry.</summary>
    public NpvsEntry(int index, Profile profile, string origin)
    {
        Index = index;
        Profile = profile;
        Origin = origin;
    }

    /// <summary>Which line / index the entry came from (1-based; 0 = container).</summary>
    public int Index { get; }

    /// <summary>Validated profile; NpvsReader guarantees Profile.Validate
    /// passed before the entry is returned.</summary>
    public Profile Profile { get; }

    /// <summary>Human-facing origin label (file name, link, line number).</summary>
    public string Origin { get; }
}

/// <summary>Reader for the .npvs shared-file family.</summary>
public static class NpvsReader
{
    /// <summary>Maximum accepted .npvs text size (2 MiB is generous).</summary>
    public const int MaxNpvsTextLen = 2 * 1024 * 1024;

    /// <summary>Maximum number of entries accepted in one bundle.</summary>
    public const int MaxBundleEntries = 500;

    /// <summary>Can this byte span be read as .npvs content?</summary>
    public static bool LooksLikeNpvs(byte[] bytes)
    {
        if (V1Container.Probe(bytes) is { IsV1: true, Magic: "NPVS" })
        {
            return true;
        }

        if (V1Container.Probe(bytes).IsV1)
        {
            // NPVT v1 files are a different family; .npvs reading covers the
            // NPVS magic, links and subscription text only.
            return false;
        }

        var text = DecodeText(bytes);
        return TextShapes(text) != NpvsShape.None;
    }

    /// <summary>
    /// Read a .npvs payload into profiles. Empty bundles yield an empty list.
    /// </summary>
    public static List<NpvsEntry> Read(byte[] bytes, string origin)
    {
        if (bytes.Length > MaxNpvsTextLen)
        {
            throw NpvException.InvalidConfig(".npvs file is too large");
        }

        var probe = V1Container.Probe(bytes);
        if (probe.IsV1)
        {
            return probe.Magic == "NPVS"
                ? ReadNpvsContainer(bytes, origin)
                : ReadSingle(V1Container.ImportBestEffort(bytes), origin, 0);
        }

        var text = DecodeText(bytes);
        return ReadText(text, origin);
    }

    /// <summary>Read .npvs content from a text form (links or subscription).</summary>
    public static List<NpvsEntry> ReadText(string text, string origin)
    {
        if (text.Length > MaxNpvsTextLen)
        {
            throw NpvException.InvalidConfig(".npvs content is too large");
        }

        var trimmed = text.Trim();
        if (trimmed.Length == 0)
        {
            throw NpvException.InvalidConfig(".npvs file is empty");
        }

        var decoded = TryDecodeBase64Text(trimmed);
        if (decoded is not null)
        {
            return ReadText(decoded, origin);
        }

        if (trimmed.StartsWith('{') || trimmed.StartsWith('['))
        {
            return ReadJsonBundle(trimmed, origin);
        }

        return ReadUriList(trimmed, origin);
    }

    private static List<NpvsEntry> ReadNpvsContainer(byte[] bytes, string origin)
    {
        var body = Encoding.UTF8.GetString(bytes, 4, bytes.Length - 4).Trim();
        return ReadJsonBundle(body, origin);
    }

    private static List<NpvsEntry> ReadJsonBundle(string body, string origin)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(body);
        }
        catch (JsonException e)
        {
            throw NpvException.InvalidConfig($"npvs JSON is invalid: {e.Message}");
        }

        using (doc)
        {
            var items = BundleItems(doc.RootElement);
            if (items.Count > MaxBundleEntries)
            {
                throw NpvException.InvalidConfig(
                    $".npvs bundle exceeds {MaxBundleEntries} entries");
            }

            var entries = new List<NpvsEntry>();
            var position = 1;
            foreach (var item in items)
            {
                entries.AddRange(ReadBundleItem(item, origin, position));
                position++;
            }

            return entries;
        }
    }

    private static List<JsonElement> BundleItems(JsonElement root) => root.ValueKind switch
    {
        JsonValueKind.Array => root.EnumerateArray().ToList(),
        JsonValueKind.Object when root.TryGetProperty("profiles", out var p)
            && p.ValueKind == JsonValueKind.Array => p.EnumerateArray().ToList(),
        JsonValueKind.Object => [root],
        _ => throw NpvException.InvalidConfig(
            "npvs JSON must be an array, an object, or {\"profiles\": [...]}"),
    };

    private static List<NpvsEntry> ReadBundleItem(JsonElement item, string origin, int index)
    {
        if (item.ValueKind == JsonValueKind.String)
        {
            var line = item.GetString() ?? string.Empty;
            return ReadUriList(line, $"{origin}#{index}");
        }

        if (item.ValueKind == JsonValueKind.Object)
        {
            var profile = UriImport.MapVmessJson(item, $"imported npvs config {index}");
            return ReadSingle(profile, origin, index);
        }

        throw NpvException.InvalidConfig(
            $"npvs bundle entry {index} must be an object or a URI string");
    }

    private static List<NpvsEntry> ReadUriList(string text, string origin)
    {
        var entries = new List<NpvsEntry>();
        var lines = text.Split('\n');
        var lineNo = 0;
        foreach (var raw in lines)
        {
            lineNo++;
            var line = raw.Trim().TrimEnd('\r');
            if (line.Length == 0 || line.StartsWith('#'))
            {
                continue;
            }

            entries.AddRange(ReadUriLine(line, origin, lineNo));
        }

        if (entries.Count == 0 && lines.Any(l => l.Trim().Length > 0))
        {
            throw NpvException.InvalidConfig(".npvs content has no importable entries");
        }

        return entries;
    }

    private static List<NpvsEntry> ReadUriLine(string line, string origin, int lineNo)
    {
        var source = UriImport.ParseImport(line);
        if (source.Kind == ImportSourceKind.Profile)
        {
            return source.Profile is { } profile
                ? ReadSingle(profile, origin, lineNo)
                : [];
        }

        // npvt:// / npvs:// links carry container bytes: dispatch by magic.
        return source.Container is { } container
            ? ImportContainer(container, $"{origin}:{lineNo}")
            : [];
    }

    /// <summary>Import already-decoded container bytes by family.</summary>
    public static List<NpvsEntry> ImportContainer(byte[] bytes, string origin)
    {
        var probe = V1Container.Probe(bytes);
        if (probe.IsV1)
        {
            var profile = V1Container.ImportBestEffort(bytes);
            return ReadSingle(profile, origin, 0);
        }

        if (bytes.AsSpan().StartsWith(V2Const.Magic))
        {
            var decoded = V2Container.Decode(bytes, V2Unlock.Any);
            return ReadSingle(decoded.Profile, origin, 0);
        }

        // Not a container: treat the bytes as subscription text (URI list,
        // wrapped base64 or JSON bundle).
        return ReadText(DecodeText(bytes), origin);
    }

    private static List<NpvsEntry> ReadSingle(Profile profile, string origin, int index) =>
        [new NpvsEntry(index, profile, origin)];

    private static string DecodeText(byte[] bytes)
    {
        try
        {
            return Encoding.UTF8.GetString(bytes);
        }
        catch (ArgumentException)
        {
            throw NpvException.InvalidConfig(".npvs file is not valid UTF-8 text");
        }
    }

    private static string? TryDecodeBase64Text(string text)
    {
        // A wrapped subscription is a single base64 blob spanning whitespace.
        var compact = string.Concat(
            text.Split('\n', StringSplitOptions.RemoveEmptyEntries)
                .Select(l => l.Trim())
                .Where(l => l.Length > 0 && !l.StartsWith('#')));
        if (compact.Length == 0 || compact.Contains("://"))
        {
            return null;
        }

        try
        {
            var normalized = compact.Replace('-', '+').Replace('_', '/').TrimEnd('=');
            normalized = normalized.PadRight((normalized.Length + 3) / 4 * 4, '=');
            var bytes = Convert.FromBase64String(normalized);
            var decoded = Encoding.UTF8.GetString(bytes);
            return decoded.Contains("://") || decoded.TrimStart().StartsWith('{')
                || decoded.TrimStart().StartsWith('[')
                ? decoded
                : null;
        }
        catch (FormatException)
        {
            return null;
        }
    }

    private static NpvsShape TextShapes(string text)
    {
        if (text.Contains("://") || text.TrimStart().StartsWith('{')
            || text.TrimStart().StartsWith('['))
        {
            return NpvsShape.Links;
        }

        return TryDecodeBase64Text(text) is null ? NpvsShape.None : NpvsShape.Links;
    }

    private enum NpvsShape
    {
        None,
        Links,
    }
}
