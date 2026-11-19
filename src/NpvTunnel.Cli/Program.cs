// The npvtunnel CLI: argument parsing and command dispatch. Command bodies
// live in Commands/*.cs; this file stays inside the file-size budget and
// only routes.

using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Parsed global options.</summary>
public sealed class Options
{
    /// <summary>Vault override path (--vault).</summary>
    public string? VaultPath { get; set; }

    /// <summary>Machine-readable output (--json).</summary>
    public bool Json { get; set; }

    /// <summary>Assume yes for interactive prompts (-y).</summary>
    public bool Yes { get; set; }

    /// <summary>Read the passphrase from stdin instead of a prompt.</summary>
    public bool PassphraseStdin { get; set; }
}

/// <summary>Entry point and dispatcher.</summary>
public static class Program
{
    /// <summary>Stable exit-code contract, mirroring the Rust CLI.</summary>
    public static int ExitCode(NpvException error) => error.Class switch
    {
        ErrorClass.Config => 2,
        ErrorClass.Network => 3,
        ErrorClass.Permission => 4,
        ErrorClass.Crypto => 5,
        ErrorClass.Io => 6,
        _ => 7,
    };

    /// <summary>CLI entry point.</summary>
    public static int Main(string[] args)
    {
        if (args.Length == 0)
        {
            Usage();
            return 1;
        }

        var (command, rest, options) = SplitArgs(args);
        var commands = new Dictionary<string, Func<string[], Options, int>>(
            StringComparer.Ordinal)
        {
            ["import"] = ImportCommand.Run,
            ["list"] = ListCommand.Run,
            ["remove"] = RemoveCommand.Run,
            ["connect"] = ConnectCommand.Run,
            ["stop"] = StopCommand.Run,
            ["keygen"] = KeygenCommand.Run,
            ["export"] = ExportCommand.Run,
            ["share"] = ShareCommand.Run,
            ["doctor"] = DoctorCommand.Run,
        };
        if (command is "--help" or "-h" or "help")
        {
            Usage();
            return 0;
        }

        if (command.Length == 0)
        {
            Usage();
            return 1;
        }

        if (!commands.TryGetValue(command, out var run))
        {
            ConsoleUi.Error($"unknown command '{command}'");
            Usage();
            return 1;
        }

        try
        {
            return run(rest, options);
        }
        catch (NpvException e)
        {
            ConsoleUi.Error(e.Message);
            return ExitCode(e);
        }
        catch (IOException e)
        {
            ConsoleUi.Error($"io error: {e.Message}");
            return 6;
        }
    }

    private static (string Name, string[] Args, Options Options) SplitArgs(
        string[] args)
    {
        var options = new Options();
        var rest = new List<string>();
        string? command = null;
        var index = 0;
        while (index < args.Length)
        {
            var arg = args[index];
            index++;
            if (arg == "--vault" && index < args.Length)
            {
                options.VaultPath = args[index];
                index++;
            }
            else if (arg == "--json")
            {
                options.Json = true;
            }
            else if (arg is "-y" or "--yes")
            {
                options.Yes = true;
            }
            else if (arg == "--passphrase-stdin")
            {
                options.PassphraseStdin = true;
            }
            else if (command is null)
            {
                command = arg;
            }
            else
            {
                rest.Add(arg);
            }
        }

        return (command ?? string.Empty, [.. rest], options);
    }

    /// <summary>Open the vault from the options (default path otherwise).</summary>
    public static (Vault, VaultData) OpenVault(Options options)
    {
        var vault = new Vault(options.VaultPath ?? Vault.DefaultPath());
        return (vault, vault.Load());
    }

    /// <summary>Persist the vault.</summary>
    public static void SaveVault(Options options, Vault vault, VaultData data) =>
        vault.Save(data);

    private static void Usage()
    {
        ConsoleUi.Line("npvtunnel - import, export, lock, share, list, connect");
        ConsoleUi.Blank();
        ConsoleUi.Line("Usage: npvtunnel <command> [args] [options]");
        ConsoleUi.Blank();
        ConsoleUi.Line("Commands:");
        ConsoleUi.Line("  import <link|file>   import a share link, .npvt/.npvs file, or npvt:// link");
        ConsoleUi.Line("  list                 list profiles");
        ConsoleUi.Line("  remove <id|name>     remove a profile");
        ConsoleUi.Line("  connect <id|name>    start the engine for a profile");
        ConsoleUi.Line("  stop                 stop the running engine");
        ConsoleUi.Line("  keygen               create the creator identity");
        ConsoleUi.Line("  export <id|name> <file> [passphrase]   export a locked .npvt v2 container");
        ConsoleUi.Line("  share <id|name>      print an npvt:// link (open container)");
        ConsoleUi.Line("  doctor               audit vault, engine and session state");
        ConsoleUi.Blank();
        ConsoleUi.Line("Options:");
        ConsoleUi.Line("  --vault <path>       override the vault file path");
        ConsoleUi.Line("  --json               machine-readable output");
        ConsoleUi.Line("  -y, --yes            assume yes");
        ConsoleUi.Line("  --passphrase-stdin   read the passphrase from stdin");
    }
}
