// Engine process management for the CLI: spawn the sing-box sidecar, keep
// the pid file alive-checked, and stop it. Rendering lives in
// NpvTunnel.Core (EngineConfig); this class is process glue only.

using System.Diagnostics;

namespace NpvTunnel.Core;

/// <summary>Spawns and stops the engine sidecar process.</summary>
public static class EngineRunner
{
    /// <summary>Spawn the engine; append stdout/stderr to the session log.</summary>
    public static int Start(string bin, string configPath, string logPath)
    {
        var logDir = Path.GetDirectoryName(logPath);
        if (!string.IsNullOrEmpty(logDir))
        {
            Directory.CreateDirectory(logDir);
        }
        using var log = new FileStream(
            logPath, FileMode.Append, FileAccess.Write, FileShare.Read);
        using var process = new Process();
        process.StartInfo = new ProcessStartInfo
        {
            FileName = bin,
            Arguments = $"run -c \"{configPath}\"",
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        process.Start();
        AttachOutput(process, log);
        if (process.HasExited)
        {
            throw NpvException.EngineError(
                $"engine exited immediately with code {process.ExitCode}");
        }

        return process.Id;
    }

    private static void AttachOutput(Process process, FileStream log)
    {
        // Forward child output into the session log file asynchronously;
        // the handlers never throw and always flush to the same file.
        process.OutputDataReceived += (_, e) => WriteLine(log, e.Data);
        process.ErrorDataReceived += (_, e) => WriteLine(log, e.Data);
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();
    }

    private static void WriteLine(FileStream log, string? line)
    {
        if (string.IsNullOrEmpty(line))
        {
            return;
        }

        try
        {
            var bytes = System.Text.Encoding.UTF8.GetBytes(line + "\n");
            log.Write(bytes);
            log.Flush();
        }
        catch (IOException)
        {
            // A closed log file must not kill the reader loop; the child
            // keeps running and the next line retries.
        }
    }

    /// <summary>Best-effort liveness probe for a pid.</summary>
    public static bool Alive(int pid)
    {
        if (OperatingSystem.IsWindows())
        {
            return AliveWindows(pid);
        }

        return Directory.Exists($"/proc/{pid}");
    }

    private static bool AliveWindows(int pid)
    {
        // Match the pid as an exact quoted CSV field of the tasklist row, so
        // a substring of another column (a memory figure) can never be
        // mistaken for a running process.
        var needle = $"\"{pid}\"";
        using var process = new Process();
        process.StartInfo = new ProcessStartInfo
        {
            FileName = "tasklist",
            Arguments = $"/FI \"PID eq {pid}\" /NH /FO CSV",
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
        };
        process.Start();
        var output = process.StandardOutput.ReadToEnd();
        process.WaitForExit(3000);
        return output.Contains(needle, StringComparison.Ordinal);
    }

    /// <summary>Stop a running engine by pid (taskkill on Windows, kill on Unix).</summary>
    public static void Stop(int pid)
    {
        using var process = new Process();
        if (OperatingSystem.IsWindows())
        {
            process.StartInfo = new ProcessStartInfo
            {
                FileName = "taskkill",
                Arguments = $"/PID {pid} /F /T",
                UseShellExecute = false,
                CreateNoWindow = true,
            };
        }
        else
        {
            process.StartInfo = new ProcessStartInfo
            {
                FileName = "kill",
                Arguments = pid.ToString(),
                UseShellExecute = false,
                CreateNoWindow = true,
            };
        }

        process.Start();
        process.WaitForExit(5000);
        if (process.ExitCode != 0)
        {
            throw NpvException.EngineError("engine was not running anymore");
        }
    }

    /// <summary>Read the pid file; null when absent or unreadable.</summary>
    public static int? ReadPidFile(string path)
    {
        if (!File.Exists(path))
        {
            return null;
        }

        var text = File.ReadAllText(path).Trim();
        if (!int.TryParse(text, out var pid))
        {
            throw NpvException.EngineError("session pid file is corrupt");
        }

        return pid;
    }

    /// <summary>Write the pid file atomically.</summary>
    public static void WritePidFile(string path, int pid) =>
        File.WriteAllText(path, pid.ToString() + "\n");

    /// <summary>Remove the pid file when present.</summary>
    public static void ClearPidFile(string path)
    {
        if (File.Exists(path))
        {
            File.Delete(path);
        }
    }
}
