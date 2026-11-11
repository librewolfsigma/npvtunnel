//! Build script: embed the application icon into the Windows executable so
//! the taskbar, Start menu, Explorer and the window itself show the branded
//! icon. Non-Windows targets (and machines without `rc`) build unchanged.

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let icon = Path::new(&manifest).join("../../assets/app.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    #[cfg(target_os = "windows")]
    if icon.is_file() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().expect("icon path is utf-8"));
        if let Err(err) = res.compile() {
            println!("cargo:warning=icon embedding skipped: {err}");
        }
    } else {
        println!("cargo:warning=assets/app.ico not found; exe icon not embedded");
    }
}
