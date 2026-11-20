// The v2 container frame constants, lock and unlock types. Crypto design:
// Argon2id + XChaCha20-Poly1305 for passphrase locks, X25519 + HKDF-SHA-256
// for key locks, Ed25519 signatures over the AAD and payload digests, with
// authentication failures deliberately indistinguishable (CryptoAuth).


using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Container frame constants.</summary>
public static class V2Const
{
    /// <summary>Container magic.</summary>
    public static readonly byte[] Magic = { (byte)'N', (byte)'P', (byte)'V', (byte)'T', (byte)'2', 0 };

    /// <summary>Container format version implemented by this module.</summary>
    public const ushort FormatVersion = 1;

    /// <summary>Maximum accepted container size on decode.</summary>
    public const int MaxContainerLen = 8 * 1024 * 1024;

    /// <summary>Maximum base64 length accepted on npvt:// links.</summary>
    public const int MaxContainerB64Len = (MaxContainerLen / 3 + 1) * 4;

    /// <summary>Maximum header (JSON) size.</summary>
    public const int MaxHeaderLen = 256 * 1024;

    /// <summary>Maximum number of key-lock recipients per container.</summary>
    public const int MaxRecipients = 16;

    /// <summary>Frame overhead: magic(6) + version(2) + header_len(4) + payload_len(4).</summary>
    public const int FrameOverhead = 6 + 2 + 4 + 4;

    /// <summary>Signature domain separator.</summary>
    public static readonly byte[] SignDomain = Encoding.UTF8.GetBytes("npvt2-sign-v1");

    /// <summary>HKDF info for the per-recipient key derivation.</summary>
    public static readonly byte[] KdfInfoRecipient =
        Encoding.UTF8.GetBytes("npvt2-recipient-v1");

    /// <summary>Argon2id defaults: m=64 MiB, t=3, p=4.</summary>
    public const int KdfMib = 65536;
    public const int KdfT = 3;
    public const int KdfP = 4;
}

/// <summary>Lock mode for encoding; zeroizes the passphrase on dispose.</summary>
public sealed class V2Lock : IDisposable
{
    private V2Lock() { }

    /// <summary>The lock kind: "open", "passphrase" or "keys".</summary>
    public string Kind { get; private init; } = string.Empty;

    /// <summary>The passphrase (passphrase lock only).</summary>
    public string? PassphraseText { get; private init; }

    /// <summary>The recipient X25519 public keys (key lock only).</summary>
    public IReadOnlyList<byte[]>? Recipients { get; private init; }

    /// <summary>Anyone with the app: plaintext payload, signed if a signer is given.</summary>
    public static V2Lock Open() => new() { Kind = "open" };

    /// <summary>Passphrase lock (Argon2id + AEAD).</summary>
    public static V2Lock Passphrase(string passphrase) => new()
    {
        Kind = "passphrase",
        PassphraseText = passphrase,
    };

    /// <summary>Key lock for one or more recipient X25519 public keys.</summary>
    public static V2Lock Keys(IReadOnlyList<byte[]> recipients) => new()
    {
        Kind = "keys",
        Recipients = recipients,
    };

    /// <summary>Zeroize the passphrase buffer.</summary>
    public void Dispose()
    {
        if (PassphraseText is not null)
        {
            SecretShredder.Zeroize(Encoding.UTF8.GetBytes(PassphraseText));
        }
    }
}

/// <summary>How to unlock a container on decode.</summary>
public abstract class V2Unlock
{
    /// <summary>Only valid for Open containers.</summary>
    public static readonly V2Unlock Any = new UnlockAny();

    /// <summary>Passphrase for Passphrase-locked containers.</summary>
    public static V2Unlock Passphrase(string passphrase) => new UnlockPassphrase(passphrase);

    /// <summary>One or more X25519 secrets; the matching slot is auto-selected.</summary>
    public static V2Unlock Keys(IReadOnlyList<byte[]> secrets) => new UnlockKeys(secrets);

    private sealed class UnlockAny : V2Unlock;
    private sealed class UnlockPassphrase(string value) : V2Unlock
    {
        public string Value { get; } = value;
    }

    private sealed class UnlockKeys(IReadOnlyList<byte[]> secrets) : V2Unlock
    {
        public IReadOnlyList<byte[]> Secrets { get; } = secrets;
    }

    /// <summary>Downcast helper for the passphrase variant.</summary>
    public string? AsPassphrase() => this is UnlockPassphrase p ? p.Value : null;

    /// <summary>Downcast helper for the keys variant.</summary>
    public IReadOnlyList<byte[]>? AsKeys() => this is UnlockKeys k ? k.Secrets : null;
}

/// <summary>Outcome of the creator-signature check on a v2 container;
/// Valid is true only when a signature was present and verified.</summary>
public sealed record V2Verification(bool Signed, string? Creator, bool Valid);

/// <summary>Payload of a container that passed the magic and unlock
/// checks: the profile, its signed share policy and whether the creator
/// signature verified.</summary>
public sealed class V2Decoded
{
    /// <summary>Validated before decode returns: ParsePayload fails
    /// closed on an invalid profile.</summary>
    public required Profile Profile { get; init; }

    /// <summary>Carried in the header, which is bound to the ciphertext
    /// as AAD - the policy cannot be swapped without failing decryption.</summary>
    public required V2Policy Policy { get; init; }

    /// <summary>Unsigned containers report Signed=false with Creator=null
    /// and Valid=false; a present but invalid signature aborts the decode
    /// (fails closed), so Valid=false only means unsigned.</summary>
    public required V2Verification Verification { get; init; }
}
