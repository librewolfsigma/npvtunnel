// The v2 container decode path: frame parsing, signature verification and
// unlocking. Fails closed: a present signature that does not verify, a
// tampered payload, or a wrong passphrase/key all yield CryptoAuth.

using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace NpvTunnel.Core;

/// <summary>Decode half of the container codec (partial class).</summary>
public static partial class V2Container
{
    /// <summary>Decode and unlock a v2 container (fails closed).</summary>
    public static V2Decoded Decode(byte[] bytes, V2Unlock unlock)
    {
        var (header, payload) = ParseFrame(bytes);
        var verification = VerifySignature(header, payload);
        var aad = V2CanonicalJson.Write(header.AadForm());
        var plaintext = Open(header, payload, aad, unlock);
        var profile = ParsePayload(plaintext);
        return new V2Decoded
        {
            Profile = profile,
            Policy = header.Policy,
            Verification = verification,
        };
    }

    private static byte[] ApplyLock(V2Header header, byte[] payloadJson, V2Lock lockMode)
    {
        switch (lockMode.Kind)
        {
            case "open":
                header.Payload.Alg = "none";
                header.Crypto = new V2CryptoHeader { Mode = "open" };
                return payloadJson;
            case "passphrase":
                var passphrase = lockMode.PassphraseText;
                if (passphrase is null)
                {
                    throw NpvException.Crypto("passphrase lock is missing the passphrase");
                }

                return ApplyPassphrase(header, payloadJson, passphrase);
            case "keys":
                var recipients = lockMode.Recipients;
                if (recipients is null)
                {
                    throw NpvException.Crypto("key lock is missing its recipients");
                }

                return ApplyKeys(header, payloadJson, recipients);
            default:
                throw NpvException.Crypto("unknown lock mode");
        }
    }

    private static byte[] ApplyPassphrase(V2Header header, byte[] payloadJson, string passphrase)
    {
        var kdf = new V2KdfParams();
        var salt = RandomBytes(16);
        var nonce = RandomBytes(24);
        var key = kdf.Derive(passphrase, salt);
        header.Crypto = new V2CryptoHeader
        {
            Mode = "passphrase",
            Kdf = kdf,
            Salt = B64(salt),
            Nonce = B64(nonce),
        };
        var aad = V2CanonicalJson.Write(header.AadForm());
        return LibSodium.AeadEncrypt(key, nonce, payloadJson, aad);
    }

    private static byte[] ApplyKeys(
        V2Header header, byte[] payloadJson, IReadOnlyList<byte[]> recipients)
    {
        if (recipients.Count == 0 || recipients.Count > V2Const.MaxRecipients)
        {
            throw NpvException.Crypto(
                $"key lock needs 1..={V2Const.MaxRecipients} recipients");
        }

        var slots = new List<(V2RecipientSlot Slot, byte[] Key)>();
        foreach (var recipientPub in recipients)
        {
            slots.Add(MakeSlot(recipientPub));
        }

        // First pass: header with empty ciphertexts fixes the AAD.
        header.Crypto = new V2CryptoHeader
        {
            Mode = "keys",
            Recipients = slots.Select(s => s.Slot).ToList(),
        };
        var aad = V2CanonicalJson.Write(header.AadForm());
        // Second pass: encrypt per slot, then store the filled slots.
        foreach (var (slot, key) in slots)
        {
            var nonce = Unb64(slot.Nonce);
            slot.Ct = B64(LibSodium.AeadEncrypt(key, nonce, payloadJson, aad));
        }

        header.Crypto = new V2CryptoHeader
        {
            Mode = "keys",
            Recipients = slots.Select(s => s.Slot).ToList(),
        };
        return [];
    }

    private static (V2RecipientSlot Slot, byte[] Key) MakeSlot(byte[] recipientPub)
    {
        if (recipientPub.Length != 32)
        {
            throw NpvException.Crypto("recipient key must be 32 bytes");
        }

        var ephSecret = V2Keys.NewX25519Secret();
        var ephPub = LibSodium.ScalarMultBase(ephSecret);
        var shared = LibSodium.ScalarMult(ephSecret, recipientPub);
        var salt = RandomBytes(16);
        var nonce = RandomBytes(24);
        var key = HkdfSha256(shared, salt);
        var slot = new V2RecipientSlot
        {
            RecipientPub = B64(recipientPub),
            EphPub = B64(ephPub),
            Salt = B64(salt),
            Nonce = B64(nonce),
            Ct = string.Empty,
        };
        return (slot, key);
    }

