// The local vault: profiles, the creator identity, and known creator keys.
// One JSON file at the OS data directory, atomic writes (temp + rename),
// owner-only permissions on Unix, and nothing is ever logged.


using System.Text.Json;
using System.Text.Json.Serialization;

namespace NpvTunnel.Core;

/// <summary>Vault schema constants.</summary>
public static class VaultSchema
{
    /// <summary>Current vault schema version.</summary>
    public const int Version = 1;
}

/// <summary>Creator identity stored in the vault (seeds are sensitive).</summary>
public sealed class Identity
{
    /// <summary>Ed25519 seed (32 bytes) for signing exports.</summary>
    [JsonPropertyName("ed25519_seed")]
    public byte[] Ed25519Seed { get; set; } = [];

    /// <summary>X25519 secret (32 bytes) for opening key-locked shares.</summary>
    [JsonPropertyName("x25519_seed")]
    public byte[] X25519Seed { get; set; } = [];
}

/// <summary>Persisted vault state.</summary>
public sealed class VaultData
{
    /// <summary>Schema version.</summary>
    [JsonPropertyName("version")]
    public int Version { get; set; } = VaultSchema.Version;

    /// <summary>Stored profiles.</summary>
    [JsonPropertyName("profiles")]
    public List<Profile> Profiles { get; set; } = [];

    /// <summary>The user's creator identity, once generated.</summary>
    [JsonPropertyName("identity")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public Identity? Identity { get; set; }

    /// <summary>Known creator signing keys (pubkey -> label), trust-on-first-use.</summary>
    [JsonPropertyName("known_creators")]
    public SortedDictionary<string, string> KnownCreators { get; set; } = new();
}

/// <summary>The vault options: file-backed store.</summary>
public sealed class Vault
{
    private readonly string _path;

    /// <summary>Vault at an explicit path (tests and --vault).</summary>
    public Vault(string path)
    {
        _path = path;
    }

    /// <summary>The vault file path.</summary>
    public string Path => _path;

    /// <summary>Default vault path for the current user.</summary>
    public static string DefaultPath()
    {
        var baseDir = BaseDir();
        return System.IO.Path.Combine(baseDir, "npvtunnel", "vault.json");
    }

    /// <summary>The per-user data directory used by engine state too.</summary>
    public static string BaseDir()
    {
        if (OperatingSystem.IsWindows())
        {
            var appData = Environment.GetFolderPath(
                Environment.SpecialFolder.ApplicationData);
            return string.IsNullOrEmpty(appData)
                ? Environment.CurrentDirectory
                : appData;
        }

        var xdg = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
        if (!string.IsNullOrEmpty(xdg))
        {
            return xdg;
        }

        var home = Environment.GetEnvironmentVariable("HOME") ?? ".";
        return System.IO.Path.Combine(home, ".local", "share");
    }

    /// <summary>Load the vault, creating an empty one if the file does not exist.</summary>
    public VaultData Load()
    {
        if (!File.Exists(_path))
        {
            return new VaultData();
        }

        var bytes = File.ReadAllBytes(_path);
        if (bytes.Length > 16 * 1024 * 1024)
        {
            throw NpvException.VaultError("vault file is implausibly large");
        }

        VaultData data;
        try
        {
            data = JsonSerializer.Deserialize<VaultData>(bytes)
                ?? throw NpvException.VaultError("vault file is empty");
        }
        catch (JsonException e)
        {
            throw NpvException.VaultError($"corrupt vault: {e.Message}");
        }

        if (data.Version != VaultSchema.Version)
        {
            throw NpvException.VaultError(
                $"unsupported vault version {data.Version} (expected {VaultSchema.Version})");
        }

        foreach (var profile in data.Profiles)
        {
            try
            {
                profile.Validate();
            }
            catch (NpvException e)
            {
                throw NpvException.VaultError(
                    $"stored profile failed validation: {e.Message}");
            }
        }

        return data;
    }

    /// <summary>Atomically persist the vault.</summary>
    public void Save(VaultData data)
    {
        var dir = System.IO.Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir))
        {
            Directory.CreateDirectory(dir);
        }

        var options = new JsonSerializerOptions
        {
            WriteIndented = true,
            Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };
        var bytes = JsonSerializer.SerializeToUtf8Bytes(data, options);
        var tmp = _path + ".tmp";
        using (var stream = new FileStream(
                    tmp, FileMode.Create, FileAccess.Write, FileShare.None))
        {
            stream.Write(bytes);
            stream.Flush(true);
        }

