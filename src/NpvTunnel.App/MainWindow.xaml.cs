// Shell window: sidebar navigation and page hosting. Navigation rebuilds
// the hosted page so every page binds to the live view model.

using System.Windows;
using System.Windows.Controls;
using NpvTunnel.App.Pages;
using NpvTunnel.App.Services;
using NpvTunnel.Core;

namespace NpvTunnel.App;

/// <summary>Main window shell.</summary>
public partial class MainWindow : Window
{
    /// <summary>Create the shell and subscribe to the tunnel state.</summary>
    public MainWindow()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        App.Services.Engine.StateChanged += OnEngineStateChanged;
        HostPage("Home");
    }

    private void OnEngineStateChanged(TunnelState state)
    {
        if (PageHost.Content is HomePage home)
        {
            home.OnTunnelState(state);
        }
    }

    private void OnNavClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string page })
        {
            HighlightNav(page);
            HostPage(page);
        }
    }

    private void HighlightNav(string page)
    {
        NavHome.FontWeight =
            page == "Home" ? FontWeights.SemiBold : FontWeights.Normal;
    }

    private void HostPage(string page)
    {
        PageHost.Content = page switch
        {
            "Configs" => new ConfigsPage(),
            "Log" => new LogPage(),
            "More" => new MorePage(),
            _ => new HomePage(),
        };
    }

    private void OnQuitClick(object sender, RoutedEventArgs e)
    {
        // Quit stops the shared engine session first; failures are logged.
        try
        {
            App.Services.Engine.Stop();
        }
        catch (NpvException ex)
        {
            App.Services.Log.Error("quit", ex);
        }

        Close();
    }
}
