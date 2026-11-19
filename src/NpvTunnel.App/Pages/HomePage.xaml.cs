// Home page: the status card and the connect action.

using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using NpvTunnel.App.Services;
using NpvTunnel.Core;

namespace NpvTunnel.App.Pages;

/// <summary>Home page interaction logic.</summary>
public partial class HomePage : UserControl
{
    private bool _subscribed;

    /// <summary>Create the home page bound to the shared view model.</summary>
    public HomePage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (!_subscribed)
        {
            App.Services.Main.PropertyChanged += (_, _) => RenderState();
            App.Services.Engine.StateChanged += _ => RenderState();
            _subscribed = true;
        }

        RenderState();
    }

    /// <summary>Called by the shell when the tunnel state changes.</summary>
    public void OnTunnelState(TunnelState state) => RenderState();

    private void RenderState()
    {
        var main = App.Services.Main;
        var connected = App.Services.Engine.State == TunnelState.Connected;
        StatusText.Text = main.StatusText;
        StatusDetail.Text = main.StatusDetail;
        ConnectButton.Content = main.ConnectLabel;
        StatusText.Foreground = connected
            ? (Brush)FindResource("Connected")
            : (Brush)FindResource("TextSecondary");
        var profile = main.Selected?.Profile;
        ProfileName.Text = profile?.Name ?? "-";
        ProfileEndpoint.Text = profile is null
            ? "select a profile on the Configs page"
            : $"{profile.Protocol.AsStr()}  {profile.Server}:{profile.Port}";
        ImportSummary.Text = main.LastImportSummary ?? string.Empty;
    }

    private async void OnConnectClick(object sender, RoutedEventArgs e)
    {
        // Event handler: exceptions are logged, never swallowed.
        try
        {
            await App.Services.Main.ToggleConnectionAsync().ConfigureAwait(true);
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("tunnel", ex);
        }
        finally
        {
            RenderState();
        }
    }
}