    private static V2SignatureBlock Sign(V2Header header, byte[] payload, byte[] seed)
    {
        var (publicKey, secretKey) = LibSodium.SignKeypairFromSeed(seed);
        var aad = V2CanonicalJson.Write(header.AadForm());
        var message = SignInput(aad, payload);
        return new V2SignatureBlock
        {
            Alg = "ed25519",
            Pubkey = B64(publicKey),
            Value = B64(LibSodium.SignDetached(message, secretKey)),
        };
    }

    private static V2Verification VerifySignature(V2Header header, byte[] payload)
    {
        var sig = header.Sig;
        if (sig is null)
        {
            return new V2Verification(false, null, false);
        }

        if (sig.Alg != "ed25519")
        {
            throw NpvException.Crypto("unsupported signature algorithm");
        }

        var publicKey = Unb64(sig.Pubkey);
        if (publicKey.Length != 32)
        {
            throw NpvException.Crypto("bad signature pubkey length");
        }

        var signature = Unb64(sig.Value);
        if (signature.Length != 64)
        {
            throw NpvException.Crypto("bad signature length");
        }

        var message = SignInput(V2CanonicalJson.Write(header.AadForm()), payload);
        if (!LibSodium.VerifyDetached(signature, message, publicKey))
        {
            throw NpvException.CryptoAuth();
        }

        return new V2Verification(true, sig.Pubkey, true);
    }

    private static byte[] Open(V2Header header, byte[] payload, byte[] aad, V2Unlock unlock)
    {
        return header.Crypto.Mode switch
        {
            "open" => OpenOpen(header, payload),
            "passphrase" => OpenPassphrase(header, payload, aad, unlock),
            "keys" => OpenKeys(header, aad, unlock),
            _ => throw NpvException.InvalidConfig(
                $"container header is invalid: unknown lock mode '{header.Crypto.Mode}'"),
        };
    }

    private static byte[] OpenOpen(V2Header header, byte[] payload)
    {
        if (header.Payload.Alg != "none")
        {
            throw NpvException.Crypto("open container with wrong payload alg");
        }

        return payload;
    }

    private static byte[] OpenPassphrase(
        V2Header header, byte[] payload, byte[] aad, V2Unlock unlock)
    {
        var passphrase = unlock.AsPassphrase();
        if (passphrase is null)
        {
            throw NpvException.Crypto(
                "this container is passphrase-locked; supply a passphrase");
        }

        var crypto = header.Crypto;
        var kdf = crypto.Kdf
            ?? throw NpvException.InvalidConfig(
                "container header is invalid: passphrase lock without kdf");
        var salt = crypto.Salt
            ?? throw NpvException.InvalidConfig(
                "container header is invalid: passphrase lock without salt");
        var nonce = crypto.Nonce
            ?? throw NpvException.InvalidConfig(
                "container header is invalid: passphrase lock without nonce");
        kdf.Validate();
        var key = kdf.Derive(passphrase, Unb64(salt));
        return LibSodium.AeadDecrypt(key, Unb64(nonce), payload, aad);
    }

    private static byte[] OpenKeys(V2Header header, byte[] aad, V2Unlock unlock)
    {
        var secrets = unlock.AsKeys();
        if (secrets is null)
        {
            throw NpvException.Crypto(
                "this container is key-locked; supply a recipient key");
        }

        var recipients = header.Crypto.Recipients
            ?? throw NpvException.InvalidConfig(
                "container header is invalid: key lock without recipients");
        if (recipients.Count == 0 || recipients.Count > V2Const.MaxRecipients)
        {
            throw NpvException.Crypto("recipient slot count out of range");
        }

        var matched = MatchSlot(recipients, secrets);
        return OpenMatchedSlot(matched.Slot, matched.Secret, aad);
    }

