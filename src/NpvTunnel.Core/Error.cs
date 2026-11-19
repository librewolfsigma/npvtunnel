// Central error type. Error messages are user-facing: they never embed
// credentials or raw secrets, only field names and reasons.

namespace NpvTunnel.Core;

/// <summary>Stable classification used for CLI exit codes.</summary>
public enum ErrorClass
{
    /// <summary>Bad configuration or bad input file (exit 2).</summary>
    Config,
    /// <summary>Network or engine failure (exit 3).</summary>
    Network,
    /// <summary>Permission or vault-access failure (exit 4).</summary>
    Permission,
    /// <summary>Cryptographic failure (exit 5).</summary>
    Crypto,
    /// <summary>Filesystem failure (exit 6).</summary>
    Io,
    /// <summary>Anything else (exit 7).</summary>
    Other,
}

/// <summary>Crate-wide exception carrying a machine-readable class.</summary>
public class NpvException : Exception
{
    /// <summary>Stable class used for CLI exit codes.</summary>
    public ErrorClass Class { get; }

    /// <summary>Create an error with an explicit class.</summary>
    public NpvException(ErrorClass @class, string message) : base(message)
    {
        Class = @class;
    }

    /// <summary>Configuration validation failure (exit 2).</summary>
    public static NpvException InvalidConfig(string message) =>
        new(ErrorClass.Config, $"invalid configuration: {message}");

    /// <summary>Legacy v1 container that cannot be decoded (exit 2).</summary>
    public static NpvException V1Unsupported(string message) =>
        new(ErrorClass.Config, $"legacy v1 container cannot be imported: {message}");

    /// <summary>Authentication failed (deliberately vague, exit 5).</summary>
    public static NpvException CryptoAuth() =>
        new(ErrorClass.Crypto, "authentication failed: wrong passphrase/key or tampered container");

    /// <summary>Cryptographic parameter or encoding problem (exit 5).</summary>
    public static NpvException Crypto(string message) =>
        new(ErrorClass.Crypto, $"cryptographic failure: {message}");

    /// <summary>Vault (local store) problem (exit 4).</summary>
    public static NpvException VaultError(string message) =>
        new(ErrorClass.Permission, $"vault error: {message}");

    /// <summary>Engine (sing-box sidecar) problem (exit 3).</summary>
    public static NpvException EngineError(string message) =>
        new(ErrorClass.Network, $"engine error: {message}");

    /// <summary>Anything else (exit 7).</summary>
    public static NpvException Other(string message) =>
        new(ErrorClass.Other, message);
}
