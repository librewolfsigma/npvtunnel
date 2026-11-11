//! NPVTunnel desktop GUI.
//!
//! `windows_subsystem = "windows"` is the entire answer to "double-clicking
//! opens a terminal": it marks the executable as a GUI-subsystem binary, so
//! Windows never allocates a console for it - in debug and release builds
//! alike. All child processes we spawn (sing-box, tasklist, taskkill, reg)
//! are additionally created with `CREATE_NO_WINDOW` (see `npvt-core::engine`).

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod app;
mod data;
mod dialogs;
mod engine_ctrl;
mod export;
mod icons;
mod net;
mod pages;
mod pick;
mod theme;
mod widgets;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NPVTunnel")
            .with_inner_size([470.0, 880.0])
            .with_min_inner_size([400.0, 680.0])
            .with_icon(icons::app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "NPVTunnel",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