    private static (V2RecipientSlot Slot, byte[] Secret) MatchSlot(
        List<V2RecipientSlot> recipients, IReadOnlyList<byte[]> secrets)
    {
        V2RecipientSlot? slot = null;
        byte[]? secret = null;
        foreach (var candidate in recipients)
        {
            var slotPub = Unb64(candidate.RecipientPub);
            foreach (var candidateSecret in secrets)
            {
                var ourPub = LibSodium.ScalarMultBase(candidateSecret);
                if (!slotPub.SequenceEqual(ourPub))
                {
                    continue;
                }

                if (slot is not null)
                {
                    throw NpvException.Crypto("duplicate recipient slot");
                }

                slot = candidate;
                secret = candidateSecret;
            }
        }

        return slot is null || secret is null
            ? throw NpvException.Crypto(
                "this container is locked to other recipient keys")
            : (slot, secret);
    }

    private static byte[] OpenMatchedSlot(V2RecipientSlot slot, byte[] secret, byte[] aad)
    {
        var ephPub = Unb64(slot.EphPub);
        if (ephPub.Length != 32)
        {
            throw NpvException.Crypto("bad ephemeral pubkey");
        }

        var shared = LibSodium.ScalarMult(secret, ephPub);
        var key = HkdfSha256(shared, Unb64(slot.Salt));
        return LibSodium.AeadDecrypt(key, Unb64(slot.Nonce), Unb64(slot.Ct), aad);
    }

    private static (V2Header Header, byte[] Payload) ParseFrame(byte[] bytes)
    {
        if (bytes.Length < V2Const.FrameOverhead || bytes.Length > V2Const.MaxContainerLen)
        {
            throw NpvException.InvalidConfig("container size out of range");
        }

        if (!bytes.AsSpan().StartsWith(V2Const.Magic))
        {
            throw NpvException.InvalidConfig(
                "not an npvt2 container (magic mismatch)");
        }

        var version = BitConverter.ToUInt16(bytes, 6);
        if (!BitConverter.IsLittleEndian)
        {
            version = System.Buffers.Binary.BinaryPrimitives.ReverseEndianness(version);
        }

        if (version != V2Const.FormatVersion)
        {
            throw NpvException.InvalidConfig(
                $"unsupported container version {version}");
        }

        var hdrLen = (int)ReadU32Le(bytes, 8);
        var payLen = (int)ReadU32Le(bytes, 12);
        if (hdrLen == 0 || hdrLen > V2Const.MaxHeaderLen)
        {
            throw NpvException.InvalidConfig("header length out of range");
        }

        if (V2Const.FrameOverhead + hdrLen + payLen != bytes.Length)
        {
            throw NpvException.InvalidConfig("container framing is inconsistent");
        }

        var headerBytes = bytes[V2Const.FrameOverhead..(V2Const.FrameOverhead + hdrLen)];
        var payload = bytes[(V2Const.FrameOverhead + hdrLen)..];
        var header = V2CanonicalJson.Parse(headerBytes);
        if (header.Format != "npvt2")
        {
            throw NpvException.InvalidConfig("container header format mismatch");
        }

        return (header, payload);
    }

    private static byte[] Frame(V2Header header, byte[] payloadBlob)
    {
        var headerBytes = V2CanonicalJson.Write(header);
        if (headerBytes.Length > V2Const.MaxHeaderLen)
        {
            throw NpvException.Crypto("header exceeds maximum size");
        }

        var outBytes = new byte[V2Const.FrameOverhead + headerBytes.Length + payloadBlob.Length];
        Array.Copy(V2Const.Magic, outBytes, V2Const.Magic.Length);
        WriteU16Le(outBytes, 6, V2Const.FormatVersion);
        WriteU32Le(outBytes, 8, headerBytes.Length);
        WriteU32Le(outBytes, 12, payloadBlob.Length);
        Array.Copy(headerBytes, 0, outBytes, V2Const.FrameOverhead, headerBytes.Length);
        Array.Copy(
            payloadBlob, 0, outBytes, V2Const.FrameOverhead + headerBytes.Length,
            payloadBlob.Length);
        return outBytes;
    }
}
