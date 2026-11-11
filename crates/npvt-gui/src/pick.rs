//! Native file dialogs, with cross-platform stubs so the Linux dev/check
//! lane never compiles the dialog stacks. The shipped product is Windows;
//! on other platforms the pickers simply return `None`.

use std::path::PathBuf;

/// Pick an image file (QR import).
pub fn pick_image() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
            .pick_file()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Pick a config file (.npvt / .npvs / links).
pub fn pick_config() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        rfd::FileDialog::new()
            .add_filter("Config files", &["npvt", "npvs", "json", "txt"])
            .pick_file()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Save dialog for exported containers; returns the chosen path.
pub fn save_config(default_name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        rfd::FileDialog::new()
            .set_file_name(default_name)
            .add_filter("NPVT container", &["npvt"])
            .save_file()
    }
    #[cfg(not(windows))]
    {
        let _ = default_name;
        None
    }
}
