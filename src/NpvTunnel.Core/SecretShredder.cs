// Secret destruction: memory zeroization and best-effort file shredding.
// Shredding cannot defeat journalling filesystems or SSD wear leveling, so
// the shredder overwrites, truncates, and deletes with an honest contract:
// it makes casual recovery impractical, not forensic recovery impossible.

namespace NpvTunnel.Core;

/// <summary>Secret destruction helpers.</summary>
public static class SecretShredder
{
    /// <summary>Overwrite every byte of a buffer with zeros.</summary>
    public static void Zeroize(byte[] buffer) =>
        System.Security.Cryptography.CryptographicOperations.ZeroMemory(buffer);

    /// <summary>Overwrite every character of a string buffer (utf-16) with zeros.</summary>
    public static void Zeroize(char[] buffer) => Array.Clear(buffer);

    /// <summary>
    /// Shred a file: overwrite passes, truncate, delete. Honest contract:
    /// casual recovery becomes impractical, not forensic recovery impossible.
    /// </summary>
    public static void ShredFile(string path)
    {
        var info = new FileInfo(path);
        if (!info.Exists)
        {
            return;
        }

        var length = info.Length;
        ShredContent(path, length);
        Truncate(path);
        File.Delete(path);
    }

    private static void ShredContent(string path, long length)
    {
        using var stream = new FileStream(
            path, FileMode.Open, FileAccess.Write, FileShare.None);
        stream.Seek(0, SeekOrigin.Begin);
        WritePattern(stream, length, 0x00);
        stream.Seek(0, SeekOrigin.Begin);
        WritePattern(stream, length, 0xFF);
        stream.Seek(0, SeekOrigin.Begin);
        WriteRandom(stream, length);
        stream.Flush(true);
    }

    private static void WritePattern(FileStream stream, long length, byte value)
    {
        var buffer = new byte[8192];
        Array.Fill(buffer, value);
        WriteChunks(stream, length, buffer);
    }

    private static void WriteRandom(FileStream stream, long length)
    {
        var buffer = new byte[8192];
        var remaining = length;
        while (remaining > 0)
        {
            var take = (int)Math.Min(buffer.Length, remaining);
            System.Security.Cryptography.RandomNumberGenerator.Fill(
                buffer.AsSpan(0, take));
            stream.Write(buffer, 0, take);
            remaining -= take;
        }
    }

    private static void WriteChunks(FileStream stream, long length, byte[] buffer)
    {
        var remaining = length;
        while (remaining > 0)
        {
            var take = (int)Math.Min(buffer.Length, remaining);
            stream.Write(buffer, 0, take);
            remaining -= take;
        }
    }

    private static void Truncate(string path)
    {
        using var stream = new FileStream(
            path, FileMode.Open, FileAccess.Write, FileShare.None);
        stream.SetLength(0);
        stream.Flush(true);
    }
}
