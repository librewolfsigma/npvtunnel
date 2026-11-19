// Single-instance guard: a named per-user mutex so a second launch focuses
// the existing window instead of starting a second app.

using System.Threading;

namespace NpvTunnel.App.Services;

/// <summary>Named-mutex single-instance guard.</summary>
public sealed class SingleInstance : IDisposable
{
    private const string MutexName = "Local\\npvtunnel-app-single-instance";
    private Mutex? _mutex;

    /// <summary>True when this process owns the instance slot.</summary>
    public bool IsOwner { get; private set; }

    /// <summary>Try to acquire the instance slot.</summary>
    public bool TryAcquire()
    {
        try
        {
            _mutex = new Mutex(initiallyOwned: true, MutexName, out var createdNew);
            IsOwner = createdNew;
            return createdNew;
        }
        catch (UnauthorizedAccessException)
        {
            // Another session already owns the name on this machine.
            IsOwner = false;
            return false;
        }
        catch (AbandonedMutexException)
        {
            // The previous owner crashed; the mutex is now owned by us and
            // the app may start.
            IsOwner = true;
            return true;
        }
    }

    /// <summary>Release the slot.</summary>
    public void Dispose()
    {
        if (_mutex is null)
        {
            return;
        }

        if (IsOwner)
        {
            try
            {
                _mutex.ReleaseMutex();
            }
            catch (ApplicationException)
            {
                // Release without ownership is impossible here; nothing to undo.
            }
        }

        _mutex.Dispose();
        _mutex = null;
    }
}
