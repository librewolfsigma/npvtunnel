// Log page: the live application log tail.

using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;

namespace NpvTunnel.App.Pages;

/// <summary>Log page interaction logic.</summary>
public partial class LogPage : UserControl
{
    /// <summary>Create the log page and load the tail.</summary>
    public LogPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        App.Services.Log.EntryWritten += OnEntryWritten;
        Reload();
    }

    private void OnEntryWritten(string line)
    {
        Dispatcher.BeginInvoke(() =>
        {
            LogBox.AppendText(line + "\n");
            LogBox.ScrollToEnd();
        });
    }

    private void Reload()
    {
        LogBox.Text = App.Services.Log.Tail(400);
        LogBox.ScrollToEnd();
    }

    private void OnRefreshClick(object sender, RoutedEventArgs e) => Reload();
}
