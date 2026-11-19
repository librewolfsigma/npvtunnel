// The v2 container codec: encode, decode, peek. Crypto primitives live in
// <see cref="LibSodium"/>; canonical JSON in <see cref="V2CanonicalJson"/>.

using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Generate a new creator signing key seed (Ed25519).</summary>
public static class V2Keys
{
    /// <summary>New 32-byte Ed25519 seed.</summary>
    public static byte[] NewSigningSeed()
    {
        var seed = new byte[32];
        RandomNumberGenerator.Fill(seed);
        return seed;
    }

    /// <summary>New X25519 secret key.</summary>
    public static byte[] NewX25519Secret()
    {
        var secret = new byte[32];
        RandomNumberGenerator.Fill(secret);
        return secret;
    }

    /// <summary>X25519 public key derived from a secret.</summary>
    public static byte[] X25519Public(byte[] secret) => LibSodium.ScalarMultBase(secret);

    /// <summary>Ed25519 public key derived from a seed.</summary>
    public static byte[] Ed25519Public(byte[] seed) =>
        LibSodium.SignKeypairFromSeed(seed).Public;
}

/// <summary>Encode half of the container codec (partial class).</summary>
public static partial class V2Container
{
    /// <summary>Encode a profile into a v2 container.</summary>
    public static byte[] Encode(Profile profile, V2Policy policy, V2Lock lockMode, byte[]? signerSeed)
    {
        profile.Validate();
        var payloadJson = PayloadJson(profile);
        var header = new V2Header
        {
            Format = "npvt2",
            Version = V2Const.FormatVersion,
            Created = ProfileValidation.UnixNow(),
            Policy = policy.Clone(),
            Crypto = new V2CryptoHeader { Mode = "open" },
            Payload = new V2PayloadDescr { Alg = "xchacha20poly1305", Encoding = "json" },
            Sig = null,
        };
        var payloadBlob = ApplyLock(header, payloadJson, lockMode);
        if (signerSeed is not null)
        {
            header.Sig = Sign(header, payloadBlob, signerSeed);
        }

        return Frame(header, payloadBlob);
    }

    /// <summary>Peek at the lock mode without unlocking.</summary>
    public static string PeekLock(byte[] bytes)
    {
        var (header, _) = ParseFrame(bytes);
        return header.Crypto.Mode;
    }
}
