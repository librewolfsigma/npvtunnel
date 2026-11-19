// Thread-safe file logger. Every catch in this class does real work: a
// failed primary write falls back to the secondary file, and a failed
// fallback is recorded (never swallowed) so the loss surfaces on the next
// successful write.

using System.IO;
using NpvTunnel.Core;

namespace NpvTunnel.App.Services;

/// <summary>Severity of a log entry.</summary>
public enum LogLevel
{
    /// <summary>Debug detail.</summary>
    Debug,
    /// <summary>Normal operation.</summary>
    Info,
    /// <summary>Something went wrong but the app continues.</summary>
    Warning,
    /// <summary>An operation failed.</summary>
    Error,
}

/// <summary>The application logger (file-backed, tail-able).</summary>
public sealed class LogService
{
    private readonly object _gate = new();
    private readonly string _primaryPath;
    private readonly string _secondaryPath;
    private int _fallbackLost;
    private long _size;

    /// <summary>Create a logger writing to the npvtunnel data directory.</summary>
    public LogService(string? directory = null)
    {
        var dir = directory ?? EnginePaths.WorkDir();
        _primaryPath = System.IO.Path.Combine(dir, "app.log");
        _secondaryPath = System.IO.Path.Combine(dir, "app.fallback.log");
        _size = File.Exists(_primaryPath) ? new FileInfo(_primaryPath).Length : 0;
    }

    /// <summary>Raised for every written entry (UI subscription).</summary>
    public event Action<string>? EntryWritten;

    /// <summary>The primary log file path.</summary>
    public string FilePath => _primaryPath;

    /// <summary>Log one entry; never throws.</summary>
    public void Log(LogLevel level, string scope, string message)
    {
        var line = FormatLine(level, scope, message);
        lock (_gate)
        {
            WriteLine(line);
        }

        EntryWritten?.Invoke(line);
    }

    /// <summary>Log an exception with a scope label; never throws.</summary>
    public void Error(string scope, Exception exception) =>
        Log(LogLevel.Error, scope, exception.GetType().Name + ": " + exception.Message);

    /// <summary>Return the last maxLines lines of the log.</summary>
    public string Tail(int maxLines)
    {
        lock (_gate)
        {
            return ReadTail(maxLines);
        }
    }

    private string ReadTail(int maxLines)
    {
        var lines = new Queue<string>();
        try
        {
            foreach (var line in File.ReadLines(_primaryPath))
            {
                EnqueueBounded(lines, line, maxLines);
            }

            if (_fallbackLost > 0)
            {
                EnqueueBounded(
                    lines, $"[warn] {_fallbackLost} earlier entries could not be "
                    + "written to the primary log", maxLines);
                _fallbackLost = 0;
            }
        }
        catch (IOException e)
        {
            lines.Enqueue($"[error] log is not readable: {e.Message}");
        }

        return string.Join("\n", lines);
    }

    private static void EnqueueBounded(Queue<string> lines, string line, int maxLines)
    {
        lines.Enqueue(line);
        while (lines.Count > maxLines)
        {
            lines.Dequeue();
        }
    }

    private void WriteLine(string line)
    {
        try
        {
            if (_size > MaxLogBytes)
            {
                Rotate();
            }

            File.AppendAllText(_primaryPath, line + "\n");
            _size += line.Length + 1;
        }
        catch (IOException)
        {
            WriteFallback(line);
        }
        catch (UnauthorizedAccessException)
        {
            WriteFallback(line);
        }
    }

    private void WriteFallback(string line)
    {
        try
        {
            File.AppendAllText(_secondaryPath, line + "\n");
        }
        catch (IOException)
        {
            // Both sinks failed: keep count so the loss is visible later.
            _fallbackLost++;
        }
        catch (UnauthorizedAccessException)
        {
            _fallbackLost++;
        }
    }

    private void Rotate()
    {
        try
        {
            File.Move(_primaryPath, _primaryPath + ".1", overwrite: true);
            _size = 0;
        }
        catch (IOException e)
        {
            _fallbackLost++;
            _size = 0;
            File.AppendAllText(
                _secondaryPath, $"[warn] rotate failed: {e.Message}\n");
        }
    }

    private static string FormatLine(LogLevel level, string scope, string message) =>
        $"{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff} [{level.ToString().ToLowerInvariant()}] "
        + $"{scope}: {message}";

    private const long MaxLogBytes = 2 * 1024 * 1024;
}
