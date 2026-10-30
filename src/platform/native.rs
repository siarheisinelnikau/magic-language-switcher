use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub window: usize,
    pub focus: usize,
    pub layout: usize,
}

impl Target {
    pub fn capture() -> Result<Self, String> {
        // SAFETY: the OS fills an initialized GUITHREADINFO; all handles remain borrowed.
        unsafe {
            let window = GetForegroundWindow();
            let mut info = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if window.is_null() || GetGUIThreadInfo(0, &mut info) == 0 || info.hwndFocus.is_null() {
                return Err("Click inside an editor and try again.".into());
            }
            let thread = GetWindowThreadProcessId(window, null_mut());
            Ok(Self {
                window: window as usize,
                focus: info.hwndFocus as usize,
                layout: GetKeyboardLayout(thread) as usize,
            })
        }
    }

    pub fn ensure_active(self) -> Result<(), String> {
        let current = Self::capture()?;
        if current.window != self.window || current.focus != self.focus {
            return Err("The active editor changed. Try the shortcut again.".into());
        }
        Ok(())
    }

    pub fn is_password(self) -> bool {
        // SAFETY: Win32 validates the borrowed HWND. The class buffer is correctly sized.
        unsafe {
            let mut name = [0u16; 256];
            let length = GetClassNameW(self.focus as HWND, name.as_mut_ptr(), name.len() as i32);
            String::from_utf16_lossy(&name[..length.max(0) as usize])
                .to_ascii_uppercase()
                .contains("EDIT")
                && GetWindowLongW(self.focus as HWND, GWL_STYLE) & ES_PASSWORD != 0
        }
    }
}

pub fn modifiers_down() -> bool {
    // SAFETY: querying virtual-key state has no pointer arguments or side effects.
    unsafe {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| GetAsyncKeyState(*key as i32) < 0)
    }
}

fn key_input(key: u16, flags: u32) -> INPUT {
    let key = match key {
        VK_CONTROL => VK_LCONTROL,
        VK_SHIFT => VK_LSHIFT,
        VK_MENU => VK_LMENU,
        _ => key,
    };
    let extended = matches!(
        key,
        VK_LEFT
            | VK_RIGHT
            | VK_UP
            | VK_DOWN
            | VK_HOME
            | VK_END
            | VK_INSERT
            | VK_DELETE
            | VK_PRIOR
            | VK_NEXT
    );
    // SAFETY: this translates a virtual-key constant into its physical scan code.
    let scan = unsafe { MapVirtualKeyW(key as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags | if extended { KEYEVENTF_EXTENDEDKEY } else { 0 },
                ..Default::default()
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    // SAFETY: inputs points to a live contiguous INPUT array and uses the native struct size.
    unsafe {
        if SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        ) != inputs.len() as u32
        {
            return Err(
                "Windows could not send input. The editor may be running as administrator.".into(),
            );
        }
    }
    Ok(())
}

pub fn chord(key: u16, modifiers: &[u16]) -> Result<(), String> {
    let mut inputs = Vec::with_capacity(2 + modifiers.len() * 2);
    inputs.extend(modifiers.iter().map(|key| key_input(*key, 0)));
    inputs.push(key_input(key, 0));
    inputs.push(key_input(key, KEYEVENTF_KEYUP));
    inputs.extend(
        modifiers
            .iter()
            .rev()
            .map(|key| key_input(*key, KEYEVENTF_KEYUP)),
    );
    send(&inputs)
}

pub fn select_previous(count: usize) -> Result<(), String> {
    let mut inputs = vec![key_input(VK_SHIFT, 0)];
    for _ in 0..count {
        inputs.push(key_input(VK_LEFT, 0));
        inputs.push(key_input(VK_LEFT, KEYEVENTF_KEYUP));
    }
    inputs.push(key_input(VK_SHIFT, KEYEVENTF_KEYUP));
    send(&inputs)
}

pub fn type_unicode(text: &str) -> Result<(), String> {
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: unit,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            });
        }
    }
    send(&inputs)
}

pub fn switch_layout(window: usize, identifier: u32) {
    // SAFETY: the first call determines the vector capacity; the OS writes at most that many handles.
    unsafe {
        let count = GetKeyboardLayoutList(0, null_mut());
        let mut layouts = vec![null_mut(); count.max(0) as usize];
        GetKeyboardLayoutList(count, layouts.as_mut_ptr());
        if let Some(layout) = layouts
            .into_iter()
            .find(|layout| *layout as usize as u32 == identifier)
        {
            PostMessageW(
                window as HWND,
                WM_INPUTLANGCHANGEREQUEST,
                0,
                layout as isize,
            );
        }
    }
}
