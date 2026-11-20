//! Per-user file association for `.npvt` / `.npvs` files.
//!
//! Writes to `HKCU\Software\Classes` only (no elevation required), so the
//! association applies to the current user and works for portable installs.
//! Double-clicking a `.npvt` / `.npvs` file launches `NPVTunnel.exe "%1"`,
//! and the app opens the import security card for that file.

use std::path::PathBuf;

/// ProgId used for the file association (Windows registry only).
#[cfg(windows)]
const PROG_ID: &str = "npvtunnel.config";
/// Friendly name shown in Explorer's "Type" column.
#[cfg(windows)]
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

    // Windows Explorer picks up HKCU\Software\Classes changes on its own;
    // the explicit SHChangeNotify refresh would need an unsafe FFI call and
    // the crate forbids unsafe code, so no shell notification is sent here.

    Ok(())
}

#[cfg(not(windows))]
pub fn associate() -> Result<(), String> {
    Err("file association is Windows-only".to_string())
}

/// Return the path a double-clicked file should be imported from, if the app
/// was launched with one. Called before the UI starts.
pub fn pending_file_from_args() -> Option<PathBuf> {
    pending_file_from(std::env::args())
}

/// Argument-list variant of [`pending_file_from_args`], split out so tests
/// can pass arguments without touching process-global state.
pub fn pending_file_from<I: IntoIterator<Item = String>>(args: I) -> Option<PathBuf> {
    let mut iter = args.into_iter();
    let _program = iter.next()?;
    let path = PathBuf::from(iter.next()?);
    // `Path::extension` yields the suffix without the leading dot, so the
    // comparison normalizes it back to the registry-key form `.npvt`.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if EXTENSIONS.contains(&format!(".{ext}").as_str()) && path.is_file() {
        return Some(path);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a uniquely named file, run `body` with its path, delete it.
    fn with_temp_file(body: impl FnOnce(&std::path::Path)) {
        let path = std::env::temp_dir().join(format!(
            "npvt-assoc-test-{}-{}.npvt",
            std::process::id(),
            npvt_core::profile::unix_now()
        ));
        std::fs::write(&path, b"test").expect("write temp file");
        body(&path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_args_yields_no_pending_file() {
        assert_eq!(pending_file_from(["npvtunnel.exe".to_owned()]), None);
        assert_eq!(pending_file_from(std::iter::empty()), None);
    }

    #[test]
    fn double_clicked_container_is_pending() {
        with_temp_file(|path| {
            let arg = path.to_string_lossy().to_string();
            assert_eq!(
                pending_file_from(["npvtunnel.exe".to_owned(), arg]),
                Some(path.to_path_buf())
            );
        });
    }

    #[test]
    fn non_container_arguments_are_ignored() {
        with_temp_file(|path| {
            let wrong_ext = path.with_extension("txt");
            std::fs::write(&wrong_ext, b"test").expect("write temp file");
            let arg = wrong_ext.to_string_lossy().to_string();
            assert_eq!(pending_file_from(["npvtunnel.exe".to_owned(), arg]), None);
            let _ = std::fs::remove_file(&wrong_ext);
        });
    }

    #[test]
    fn missing_files_are_not_imported() {
        let arg = std::env::temp_dir().join("npvt-assoc-does-not-exist.npvt");
        assert_eq!(
            pending_file_from(["npvtunnel.exe".to_owned(), arg.to_string_lossy().to_string()]),
            None
        );
    }
}
