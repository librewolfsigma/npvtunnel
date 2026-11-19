// list and remove commands.

using System.Text.Json.Nodes;
using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Implementation of `npvtunnel list`.</summary>
public static class ListCommand
{
    /// <summary>Run the list command.</summary>
    public static int Run(string[] args, Options options)
    {
        var (_, data) = Program.OpenVault(options);
        if (options.Json)
        {
            ListJson(data);
            return 0;
        }

        if (data.Profiles.Count == 0)
        {
            ConsoleUi.Line("no profiles yet - import a link or a .npvt/.npvs file");
            return 0;
        }

        foreach (var profile in data.Profiles)
        {
            ConsoleUi.Line(
                $"{profile.Id}  {profile.Name,-24}  "
                + $"{profile.Protocol.AsStr(),-11}  {profile.Server}:{profile.Port}");
        }

        return 0;
    }

    private static void ListJson(VaultData data)
    {
        var profiles = new JsonArray();
        foreach (var profile in data.Profiles)
        {
            profiles.Add(ConsoleUi.Obj(
                ("id", ConsoleUi.S(profile.Id)),
                ("name", ConsoleUi.S(profile.Name)),
                ("protocol", ConsoleUi.S(profile.Protocol.AsStr())),
                ("server", ConsoleUi.S(profile.Server)),
                ("port", ConsoleUi.N(profile.Port)),
                ("udp", ConsoleUi.B(profile.Udp)),
                ("created_at", ConsoleUi.N((long)profile.CreatedAt))));
        }

        ConsoleUi.Json(ConsoleUi.Obj(("profiles", profiles)), false);
    }
}

/// <summary>Implementation of `npvtunnel remove`.</summary>
public static class RemoveCommand
{
    /// <summary>Run the remove command.</summary>
    public static int Run(string[] args, Options options)
    {
        if (args.Length != 1)
        {
            ConsoleUi.Error("usage: npvtunnel remove <id|name>");
            return 1;
        }

        var (vault, data) = Program.OpenVault(options);
        var removed = VaultOps.RemoveProfile(data, args[0]);
        Program.SaveVault(options, vault, data);
        if (options.Json)
        {
            ConsoleUi.Json(ConsoleUi.Obj(
                ("removed", ConsoleUi.S(removed.Id)),
                ("name", ConsoleUi.S(removed.Name))), false);
        }
        else
        {
            ConsoleUi.Line($"removed  {removed.Name}  ({removed.Id})");
        }

        return 0;
    }
}
