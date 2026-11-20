// What authenticates a profile. Variant names mirror the Rust serde
// externally-tagged form ("uuid", "password", "ssmethod", "token", "none").

namespace NpvTunnel.Core;

/// <summary>Base type of the credential enum; see variants below.</summary>
public abstract class Credential : IEquatable<Credential>
{
    /// <summary>The externally-tagged variant name.</summary>
    public abstract string Kind { get; }

    /// <summary>True for the no-credential variant.</summary>
    public bool IsNone => this is NoneCredential;

    /// <inheritdoc />
    public bool Equals(Credential? other)
    {
        if (other is null || other.Kind != Kind)
        {
            return false;
        }

        return Kind == "ssmethod"
            ? ((SsMethodCredential)this).Method == ((SsMethodCredential)other).Method
                && ((SsMethodCredential)this).Password == ((SsMethodCredential)other).Password
            : CredentialSecret(this) == CredentialSecret(other);
    }

    /// <inheritdoc />
    public override bool Equals(object? obj) => obj is Credential c && Equals(c);

    /// <inheritdoc />
    public override int GetHashCode() => HashCode.Combine(Kind, CredentialSecret(this));

    /// <summary>The secret text of a single-value credential ("" otherwise).</summary>
    public static string CredentialSecret(Credential c) => c switch
    {
        UuidCredential u => u.Value,
        PasswordCredential p => p.Value,
        TokenCredential t => t.Value,
        SsMethodCredential s => s.Method + ":" + s.Password,
        _ => string.Empty,
    };
}

/// <summary>UUID-style credential (VLESS/VMess).</summary>
public sealed class UuidCredential(string value) : Credential
{
    /// <summary>Client UUID; serialized verbatim under the "uuid" key by
    /// CredentialJsonConverter.</summary>
    public string Value { get; set; } = value;

    /// <inheritdoc />
    public override string Kind => "uuid";
}

/// <summary>Password-style credential (Trojan, Hysteria2, SSH).</summary>
public sealed class PasswordCredential(string value) : Credential
{
    /// <summary>Password text; serialized verbatim under the "password"
    /// key by CredentialJsonConverter.</summary>
    public string Value { get; set; } = value;

    /// <inheritdoc />
    public override string Kind => "password";
}

/// <summary>Shadowsocks method + password pair.</summary>
public sealed class SsMethodCredential(string method, string password) : Credential
{
    /// <summary>Cipher method name; import validation caps it at 128
    /// bytes.</summary>
    public string Method { get; set; } = method;

    /// <summary>Password half of the pair; validated with the standard
    /// 512-byte field cap.</summary>
    public string Password { get; set; } = password;

    /// <inheritdoc />
    public override string Kind => "ssmethod";
}

/// <summary>Opaque token (SOCKS/HTTP auth, DNSTT key material).</summary>
public sealed class TokenCredential(string value) : Credential
{
    /// <summary>Token text; serialized verbatim under the "token" key by
    /// CredentialJsonConverter.</summary>
    public string Value { get; set; } = value;

    /// <inheritdoc />
    public override string Kind => "token";
}

/// <summary>No credential.</summary>
public sealed class NoneCredential() : Credential
{
    /// <inheritdoc />
    public override string Kind => "none";
}
