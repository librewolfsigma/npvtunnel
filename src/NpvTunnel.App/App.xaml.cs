// Application entry: wires the services, guards single-instance, and
// installs global exception handlers that LOG every failure (a crash is
// always recorded, never swallowed).

using System.Windows;
using NpvTunnel.App.Services;
using NpvTunnel.App.ViewModels;
using NpvTunnel.Core;

namespace NpvTunnel.App;

/// <summary>Application shell.</summary>
public partial class App : Application
{
    private static AppServices? _services;

    /// <summary>The shared services (created in OnStartup).</summary>
    public static AppServices Services =>
        _services
        ?? throw new InvalidOperationException(
            "the application has not completed startup yet");

    /// <summary>Startup: services, single-instance guard, exception hooks.</summary>
    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        var log = new LogService();
        log.Log(LogLevel.Info, "app", $"starting {Environment.OSVersion.VersionString}");

        var single = new SingleInstance();
        if (!single.TryAcquire())
        {
            log.Log(LogLevel.Warning, "app", "another instance is already running");
            Shutdown(0);
            return;
        }

        DispatcherUnhandledException += OnDispatcherUnhandledException;
        AppDomain.CurrentDomain.UnhandledException += OnDomainUnhandledException;
        TaskScheduler.UnobservedTaskException += OnUnobservedTaskException;

        var installer = new EngineInstaller(log);
        installer.Locate();
        var engine = new EngineManager(log, installer.EnginePath);
        var vault = new Vault(Vault.DefaultPath());
        _services = new AppServices(
            log, single, installer, engine, new MainViewModel(log, vault, engine));
        log.Log(LogLevel.Info, "app", "services ready");
    }

    private void OnDispatcherUnhandledException(
        object sender, System.Windows.Threading.DispatcherUnhandledExceptionEventArgs e)
    {
        Services.Log.Error("ui", e.Exception);
        e.Handled = true;
    }

    private void OnDomainUnhandledException(object sender, UnhandledExceptionEventArgs e)
    {
        if (e.ExceptionObject is Exception exception)
        {
            Services.Log.Error("app", exception);
        }
        else
        {
            Services.Log.Log(
                LogLevel.Error, "app", $"unhandled failure: {e.ExceptionObject}");
        }
    }

    private void OnUnobservedTaskException(
        object? sender, UnobservedTaskExceptionEventArgs e)
    {
        Services.Log.Error("task", e.Exception);
        e.SetObserved();
    }

    /// <summary>Exit: release the single-instance slot and log the stop.</summary>
    protected override void OnExit(ExitEventArgs e)
    {
        Services.Log.Log(LogLevel.Info, "app", $"exiting (code {e.ApplicationExitCode})");
        Services.Single.Dispose();
        base.OnExit(e);
    }
}

/// <summary>
/// Shared services built once in <see cref="App.OnStartup"/> and consumed by
/// pages, event handlers and the view model; every member is initialized
/// before the first window shows and stays alive for the whole session.
/// </summary>
public sealed class AppServices
{
    /// <summary>Create the bundle.</summary>
    public AppServices(
        LogService log,
        SingleInstance single,
        EngineInstaller installer,
        EngineManager engine,
        MainViewModel main)
    {
        Log = log;
        Single = single;
        Installer = installer;
        Engine = engine;
        Main = main;
    }

    /// <summary>File-backed sink for every GUI-side diagnostic line.</summary>
    public LogService Log { get; }

    /// <summary>The single-instance guard.</summary>
    public SingleInstance Single { get; }

    /// <summary>Locates and repairs the bundled engine binary on demand.</summary>
    public EngineInstaller Installer { get; }

    /// <summary>Owns the engine process: spawn, state transitions, stop.</summary>
    public EngineManager Engine { get; }

    /// <summary>Backs every page; holds the profile library and connect actions.</summary>
    public MainViewModel Main { get; }
}
