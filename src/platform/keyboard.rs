//! Read installed keyboard tables from Windows, including Shift and AltGr states.
use super::native::wide;
use crate::layout::{Catalog, KeyboardLayout, Stroke};
use std::{
    collections::BTreeMap,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Globalization::*,
    System::Registry::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn installed() -> Catalog {
    #[cfg(feature = "desktop-tests")]
    let _timing = super::diagnostics::Scope::new("keyboard_tables");
    // Enumeration runs on a fresh thread with no pending dead-key composition. Flag 4
    // keeps ToUnicodeEx from modifying the thread's keyboard state.
    std::thread::spawn(enumerate).join().unwrap_or_default()
}

/// A cheap identity snapshot; mapping tables only need rebuilding when this changes.
pub fn installed_ids() -> Vec<u32> {
    // SAFETY: Windows writes at most the explicitly allocated handle count.
    unsafe {
        let count = GetKeyboardLayoutList(0, null_mut());
        let mut handles = vec![null_mut(); count.max(0) as usize];
        let actual = GetKeyboardLayoutList(count, handles.as_mut_ptr());
        handles.truncate(actual.max(0) as usize);
        handles
            .into_iter()
            .map(|handle| handle as usize as u32)
            .collect()
    }
}

fn enumerate() -> Catalog {
    // SAFETY: Windows bounds both the handle array and each initialized UTF-16 buffer.
    unsafe {
        let count = GetKeyboardLayoutList(0, null_mut());
        let mut handles = vec![null_mut(); count.max(0) as usize];
        let actual = GetKeyboardLayoutList(count, handles.as_mut_ptr());
        handles.truncate(actual.max(0) as usize);
        let layouts = handles
            .into_iter()
            .map(|handle| {
                let id = handle as usize as u32;
                let mut keys = BTreeMap::new();
                for modifiers in 0..4u8 {
                    let mut state = [0u8; 256];
                    if modifiers & 1 != 0 {
                        state[VK_SHIFT as usize] = 0x80;
                        state[VK_LSHIFT as usize] = 0x80;
                    }
                    if modifiers & 2 != 0 {
                        for key in [VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU] {
                            state[key as usize] = 0x80;
                        }
                    }
                    for scan in (0x02..=0x35u16).chain([0x56, 0x73, 0x7d]) {
                        if [0x0e, 0x0f, 0x1c, 0x1d, 0x2a].contains(&scan) {
                            continue;
                        }
                        let virtual_key = MapVirtualKeyExW(scan as u32, MAPVK_VSC_TO_VK_EX, handle);
                        if virtual_key == 0 {
                            continue;
                        }
                        let mut buffer = [0u16; 16];
                        let length = ToUnicodeEx(
                            virtual_key,
                            scan as u32,
                            state.as_ptr(),
                            buffer.as_mut_ptr(),
                            buffer.len() as i32,
                            4,
                            handle,
                        );
                        // Dead keys require composition history, not a one-to-one character map.
                        if length <= 0 || length as usize > buffer.len() {
                            continue;
                        }
                        let value = String::from_utf16_lossy(&buffer[..length as usize]);
                        if value.chars().any(char::is_control)
                            || value.chars().all(char::is_whitespace)
                        {
                            continue;
                        }
                        keys.insert(Stroke { scan, modifiers }, value);
                    }
                }
                KeyboardLayout {
                    id,
                    name: layout_name(id),
                    keys,
                }
            })
            .collect();
        Catalog { layouts }
    }
}

pub fn active() -> u32 {
    // SAFETY: GetKeyboardLayout accepts the thread ID of the current foreground window.
    unsafe {
        GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), null_mut())) as usize
            as u32
    }
}

fn layout_name(id: u32) -> String {
    // SAFETY: GetLocaleInfoW writes within the provided UTF-16 array.
    let language = unsafe {
        let mut buffer = [0u16; 128];
        let length = GetLocaleInfoW(
            id & 0xffff,
            LOCALE_SENGLISHDISPLAYNAME,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        );
        if length > 1 {
            String::from_utf16_lossy(&buffer[..length as usize - 1])
        } else {
            "Keyboard layout".into()
        }
    };
    // Default HKLs encode their KLID directly. Variant HKLs encode a Layout Id from
    // the keyboard registry. Layout Text is the driver's English human-readable name.
    let high = id >> 16;
    let key_name = if high & 0xf000 == 0xf000 {
        variant_key(id & 0xffff, high & 0x0fff)
    } else {
        Some(format!("{:08X}", id & 0xffff))
    };
    let variant = key_name.and_then(|key| {
        registry_text(
            &format!("SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\\{key}"),
            "Layout Text",
        )
    });
    match variant {
        Some(variant) => format!("{language} — {variant}"),
        None => language,
    }
}

fn variant_key(language: u32, variant: u32) -> Option<String> {
    // SAFETY: registry buffers are initialized; every opened handle is closed before return.
    unsafe {
        let mut root = null_mut();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            wide("SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts").as_ptr(),
            0,
            KEY_READ,
            &mut root,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let mut result = None;
        for index in 0..1024 {
            let mut name = [0u16; 64];
            let mut length = name.len() as u32;
            if RegEnumKeyExW(
                root,
                index,
                name.as_mut_ptr(),
                &mut length,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            ) != ERROR_SUCCESS
            {
                break;
            }
            let key = String::from_utf16_lossy(&name[..length as usize]);
            if u32::from_str_radix(&key, 16).is_ok_and(|id| id & 0xffff == language)
                && registry_text(
                    &format!("SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\\{key}"),
                    "Layout Id",
                )
                .and_then(|text| u32::from_str_radix(&text, 16).ok())
                    == Some(variant)
            {
                result = Some(key);
                break;
            }
        }
        RegCloseKey(root);
        result
    }
}

fn registry_text(path: &str, name: &str) -> Option<String> {
    // SAFETY: registry values are read into a bounded UTF-16 array and the key is closed.
    unsafe {
        let mut key = null_mut();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            wide(path).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let mut buffer = [0u16; 256];
        let mut length = (buffer.len() * 2) as u32;
        let mut kind = 0;
        let status = RegQueryValueExW(
            key,
            wide(name).as_ptr(),
            null(),
            &mut kind,
            buffer.as_mut_ptr() as *mut u8,
            &mut length,
        );
        RegCloseKey(key);
        if status != ERROR_SUCCESS || kind != REG_SZ {
            return None;
        }
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Selection;
    #[test]
    fn windows_tables_are_not_limited_to_a_hardcoded_pair() {
        let catalog = installed();
        assert!(!catalog.layouts.is_empty());
        for layout in &catalog.layouts {
            assert!(!layout.name.is_empty());
            assert!(!layout.keys.is_empty());
            println!(
                "Installed: {} ({:08X}), {} key states",
                layout.name,
                layout.id,
                layout.keys.len()
            );
        }
        for source in &catalog.layouts {
            for destination in &catalog.layouts {
                if source.id != destination.id {
                    let text: String = source.keys.values().take(20).map(String::as_str).collect();
                    assert!(
                        catalog
                            .convert(
                                &text,
                                Selection {
                                    from: Some(source.id),
                                    to: Some(destination.id)
                                },
                                source.id
                            )
                            .is_ok()
                    );
                }
            }
        }
    }
}
