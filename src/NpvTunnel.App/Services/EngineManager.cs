// Engine manager: the GUI-side session state machine. Owns the pid file,
// renders configs and starts/stops the sidecar through EngineRunner.

using System.Diagnostics;
using System.IO;
using NpvTunnel.Core;

namespace NpvTunnel.App.Services;

/// <summary>Tunnel session state.</summary>
public enum TunnelState
{
    /// <summary>No engine running.</summary>
    Disconnected,
    /// <summary>The engine is starting.</summary>
    Connecting,
    /// <summary>The engine is running and the inbound is up.</summary>
    Connected,
}

/// <summary>Manages the engine session for the GUI.</summary>
public sealed class EngineManager
{
    private readonly LogService _log;
    private readonly string? _enginePath;

    /// <summary>Create the manager with a logger and a located engine path.</summary>
    public EngineManager(LogService log, string? enginePath)
    {
        _log = log;
        _enginePath = enginePath;
    }

    /// <summary>The current session state.</summary>
    public TunnelState State
    {
        get => _state;
        private set
        {
            if (_state != value)
            {
                _state = value;
                StateChanged?.Invoke(value);
            }
        }
    }

    private TunnelState _state = TunnelState.Disconnected;

    /// <summary>Raised on every tunnel state transition.</summary>
    public event Action<TunnelState>? StateChanged;

    /// <summary>The running process id, or null.</summary>
    public int? Pid { get; private set; }

    /// <summary>Start the engine for a profile; returns the pid.</summary>
    public int Start(Profile profile)
    {
        if (_enginePath is null)
        {
            throw NpvException.EngineError(
                "no sing-box engine found (see the More page for setup)");
        }

        if (State == TunnelState.Connected || State == TunnelState.Connecting)
        {
            throw NpvException.EngineError(
                $"engine already running (pid {Pid}); disconnect first");
        }

        // SpawnEngine throws on failure; the state stays Disconnected then,
        // so there is no rollback catch to write.
        var pid = SpawnEngine(profile, _enginePath);
        Pid = pid;
        State = TunnelState.Connected;
        _log.Log(LogLevel.Info, "engine", $"running (pid {pid})");
        return pid;
    }

    private int SpawnEngine(Profile profile, string enginePath)
    {
        var configPath = EnginePaths.ConfigPath();
        var config = EngineConfig.Config(
            [profile], profile.Id, EngineConfig.DefaultSocksPort, "1.1.1.1");
        File.WriteAllBytes(configPath, EngineConfig.Render(config));
        var logPath = EnginePaths.LogPath();
        var pid = EngineRunner.Start(enginePath, configPath, logPath);
        EngineRunner.WritePidFile(EnginePaths.PidPath(), pid);
        _log.Log(
            LogLevel.Info, "engine",
            $"config rendered for {profile.Name}; pid {pid}");
        return pid;
    }

    /// <summary>Stop a running engine; stale sessions are cleaned up.</summary>
    public void Stop()
    {
        var pid = EngineRunner.ReadPidFile(EnginePaths.PidPath());
        if (pid is not null && EngineRunner.Alive(pid.Value))
        {
            EngineRunner.Stop(pid.Value);
            _log.Log(LogLevel.Info, "engine", $"stopped (pid {pid.Value})");
        }
        else
        {
            _log.Log(LogLevel.Info, "engine", "session was not running");
        }

        EngineRunner.ClearPidFile(EnginePaths.PidPath());
        Pid = null;
        State = TunnelState.Disconnected;
    }

    /// <summary>Restore session state after an app restart (pid file may be stale).</summary>
    public TunnelState ReconnectSession()
    {
        var pid = EngineRunner.ReadPidFile(EnginePaths.PidPath());
        if (pid is { } running && EngineRunner.Alive(running))
        {
            Pid = running;
            State = TunnelState.Connected;
            _log.Log(LogLevel.Info, "engine", $"reattached to session (pid {running})");
        }
        else
        {
            EngineRunner.ClearPidFile(EnginePaths.PidPath());
            State = TunnelState.Disconnected;
        }

        return State;
    }
}
