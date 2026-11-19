// connect and stop commands: engine lifecycle through EngineRunner.

using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Implementation of `npvtunnel connect`.</summary>
public static class ConnectCommand
{
    /// <summary>Run the connect command.</summary>
    public static int Run(string[] args, Options options)
    {
        if (args.Length != 1)
        {
            ConsoleUi.Error("usage: npvtunnel connect <id|name>");
            return 1;
        }

        var (_, data) = Program.OpenVault(options);
        var profile = VaultOps.FindProfile(data, args[0]);
        EnsureNoRunningEngine();

        var bin = EnginePaths.FindEngineBinary(null)
            ?? throw NpvException.EngineError(
                "no sing-box engine found (place sing-box next to the "
                + "executable or in the npvtunnel data directory)");
        var config = EngineConfig.Config(
            [profile], profile.Id, EngineConfig.DefaultSocksPort, "1.1.1.1");
        var configPath = EnginePaths.ConfigPath();
        File.WriteAllBytes(
            configPath, EngineConfig.Render(config));
        var logPath = EnginePaths.LogPath();
        var pid = EngineRunner.Start(bin, configPath, logPath);
        EngineRunner.WritePidFile(EnginePaths.PidPath(), pid);
        Report(profile, pid, options);
        return 0;
    }

    private static void EnsureNoRunningEngine()
    {
        var pid = EngineRunner.ReadPidFile(EnginePaths.PidPath());
        if (pid is { } running && EngineRunner.Alive(running))
        {
            throw NpvException.EngineError(
                $"engine already running (pid {running}); run 'npvtunnel stop' first");
        }

        EngineRunner.ClearPidFile(EnginePaths.PidPath());
    }

    private static void Report(Profile profile, int pid, Options options)
    {
        if (options.Json)
        {
            ConsoleUi.Json(ConsoleUi.Obj(
                ("pid", ConsoleUi.N(pid)),
                ("profile", ConsoleUi.S(profile.Id)),
                ("socks", $"127.0.0.1:{EngineConfig.DefaultSocksPort}")), false);
            return;
        }

        ConsoleUi.Line(
            $"engine running (pid {pid}); SOCKS5 at 127.0.0.1:"
            + $"{EngineConfig.DefaultSocksPort} via {profile.Name}");
    }
}

/// <summary>Implementation of `npvtunnel stop`.</summary>
public static class StopCommand
{
    /// <summary>Run the stop command.</summary>
    public static int Run(string[] args, Options options)
    {
        _ = args;
        var pid = EngineRunner.ReadPidFile(EnginePaths.PidPath());
        if (pid is null || !EngineRunner.Alive(pid.Value))
        {
            EngineRunner.ClearPidFile(EnginePaths.PidPath());
            ConsoleUi.Line("engine is not running");
            return 0;
        }

        EngineRunner.Stop(pid.Value);
        EngineRunner.ClearPidFile(EnginePaths.PidPath());
        ConsoleUi.Line(options.Json
            ? ConsoleUi.Obj(("stopped", ConsoleUi.N(pid.Value))).ToJsonString()
            : $"engine stopped (pid {pid.Value})");
        return 0;
    }
}
