use super::native::wide;
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    UI::{Controls::*, Shell::*, WindowsAndMessaging::*},
};

const REPOSITORY: &str = "https://github.com/siarheisinelnikau/magic-language-switcher";

pub fn show(parent: HWND, icon: HICON) {
    let title = wide("About Magic Language Switcher");
    let heading = wide("Magic Language Switcher");
    let content = wide(&format!(
        "Version {}\n\nFix text typed in the wrong keyboard layout.\nWorks with the keyboard layouts installed in Windows.\n\nBuilt with Rust. One portable EXE. Works offline.",
        env!("CARGO_PKG_VERSION")
    ));
    let footer = wide(&format!(
        "<a href=\"{REPOSITORY}\">GitHub repository</a>\nCopyright © 2026 siarheisinelnikau"
    ));
    let license = wide(include_str!("../../LICENSE"));
    let expand = wide("MIT license");
    let collapse = wide("Hide license");
    let configuration = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: parent,
        dwFlags: TDF_ENABLE_HYPERLINKS
            | TDF_ALLOW_DIALOG_CANCELLATION
            | TDF_USE_HICON_MAIN
            | TDF_SIZE_TO_CONTENT,
        dwCommonButtons: TDCBF_CLOSE_BUTTON,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pszContent: content.as_ptr(),
        Anonymous1: TASKDIALOGCONFIG_0 { hMainIcon: icon },
        pszFooter: footer.as_ptr(),
        pszExpandedInformation: license.as_ptr(),
        pszCollapsedControlText: expand.as_ptr(),
        pszExpandedControlText: collapse.as_ptr(),
        pfCallback: Some(callback),
        cxWidth: 290,
        ..Default::default()
    };
    // SAFETY: all text buffers outlive the synchronous dialog; the icon remains app-owned.
    let status = unsafe { TaskDialogIndirect(&configuration, null_mut(), null_mut(), null_mut()) };
    if status < 0 {
        // SAFETY: the fallback also uses live, NUL-terminated buffers.
        unsafe {
            MessageBoxW(
                parent,
                content.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

unsafe extern "system" fn callback(
    window: HWND,
    message: u32,
    _: WPARAM,
    _: LPARAM,
    _: isize,
) -> i32 {
    if message == TDN_HYPERLINK_CLICKED as u32 {
        // SAFETY: the only action opens this fixed project URL after an explicit link click.
        unsafe {
            ShellExecuteW(
                window,
                wide("open").as_ptr(),
                wide(REPOSITORY).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    0
}