        RestrictPermissions(tmp);
        File.Move(tmp, _path, overwrite: true);
        CheckPermissions(_path);
    }

    private static void RestrictPermissions(string path)
    {
        if (!OperatingSystem.IsLinux() && !OperatingSystem.IsMacOS())
        {
            return;
        }

        try
        {
            File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite);
        }
        catch (PlatformNotSupportedException)
        {
            // Nothing to restrict on this platform.
        }
    }

    private static void CheckPermissions(string path)
    {
        if (!OperatingSystem.IsLinux() && !OperatingSystem.IsMacOS())
        {
            return;
        }

        try
        {
            var mode = File.GetUnixFileMode(path);
            var groupOrOther = UnixFileMode.GroupRead | UnixFileMode.GroupWrite
                | UnixFileMode.GroupExecute | UnixFileMode.OtherRead
                | UnixFileMode.OtherWrite | UnixFileMode.OtherExecute;
            if ((mode & groupOrOther) != 0)
            {
                throw NpvException.VaultError("vault file permissions are too open");
            }
        }
        catch (PlatformNotSupportedException)
        {
            // Nothing to verify on this platform.
        }
    }
}

/// <summary>Vault profile operations (find/upsert/remove), mirroring Rust.</summary>
public static class VaultOps
{
    /// <summary>Look a profile up by exact id or unique case-insensitive name.</summary>
    public static Profile FindProfile(VaultData data, string key)
    {
        var byId = data.Profiles.FirstOrDefault(p => p.Id == key);
        if (byId is not null)
        {
            return byId;
        }

        var matches = data.Profiles
            .Where(p => string.Equals(p.Name, key, StringComparison.OrdinalIgnoreCase))
            .ToList();
        return matches.Count switch
        {
            1 => matches[0],
            0 => throw NpvException.InvalidConfig($"no profile named '{key}'"),
            _ => throw NpvException.InvalidConfig(
                $"'{key}' is ambiguous, {matches.Count} profiles share that name; "
                + "use an id"),
        };
    }

    /// <summary>Insert or replace a profile (matched by id); true when replaced.</summary>
    public static bool UpsertProfile(VaultData data, Profile profile)
    {
        profile.Validate();
        var index = data.Profiles.FindIndex(p => p.Id == profile.Id);
        if (index >= 0)
        {
            profile.CreatedAt = data.Profiles[index].CreatedAt;
            data.Profiles[index] = profile;
            return true;
        }

        if (data.Profiles.Count >= 10000)
        {
            throw NpvException.VaultError("vault profile limit reached");
        }

        data.Profiles.Add(profile);
        return false;
    }

    /// <summary>Remove a profile by id or unique name; returns the removed one.</summary>
    public static Profile RemoveProfile(VaultData data, string key)
    {
        var profile = FindProfile(data, key);
        data.Profiles.RemoveAll(p => ReferenceEquals(p, profile));
        return profile;
    }

    /// <summary>Sanitize a user-supplied display name for storage.</summary>
    public static string CleanNameInput(string name)
    {
        var v = name.Trim();
        if (v.Length == 0)
        {
            throw NpvException.InvalidConfig("name must not be empty");
        }

        if (v.Any(char.IsControl))
        {
            throw NpvException.InvalidConfig(
                "name must not contain control characters");
        }

        return new string(v.Take(Limits.MaxNameLen).ToArray());
    }

    /// <summary>Generate a fresh creator identity (both keypairs).</summary>
    public static Identity GenerateIdentity()
    {
        var ed25519 = V2Keys.NewSigningSeed();
        var x25519 = V2Keys.NewX25519Secret();
        return new Identity
        {
            Ed25519Seed = ed25519,
            X25519Seed = x25519,
        };
    }

    /// <summary>Sanitize a filename-derived label for imported files.</summary>
    public static string LabelFromFilename(string path)
    {
        var stem = System.IO.Path.GetFileNameWithoutExtension(path);
        var label = stem.Length == 0 ? "imported" : stem;
        return new string(label.Take(Limits.MaxNameLen).ToArray());
    }

    /// <summary>Random 8-hex-digit suffix for deduplicating imported names.</summary>
    public static string ShortSuffix()
    {
        Span<byte> b = stackalloc byte[4];
        System.Security.Cryptography.RandomNumberGenerator.Fill(b);
        return Convert.ToHexString(b).ToLowerInvariant();
    }
}
