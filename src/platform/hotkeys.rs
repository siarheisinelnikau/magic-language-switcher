use crate::shortcuts::{Shortcut, Shortcuts};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub const CAPTURE_MESSAGE: u32 = WM_APP + 3;
static CAPTURE_WINDOW: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_MODIFIERS: AtomicU16 = AtomicU16::new(0);
static INITIAL_MODIFIERS: AtomicU16 = AtomicU16::new(0);

/// A short-lived recording hook. It consumes primary keys so a conflicting global
/// shortcut cannot launch another app while the user is choosing a binding.
pub struct CaptureHook(HHOOK);

impl CaptureHook {
    pub fn start(window: HWND) -> Result<Self, String> {
        let mut pressed = 0;
        for key in [
            VK_LCONTROL,
            VK_RCONTROL,
            VK_LMENU,
            VK_RMENU,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_LWIN,
            VK_RWIN,
        ] {
            // SAFETY: querying these modifier constants has no pointer arguments.
            if unsafe { GetAsyncKeyState(key as i32) } < 0 {
                pressed |= modifier_bit(key);
            }
        }
        CAPTURE_MODIFIERS.store(pressed, Ordering::Relaxed);
        INITIAL_MODIFIERS.store(pressed, Ordering::Relaxed);
        CAPTURE_WINDOW.store(window as usize, Ordering::Relaxed);
        // SAFETY: the callback is static, the module remains loaded, and this UI thread
        // pumps messages until the hook is removed. No application state is borrowed.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(record_key),
                GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        if hook.is_null() {
            CAPTURE_WINDOW.store(0, Ordering::Relaxed);
            return Err("Windows could not start shortcut recording.".into());
        }
        Ok(Self(hook))
    }
}

impl Drop for CaptureHook {
    fn drop(&mut self) {
        CAPTURE_WINDOW.store(0, Ordering::Relaxed);
        // SAFETY: this guard owns the hook returned by SetWindowsHookExW on this thread.
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
    }
}

fn modifier_bit(key: u16) -> u16 {
    match key {
        VK_CONTROL | VK_LCONTROL => 1,
        VK_RCONTROL => 2,
        VK_MENU | VK_LMENU => 4,
        VK_RMENU => 8,
        VK_SHIFT | VK_LSHIFT => 16,
        VK_RSHIFT => 32,
        VK_LWIN => 64,
        VK_RWIN => 128,
        _ => 0,
    }
}

