use super::native::wide;
use crate::layout::Selection;
use std::ptr::{null, null_mut};
use std::{fs, path::PathBuf};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

const STARTUP_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const STARTUP_NAME: &str = "MagicLanguageSwitcher";

fn preferences_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        PathBuf::from(root)
            .join("MagicLanguageSwitcher")
            .join("settings.conf")
    })
}

pub fn load_selection() -> Selection {
    preferences_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map_or(Selection::default(), |text| parse_selection(&text))
}

fn parse_selection(text: &str) -> Selection {
    let mut selection = Selection::default();
    for line in text.lines() {
        if let Some((name, value)) = line.split_once('=') {
            let id = u32::from_str_radix(value.trim(), 16).ok();
            match name.trim() {
                "source" => selection.from = id,
                "destination" => selection.to = id,
                _ => {}
            }
        }
    }
    selection
}

pub fn save_selection(selection: Selection) -> Result<(), String> {
    #[cfg(feature = "desktop-tests")]
    if std::env::var("MAGIC_LANGUAGE_SWITCHER_SMOKE").as_deref() == Ok("1") {
        return Ok(());
    }
    let path = preferences_path().ok_or("Windows did not provide a local settings folder.")?;
    let directory = path.parent().ok_or("Invalid settings path.")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let value = |id: Option<u32>| id.map_or_else(|| "auto".into(), |id| format!("{id:08X}"));
    let text = format!(
        "source={}\ndestination={}\n",
        value(selection.from),
        value(selection.to)
    );
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, text)
        .and_then(|()| fs::rename(temporary, path))
        .map_err(|error| format!("Could not save layout preferences: {error}"))
}

fn command() -> Result<String, String> {
    std::env::current_exe()
        .map(|path| format!("\"{}\" --background", path.display()))
        .map_err(|e| e.to_string())
}

pub fn startup_enabled() -> bool {
    // SAFETY: all registry handles are closed on the same path; buffers include NUL terminators.
    unsafe {
        let mut key = null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(STARTUP_KEY).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let mut bytes = vec![0u16; 32768];
        let mut length = (bytes.len() * 2) as u32;
        let mut kind = 0;
        let result = RegQueryValueExW(
            key,
            wide(STARTUP_NAME).as_ptr(),
            null(),
            &mut kind,
            bytes.as_mut_ptr() as *mut u8,
            &mut length,
        );
        RegCloseKey(key);
        if result != ERROR_SUCCESS || kind != REG_SZ {
            return false;
        }
        let count = (length as usize / 2).min(bytes.len());
        let end = bytes[..count]
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(count);
        command().is_ok_and(|expected| String::from_utf16_lossy(&bytes[..end]) == expected)
    }
}

pub fn set_startup(enabled: bool) -> Result<(), String> {
    // SAFETY: the writable key is scoped to this user and this app's one value.
    unsafe {
        let mut key = null_mut();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(STARTUP_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        );
        if result != ERROR_SUCCESS {
            return Err("Could not update Windows startup settings.".into());
        }
        let result = if enabled {
            let value = match command() {
                Ok(value) => wide(&value),
                Err(error) => {
                    RegCloseKey(key);
                    return Err(error);
                }
            };
            RegSetValueExW(
                key,
                wide(STARTUP_NAME).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide(STARTUP_NAME).as_ptr())
        };
        RegCloseKey(key);
        if result != ERROR_SUCCESS && (result != ERROR_FILE_NOT_FOUND || enabled) {
            return Err("Could not update Windows startup settings.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_accept_any_layout_identifier_and_ignore_unknown_keys() {
        assert_eq!(
            parse_selection("source=00000407\ndestination=F0020409\nunknown=ignored"),
            Selection {
                from: Some(0x407),
                to: Some(0xF0020409)
            }
        );
        assert_eq!(
            parse_selection("source=auto\ndestination=invalid"),
            Selection::default()
        );
    }
}
