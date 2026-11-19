// .npvs reading compatibility tests: every accepted shape of the NPVS
// shared-file family, plus the rejection paths.

using System.Text;

namespace NpvTunnel.Core.Tests;

public class NpvsReaderTests
{
    [Fact]
    public void ReadsNpvsContainerSingleObject()
    {
        var body = """{"add":"a.com","port":"443","id":"u1","ps":"shop"}""";
        var bytes = Encoding.UTF8.GetBytes("NPVS" + body);
        var entries = NpvsReader.Read(bytes, "bundle.npvs");
        var entry = Assert.Single(entries);
        Assert.Equal(Protocol.Vmess, entry.Profile.Protocol);
        Assert.Equal("a.com", entry.Profile.Server);
        Assert.Equal(443, entry.Profile.Port);
        Assert.Equal("shop", entry.Profile.Name);
        Assert.Equal("bundle.npvs", entry.Origin);
    }

    [Fact]
    public void ReadsNpvsContainerArrayBundle()
    {
        var body = """
            [{"add":"a.com","port":"443","id":"u1","ps":"one"},
                {"add":"b.com","port":"8443","id":"u2","ps":"two"}]
            """;
        var bytes = Encoding.UTF8.GetBytes("NPVS" + body);
        var entries = NpvsReader.Read(bytes, "bundle.npvs");
        Assert.Equal(2, entries.Count);
        Assert.Equal("one", entries[0].Profile.Name);
        Assert.Equal("two", entries[1].Profile.Name);
        Assert.Equal(1, entries[0].Index);
        Assert.Equal(2, entries[1].Index);
    }

    [Fact]
    public void ReadsNpvsProfilesWrapper()
    {
        var body = """{"profiles":[{"add":"a.com","port":"443","id":"u1","ps":"one"}]}""";
        var bytes = Encoding.UTF8.GetBytes("NPVS" + body);
        var entries = NpvsReader.Read(bytes, "bundle.npvs");
        var entry = Assert.Single(entries);
        Assert.Equal("one", entry.Profile.Name);
    }

    [Fact]
    public void ReadsUriSubscriptionList()
    {
        var text = "# my shop list\n"
            + "vless://11111111-2222-3333-4444-555555555555@a.com:443?security=tls#A\n"
            + "\n"
            + "trojan://pw@b.io:8443#B\n";
        var entries = NpvsReader.Read(Encoding.UTF8.GetBytes(text), "subs.npvs");
        Assert.Equal(2, entries.Count);
        Assert.Equal(Protocol.Vless, entries[0].Profile.Protocol);
        Assert.Equal(Protocol.Trojan, entries[1].Profile.Protocol);
        Assert.Equal("A", entries[0].Profile.Name);
        Assert.Equal("B", entries[1].Profile.Name);
    }

    [Fact]
    public void ReadsBase64WrappedSubscription()
    {
        var list = "vless://11111111-2222-3333-4444-555555555555@a.com:443#A\n"
            + "trojan://pw@b.io:8443#B";
        var wrapped = Convert.ToBase64String(Encoding.UTF8.GetBytes(list));
        var entries = NpvsReader.Read(Encoding.UTF8.GetBytes(wrapped), "subs.npvs");
        Assert.Equal(2, entries.Count);
    }

    [Fact]
    public void ReadsNpvsLinkCarryingUriList()
    {
        var list = "trojan://pw@b.io:8443#B";
        var body = Convert.ToBase64String(Encoding.UTF8.GetBytes(list))
            .TrimEnd('=').Replace('+', '-').Replace('/', '_');
        var entries = NpvsReader.ReadText($"npvs://{body}", "link");
        var entry = Assert.Single(entries);
        Assert.Equal(Protocol.Trojan, entry.Profile.Protocol);
    }

    [Fact]
    public void ReadsNpvsLinkCarryingV2Container()
    {
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Open(),
            Fixtures.SignerSeed());
        var body = Convert.ToBase64String(bytes)
            .TrimEnd('=').Replace('+', '-').Replace('/', '_');
        var entries = NpvsReader.ReadText($"npvs://{body}", "link");
        var entry = Assert.Single(entries);
        Assert.Equal(Fixtures.Sample(), entry.Profile);
    }

    [Fact]
    public void NpvtContainerImportViaBatch()
    {
        var bytes = V2Container.Encode(
            Fixtures.Sample(),
            new V2Policy(),
            V2Lock.Open(),
            Fixtures.SignerSeed());
        var entries = NpvsReader.ImportContainer(bytes, "open.npvt");
        var entry = Assert.Single(entries);
        Assert.Equal(Fixtures.Sample(), entry.Profile);
    }

    [Fact]
    public void BundleExceedingLimitIsRejected()
    {
        var item = """{"add":"a.com","port":"443","id":"u1","ps":"one"}""";
        var body = "[" + string.Join(",", Enumerable.Repeat(item, NpvsReader.MaxBundleEntries + 1)) + "]";
        var bytes = Encoding.UTF8.GetBytes("NPVS" + body);
        Assert.Throws<NpvException>(() => NpvsReader.Read(bytes, "big.npvs"));
    }

    [Fact]
    public void InvalidBundleJsonIsRejected()
    {
        var bytes = Encoding.UTF8.GetBytes("NPVS{not json");
        Assert.Throws<NpvException>(() => NpvsReader.Read(bytes, "bad.npvs"));
    }

    [Fact]
    public void EmptySubscriptionIsRejected()
    {
        Assert.Throws<NpvException>(() =>
            NpvsReader.ReadText("# only a comment\n\n", "subs.npvs"));
    }

    [Fact]
    public void LooksLikeNpvsDetectsAllShapes()
    {
        var container = Encoding.UTF8.GetBytes("NPVS{}");
        Assert.True(NpvsReader.LooksLikeNpvs(container));
        var list = "vless://x@a.com:1#A";
        Assert.True(NpvsReader.LooksLikeNpvs(Encoding.UTF8.GetBytes(list)));
        var wrapped = Convert.ToBase64String(Encoding.UTF8.GetBytes(list));
        Assert.True(NpvsReader.LooksLikeNpvs(Encoding.UTF8.GetBytes(wrapped)));
        Assert.False(NpvsReader.LooksLikeNpvs([1, 2, 3, 4]));
    }

    [Fact]
    public void BatchImportFileDeduplicatesNames()
    {
        var body = """
            [{"add":"a.com","port":"443","id":"u1","ps":"one"},
                {"add":"a.com","port":"443","id":"u1","ps":"one"}]
            """;
        var tmp = Path.Combine(Path.GetTempPath(), $"npvs-dedup-{Guid.NewGuid():N}.npvs");
        File.WriteAllBytes(tmp, Encoding.UTF8.GetBytes("NPVS" + body));
        try
        {
            var data = new VaultData();
            var result = new BatchResult();
            ImportBatch.ImportFile(tmp, data, result);
            Assert.Empty(result.Failed);
            Assert.Equal(2, result.Imported.Count);
            Assert.NotEqual(
                result.Imported[0].Profile.Name, result.Imported[1].Profile.Name);
        }
        finally
        {
            File.Delete(tmp);
        }
    }
}
