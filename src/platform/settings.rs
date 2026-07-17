use super::native::wide;
use crate::{
    layout::Selection,
    shortcuts::{Shortcut, Shortcuts},
};
use std::ptr::{null, null_mut};
use std::{fs, path::PathBuf};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

const STARTUP_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const STARTUP_NAME: &str = "MagicLanguageSwitcher";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub selection: Selection,
    pub shortcuts: Shortcuts,
}

fn preferences_path() -> Option<PathBuf> {
    #[cfg(feature = "desktop-tests")]
    if let Some(path) = std::env::var_os("MAGIC_LANGUAGE_SWITCHER_SETTINGS") {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("LOCALAPPDATA").map(|root| {
        PathBuf::from(root)
            .join("MagicLanguageSwitcher")
            .join("settings.conf")
    })
}

pub fn load() -> Preferences {
    preferences_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map_or(Preferences::default(), |text| parse_preferences(&text))
}

fn parse_preferences(text: &str) -> Preferences {
    let mut preferences = Preferences::default();
    let mut correct = None;
    let mut legacy_correct = None;
    for line in text.lines() {
        if let Some((name, value)) = line.split_once('=') {
            match name.trim() {
                "source" => preferences.selection.from = u32::from_str_radix(value.trim(), 16).ok(),
                "destination" => {
                    preferences.selection.to = u32::from_str_radix(value.trim(), 16).ok()
                }
                "hotkey_correct" => correct = Shortcut::decode(value.trim()),
                "hotkey_last_word" => legacy_correct = Shortcut::decode(value.trim()),
                "hotkey_undo" => {
                    if let Some(shortcut) = Shortcut::decode(value.trim()) {
                        preferences.shortcuts.0[1] = shortcut;
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(shortcut) = correct.or(legacy_correct) {
        preferences.shortcuts.0[0] = shortcut;
    }
    if preferences.shortcuts.validate().is_err() {
        preferences.shortcuts = Shortcuts::default();
    }
    preferences
}

fn serialize(preferences: Preferences) -> String {
    let value = |id: Option<u32>| id.map_or_else(|| "auto".into(), |id| format!("{id:08X}"));
    format!(
        "source={}\ndestination={}\nhotkey_correct={}\nhotkey_undo={}\n",
        value(preferences.selection.from),
        value(preferences.selection.to),
        preferences.shortcuts.0[0].encode(),
        preferences.shortcuts.0[1].encode()
    )
}

pub fn save(preferences: Preferences) -> Result<(), String> {
    preferences.shortcuts.validate().map_err(str::to_owned)?;
    let path = preferences_path().ok_or("Windows did not provide a local settings folder.")?;
    let directory = path.parent().ok_or("Invalid settings path.")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, serialize(preferences))
        .and_then(|()| fs::rename(&temporary, path))
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            format!("Could not save settings: {error}")
        })
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
    use crate::shortcuts::{CTRL, SHIFT};

    #[test]
    fn old_layout_settings_keep_default_shortcuts() {
        let value = parse_preferences("source=00000407\ndestination=F0020409\nunknown=ignored");
        assert_eq!(
            value.selection,
            Selection {
                from: Some(0x407),
                to: Some(0xF0020409)
            }
        );
        assert_eq!(value.shortcuts, Shortcuts::default());
        assert_eq!(
            parse_preferences("source=auto\ndestination=invalid"),
            Preferences::default()
        );
    }

    #[test]
    fn changing_layouts_does_not_drop_shortcut_preferences() {
        let mut preferences = Preferences::default();
        preferences.shortcuts = preferences
            .shortcuts
            .updated(
                0,
                Shortcut {
                    key: 0x4b,
                    modifiers: CTRL | SHIFT,
                },
            )
            .unwrap();
        preferences.selection = Selection {
            from: Some(0x4090409),
            to: Some(0x4190419),
        };
        assert_eq!(parse_preferences(&serialize(preferences)), preferences);
        preferences.selection.to = None;
        assert_eq!(parse_preferences(&serialize(preferences)), preferences);
    }

    #[test]
    fn legacy_last_word_and_undo_shortcuts_migrate_to_two_actions() {
        let migrated = parse_preferences(
            "hotkey_last_word=0006:0078\nhotkey_selection=0001:0082\nhotkey_undo=0006:0079",
        );
        assert_eq!(
            migrated.shortcuts.0[0],
            Shortcut {
                modifiers: CTRL | SHIFT,
                key: 0x78
            }
        );
        assert_eq!(
            migrated.shortcuts.0[1],
            Shortcut {
                modifiers: CTRL | SHIFT,
                key: 0x79
            }
        );
        let serialized = serialize(migrated);
        assert!(!serialized.contains("hotkey_selection"));
        assert!(!serialized.contains("hotkey_last_word"));
        assert_eq!(parse_preferences(&serialized), migrated);
    }

    #[test]
    fn explicit_unified_binding_takes_precedence_over_the_legacy_name() {
        let migrated = parse_preferences("hotkey_correct=0002:004B\nhotkey_last_word=0006:0078");
        assert_eq!(
            migrated.shortcuts.0[0],
            Shortcut {
                modifiers: CTRL,
                key: 0x4b
            }
        );
    }

    #[test]
    fn malformed_and_duplicate_saved_shortcuts_are_recoverable() {
        assert_eq!(
            parse_preferences("hotkey_last_word=0002:0077").shortcuts,
            Shortcuts::default()
        );
        assert_eq!(
            parse_preferences("hotkey_last_word=0008:0045\nhotkey_undo=not-a-key").shortcuts,
            Shortcuts::default()
        );
    }
}
