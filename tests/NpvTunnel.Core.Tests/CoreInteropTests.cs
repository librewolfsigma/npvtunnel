// Cross-language interop: containers encoded by the Rust implementation
// (crates/npvt-core/examples/genvec.rs) decode byte-exactly here, and the
// C# encoder produces containers the Rust tests can decode in return.

using System.Security.Cryptography;
using System.Text;

namespace NpvTunnel.Core.Tests;

public class CoreInteropTests
{
    [Fact]
    public void DecodesRustOpenContainer()
    {
        var bytes = Fixtures.Vector("rust-open.npvt2");
        var decoded = V2Container.Decode(bytes, V2Unlock.Any);
        Assert.Equal(Fixtures.Sample(), decoded.Profile);
        Assert.True(decoded.Verification.Signed);
        Assert.True(decoded.Verification.Valid);
        Assert.Equal(B64(V2Keys.Ed25519Public(Fixtures.SignerSeed())), decoded.Verification.Creator);
        Assert.Equal("open", V2Container.PeekLock(bytes));
    }

    [Fact]
    public void DecodesRustPassphraseContainer()
    {
        var bytes = Fixtures.Vector("rust-passphrase.npvt2");
        var decoded = V2Container.Decode(
            bytes, V2Unlock.Passphrase("correct horse"));
        Assert.Equal(Fixtures.Sample(), decoded.Profile);
        Assert.True(decoded.Verification.Valid);
        Assert.Equal("passphrase", V2Container.PeekLock(bytes));
    }

    [Fact]
    public void WrongPassphraseOnRustContainerFailsClosed()
    {
        var bytes = Fixtures.Vector("rust-passphrase.npvt2");
        var ex = Assert.Throws<NpvException>(() => V2Container.Decode(
            bytes, V2Unlock.Passphrase("wrong")));
        Assert.Equal(ErrorClass.Crypto, ex.Class);
    }

    [Fact]
    public void DecodesRustKeysContainer()
    {
        var bytes = Fixtures.Vector("rust-keys.npvt2");
        var decoded = V2Container.Decode(bytes, V2Unlock.Keys([Fixtures.RecipientSecret()]));
        Assert.Equal(Fixtures.Sample(), decoded.Profile);
        Assert.Equal("keys", V2Container.PeekLock(bytes));
        Assert.True(decoded.Policy.MeteredOnly);
        Assert.Equal(1900000000UL, decoded.Policy.ExpiresAt);
        Assert.Equal("work", decoded.Policy.GroupHint);
    }

    [Fact]
    public void RustContainerRejectsForeignRecipientKey()
    {
        var bytes = Fixtures.Vector("rust-keys.npvt2");
        var foreign = Enumerable.Repeat((byte)11, 32).ToArray();
        var ex = Assert.Throws<NpvException>(() =>
            V2Container.Decode(bytes, V2Unlock.Keys([foreign])));
        Assert.Contains("recipient keys", ex.Message);
    }

    [Fact]
    public void CsharpPassphraseContainerDecodesBack()
    {
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Passphrase("correct horse"),
            Fixtures.SignerSeed());
        var decoded = V2Container.Decode(bytes, V2Unlock.Passphrase("correct horse"));
        Assert.Equal(Fixtures.Sample(), decoded.Profile);
        Assert.True(decoded.Verification.Signed);
    }

    [Fact]
    public void CsharpKeysContainerDecodesBack()
    {
        // V2Lock.Keys takes recipient PUBLIC keys; unlock uses the secret.
        var publicKey = V2Keys.X25519Public(Fixtures.RecipientSecret());
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Keys([publicKey]),
            Fixtures.SignerSeed());
        var decoded = V2Container.Decode(bytes, V2Unlock.Keys([Fixtures.RecipientSecret()]));
        Assert.Equal(Fixtures.Sample(), decoded.Profile);
        Assert.True(decoded.Verification.Signed);
    }

    [Fact]
    public void TamperedPayloadFailsClosed()
    {
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Passphrase("pw"),
            Fixtures.SignerSeed());
        var last = bytes.Length - 1;
        bytes[last] ^= 0x01;
        Assert.Throws<NpvException>(() =>
            V2Container.Decode(bytes, V2Unlock.Passphrase("pw")));
    }

    [Fact]
    public void MissingPassphraseUnlockIsRejectedWithGuidance()
    {
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Passphrase("pw"),
            null);
        var ex = Assert.Throws<NpvException>(() => V2Container.Decode(bytes, V2Unlock.Any));
        Assert.Contains("passphrase", ex.Message);
    }

    private static string B64(byte[] data) =>
        Convert.ToBase64String(data).TrimEnd('=').Replace('+', '-').Replace('/', '_');
}
