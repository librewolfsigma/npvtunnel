// Argon2id parameters, recipient slots and the header model for the v2
// container. Nested object order matches the Rust serde output exactly.


using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Share policy carried inside the signed header.</summary>
public sealed class V2Policy
{
    /// <summary>Unix seconds after which this config is considered expired.</summary>
    public ulong? ExpiresAt { get; set; }

    /// <summary>Creator asks that this config only run on metered connections.</summary>
    public bool MeteredOnly { get; set; }

    /// <summary>Suggested group label.</summary>
    public string? GroupHint { get; set; }

    /// <summary>True when ExpiresAt is set and in the past relative to now.</summary>
    public bool IsExpired(ulong nowUnix) => ExpiresAt is { } e && e <= nowUnix;

    /// <summary>Deep copy.</summary>
    public V2Policy Clone() => new()
    {
        ExpiresAt = ExpiresAt,
        MeteredOnly = MeteredOnly,
        GroupHint = GroupHint,
    };
}

/// <summary>Argon2id parameters stored in the header.</summary>
public sealed class V2KdfParams
{
    /// <summary>Always "argon2id".</summary>
    public string Alg { get; set; } = "argon2id";

    /// <summary>Memory cost in KiB.</summary>
    public uint MKib { get; set; } = V2Const.KdfMib;

    /// <summary>Time cost (iterations).</summary>
    public uint T { get; set; } = V2Const.KdfT;

    /// <summary>Parallelism (lanes).</summary>
    public uint P { get; set; } = V2Const.KdfP;

    /// <summary>Reject unknown algorithms and out-of-range parameters.</summary>
    public void Validate()
    {
        if (Alg != "argon2id")
        {
            throw NpvException.Crypto("unsupported KDF algorithm");
        }

        if (MKib is < 8192 or > 1048576 || T is < 1 or > 10 || P is < 1 or > 8)
        {
            throw NpvException.Crypto("KDF parameters out of accepted range");
        }
    }

    /// <summary>Derive a 32-byte AEAD key from a passphrase and salt.</summary>
    public byte[] Derive(string passphrase, byte[] salt)
    {
        Validate();
        using var argon2 = new Konscious.Security.Cryptography.Argon2id(
            Encoding.UTF8.GetBytes(passphrase))
        {
            Salt = salt,
            // Konscious takes memory in KiB, exactly the header's m_kib unit.
            MemorySize = checked((int)MKib),
            Iterations = (int)T,
            DegreeOfParallelism = (int)P,
        };
        return argon2.GetBytes(32);
    }
}

/// <summary>One key-lock recipient slot.</summary>
public sealed class V2RecipientSlot
{
    /// <summary>Recipient X25519 public key (base64url).</summary>
    public string RecipientPub { get; set; } = string.Empty;

    /// <summary>Ephemeral sender X25519 public key (base64url).</summary>
    public string EphPub { get; set; } = string.Empty;

    /// <summary>HKDF salt (base64url).</summary>
    public string Salt { get; set; } = string.Empty;

    /// <summary>AEAD nonce (base64url).</summary>
    public string Nonce { get; set; } = string.Empty;

    /// <summary>AEAD ciphertext (base64url).</summary>
    public string Ct { get; set; } = string.Empty;

    /// <summary>The AAD form of this slot: ciphertext cleared.</summary>
    public V2RecipientSlot NormalizedForAad() => new()
    {
        RecipientPub = RecipientPub,
        EphPub = EphPub,
        Salt = Salt,
        Nonce = Nonce,
        Ct = string.Empty,
    };
}

/// <summary>Creator signature block.</summary>
public sealed class V2SignatureBlock
{
    /// <summary>Always "ed25519".</summary>
    public string Alg { get; set; } = "ed25519";

    /// <summary>Creator Ed25519 public key (base64url).</summary>
    public string Pubkey { get; set; } = string.Empty;

    /// <summary>Signature value (base64url).</summary>
    public string Value { get; set; } = string.Empty;
}

