//! NPVTunnel for Windows: the native desktop GUI client.
//!
//! Window subsystem: `windows` - double-clicking the exe never opens a
//! terminal. All diagnostics go to the in-app LOGS page and to a panic log
//! file under the npvtunnel data directory.

#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]
// UI code passes unsuffixed float constants (1.0, 20.0, ...) into f32
// parameters all over the egui/epaint API surface. Rust 1.100 warns about
// this inference fallback (future-incompat); the constants are intentional
// f32 values, so allow it once here instead of suffixing hundreds of sites.
#![allow(float_literal_f32_fallback)]

mod app;
mod dialogs;
mod engine_bridge;
mod icons;
mod net;
mod pages;
mod settings;
mod theme;
mod vault_actions;
mod widgets;

use std::path::PathBuf;

fn panic_log_path() -> Option<PathBuf> {
    npvt_core::engine::work_dir()
        .ok()
        .map(|dir| dir.join("npvtunnel-gui-panic.log"))
}

fn install_panic_hook() {
    let path = panic_log_path();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("[unix {}] panic: {info}\n", npvt_core::profile::unix_now());
        if let Some(path) = &path {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
        }
    }));
}

fn window_icon() -> Option<eframe::egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?;
    let rgba = img.to_rgba8();
    Some(eframe::egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

fn main() {
    install_panic_hook();

    let icon = window_icon();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("io.github.librewolfsigma.npvtunnel")
        .with_title("NPVTunnel")
        .with_inner_size([430.0, 780.0])
        .with_min_inner_size([380.0, 660.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "NPVTunnel",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    ) {
        if let Some(path) = panic_log_path() {
            let _ = std::fs::write(
                path.with_extension("run.log"),
                format!("GUI failed to start: {err}\n"),
            );
        }
    }
}
