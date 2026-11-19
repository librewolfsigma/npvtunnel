// Tunnel lifecycle commands on the main view model.

using NpvTunnel.App.Services;
using NpvTunnel.Core;

namespace NpvTunnel.App.ViewModels;

/// <summary>Connect / disconnect behaviour.</summary>
public sealed partial class MainViewModel
{
    /// <summary>Connect the selected profile (or disconnect when connected).</summary>
    public async Task ToggleConnectionAsync()
    {
        if (_engine.State == TunnelState.Connected)
        {
            await DisconnectAsync().ConfigureAwait(true);
            return;
        }

        await ConnectSelectedAsync().ConfigureAwait(true);
    }

    /// <summary>Connect the selected profile.</summary>
    public async Task ConnectSelectedAsync()
    {
        if (Selected is null)
        {
            _log.Log(LogLevel.Warning, "tunnel", "connect requested with no selection");
            return;
        }

        Raise(nameof(State), nameof(StatusText), nameof(StatusDetail), nameof(ConnectLabel));
        try
        {
            var profile = Selected.Profile;
            await Task.Run(() => _engine.Start(profile)).ConfigureAwait(true);
            _log.Log(LogLevel.Info, "tunnel", $"connected via {profile.Name}");
        }
        catch (NpvException e)
        {
            _log.Error("tunnel", e);
            _log.Log(LogLevel.Error, "tunnel", e.Message);
        }
        finally
        {
            Raise(nameof(State), nameof(StatusText), nameof(StatusDetail), nameof(ConnectLabel));
        }
    }

    /// <summary>Disconnect and stop the engine.</summary>
    public async Task DisconnectAsync()
    {
        try
        {
            await Task.Run(() => _engine.Stop()).ConfigureAwait(true);
            _log.Log(LogLevel.Info, "tunnel", "disconnected");
        }
        catch (NpvException e)
        {
            _log.Error("tunnel", e);
            _log.Log(LogLevel.Error, "tunnel", e.Message);
        }
        finally
        {
            Raise(nameof(State), nameof(StatusText), nameof(StatusDetail), nameof(ConnectLabel));
        }
    }
}
