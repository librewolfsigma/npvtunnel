// Import flows on the main view model: links, files, .npvt/.npvs
// containers and .npvs subscription bundles - all through ImportBatch.

using NpvTunnel.App.Services;
using NpvTunnel.Core;

namespace NpvTunnel.App.ViewModels;

/// <summary>Import behaviour.</summary>
public sealed partial class MainViewModel
{
    /// <summary>Outcome of an import, for the status bar.</summary>
    public string? LastImportSummary { get; private set; }

    /// <summary>Import one share link or npvt:// / npvs:// link.</summary>
    public void ImportLink(string link)
    {
        try
        {
            var (vault, data) = Reload();
            var result = new BatchResult();
            ImportBatch.ImportLink(link, "link", data, result);
            Commit(vault, data);
            Summarize(result);
        }
        catch (NpvException e)
        {
            _log.Error("import", e);
            LastImportSummary = e.Message;
            Raise(nameof(LastImportSummary));
        }
    }

    /// <summary>Import a file: .npvt, .npvs, v2 container or subscription text.</summary>
    public void ImportFile(string path)
    {
        try
        {
            var (vault, data) = Reload();
            var result = new BatchResult();
            ImportBatch.ImportFile(path, data, result);
            Commit(vault, data);
            Summarize(result);
        }
        catch (NpvException e)
        {
            _log.Error("import", e);
            LastImportSummary = e.Message;
            Raise(nameof(LastImportSummary));
        }
    }

    /// <summary>Import an .npvs subscription bundle from raw text.</summary>
    public void ImportNpvsText(string text, string origin)
    {
        try
        {
            var (vault, data) = Reload();
            var entries = NpvsReader.ReadText(text, origin);
            var result = new BatchResult();
            ImportBatch.ImportNpvsEntries(entries, data, result);
            Commit(vault, data);
            Summarize(result);
        }
        catch (NpvException e)
        {
            _log.Error("import", e);
            LastImportSummary = e.Message;
            Raise(nameof(LastImportSummary));
        }
    }

    private (Vault Vault, VaultData Data) Reload()
    {
        var data = _vault.Load();
        return (_vault, data);
    }

    private void Commit(Vault vault, VaultData data)
    {
        vault.Save(data);
        RefreshProfiles();
        RefreshDoctor();
    }

    private void Summarize(BatchResult result)
    {
        _log.Log(
            LogLevel.Info, "import",
            $"{result.Imported.Count} imported, {result.Failed.Count} failed");
        foreach (var (origin, error) in result.Failed)
        {
            _log.Log(LogLevel.Warning, "import", $"{origin}: {error}");
        }

        LastImportSummary = result.Imported.Count == 0 && result.Failed.Count > 0
            ? result.Failed[0].Error
            : $"{result.Imported.Count} imported, {result.Failed.Count} failed";
        Raise(nameof(LastImportSummary));
    }

    /// <summary>Remove the selected profile.</summary>
    public void RemoveSelected()
    {
        if (Selected is null)
        {
            return;
        }

        try
        {
            var data = _vault.Load();
            VaultOps.RemoveProfile(data, Selected.Id);
            _vault.Save(data);
            _log.Log(LogLevel.Info, "vault", $"removed {Selected.Id}");
            RefreshProfiles();
            RefreshDoctor();
        }
        catch (NpvException e)
        {
            _log.Error("vault", e);
            LastImportSummary = e.Message;
            Raise(nameof(LastImportSummary));
        }
    }

    /// <summary>Generate the creator identity (replaces any existing one).</summary>
    public void GenerateIdentity()
    {
        try
        {
            var data = _vault.Load();
            data.Identity = VaultOps.GenerateIdentity();
            _vault.Save(data);
            RefreshProfiles();
            RefreshDoctor();
            _log.Log(LogLevel.Info, "keygen", "creator identity generated");
            LastImportSummary = "Creator identity generated";
            Raise(nameof(LastImportSummary));
        }
        catch (NpvException e)
        {
            _log.Error("keygen", e);
            LastImportSummary = e.Message;
            Raise(nameof(LastImportSummary));
        }
    }
}
