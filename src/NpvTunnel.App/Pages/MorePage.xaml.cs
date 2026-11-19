// More page: engine verification/install and the doctor audit.

using System.IO;
using System.Windows;
using System.Windows.Controls;
using NpvTunnel.Core;

namespace NpvTunnel.App.Pages;

/// <summary>More page interaction logic.</summary>
public partial class MorePage : UserControl
{
    /// <summary>Create the More page.</summary>
    public MorePage()
    {
        InitializeComponent();
        DataContext = App.Services.Main;
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        EnginePath.Text = App.Services.Installer.EnginePath
            ?? "no sing-box engine found (place sing-box next to NPVTunnel.exe "
            + "or in the npvtunnel data directory)";
    }

    private async void OnVerifyClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var version = await App.Services.Installer
                .VerifyAsync(CancellationToken.None).ConfigureAwait(true);
            EngineStatus.Text = $"Engine OK: {version}";
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("engine", ex);
            EngineStatus.Text = ex.Message;
        }
        catch (ObjectDisposedException ex)
        {
            App.Services.Log.Error("engine", ex);
            EngineStatus.Text = "engine process was disposed mid-verify";
        }
        catch (System.OperationCanceledException ex)
        {
            App.Services.Log.Error("engine", ex);
            EngineStatus.Text = "engine verify was cancelled";
        }
    }

    private void OnInstallClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var target = App.Services.Installer.InstallToDataDir();
            EngineStatus.Text = $"Installed: {target}";
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("engine", ex);
            EngineStatus.Text = ex.Message;
        }
        catch (IOException ex)
        {
            App.Services.Log.Error("engine", ex);
            EngineStatus.Text = $"install failed: {ex.Message}";
        }
    }

    private void OnDoctorClick(object sender, RoutedEventArgs e)
    {
        App.Services.Main.RefreshDoctor();
        DoctorList.Items.Refresh();
    }

    private void OnKeygenClick(object sender, RoutedEventArgs e)
    {
        App.Services.Main.GenerateIdentity();
        App.Services.Main.RefreshDoctor();
        DoctorList.Items.Refresh();
    }
}
