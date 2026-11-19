// Share-link and container-link parsing, ported from the Rust uri.rs tests.

namespace NpvTunnel.Core.Tests;

public class UriImportTests
{
    [Fact]
    public void ParsesVlessWithTlsAndWs()
    {
        var p = UriImport.ParseUri(
            "vless://11111111-2222-3333-4444-555555555555@example.com:443"
            + "?security=tls&type=ws&path=%2Fws&sni=cdn.example.com&udp=1#My%20Shop");
        Assert.Equal(Protocol.Vless, p.Protocol);
        Assert.Equal("example.com", p.Server);
        Assert.Equal(443, p.Port);
        Assert.Equal(Transport.Tls, p.Transport);
        Assert.Equal("/ws", p.Path);
        Assert.Equal("cdn.example.com", p.Sni);
        Assert.Equal("My Shop", p.Name);
        p.Validate();
    }

    [Fact]
    public void ParsesTrojanWithInsecureFlag()
    {
        var p = UriImport.ParseUri("trojan://passw0rd@server.io:8443?allowInsecure=1#t");
        Assert.Equal(Protocol.Trojan, p.Protocol);
        Assert.True(p.AllowInsecure);
        Assert.Equal(8443, p.Port);
    }

    [Fact]
    public void ParsesVmessBase64Json()
    {
        var json = """
            {"v":"2","ps":"shop","add":"vm.example.com","port":"8443",
                "id":"11111111-2222-3333-4444-555555555555","aid":"0",
                "net":"ws","path":"/v","tls":"tls","sni":"s.example.com"}
            """;
        var link = "vmess://" + StdB64(json);
        var p = UriImport.ParseUri(link);
        Assert.Equal(Protocol.Vmess, p.Protocol);
        Assert.Equal("vm.example.com", p.Server);
        Assert.Equal(8443, p.Port);
        Assert.Equal(Transport.Tls, p.Transport);
        Assert.Equal("s.example.com", p.Sni);
    }

    [Fact]
    public void ParsesSsBase64Userinfo()
    {
        var userinfo = StdB64("aes-256-gcm:sesame");
        var p = UriImport.ParseUri($"ss://{userinfo}@1.2.3.4:8388#enc");
        Assert.Equal(Protocol.Shadowsocks, p.Protocol);
        var ss = Assert.IsType<SsMethodCredential>(p.Credential);
        Assert.Equal("aes-256-gcm", ss.Method);
        Assert.Equal("sesame", ss.Password);
    }

    [Fact]
    public void ParsesSsWholeBodyBase64()
    {
        var body = StdB64("aes-128-gcm:pw@[2001:db8::1]:8388");
        var p = UriImport.ParseUri($"ss://{body}#v6");
        Assert.Equal("2001:db8::1", p.Server);
        Assert.Equal(8388, p.Port);
    }

    [Fact]
    public void ParsesHysteria2()
    {
        var p = UriImport.ParseUri(
            "hysteria2://auth-token@h.example.com:443?sni=h.example.com&insecure=1#hy2");
        Assert.Equal(Protocol.Hysteria2, p.Protocol);
        Assert.True(p.AllowInsecure);
        Assert.Equal(Transport.Tls, p.Transport);
        Assert.Equal("auth-token", Assert.IsType<PasswordCredential>(p.Credential).Value);
    }

    [Fact]
    public void ParsesSocksAndHttps()
    {
        var p = UriImport.ParseUri("socks5://user:secret@gw.local:1080#lan");
        Assert.Equal(Protocol.Socks, p.Protocol);
        Assert.Equal(Transport.Plain, p.Transport);
        Assert.Equal("user:secret", Assert.IsType<TokenCredential>(p.Credential).Value);
        var p2 = UriImport.ParseUri("https://u:p@proxy.example.net#hp");
        Assert.Equal(Protocol.Http, p2.Protocol);
        Assert.Equal(Transport.Tls, p2.Transport);
    }

    [Fact]
    public void RejectsUnknownScheme()
    {
        Assert.Throws<NpvException>(() => UriImport.ParseUri("gopher://x"));
        Assert.Throws<NpvException>(() => UriImport.ParseUri("javascript:alert(1)"));
    }

    [Fact]
    public void NpvtLinkYieldsContainer()
    {
        var payload = "NPVT2\0xx"u8.ToArray();
        var link = "npvt://" + UrlB64(payload);
        var source = UriImport.ParseImport(link);
        Assert.Equal(ImportSourceKind.Container, source.Kind);
        Assert.Equal(payload, source.Container);
    }

    [Fact]
    public void NpvsLinkYieldsContainer()
    {
        var payload = "NPVT2\0yy"u8.ToArray();
        var link = "npvs://" + UrlB64(payload);
        var source = UriImport.ParseImport(link);
        Assert.Equal(ImportSourceKind.Container, source.Kind);
        Assert.Equal(payload, source.Container);
    }

    [Fact]
    public void RejectsOversizedInput()
    {
        var big = "vless://a@b:1#" + new string('x', Limits.MaxUriLen + 1);
        Assert.Throws<NpvException>(() => UriImport.ParseImport(big));
    }

    [Fact]
    public void RejectsMissingCredentialForVless()
    {
        // No userinfo: the vless link has no UUID at all.
        Assert.Throws<NpvException>(() =>
            UriImport.ParseUri("vless://example.com:443#no-cred"));
    }

    [Fact]
    public void SixtyFourBitPortsAndZerosRejected()
    {
        Assert.Throws<NpvException>(() =>
            UriImport.ParseUri("vless://11111111-2222-3333-4444-555555555555@a.com:0#z"));
        Assert.Throws<NpvException>(() =>
            UriImport.ParseUri("vless://11111111-2222-3333-4444-555555555555@a.com:x#z"));
    }

    private static string StdB64(string text) => Convert.ToBase64String(
        System.Text.Encoding.UTF8.GetBytes(text));

    private static string UrlB64(byte[] bytes) => Convert.ToBase64String(bytes)
        .TrimEnd('=').Replace('+', '-').Replace('/', '_');
}