unsafe extern "system" fn record_key(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    let window = CAPTURE_WINDOW.load(Ordering::Relaxed);
    if code >= 0 && window != 0 {
        // SAFETY: a nonnegative keyboard hook callback supplies a valid KBDLLHOOKSTRUCT.
        let key = unsafe { (*(data as *const KBDLLHOOKSTRUCT)).vkCode as u16 };
        let up = matches!(message as u32, WM_KEYUP | WM_SYSKEYUP);
        let bit = modifier_bit(key);
        let pressed = if bit == 0 {
            CAPTURE_MODIFIERS.load(Ordering::Relaxed)
        } else if up {
            CAPTURE_MODIFIERS.fetch_and(!bit, Ordering::Relaxed) & !bit
        } else {
            CAPTURE_MODIFIERS.fetch_or(bit, Ordering::Relaxed) | bit
        };
        let modifiers = [
            (3, crate::shortcuts::CTRL),
            (12, crate::shortcuts::ALT),
            (48, crate::shortcuts::SHIFT),
            (192, 8),
        ]
        .into_iter()
        .filter(|(mask, _)| pressed & mask != 0)
        .fold(0, |value, (_, flag)| value | flag);
        // SAFETY: only integer values are posted to the live recording window. The
        // callback stays short and never copies text, touches the clipboard, or logs keys.
        unsafe {
            PostMessageW(
                window as HWND,
                CAPTURE_MESSAGE,
                key as usize,
                modifiers as isize | if up { 0x100 } else { 0 },
            );
        }
        // Keep new key sequences out of Windows (including Alt menu and language-switch
        // shortcuts). Release only modifiers that Windows already saw before recording.
        let initial_release =
            up && bit != 0 && INITIAL_MODIFIERS.fetch_and(!bit, Ordering::Relaxed) & bit != 0;
        if !initial_release {
            return 1;
        }
    }
    // SAFETY: unhandled keyboard events must continue through the hook chain.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}

pub struct Registry {
    window: usize,
    active: [bool; 2],
}

impl Registry {
    pub fn new(window: HWND) -> Self {
        Self {
            window: window as usize,
            active: [false; 2],
        }
    }
    pub fn suspend(&mut self) {
        for (index, active) in self.active.iter_mut().enumerate() {
            if *active {
                // SAFETY: the registrations belong to this window and this UI thread.
                unsafe {
                    UnregisterHotKey(self.window as HWND, index as i32 + 1);
                }
                *active = false;
            }
        }
    }
    pub fn resume(&mut self, shortcuts: Shortcuts) {
        self.suspend();
        for (index, shortcut) in shortcuts.0.into_iter().enumerate() {
            let _ = self.register(index, shortcut);
        }
    }
    fn register(&mut self, index: usize, shortcut: Shortcut) -> Result<(), String> {
        // SAFETY: the window was created by this UI thread; values have already been validated.
        if unsafe {
            RegisterHotKey(
                self.window as HWND,
                index as i32 + 1,
                shortcut.modifiers as u32 | MOD_NOREPEAT,
                shortcut.key as u32,
            )
        } == 0
        {
            return Err(format!(
                "{shortcut} is unavailable or already used by another app."
            ));
        }
        self.active[index] = true;
        Ok(())
    }
    pub fn conflicts(&self, shortcuts: Shortcuts) -> Vec<String> {
        shortcuts
            .0
            .iter()
            .enumerate()
            .filter(|(index, _)| !self.active[*index])
            .map(|(_, shortcut)| shortcut.to_string())
            .collect()
    }
    /// Validate, reserve the edited binding, persist, then resume other actions. On failure,
    /// restore the old set before returning; no registration with the same ID is leaked.
    pub fn update(
        &mut self,
        current: Shortcuts,
        index: usize,
        shortcut: Shortcut,
        persist: impl FnOnce(Shortcuts) -> Result<(), String>,
    ) -> Result<Shortcuts, String> {
        let candidate = match current.updated(index, shortcut) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.resume(current);
                return Err(error.into());
            }
        };
        self.suspend();
        let result = self
            .register(index, shortcut)
            .and_then(|()| persist(candidate));
        if let Err(error) = result {
            self.resume(current);
            return Err(error);
        }
        for (other, value) in candidate.0.iter().enumerate() {
            if other != index {
                let _ = self.register(other, *value);
            }
        }
        Ok(candidate)
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.suspend();
    }
}

#[cfg(test)]
mod tests {
    use super::super::native::wide;
    use super::*;
    use crate::shortcuts::{ALT, CTRL, SHIFT};
    use std::ptr::{null, null_mut};

    #[test]
    #[ignore = "requires an unlocked Windows desktop for global shortcut registration"]
    fn failed_persistence_restores_the_registered_shortcut() {
        // SAFETY: the test owns this message-only window and destroys it on this thread.
        unsafe {
            let window = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                null_mut(),
                null(),
            );
            assert!(!window.is_null());
            let mut registry = Registry::new(window);
            let current = Shortcuts([0x83, 0x84].map(|key| Shortcut {
                key,
                modifiers: CTRL | ALT | SHIFT,
            }));
            registry.resume(current);
            assert!(registry.conflicts(current).is_empty());
            let result = registry.update(
                current,
                0,
                Shortcut {
                    key: 0x86,
                    modifiers: CTRL | ALT | SHIFT,
                },
                |_| Err("Simulated write failure".into()),
            );
            assert!(result.is_err());
            assert!(registry.conflicts(current).is_empty());
            assert_eq!(
                RegisterHotKey(
                    window,
                    99,
                    current.0[0].modifiers as u32 | MOD_NOREPEAT,
                    current.0[0].key as u32
                ),
                0
            );
            drop(registry);
            DestroyWindow(window);
        }
    }
}
