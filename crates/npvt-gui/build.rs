//! Build script: embed the application icon into the Windows executable so
//! the taskbar, Start menu and Explorer all show the NPVTunnel icon.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(err) = res.compile() {
            eprintln!("warning: could not embed icon resource: {err}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
