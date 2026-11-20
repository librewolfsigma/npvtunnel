// Batch import orchestration: files, links, .npvt/.npvs containers and
// .npvs subscription bundles all funnel through here so the CLI and the GUI
// share one dispatch, one name-deduping pass and one reporting shape.

using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Where one imported profile came from.</summary>
public sealed class BatchEntry
{
    /// <summary>Create one batch entry.</summary>
    public BatchEntry(Profile profile, string origin)
    {
        Profile = profile;
        Origin = origin;
    }

    /// <summary>Accepted profile carrying its post-dedup name (the
    /// dispatcher renames before upsert).</summary>
    public Profile Profile { get; }

    /// <summary>Human-facing origin label.</summary>
    public string Origin { get; }
}

/// <summary>Outcome of one batch import.</summary>
public sealed class BatchResult
{
    /// <summary>Profiles that were imported (post-dedup names).</summary>
    public List<BatchEntry> Imported { get; } = [];

    /// <summary>Entries that failed, with the reason (never contains secrets).</summary>
    public List<(string Origin, string Error)> Failed { get; } = [];

    /// <summary>Total count across both lists.</summary>
    public int Total => Imported.Count + Failed.Count;
}

/// <summary>Batch import dispatcher (files, links, containers, bundles).</summary>
public static class ImportBatch
{
    /// <summary>Max bytes accepted for one import file (8 MiB).</summary>
    public const int MaxFileLen = 8 * 1024 * 1024;

    /// <summary>
    /// Import from an arbitrary source: an existing file path, a share link,
    /// an npvt:// link, or an npvs:// link.
    /// </summary>
    public static BatchResult Run(string input, VaultData data)
    {
        var result = new BatchResult();
        if (File.Exists(input))
        {
            ImportFile(input, data, result);
            return result;
        }

        ImportLink(input, input, data, result);
        return result;
    }

    private static string ReadFailure(Exception e) =>
        string.Concat("file is not readable: ", e.Message);

    /// <summary>Import one or more profiles from a file path.</summary>
    public static void ImportFile(string path, VaultData data, BatchResult result)
    {
        var origin = VaultOps.LabelFromFilename(path);
        byte[] bytes;
        try
        {
            bytes = File.ReadAllBytes(path);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            result.Failed.Add((path, ReadFailure(e)));
            return;
        }

        if (bytes.Length > MaxFileLen)
        {
            result.Failed.Add((path, "file is too large"));
            return;
        }

        if (bytes.AsSpan().StartsWith(V2Const.Magic))
        {
            ImportOne(
                () => V2Container.Decode(bytes, V2Unlock.Any).Profile, origin, data, result);
            return;
        }

        var probe = V1Container.Probe(bytes);
        if (probe.IsV1 && probe.Magic == "NPVS")
        {
            ImportNpvsEntries(NpvsReader.Read(bytes, origin), data, result);
            return;
        }

        if (probe.IsV1)
        {
            ImportOne(() => V1Container.ImportBestEffort(bytes), origin, data, result);
            return;
        }

        // A text file: subscription list or a bare share link.
        ImportNpvsEntries(NpvsReader.Read(bytes, origin), data, result);
    }

    /// <summary>Import one or more profiles from a link or raw input text.</summary>
    public static void ImportLink(string input, string origin, VaultData data, BatchResult result)
    {
        var lower = input.Trim().ToLowerInvariant();
        if (lower.StartsWith("npvs://", StringComparison.Ordinal)
            && !input.Contains('\n', StringComparison.Ordinal))
        {
            ImportNpvsLink(input, origin, data, result);
            return;
        }

        var parsed = UriImport.ParseImport(input);
        if (parsed.Kind == ImportSourceKind.Container)
        {
            ImportContainerBytes(parsed, origin, data, result);
            return;
        }

        if (parsed.Profile is { } profile)
        {
            ImportOne(() => profile, origin, data, result);
        }
        else
        {
            result.Failed.Add((origin, "import source carried no profile"));
        }
    }

    private static void ImportContainerBytes(
        ImportSource parsed, string origin, VaultData data, BatchResult result)
    {
        var container = parsed.Container;
        if (container is null)
        {
            result.Failed.Add((origin, "container link carried no payload"));
            return;
        }

        ImportNpvsEntries(NpvsReader.ImportContainer(container, origin), data, result);
    }

    private static void ImportNpvsLink(string link, string origin, VaultData data, BatchResult result)
    {
        try
        {
            var body = link.Trim()[(("npvs://").Length)..];
            var hash = body.IndexOf('#');
            if (hash >= 0)
            {
                body = body[..hash];
            }

            var bytes = ImportText.B64Flex(body);
            var entries = NpvsReader.Read(bytes, origin);
            ImportNpvsEntries(entries, data, result);
        }
        catch (NpvException e)
        {
            result.Failed.Add((origin, e.Message));
        }
    }

    /// <summary>Import .npvs bundle entries with per-entry error capture.</summary>
    public static void ImportNpvsEntries(
        List<NpvsEntry> entries, VaultData data, BatchResult result)
    {
        foreach (var entry in entries)
        {
            ImportOne(() => entry.Profile, entry.Origin, data, result);
        }
    }

    private static void ImportOne(
        Func<Profile> produce, string origin, VaultData data, BatchResult result)
    {
        try
        {
            var profile = produce();
            profile.Name = UniqueName(data, profile.Name);
            VaultOps.UpsertProfile(data, profile);
            result.Imported.Add(new BatchEntry(profile, origin));
        }
        catch (NpvException e)
        {
            result.Failed.Add((origin, e.Message));
        }
    }

    /// <summary>Ensure a unique display name (random suffix on collision).</summary>
    public static string UniqueName(VaultData data, string wanted)
    {
        var taken = data.Profiles
            .Select(p => p.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        if (!taken.Contains(wanted))
        {
            return wanted;
        }

        for (var attempt = 0; attempt < 16; attempt++)
        {
            var candidate = $"{wanted} ({VaultOps.ShortSuffix()})";
            if (!taken.Contains(candidate))
            {
                return candidate;
            }
        }

        return $"{wanted} ({ProfileValidation.UnixNow()})";
    }
}
