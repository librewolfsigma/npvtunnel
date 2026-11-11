//! Embeds the Windows resource icon (Explorer + taskbar) into NPVTunnel.exe.
//! The in-app icon is drawn at runtime by `icons::app_icon`.

fn main() {
    #[cfg(windows)]
    {
        let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/app.ico");
        if std::path::Path::new(icon).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon);
            res.compile().expect("windows resource compile");
        }
    }
}
