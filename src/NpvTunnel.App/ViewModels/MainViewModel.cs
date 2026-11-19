// Main view model: profile list, tunnel state and the doctor summary.
// Split across partials: .Connect.cs (tunnel lifecycle) and .Import.cs
// (the import flows).

using System.Collections.ObjectModel;
using System.ComponentModel;
using NpvTunnel.App.Services;
using NpvTunnel.Core;

namespace NpvTunnel.App.ViewModels;

/// <summary>The main application view model.</summary>
public sealed partial class MainViewModel : INotifyPropertyChanged
{
    private readonly LogService _log;
    private readonly Vault _vault;
    private readonly EngineManager _engine;

    /// <summary>Create the view model over the default vault.</summary>
    public MainViewModel(LogService log, Vault vault, EngineManager engine)
    {
        _log = log;
        _vault = vault;
        _engine = engine;
        Data = vault.Load();
        _engine.ReconnectSession();
        RefreshProfiles();
        RefreshDoctor();
    }

    /// <summary>Property-change notifications for bindings.</summary>
    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>The loaded vault data.</summary>
    public VaultData Data { get; private set; }

    /// <summary>Displayed profile rows.</summary>
    public ObservableCollection<ProfileRow> Profiles { get; } = [];

    /// <summary>The selected profile row.</summary>
    public ProfileRow? Selected { get; set; }

    /// <summary>Doctor summary lines for the More page.</summary>
    public ObservableCollection<string> Doctor { get; } = [];

    /// <summary>The current tunnel state.</summary>
    public TunnelState State => _engine.State;

    /// <summary>Big status text for the home card.</summary>
    public string StatusText => State switch
    {
        TunnelState.Connected => "Connected",
        TunnelState.Connecting => "Connecting...",
        _ => "Disconnected",
    };

    /// <summary>Detail line under the status.</summary>
    public string StatusDetail => State == TunnelState.Connected && _engine.Pid is { } pid
        ? $"SOCKS5 127.0.0.1:{EngineConfig.DefaultSocksPort} - engine pid {pid}"
        : "The tunnel is not running";

    /// <summary>Connect button label.</summary>
    public string ConnectLabel => State == TunnelState.Connected ? "DISCONNECT" : "CONNECT";

    /// <summary>Raise PropertyChanged for the given property.</summary>
    private void Raise(params string[] names)
    {
        foreach (var name in names)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    /// <summary>Reload the vault and rebuild the profile rows.</summary>
    public void RefreshProfiles()
    {
        Data = _vault.Load();
        var selectedId = Selected?.Id;
        Profiles.Clear();
        foreach (var profile in Data.Profiles)
        {
            Profiles.Add(new ProfileRow(profile));
        }

        Selected = Profiles.FirstOrDefault(p => p.Id == selectedId)
            ?? Profiles.FirstOrDefault();
        Raise(nameof(Profiles), nameof(Selected));
    }

    /// <summary>Reload the doctor summary.</summary>
    public void RefreshDoctor()
    {
        Doctor.Clear();
        foreach (var line in BuildDoctorLines())
        {
            Doctor.Add(line);
        }

        Raise(nameof(Doctor));
    }

    private List<string> BuildDoctorLines()
    {
        var lines = new List<string>();
        try
        {
            var findings = ConfigAudit.Audit(Data.Profiles);
            lines.Add($"Profiles: {Data.Profiles.Count}");
            lines.Add(Data.Identity is null
                ? "Creator identity: not generated (keygen)"
                : "Creator identity: present");
            lines.Add(findings.Count == 0
                ? "Config audit: clean"
                : $"Config audit: {findings.Count} finding(s)");
            foreach (var finding in findings.Take(5))
            {
                lines.Add($"  [{finding.Severity}] {finding.Message}");
            }
        }
        catch (NpvException e)
        {
            _log.Error("doctor", e);
            lines.Add($"Doctor failed: {e.Message}");
        }

        return lines;
    }
}

/// <summary>One profile row for the list view.</summary>
public sealed class ProfileRow
{
    /// <summary>Wrap a profile.</summary>
    public ProfileRow(Profile profile)
    {
        Profile = profile;
    }

    /// <summary>The underlying profile.</summary>
    public Profile Profile { get; }

    /// <summary>Profile id.</summary>
    public string Id => Profile.Id;

    /// <summary>Display name.</summary>
    public string Name => Profile.Name;

    /// <summary>Protocol name.</summary>
    public string Protocol => Profile.Protocol.AsStr();

    /// <summary>Endpoint text.</summary>
    public string Endpoint => $"{Profile.Server}:{Profile.Port}";
}
