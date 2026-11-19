// import command: share links, files, .npvt/.npvs containers and .npvs
// subscription bundles, all through the shared ImportBatch dispatcher.

using System.Text.Json.Nodes;
using NpvTunnel.Core;

namespace NpvTunnel.Cli;

/// <summary>Implementation of `npvtunnel import`.</summary>
public static class ImportCommand
{
    /// <summary>Run the import command.</summary>
    public static int Run(string[] args, Options options)
    {
        if (args.Length == 0)
        {
            ConsoleUi.Error("usage: npvtunnel import <link|file>");
            return 1;
        }

        var (vault, data) = Program.OpenVault(options);
        var result = new BatchResult();
        foreach (var input in args)
        {
            if (File.Exists(input))
            {
                ImportBatch.ImportFile(input, data, result);
            }
            else
            {
                ImportBatch.ImportLink(input, input, data, result);
            }
        }

        Program.SaveVault(options, vault, data);
        Report(result, options);
        return result.Imported.Count > 0 ? 0 : 2;
    }

    private static void Report(BatchResult result, Options options)
    {
        if (options.Json)
        {
            ReportJson(result);
            return;
        }

        foreach (var entry in result.Imported)
        {
            ConsoleUi.Line(
                $"added  {entry.Profile.Name}  ({entry.Profile.Protocol.AsStr()} "
                + $"{entry.Profile.Server}:{entry.Profile.Port})  "
                + $"id {entry.Profile.Id}");
        }

        foreach (var (origin, error) in result.Failed)
        {
            ConsoleUi.Warn($"failed  {origin}: {error}");
        }

        ConsoleUi.Line($"{result.Imported.Count} imported, {result.Failed.Count} failed");
    }

    private static void ReportJson(BatchResult result)
    {
        var imported = new JsonArray();
        foreach (var entry in result.Imported)
        {
            imported.Add(ConsoleUi.Obj(
                ("id", ConsoleUi.S(entry.Profile.Id)),
                ("name", ConsoleUi.S(entry.Profile.Name)),
                ("protocol", ConsoleUi.S(entry.Profile.Protocol.AsStr())),
                ("server", ConsoleUi.S(entry.Profile.Server)),
                ("port", ConsoleUi.N(entry.Profile.Port)),
                ("origin", ConsoleUi.S(entry.Origin))));
        }

        var failed = new JsonArray();
        foreach (var (origin, error) in result.Failed)
        {
            failed.Add(ConsoleUi.Obj(
                ("origin", ConsoleUi.S(origin)),
                ("error", ConsoleUi.S(error))));
        }

        ConsoleUi.Json(ConsoleUi.Obj(
            ("imported", imported),
            ("failed", failed)), false);
    }
}
