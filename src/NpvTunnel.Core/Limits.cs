// Shared hard caps for profile fields, mirroring the Rust constants.

namespace NpvTunnel.Core;

/// <summary>Hard caps for validated string fields.</summary>
public static class Limits
{
    /// <summary>Hard cap for any single string field.</summary>
    public const int MaxFieldLen = 512;

    /// <summary>Hard cap for display names.</summary>
    public const int MaxNameLen = 128;

    /// <summary>Hard cap for a single share link.</summary>
    public const int MaxUriLen = 16 * 1024;
}
