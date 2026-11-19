// doctor command: audit the vault, the engine and the session state.

using System.Text.Json.Nodes;
using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Implementation of `npvtunnel doctor`.</summary>
public static class DoctorCommand
{
    /// <summary>Run the doctor command.</summary>
    public static int Run(string[] args, Options options)
    {
        _ = args;
        var checks = new List<AuditFinding>();
        CheckVault(checks, options);
        CheckEngine(checks);
        CheckSession(checks);

        if (options.Json)
        {
            ReportJson(checks);
        }
        else
        {
            ReportHuman(checks);
        }

        return checks.Any(c => c.Severity == AuditSeverity.High) ? 2 : 0;
    }

    private static void CheckVault(List<AuditFinding> checks, Options options)
    {
        var path = options.VaultPath ?? Vault.DefaultPath();
        if (!File.Exists(path))
        {
            checks.Add(new AuditFinding
            {
                Severity = AuditSeverity.Info,
                Check = "vault",
                Message = $"vault does not exist yet ({path})",
            });
            return;
        }

        var data = new Vault(path).Load();
        checks.Add(new AuditFinding
        {
            Severity = AuditSeverity.Info,
            Check = "vault",
            Message = $"{data.Profiles.Count} profiles, "
                + (data.Identity is null ? "no" : "one") + " creator identity",
        });
        checks.AddRange(ConfigAudit.Audit(data.Profiles));
    }

    private static void CheckEngine(List<AuditFinding> checks)
    {
        var bin = EnginePaths.FindEngineBinary(null);
        checks.Add(new AuditFinding
        {
            Severity = bin is null ? AuditSeverity.Warning : AuditSeverity.Info,
            Check = "engine",
            Message = bin is null
                ? "no sing-box engine found; connect will fail until one is bundled"
                : $"engine found at {bin}",
        });
    }

    private static void CheckSession(List<AuditFinding> checks)
    {
        var pid = EngineRunner.ReadPidFile(EnginePaths.PidPath());
        if (pid is null)
        {
            checks.Add(new AuditFinding
            {
                Severity = AuditSeverity.Info,
                Check = "session",
                Message = "no engine session",
            });
            return;
        }

        var alive = EngineRunner.Alive(pid.Value);
        checks.Add(new AuditFinding
        {
            Severity = alive ? AuditSeverity.Info : AuditSeverity.Warning,
            Check = "session",
            Message = alive
                ? $"engine running (pid {pid.Value})"
                : $"stale pid file: pid {pid.Value} is not running",
        });
    }

    private static void ReportJson(List<AuditFinding> checks)
    {
        var findings = new JsonArray();
        foreach (var finding in checks)
        {
            findings.Add(ConsoleUi.Obj(
                ("severity", ConsoleUi.S(finding.Severity.ToString().ToLowerInvariant())),
                ("check", ConsoleUi.S(finding.Check)),
                ("message", ConsoleUi.S(finding.Message))));
        }

        ConsoleUi.Json(ConsoleUi.Obj(("checks", findings)), false);
    }

    private static void ReportHuman(List<AuditFinding> checks)
    {
        foreach (var finding in checks)
        {
            var marker = finding.Severity switch
            {
                AuditSeverity.High => "!!",
                AuditSeverity.Warning => " ?",
                _ => "  ",
            };
            ConsoleUi.Line($"{marker} [{finding.Check,-8}] {finding.Message}");
        }

        var high = checks.Count(c => c.Severity == AuditSeverity.High);
        ConsoleUi.Line($"{high} high-severity findings");
    }
}
