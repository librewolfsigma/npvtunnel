// Engine installer: locate the sing-box sidecar, verify it can run, and
// place it into the per-user data directory when found next to the app.
// No network download in this release (documented; the bundled or PATH
// copy is used).

using System.Diagnostics;
using System.IO;
using NpvTunnel.Core;

namespace NpvTunnel.App.Services;

/// <summary>Locates and installs the engine sidecar binary.</summary>
public sealed class EngineInstaller
{
    private readonly LogService _log;

    /// <summary>Create the installer with a logger.</summary>
    public EngineInstaller(LogService log)
    {
        _log = log;
    }

    /// <summary>The located engine path, or null when missing.</summary>
    public string? EnginePath { get; private set; }

    /// <summary>Locate the engine; returns the path or null.</summary>
    public string? Locate()
    {
        var candidate = EnginePaths.FindEngineBinary(null);
        if (candidate is null)
        {
            _log.Log(LogLevel.Warning, "engine", "no sing-box engine found");
            EnginePath = null;
            return null;
        }

        EnginePath = candidate;
        _log.Log(LogLevel.Info, "engine", $"sing-box located: {candidate}");
        return candidate;
    }

    /// <summary>
    /// Copy the engine next to the app into the data directory so the CLI
    /// finds the same sidecar; returns the installed path.
    /// </summary>
    public string InstallToDataDir()
    {
        if (EnginePath is null)
        {
            throw NpvException.EngineError("locate the engine before installing");
        }

        var binDir = Path.Combine(EnginePaths.WorkDir(), "bin");
        Directory.CreateDirectory(binDir);
        var target = Path.Combine(binDir, Path.GetFileName(EnginePath));
        if (File.Exists(target))
        {
            _log.Log(LogLevel.Info, "engine", $"already installed: {target}");
            return target;
        }

        File.Copy(EnginePath, target, overwrite: false);
        _log.Log(LogLevel.Info, "engine", $"installed to {target}");
        return target;
    }

    /// <summary>Run `sing-box version` to confirm the binary executes.</summary>
    public async Task<string> VerifyAsync(CancellationToken cancellation)
    {
        if (EnginePath is null)
        {
            throw NpvException.EngineError("no engine located");
        }

        using var process = new Process();
        process.StartInfo = new ProcessStartInfo
        {
            FileName = EnginePath,
            Arguments = "version",
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        process.Start();
        var output = await process.StandardOutput.ReadToEndAsync(cancellation)
            .ConfigureAwait(false);
        await process.WaitForExitAsync(cancellation).ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw NpvException.EngineError(
                $"engine verify failed with exit code {process.ExitCode}");
        }

        var first = output.Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .FirstOrDefault() ?? "sing-box";
        _log.Log(LogLevel.Info, "engine", $"verify: {first}");
        return first;
    }
}
