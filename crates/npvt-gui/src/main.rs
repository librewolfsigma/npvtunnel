//! `npvtunnel-gui` — the NPVTunnel native desktop client.
//!
//! Security rules inherited from `npvt-core`: the engine inbound binds
//! 127.0.0.1 only, expired containers are refused at import, wrong
//! passphrases and tampered containers are indistinguishable, and dialog
//! passphrases are scrubbed from memory when a dialog closes.

#![forbid(unsafe_code)]

mod app;
mod ui;

use app::NpvtApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([880.0, 560.0])
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "NPVTunnel",
        options,
        Box::new(|cc| {
            ui::apply_theme(&cc.egui_ctx);
            Ok(Box::new(NpvtApp::new()))
        }),
    )
}

/// Decode the embedded application icon for the window/taskbar.
fn load_icon() -> egui::IconData {
    const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
    let decoded = image::load_from_memory(ICON_PNG).expect("embedded icon is a valid PNG");
    let rgba = decoded.to_rgba8();
    egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}
