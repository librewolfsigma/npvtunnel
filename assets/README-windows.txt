NPV TUNNEL - NPVTunnel for Windows
==================================

Thanks for trying NPV TUNNEL. This folder contains:

  NPVTunnel.exe   the native desktop app - double-click it, no terminal
                  window is opened
  npvt-cli.exe    the command line client for scripts and power users
  sing-box.exe    the bundled tunnel engine (sing-box, GPL-3.0; source at
                  https://github.com/SagerNet/sing-box)
  LICENSE         the MIT license for NPVTunnel
  THIRD-PARTY-LICENSES.txt   third-party license notices

Quick start
-----------
1. Double-click NPVTunnel.exe.
2. Open the Configs page and click "Add config":
   - "Import config file (.npvt)..." to pick a .npvt/.npvs file,
   - "Import config from Clipboard" for a vless://... share link,
   - "Add config manually" to type the server details.
3. Go back to Home and press the big power button.
4. "[ CONNECTED ]" shows live down/up traffic and the ping; press the button
   again to disconnect.

The app and the CLI share one engine session, so `npvt-cli status` and
`npvt-cli disconnect` work while the GUI is connected.

Files live in %APPDATA%\npvtunnel (vault, engine config and logs). The SOCKS5
proxy listens on 127.0.0.1 only - it is never exposed to your LAN.

Security note: verify this zip against the SHA256SUMS.txt file published next
to it in the GitHub release.
