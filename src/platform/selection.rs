//! Read selection state without copying an editor's entire current line.
use super::native::Target;
use windows::{
    Win32::{System::Com::*, UI::Accessibility::*},
    core::Interface,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Controls::EM_GETSEL, WindowsAndMessaging::*},
};

/// None means the editor does not expose selection state; the caller can use a
/// clipboard probe. A known selection must never fall back after a failed copy.
pub fn has_selection(target: Target) -> Result<Option<bool>, String> {
    target.ensure_active()?;
    if let Some(selected) = native_selection(target) {
        return Ok(Some(selected));
    }
    let selected = accessible_selection()?;
    target.ensure_active()?;
    Ok(selected)
}

fn native_selection(target: Target) -> Option<bool> {
    // SAFETY: Windows validates the borrowed HWND and marshals EM_GETSEL's DWORD
    // pointers between processes. SendMessageTimeout bounds an unresponsive editor.
    unsafe {
        let mut class = [0u16; 256];
        let length = GetClassNameW(target.focus as HWND, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..length.max(0) as usize]).to_ascii_uppercase();
        if !class.contains("EDIT") {
            return None;
        }
        let mut start = 0u32;
        let mut end = 0u32;
        let mut result = 0;
        if SendMessageTimeoutW(
            target.focus as HWND,
            EM_GETSEL,
            &mut start as *mut u32 as usize,
            &mut end as *mut u32 as isize,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            250,
            &mut result,
        ) == 0
        {
            return None;
        }
        Some(start != end)
    }
}

pub(super) fn accessible_selection() -> Result<Option<bool>, String> {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: Apartment exists only after a successful CoInitializeEx on this thread.
            unsafe {
                CoUninitialize();
            }
        }
    }
    // SAFETY: this runs on the correction worker, not the target editor's UI thread.
    // COM objects are dropped before the apartment guard, and provider calls have timeouts.
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return Ok(None);
        }
        let _apartment = Apartment;
        let Ok(automation) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
        else {
            return Ok(None);
        };
        if let Ok(options) = automation.cast::<IUIAutomation2>() {
            let _ = options.SetConnectionTimeout(250);
            let _ = options.SetTransactionTimeout(250);
        }
        let Ok(mut element) = automation.GetFocusedElement() else {
            return Ok(None);
        };
        if element
            .CurrentIsPassword()
            .is_ok_and(|value| value.as_bool())
        {
            return Err("Password fields are not supported.".into());
        }
        let walker = automation.ControlViewWalker().ok();
        for _ in 0..4 {
            if let Ok(pattern) =
                element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                && let Ok(ranges) = pattern.GetSelection()
                && let Ok(length) = ranges.Length()
            {
                for index in 0..length.min(256) {
                    let Ok(range) = ranges.GetElement(index) else {
                        return Ok(None);
                    };
                    match range.CompareEndpoints(
                        TextPatternRangeEndpoint_Start,
                        &range,
                        TextPatternRangeEndpoint_End,
                    ) {
                        Ok(0) => {}
                        Ok(_) => return Ok(Some(true)),
                        Err(_) => return Ok(None),
                    }
                }
                return Ok(Some(false));
            }
            let Some(walker) = &walker else {
                break;
            };
            let Ok(parent) = walker.GetParentElement(&element) else {
                break;
            };
            element = parent;
        }
    }
    Ok(None)
}
