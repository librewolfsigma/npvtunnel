// Direct libsodium P/Invoke surface: the exact primitives the container
// needs (XChaCha20-Poly1305 AEAD, X25519 scalarmult, Ed25519 seed keypair
// and detached signatures). Sodium.Core carries the native binaries.


using System.Runtime.InteropServices;

namespace NpvTunnel.Core;

/// <summary>libsodium native entry points (lazy-initialized).</summary>
public static class LibSodium
{
    private static int _initialized;

    /// <summary>Initialize libsodium once per process; safe to call repeatedly.</summary>
    public static void Init()
    {
        if (Interlocked.Exchange(ref _initialized, 1) == 0)
        {
            _ = sodium_init();
        }
    }

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int sodium_init();

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_aead_xchacha20poly1305_ietf_encrypt(
        byte[] c, out ulong clenP, byte[] m, ulong mlen,
        byte[]? ad, ulong adlen, byte[]? nsec, byte[] npub, byte[] k);

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_aead_xchacha20poly1305_ietf_decrypt(
        byte[] m, out ulong mlenP, byte[]? nsec, byte[] c, ulong clen,
        byte[]? ad, ulong adlen, byte[] npub, byte[] k);

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_scalarmult(
        byte[] q, byte[] n, byte[] p);

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_sign_seed_keypair(
        byte[] pk, byte[] sk, byte[] seed);

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_sign_detached(
        byte[] sig, out ulong siglenP, byte[] m, ulong mlen, byte[] sk);

    [DllImport("libsodium", CallingConvention = CallingConvention.Cdecl)]
    private static extern int crypto_sign_verify_detached(
        byte[] sig, byte[] m, ulong mlen, byte[] pk);

    /// <summary>XChaCha20-Poly1305 AEAD encrypt; returns ciphertext||tag.</summary>
    public static byte[] AeadEncrypt(byte[] key, byte[] nonce, byte[] plaintext, byte[] aad)
    {
        Init();
        var c = new byte[plaintext.Length + 16];
        var rc = crypto_aead_xchacha20poly1305_ietf_encrypt(
            c, out var clen, plaintext, (ulong)plaintext.Length,
            aad, (ulong)aad.Length, null, nonce, key);
        if (rc != 0 || clen != (ulong)c.Length)
        {
            throw NpvException.Crypto("encryption failed");
        }

        return c;
    }

    /// <summary>XChaCha20-Poly1305 AEAD decrypt; auth failure is CryptoAuth.</summary>
    public static byte[] AeadDecrypt(byte[] key, byte[] nonce, byte[] ciphertext, byte[] aad)
    {
        Init();
        var m = new byte[Math.Max(ciphertext.Length, 1)];
        var rc = crypto_aead_xchacha20poly1305_ietf_decrypt(
            m, out var mlen, null, ciphertext, (ulong)ciphertext.Length,
            aad, (ulong)aad.Length, nonce, key);
        if (rc != 0)
        {
            throw NpvException.CryptoAuth();
        }

        Array.Resize(ref m, (int)mlen);
        return m;
    }

    /// <summary>X25519 scalar multiplication (ECDH shared secret).</summary>
    public static byte[] ScalarMult(byte[] secret, byte[] point)
    {
        Init();
        var q = new byte[32];
        if (crypto_scalarmult(q, secret, point) != 0)
        {
            throw NpvException.Crypto("bad ephemeral pubkey");
        }

        return q;
    }

    /// <summary>Ed25519 keypair from a 32-byte seed: returns (public, secret).</summary>
    public static (byte[] Public, byte[] Secret) SignKeypairFromSeed(byte[] seed)
    {
        Init();
        var pk = new byte[32];
        var sk = new byte[64];
        if (crypto_sign_seed_keypair(pk, sk, seed) != 0)
        {
            throw NpvException.Crypto("bad signing seed");
        }

        return (pk, sk);
    }

    /// <summary>Ed25519 detached signature (64 bytes).</summary>
    public static byte[] SignDetached(byte[] message, byte[] secretKey)
    {
        Init();
        var sig = new byte[64];
        if (crypto_sign_detached(
                sig, out var siglen, message, (ulong)message.Length, secretKey) != 0
            || siglen != 64UL)
        {
            throw NpvException.Crypto("signing failed");
        }

        return sig;
    }

    /// <summary>Ed25519 detached verify; returns false on any failure.</summary>
    public static bool VerifyDetached(byte[] sig, byte[] message, byte[] publicKey)
    {
        Init();
        return crypto_sign_verify_detached(
            sig, message, (ulong)message.Length, publicKey) == 0;
    }

    /// <summary>X25519 public key derived from a secret (for slot matching).</summary>
    public static byte[] ScalarMultBase(byte[] secret)
    {
        // libsodium requires a non-zero base point; 9 is the standard base.
        var basePoint = new byte[32];
        basePoint[0] = 9;
        return ScalarMult(secret, basePoint);
    }
}
