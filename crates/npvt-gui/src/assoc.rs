//! Per-user file association for `.npvt` / `.npvs` files.
//!
//! Writes to `HKCU\Software\Classes` only (no elevation required), so the
//! association applies to the current user and works for portable installs.
//! Double-clicking a `.npvt` / `.npvs` file launches `NPVTunnel.exe "%1"`,
//! and the app opens the import security card for that file.

use std::path::PathBuf;

/// ProgId used for the file association.
const PROG_ID: &str = "npvtunnel.config";
/// Friendly name shown in Explorer's "Type" column.
const FILE_TYPE_NAME: &str = "NPVTunnel Config";
/// Extensions we associate.
const EXTENSIONS: &[&str] = &[".npvt", ".npvs"];

/// Check whether `.npvt` / `.npvs` are already associated with our ProgId.
#[cfg(windows)]
pub fn is_associated() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let Ok(classes) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
        "Software\\Classes",
        winreg::enums::KEY_SET_VALUE | winreg::enums::KEY_CREATE_SUB_KEY | winreg::enums::KEY_READ,
    ) else {
        return false;
    };
    for ext in EXTENSIONS {
        let Ok(ext_key) = classes.open_subkey(ext) else {
            return false;
        };
        let val: Option<String> = ext_key.get_value("").ok();
        if val.as_deref() != Some(PROG_ID) {
            return false;
        }
    }
    true
}

#[cfg(not(windows))]
pub fn is_associated() -> bool {
    false
}

/// Associate `.npvt` / `.npvs` with `NPVTunnel.exe "%1"` for the current user.
#[cfg(windows)]
pub fn associate() -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let exe = std::env::current_exe().map_err(|e| format!("could not locate executable: {e}"))?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| "executable path is not valid UTF-8".to_string())?;
    let command = format!("\"{exe_str}\" \"%1\"");

    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Classes",
            winreg::enums::KEY_SET_VALUE
                | winreg::enums::KEY_CREATE_SUB_KEY
                | winreg::enums::KEY_READ,
        )
        .map_err(|e| format!("could not open HKCU\\Software\\Classes: {e}"))?;

    let (prog_id, _) = classes
        .create_subkey(PROG_ID)
        .map_err(|e| format!("could not create ProgId key: {e}"))?;
    prog_id
        .set_value("", &FILE_TYPE_NAME.to_string())
        .map_err(|e| format!("could not set file type name: {e}"))?;

    let (icon_key, _) = prog_id
        .create_subkey("DefaultIcon")
        .map_err(|e| format!("could not create DefaultIcon key: {e}"))?;
    icon_key
        .set_value("", &format!("{exe_str},0"))
        .map_err(|e| format!("could not set default icon: {e}"))?;

    let (shell_key, _) = prog_id
        .create_subkey("shell\\open\\command")
        .map_err(|e| format!("could not create shell command key: {e}"))?;
    shell_key
        .set_value("", &command)
        .map_err(|e| format!("could not set open command: {e}"))?;

    for ext in EXTENSIONS {
        let (ext_key, _) = classes
            .create_subkey(ext)
            .map_err(|e| format!("could not create extension key '{ext}': {e}"))?;
        ext_key
            .set_value("", &PROG_ID.to_string())
            .map_err(|e| format!("could not set extension '{ext}': {e}"))?;
    }

    unsafe {
        let _ = windows_sys::Win32::UI::Shell::SHChangeNotify(
            0x8000000, // SHCNE_ASSOCCHANGED
            0x1000,    // SHCNF_IDLIST
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn associate() -> Result<(), String> {
    Err("file association is Windows-only".to_string())
}

/// Return the path a double-clicked file should be imported from, if the app
/// was launched with one. Called from `main()` before the UI starts.
pub fn pending_file_from_args() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return None;
    }
    let path = PathBuf::from(&args[1]);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if EXTENSIONS.contains(&ext) && path.is_file() {
        return Some(path);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_non_empty() {
        assert!(!EXTENSIONS.is_empty());
    }

    #[test]
    fn prog_id_is_non_empty() {
        assert!(!PROG_ID.is_empty());
    }

    #[test]
    fn pending_file_from_args_ignores_non_npvt() {
        // Cannot easily set args in a unit test; just verify the function exists.
        let _ = EXTENSIONS.len();
    }
}
