// keygen, export and share commands: creator identity and container output.

using System.Text.Json.Nodes;
using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Implementation of `npvtunnel keygen`.</summary>
public static class KeygenCommand
{
    /// <summary>Run the keygen command.</summary>
    public static int Run(string[] args, Options options)
    {
        _ = args;
        var (vault, data) = Program.OpenVault(options);
        if (data.Identity is { } existing && !options.Yes)
        {
            throw NpvException.VaultError(
                "an identity already exists; pass -y to replace it");
        }

        data.Identity = VaultOps.GenerateIdentity();
        Program.SaveVault(options, vault, data);
        var creator = B64(V2Keys.Ed25519Public(data.Identity.Ed25519Seed));
        if (options.Json)
        {
            ConsoleUi.Json(ConsoleUi.Obj(("creator", ConsoleUi.S(creator))), false);
        }
        else
        {
            ConsoleUi.Line($"creator identity created: {creator}");
        }

        return 0;
    }

    internal static string B64(byte[] data) => Convert.ToBase64String(data)
        .TrimEnd('=').Replace('+', '-').Replace('/', '_');
}

/// <summary>Implementation of `npvtunnel export`.</summary>
public static class ExportCommand
{
    /// <summary>Run the export command: export <id|name> <file> [passphrase].</summary>
    public static int Run(string[] args, Options options)
    {
        if (args.Length < 2 || args.Length > 3)
        {
            ConsoleUi.Error("usage: npvtunnel export <id|name> <file> [passphrase]");
            return 1;
        }

        var (_, data) = Program.OpenVault(options);
        var profile = VaultOps.FindProfile(data, args[0]);
        var passphrase = args.Length == 3
            ? args[2]
            : ReadPassphrase(options);
        var policy = new V2Policy();
        using var lockMode = V2Lock.Passphrase(passphrase);
        var signerSeed = data.Identity?.Ed25519Seed;
        var bytes = V2Container.Encode(profile, policy, lockMode, signerSeed);
        File.WriteAllBytes(args[1], bytes);
        Report(args[1], bytes.Length, signerSeed is not null, options);
        return 0;
    }

    private static string ReadPassphrase(Options options)
    {
        if (!options.PassphraseStdin)
        {
            throw NpvException.InvalidConfig(
                "a passphrase is required (pass it as an argument or use "
                + "--passphrase-stdin)");
        }

        var line = Console.ReadLine();
        if (string.IsNullOrEmpty(line))
        {
            throw NpvException.InvalidConfig("passphrase must not be empty");
        }

        return line;
    }

    private static void Report(string path, int length, bool signed, Options options)
    {
        if (options.Json)
        {
            ConsoleUi.Json(ConsoleUi.Obj(
                ("file", ConsoleUi.S(path)),
                ("bytes", ConsoleUi.N(length)),
                ("signed", ConsoleUi.B(signed))), false);
            return;
        }

        ConsoleUi.Line(
            $"wrote {path} ({length} bytes, "
            + (signed ? "signed" : "unsigned") + ")");
    }
}

/// <summary>Implementation of `npvtunnel share`.</summary>
public static class ShareCommand
{
    /// <summary>Run the share command: share <id|name> prints an npvt:// link.</summary>
    public static int Run(string[] args, Options options)
    {
        if (args.Length != 1)
        {
            ConsoleUi.Error("usage: npvtunnel share <id|name>");
            return 1;
        }

        var (_, data) = Program.OpenVault(options);
        var profile = VaultOps.FindProfile(data, args[0]);
        using var lockMode = V2Lock.Open();
        var bytes = V2Container.Encode(
            profile, new V2Policy(), lockMode, data.Identity?.Ed25519Seed);
        var body = KeygenCommand.B64(bytes);
        if (options.Json)
        {
            ConsoleUi.Json(ConsoleUi.Obj(
                ("link", ConsoleUi.S("npvt://" + body))), false);
        }
        else
        {
            ConsoleUi.Line("npvt://" + body);
        }

        return 0;
    }
}
