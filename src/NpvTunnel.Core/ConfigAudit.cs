// Doctor-style config audit: the same checks the CLI doctor reports,
// extracted so the GUI and CLI share one implementation.

namespace NpvTunnel.Core;

/// <summary>Severity of one audit finding.</summary>
public enum AuditSeverity
{
    /// <summary>Informational; nothing to fix.</summary>
    Info,
    /// <summary>Worth fixing; defaults and hygiene.</summary>
    Warning,
    /// <summary>Security-relevant; fix before sharing the profile.</summary>
    High,
}

/// <summary>One audit finding.</summary>
public sealed class AuditFinding
{
    /// <summary>Profile under audit.</summary>
    public string ProfileId { get; init; } = string.Empty;

    /// <summary>Severity bucket.</summary>
    public AuditSeverity Severity { get; init; }

    /// <summary>Stable check name (machine-readable).</summary>
    public string Check { get; init; } = string.Empty;

    /// <summary>User-facing message (never contains secrets).</summary>
    public string Message { get; init; } = string.Empty;
}

/// <summary>Audit a set of profiles; deterministic ordering by id then check.</summary>
public static class ConfigAudit
{
    /// <summary>Run all checks against the given profiles.</summary>
    public static List<AuditFinding> Audit(IEnumerable<Profile> profiles)
    {
        var findings = new List<AuditFinding>();
        foreach (var profile in profiles)
        {
            findings.AddRange(AuditProfile(profile));
        }

        return findings
            .OrderBy(f => f.ProfileId, StringComparer.Ordinal)
            .ThenBy(f => f.Check, StringComparer.Ordinal)
            .ToList();
    }

    /// <summary>Run all checks against one profile.</summary>
    public static List<AuditFinding> AuditProfile(Profile profile)
    {
        var findings = new List<AuditFinding>();
        findings.AddRange(AuditInsecure(profile));
        findings.AddRange(AuditTransport(profile));
        findings.AddRange(AuditNaming(profile));
        return findings;
    }

    private static List<AuditFinding> AuditInsecure(Profile profile)
    {
        if (!profile.AllowInsecure)
        {
            return [];
        }

        return
        [
            new AuditFinding
            {
                ProfileId = profile.Id,
                Severity = AuditSeverity.High,
                Check = "allow-insecure",
                Message = "TLS certificate verification is disabled "
                    + "(allowInsecure=1): the profile is vulnerable to "
                    + "machine-in-the-middle attacks",
            },
        ];
    }

    private static List<AuditFinding> AuditTransport(Profile profile)
    {
        var findings = new List<AuditFinding>();
        if (profile.Transport == Transport.Tls && string.IsNullOrEmpty(profile.Sni))
        {
            findings.Add(new AuditFinding
            {
                ProfileId = profile.Id,
                Severity = AuditSeverity.Warning,
                Check = "missing-sni",
                Message = "TLS transport without an SNI server name: "
                    + "handshakes may fail behind CDNs or Reality remotes",
            });
        }

        return findings;
    }

    private static List<AuditFinding> AuditNaming(Profile profile)
    {
        var findings = new List<AuditFinding>();
        if (profile.Name.Length > 0 && char.IsWhiteSpace(profile.Name[0]))
        {
            findings.Add(new AuditFinding
            {
                ProfileId = profile.Id,
                Severity = AuditSeverity.Info,
                Check = "name-padding",
                Message = "profile name starts with whitespace",
            });
        }

        if (profile.Group is { } group && group.Length > 64)
        {
            findings.Add(new AuditFinding
            {
                ProfileId = profile.Id,
                Severity = AuditSeverity.Info,
                Check = "group-length",
                Message = "group label is unusually long",
            });
        }

        return findings;
    }
}
