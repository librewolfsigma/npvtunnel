// Configs page: the import card and the profile library.

using System.IO;
using System.Windows;
using System.Windows.Controls;
using Microsoft.Win32;
using NpvTunnel.Core;

namespace NpvTunnel.App.Pages;

/// <summary>Configs page interaction logic.</summary>
public partial class ConfigsPage : UserControl
{
    /// <summary>Create the page and bind the profile library.</summary>
    public ConfigsPage()
    {
        InitializeComponent();
        DataContext = App.Services.Main;
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e) => RenderStatus();

    private void RenderStatus()
    {
        ImportStatus.Text = App.Services.Main.LastImportSummary
            ?? "Nothing imported in this session yet.";
    }

    private void OnAddLinkClick(object sender, RoutedEventArgs e)
    {
        var link = LinkBox.Text.Trim();
        if (link.Length == 0)
        {
            ImportStatus.Text = "Paste a share link first.";
            return;
        }

        try
        {
            App.Services.Main.ImportLink(link);
            RefreshAfterImport();
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("import", ex);
            ImportStatus.Text = ex.Message;
        }
    }

    private void OnOpenFileClick(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFileDialog
        {
            Title = "Import .npvt / .npvs",
            Filter = "NPV Tunnel files (*.npvt;*.npvs)|*.npvt;*.npvs|"
                + "Subscription text (*.txt)|*.txt|All files (*.*)|*.*",
            CheckFileExists = true,
        };
        if (dialog.ShowDialog(App.Current.MainWindow) != true)
        {
            return;
        }

        try
        {
            App.Services.Main.ImportFile(dialog.FileName);
            RefreshAfterImport();
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("import", ex);
            ImportStatus.Text = ex.Message;
        }
    }

    private void RefreshAfterImport()
    {
        ProfileList.Items.Refresh();
        RenderStatus();
    }

    private void OnConnectClick(object sender, RoutedEventArgs e)
    {
        _ = ConnectSelectedAsync();
    }

    private async Task ConnectSelectedAsync()
    {
        try
        {
            await App.Services.Main.ConnectSelectedAsync().ConfigureAwait(true);
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("tunnel", ex);
        }
    }

    private void OnRemoveClick(object sender, RoutedEventArgs e)
    {
        App.Services.Main.RemoveSelected();
        ProfileList.Items.Refresh();
        RenderStatus();
    }

    private void OnRefreshClick(object sender, RoutedEventArgs e)
    {
        App.Services.Main.RefreshProfiles();
        ProfileList.Items.Refresh();
        RenderStatus();
    }
}
