// Vault, v1 container, audit, shredder and engine-config tests, ported
// from the Rust suites plus the C#-specific additions.

using System.Text;
using System.Text.Json.Nodes;

namespace NpvTunnel.Core.Tests;

public class VaultTests
{
    [Fact]
    public void SaveLoadRoundtripAndUpsert()
    {
        var vault = TempVault();
        var data = new VaultData();
        VaultOps.UpsertProfile(data, ProfileOf("shop"));
        Assert.False(VaultOps.UpsertProfile(data, ProfileOf("shop")));
        vault.Save(data);
        var loaded = vault.Load();
        Assert.Equal(2, loaded.Profiles.Count);
        Assert.True(File.Exists(vault.Path));
        File.Delete(vault.Path);
    }

    [Fact]
    public void FindByIdAndUniqueName()
    {
        var data = new VaultData();
        var p = ProfileOf("alpha");
        data.Profiles.Add(p);
        data.Profiles.Add(ProfileOf("beta"));
        Assert.Equal("alpha", VaultOps.FindProfile(data, p.Id).Name);
        Assert.Equal("alpha", VaultOps.FindProfile(data, "ALPHA").Name);
        Assert.Throws<NpvException>(() => VaultOps.FindProfile(data, "gamma"));
    }

    [Fact]
    public void AmbiguousNameIsRejected()
    {
        var data = new VaultData();
        data.Profiles.Add(ProfileOf("dup"));
        data.Profiles.Add(ProfileOf("dup"));
        Assert.Throws<NpvException>(() => VaultOps.FindProfile(data, "dup"));
    }

    [Fact]
    public void RemoveByUniqueName()
    {
        var data = new VaultData();
        data.Profiles.Add(ProfileOf("solo"));
        var removed = VaultOps.RemoveProfile(data, "solo");
        Assert.Equal("solo", removed.Name);
        Assert.Empty(data.Profiles);
    }

    [Fact]
    public void SaveTightensPermissionsOnUnix()
    {
        if (!OperatingSystem.IsLinux() && !OperatingSystem.IsMacOS())
        {
            return;
        }

        var vault = TempVault();
        var data = new VaultData();
        VaultOps.UpsertProfile(data, ProfileOf("shop"));
        vault.Save(data);
        File.SetUnixFileMode(
            vault.Path,
            UnixFileMode.UserRead | UnixFileMode.UserWrite
            | UnixFileMode.GroupRead | UnixFileMode.OtherRead);
        vault.Save(data);
        var mode = File.GetUnixFileMode(vault.Path);
        Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, mode);
        File.Delete(vault.Path);
    }

    private static Vault TempVault()
    {
        var path = Path.Combine(
            Path.GetTempPath(),
            $"npvt-test-{Guid.NewGuid():N}-vault.json");
        return new Vault(path);
    }

    internal static Profile ProfileOf(string name) => new()
    {
        Id = ProfileValidation.NewProfileId(),
        Name = name,
        Protocol = Protocol.Vless,
        Server = "example.com",
        Port = 443,
        Credential = new UuidCredential("11111111-2222-3333-4444-555555555555"),
        Transport = Transport.Tls,
        Sni = null,
        Path = null,
        AllowInsecure = false,
        Udp = true,
        Group = null,
        CreatedAt = 1_700_000_000,
    };
}

public class V1ContainerTests
{
    [Fact]
    public void DetectsPlainJsonV1()
    {
        var data = Encoding.UTF8.GetBytes(
            """NPVT{"add":"a.com","port":"443","id":"u1","ps":"shop"}""");
        var probe = V1Container.Probe(data);
        Assert.True(probe.IsV1);
        Assert.True(probe.LooksPlaintextJson);
        Assert.Equal("NPVT", probe.Magic);
        var profile = V1Container.ImportBestEffort(data);
        Assert.Equal(Protocol.Vmess, profile.Protocol);
        Assert.Equal("a.com", profile.Server);
        Assert.Equal(443, profile.Port);
        Assert.Equal("shop", profile.Name);
    }

    [Fact]
    public void RejectsLockedV1Honestly()
    {
        var data = Encoding.UTF8.GetBytes("NPVT\x01\x02\x03\x04not json at all");
        var probe = V1Container.Probe(data);
        Assert.True(probe.IsV1);
        Assert.False(probe.LooksPlaintextJson);
        var ex = Assert.Throws<NpvException>(() => V1Container.ImportBestEffort(data));
        Assert.Equal(ErrorClass.Config, ex.Class);
        Assert.Contains("v1", ex.Message);
    }

