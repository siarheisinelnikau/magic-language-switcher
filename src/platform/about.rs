use super::native::wide;
use crate::i18n::Language;
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    UI::{Controls::*, Shell::*, WindowsAndMessaging::*},
};

const REPOSITORY: &str = "https://github.com/siarheisinelnikau/magic-language-switcher";

pub struct Choice {
    pub check: bool,
    pub startup_checks: bool,
}
pub fn show(
    parent: HWND,
    icon: HICON,
    startup_checks: bool,
    update_status: &str,
    language: Language,
) -> Option<Choice> {
    let title = wide(language.text("About Magic Language Switcher"));
    let heading = wide("Magic Language Switcher");
    let content = wide(&language.format("Version {version}\n\nFix text typed in the wrong keyboard layout.\nWorks with the keyboard layouts installed in Windows.\n\n{status}\n\nBuilt with Rust. One portable EXE.", &[("version", env!("CARGO_PKG_VERSION")), ("status", update_status)]));
    let footer = wide(&format!(
        "<a href=\"{REPOSITORY}\">{}</a>\nCopyright © 2026 siarheisinelnikau",
        language.text("GitHub repository")
    ));
    let license = wide(include_str!("../../LICENSE"));
    let expand = wide(language.text("MIT license"));
    let collapse = wide(language.text("Hide license"));
    let check = wide(language.text("Check for updates"));
    let verification = wide(language.text("Check for updates on startup"));
    let button = TASKDIALOG_BUTTON {
        nButtonID: 100,
        pszButtonText: check.as_ptr(),
    };
    let close = wide(language.text("Close"));
    let buttons = [
        button,
        TASKDIALOG_BUTTON {
            nButtonID: 101,
            pszButtonText: close.as_ptr(),
        },
    ];
    let configuration = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: parent,
        dwFlags: TDF_ENABLE_HYPERLINKS
            | TDF_ALLOW_DIALOG_CANCELLATION
            | TDF_USE_HICON_MAIN
            | TDF_SIZE_TO_CONTENT
            | if startup_checks {
                TDF_VERIFICATION_FLAG_CHECKED
            } else {
                0
            },
        dwCommonButtons: 0,
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pszContent: content.as_ptr(),
        pszVerificationText: verification.as_ptr(),
        Anonymous1: TASKDIALOGCONFIG_0 { hMainIcon: icon },
        pszFooter: footer.as_ptr(),
        pszExpandedInformation: license.as_ptr(),
        pszCollapsedControlText: expand.as_ptr(),
        pszExpandedControlText: collapse.as_ptr(),
        pfCallback: Some(callback),
        cxWidth: 290,
        ..Default::default()
    };
    let mut selected = 0;
    let mut checked = i32::from(startup_checks);
    // SAFETY: all text buffers outlive the synchronous dialog; the icon remains app-owned.
    let status =
        unsafe { TaskDialogIndirect(&configuration, &mut selected, null_mut(), &mut checked) };
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
        return None;
    }
    Some(Choice {
        check: selected == 100,
        startup_checks: checked != 0,
    })
}

pub fn offer_update(parent: HWND, update: &crate::updates::Update, language: Language) -> bool {
    let title = wide(language.text("Magic Language Switcher update"));
    let heading = wide(&language.format(
        "Version {version} is available",
        &[("version", &update.version.to_string())],
    ));
    let content = wide(&language.format("Installed: {version}\nDownload: {size} MB\n\nDownload from GitHub, verify SHA-256, and restart the app. Your settings and EXE location will be kept.", &[("version", env!("CARGO_PKG_VERSION")), ("size", &format!("{:.1}", update.executable_size() as f64 / 1048576.0))]));
    let label = wide(language.text("Download and install"));
    let button = TASKDIALOG_BUTTON {
        nButtonID: 100,
        pszButtonText: label.as_ptr(),
    };
    let cancel = wide(language.text("Cancel"));
    let buttons = [
        button,
        TASKDIALOG_BUTTON {
            nButtonID: 101,
            pszButtonText: cancel.as_ptr(),
        },
    ];
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: parent,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT,
        dwCommonButtons: 0,
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pszContent: content.as_ptr(),
        cxWidth: 280,
        ..Default::default()
    };
    let mut selected = 0;
    // SAFETY: all strings and the button outlive this synchronous native dialog.
    unsafe {
        TaskDialogIndirect(&config, &mut selected, null_mut(), null_mut());
    }
    selected == 100
}

pub fn update_status(parent: HWND, message: &str, failed: bool, language: Language) {
    // SAFETY: both terminated strings remain live during the synchronous dialog.
    unsafe {
        MessageBoxW(
            parent,
            wide(message).as_ptr(),
            wide(language.text("Magic Language Switcher update")).as_ptr(),
            MB_OK
                | if failed {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        );
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