/// <summary>Payload encoding description.</summary>
public sealed class V2PayloadDescr
{
    /// <summary>"xchacha20poly1305" (locked modes) or "none" (open mode).</summary>
    public string Alg { get; set; } = string.Empty;

    /// <summary>Always "json".</summary>
    public string Encoding { get; set; } = "json";
}

/// <summary>Lock-mode description stored in the header (serde tagged enum).</summary>
public sealed class V2CryptoHeader
{
    /// <summary>"passphrase", "keys" or "open".</summary>
    public string Mode { get; set; } = "open";

    /// <summary>Passphrase-lock KDF parameters.</summary>
    public V2KdfParams? Kdf { get; set; }

    /// <summary>Passphrase-lock salt (base64url).</summary>
    public string? Salt { get; set; }

    /// <summary>Passphrase-lock nonce (base64url).</summary>
    public string? Nonce { get; set; }

    /// <summary>Key-lock recipient slots.</summary>
    public List<V2RecipientSlot>? Recipients { get; set; }

    /// <summary>The AAD form: key-lock slots with ciphertexts cleared.</summary>
    public V2CryptoHeader NormalizedForAad() => Mode == "keys"
        ? new V2CryptoHeader
        {
            Mode = Mode,
            Recipients = (Recipients ?? []).Select(r => r.NormalizedForAad()).ToList(),
        }
        : new V2CryptoHeader
        {
            Mode = Mode,
            Kdf = Kdf,
            Salt = Salt,
            Nonce = Nonce,
        };

    /// <summary>Deep copy.</summary>
    public V2CryptoHeader Clone() => new()
    {
        Mode = Mode,
        Kdf = Kdf is null ? null : new V2KdfParams
        {
            Alg = Kdf.Alg, MKib = Kdf.MKib, T = Kdf.T, P = Kdf.P,
        },
        Salt = Salt,
        Nonce = Nonce,
        Recipients = Recipients?.Select(r => new V2RecipientSlot
        {
            RecipientPub = r.RecipientPub,
            EphPub = r.EphPub,
            Salt = r.Salt,
            Nonce = r.Nonce,
            Ct = r.Ct,
        }).ToList(),
    };
}

/// <summary>The container header (serde field order; deny-unknown-fields).</summary>
public sealed class V2Header
{
    /// <summary>Always "npvt2".</summary>
    public string Format { get; set; } = "npvt2";

    /// <summary>Container format version.</summary>
    public ushort Version { get; set; } = V2Const.FormatVersion;

    /// <summary>Creation time, unix seconds.</summary>
    public ulong Created { get; set; }

    /// <summary>Signed share policy.</summary>
    public V2Policy Policy { get; set; } = new();

    /// <summary>Lock mode description.</summary>
    public V2CryptoHeader Crypto { get; set; } = new() { Mode = "open" };

    /// <summary>Payload encoding description.</summary>
    public V2PayloadDescr Payload { get; set; } = new();

    /// <summary>Creator signature, if signed.</summary>
    public V2SignatureBlock? Sig { get; set; }

    /// <summary>Deep copy with independent nested objects.</summary>
    public V2Header Clone() => new()
    {
        Format = Format,
        Version = Version,
        Created = Created,
        Policy = Policy.Clone(),
        Crypto = Crypto.Clone(),
        Payload = new V2PayloadDescr { Alg = Payload.Alg, Encoding = Payload.Encoding },
        Sig = Sig is null ? null : new V2SignatureBlock
        {
            Alg = Sig.Alg, Pubkey = Sig.Pubkey, Value = Sig.Value,
        },
    };

    /// <summary>Header with the AAD-normalized crypto and no signature.</summary>
    public V2Header AadForm() => new()
    {
        Format = Format,
        Version = Version,
        Created = Created,
        Policy = Policy.Clone(),
        Crypto = Crypto.NormalizedForAad(),
        Payload = new V2PayloadDescr { Alg = Payload.Alg, Encoding = Payload.Encoding },
        Sig = null,
    };
}