    [Fact]
    public void RejectsNonV1()
    {
        Assert.Throws<NpvException>(() => V1Container.ImportBestEffort("NOPE"u8.ToArray()));
    }

    [Fact]
    public void ProbeNpvsMagic()
    {
        var probe = V1Container.Probe("NPVS{}"u8.ToArray());
        Assert.True(probe.IsV1);
        Assert.Equal("NPVS", probe.Magic);
    }

    [Fact]
    public void TransportFieldsFlowThrough()
    {
        var data = Encoding.UTF8.GetBytes(
            """NPVT{"add":"a.com","port":"443","id":"u1","ps":"shop","net":"ws","path":"/ws","tls":"tls","sni":"s.com"}""");
        var profile = V1Container.ImportBestEffort(data);
        Assert.Equal(Transport.Tls, profile.Transport);
        Assert.Equal("/ws", profile.Path);
    }
}

public class ConfigAuditTests
{
    [Fact]
    public void FlagsAllowInsecureAsHigh()
    {
        var profile = VaultTests.ProfileOf("risky");
        profile.AllowInsecure = true;
        var findings = ConfigAudit.Audit([profile]);
        var finding = Assert.Single(findings, f => f.Check == "allow-insecure");
        Assert.Equal(AuditSeverity.High, finding.Severity);
    }

    [Fact]
    public void FlagsMissingSniOnTls()
    {
        var profile = VaultTests.ProfileOf("nosni");
        profile.Sni = null;
        var findings = ConfigAudit.Audit([profile]);
        Assert.Contains(findings, f => f.Check == "missing-sni");
    }

    [Fact]
    public void CleanProfileHasNoFindings()
    {
        var ok = VaultTests.ProfileOf("ok");
        ok.Sni = "example.com";
        Assert.Empty(ConfigAudit.Audit([ok]));
    }
}

public class SecretShredderTests
{
    [Fact]
    public void ShredRemovesFile()
    {
        var path = Path.Combine(Path.GetTempPath(), $"shred-{Guid.NewGuid():N}.bin");
        File.WriteAllBytes(path, Enumerable.Repeat((byte)1, 4096).ToArray());
        SecretShredder.ShredFile(path);
        Assert.False(File.Exists(path));
    }

    [Fact]
    public void ShredMissingFileIsSuccess()
    {
        var path = Path.Combine(Path.GetTempPath(), $"shred-{Guid.NewGuid():N}.bin");
        SecretShredder.ShredFile(path);
        Assert.False(File.Exists(path));
    }

    [Fact]
    public void ZeroizeClearsBuffer()
    {
        var buffer = Enumerable.Repeat((byte)7, 32).ToArray();
        SecretShredder.Zeroize(buffer);
        Assert.All(buffer, b => Assert.Equal(0, b));
    }
}

public class EngineConfigTests
{
    [Fact]
    public void VlessOutboundShape()
    {
        var ob = EngineConfig.Outbound(VaultTests.ProfileOf("shop"));
        Assert.Equal("vless", (string?)ob["type"]);
        Assert.Equal("example.com", (string?)ob["server"]);
        Assert.Equal(443, (int?)ob["server_port"]);
        Assert.Equal("11111111-2222-3333-4444-555555555555", (string?)ob["uuid"]);
        Assert.NotNull(ob["tls"]);
    }

    [Fact]
    public void UnsupportedProtocolFailsWithRoadmapMessage()
    {
        var profile = VaultTests.ProfileOf("wg");
        profile.Protocol = Protocol.WireGuard;
        var ex = Assert.Throws<NpvException>(() => EngineConfig.Outbound(profile));
        Assert.Contains("P1.5", ex.Message);
    }

    [Fact]
    public void FullConfigBindsLoopbackOnly()
    {
        var shop = VaultTests.ProfileOf("shop");
        var config = EngineConfig.Config(
            [shop],
            shop.Id,
            EngineConfig.DefaultSocksPort,
            "1.1.1.1");
        var inbound = (JsonObject?)((JsonArray?)config["inbounds"])?[0]?.AsObject();
        Assert.NotNull(inbound);
        Assert.Equal("127.0.0.1", (string?)inbound?["listen"]);
        Assert.Equal(shop.Id, (string?)((JsonObject?)config["route"])?["final"]);
    }
}
