//! Opt-in end-to-end checks using an isolated native editor and the real application process.
//! Compiled only with `--features desktop-tests`; never included in release builds.
use super::{
    clipboard::{self, Session},
    native::{self, wide},
    ui::{QUIT_ID, WINDOW_CLASS, window_text},
};
use std::{
    path::Path,
    process::{Child, Command},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Threading::*},
    UI::{
        Controls::{EM_GETSEL, EM_SETSEL},
        Input::KeyboardAndMouse::*,
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

static BLOCK_COPY: AtomicBool = AtomicBool::new(false);

pub fn run(binary: &Path) -> Result<(), String> {
    // SAFETY: the test registers its own class, creates only test-owned windows, and pumps
    // their messages on the creating thread. Product inputs target this isolated editor.
    unsafe {
        if !FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()).is_null() {
            return Err("Close Magic Language Switcher before running desktop checks.".into());
        }
        let instance = GetModuleHandleW(null());
        let class_name = wide("MagicLanguageSwitcher.SmokeEditor");
        let class = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&class);
        let host = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("Magic Language Switcher test editor").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            80,
            80,
            640,
            420,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if host.is_null() {
            return Err("Could not create the smoke-test editor.".into());
        }
        struct HostGuard(HWND);
        impl Drop for HostGuard {
            fn drop(&mut self) {
                // SAFETY: the guard owns this test window.
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let _host = HostGuard(host);
        let editor = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_BORDER
                | (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
            12,
            12,
            600,
            295,
            host,
            null_mut(),
            instance,
            null(),
        );
        if SetWindowSubclass(editor, Some(copy_line_when_empty), 1, 0) == 0 {
            return Err("Could not install the editor regression fixture.".into());
        }
        let password = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_PASSWORD as u32,
            12,
            320,
            600,
            28,
            host,
            null_mut(),
            instance,
            null(),
        );
        let mut original_clipboard = Session::capture(host as usize)?;
        ShowWindow(host, SW_SHOW);
        let settings_path = std::env::temp_dir().join(format!(
            "magic-language-switcher-smoke-{}-{}.conf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        struct SettingsGuard(std::path::PathBuf);
        impl Drop for SettingsGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _settings = SettingsGuard(settings_path.clone());
        std::fs::write(&settings_path, "check_updates_on_startup=false\n")
            .map_err(|e| e.to_string())?;
        let spawn_app = || {
            Command::new(binary)
                .arg("--background")
                .env("MAGIC_LANGUAGE_SWITCHER_SETTINGS", &settings_path)
                .spawn()
                .map_err(|error| error.to_string())
        };
        let child = spawn_app()?;
        struct ProcessGuard(Child);
        impl Drop for ProcessGuard {
            fn drop(&mut self) {
                // SAFETY: the test has exclusive ownership of this application instance.
                unsafe {
                    let window = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
                    if !window.is_null() {
                        PostMessageW(window, WM_COMMAND, QUIT_ID, 0);
                    }
                }
                for _ in 0..40 {
                    if matches!(self.0.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut process = ProcessGuard(child);
        wait_for(
            || !FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()).is_null(),
            "Application startup",
        )?;
        pump_for(Duration::from_millis(350));
        let mut app = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
        println!(
            "Application status: {} / {}",
            window_text(GetDlgItem(app, 108)),
            window_text(GetDlgItem(app, 107))
        );
        let result = (|| {
            select_pair(app, 0x0409, 0x0419)?;
            pump_for(Duration::from_millis(100));
            let prepare = |text: &str| -> Result<(), String> {
                let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
                let current_thread = GetCurrentThreadId();
                let attached = foreground_thread != current_thread
                    && AttachThreadInput(current_thread, foreground_thread, 1) != 0;
                BringWindowToTop(host);
                SetForegroundWindow(host);
                SetFocus(editor);
                if attached {
                    AttachThreadInput(current_thread, foreground_thread, 0);
                }
                SetWindowTextW(editor, wide(text).as_ptr());
                let end = text.encode_utf16().count();
                SendMessageW(editor, EM_SETSEL, end, end as isize);
                clipboard::write_text(host as usize, "clipboard sentinel")?;
                pump_for(Duration::from_millis(100));
                if GetForegroundWindow() != host {
                    return Err(format!(
                        "The test editor could not become foreground; active window: {:?}, expected host: {:?}, actual: {:?}",
                        window_text(GetForegroundWindow()),
                        host,
                        GetForegroundWindow()
                    ));
                }
                Ok(())
            };
            let trigger = |key: u16, expected: &str, label: &str| -> Result<(), String> {
                shortcut(key)?;
                wait_for(|| window_text(editor) == expected, label).map_err(|error| {
                    format!(
                        "{error}; editor: {:?}; app: {:?}; clipboard: {:?}",
                        window_text(editor),
                        window_text(GetDlgItem(app, 107)),
                        clipboard::read_text()
                    )
                })?;
                pump_for(Duration::from_millis(500));
                check(
                    clipboard::read_text()?.as_str() == "clipboard sentinel",
                    &format!("{label}: clipboard restored"),
                )
            };
            prepare("before ghbdtn")?;
            check_accessible_selection(false)?;
            SendMessageW(editor, EM_SETSEL, 7, 13);
            check_accessible_selection(true)?;
            prepare("before ghbdtn")?;
            {
                let owner = host as usize;
                let worker = thread::spawn(move || -> Result<String, String> {
                    let mut clipboard = Session::capture(owner)?;
                    let copied = clipboard.copy_selection()?;
                    clipboard.restore()?;
                    Ok(copied)
                });
                while !worker.is_finished() {
                    pump_for(Duration::from_millis(15));
                }
                let copied = worker
                    .join()
                    .map_err(|_| "Editor fixture worker panicked.")??;
                check(
                    copied == "before ghbdtn",
                    "Editor fixture copies a whole line with no selection",
                )?;
            }
            trigger(
                VK_SPACE,
                "before привет",
                "Last word through global shortcut",
            )?;
            trigger(VK_BACK, "before ghbdtn", "Verified undo")?;
            prepare("Ghbdtn vbh!")?;
            SendMessageW(editor, EM_SETSEL, 0, -1);
            trigger(VK_RETURN, "Привет мир!", "Selected sentence and case")?;
            prepare("руддщ цщкдв")?;
            select_pair(app, 0x0419, 0x0409)?;
            pump_for(Duration::from_millis(100));
            SendMessageW(editor, EM_SETSEL, 0, -1);
            trigger(VK_RETURN, "hello world", "Russian to English")?;
            select_pair(app, 0x0409, 0x0419)?;
            pump_for(Duration::from_millis(100));
            prepare("ghbdtn\r\nvbh\t123")?;
            SendMessageW(editor, EM_SETSEL, 0, -1);
            trigger(VK_RETURN, "привет\r\nмир\t123", "Multiline text and tabs")?;
            trigger(VK_BACK, "ghbdtn\r\nvbh\t123", "Multiline undo")?;
            prepare("ghbdtn")?;
            trigger(VK_SPACE, "привет", "Correction before undo refusal")?;
            native::type_unicode(" changed")?;
            pump_for(Duration::from_millis(60));
            shortcut(VK_BACK)?;
            pump_for(Duration::from_millis(850));
            check(
                window_text(editor) == "привет changed",
                "Undo refuses changed text",
            )?;
            prepare("")?;
            shortcut(VK_SPACE)?;
            pump_for(Duration::from_millis(900));
            check(
                window_text(editor).is_empty(),
                "Empty editor remains unchanged",
            )?;
            check(
                clipboard::read_text()? == "clipboard sentinel",
                "Empty editor preserves clipboard",
            )?;
            prepare("before ghbdtn after")?;
            SendMessageW(editor, EM_SETSEL, 7, 13);
            trigger(
                VK_SPACE,
                "before привет after",
                "Selection takes priority without extending its bounds",
            )?;
            trigger(
                VK_BACK,
                "before ghbdtn after",
                "Undo restores a selected correction",
            )?;
            prepare("before ghbdtn after")?;
            SendMessageW(editor, EM_SETSEL, 13, 7);
            trigger(
                VK_SPACE,
                "before привет after",
                "Backward selections use the same correction key",
            )?;
            prepare("ghbdtn 123 vbh")?;
            SendMessageW(editor, EM_SETSEL, 7, 10);
            shortcut(VK_SPACE)?;
            pump_for(Duration::from_millis(650));
            check(
                window_text(editor) == "ghbdtn 123 vbh",
                "Unchanged selection does not fall back to the last word",
            )?;
            let range = SendMessageW(editor, EM_GETSEL, 0, 0) as u32;
            check(
                range & 0xffff == 7 && range >> 16 == 10,
                "An unchanged selection stays selected",
            )?;
            prepare("ghbdtn")?;
            native::chord(VK_F8, &[VK_SHIFT])?;
            pump_for(Duration::from_millis(200));
            check(
                window_text(editor) == "ghbdtn",
                "Retired Shift+F8 binding is not registered",
            )?;
            trigger(
                VK_SPACE,
                "привет",
                "No selection converts the last word using the same key",
            )?;
            prepare("before ghbdtn")?;
            SendMessageW(editor, EM_SETSEL, 7, 13);
            {
                use windows_sys::Win32::System::DataExchange::{CloseClipboard, OpenClipboard};
                if OpenClipboard(host) == 0 {
                    return Err("Could not reserve the test clipboard.".into());
                }
                struct BusyClipboard;
                impl Drop for BusyClipboard {
                    fn drop(&mut self) {
                        // SAFETY: this guard owns the clipboard opened on this test thread.
                        unsafe {
                            CloseClipboard();
                        }
                    }
                }
                let _clipboard = BusyClipboard;
                shortcut(VK_SPACE)?;
                pump_for(Duration::from_millis(650));
                check(
                    window_text(editor) == "before ghbdtn",
                    "Busy clipboard does not trigger a fallback edit",
                )?;
                let range = SendMessageW(editor, EM_GETSEL, 0, 0) as u32;
                check(
                    range & 0xffff == 7 && range >> 16 == 13,
                    "Clipboard failure preserves the selected range",
                )?;
            }
            prepare("before ghbdtn")?;
            SendMessageW(editor, EM_SETSEL, 7, 13);
            {
                struct CopyFailure;
                impl Drop for CopyFailure {
                    fn drop(&mut self) {
                        BLOCK_COPY.store(false, Ordering::Relaxed);
                    }
                }
                let _failure = CopyFailure;
                BLOCK_COPY.store(true, Ordering::Relaxed);
                shortcut(VK_SPACE)?;
                pump_for(Duration::from_millis(950));
                check(
                    window_text(editor) == "before ghbdtn",
                    "Failed selection copy does not fall back to another range",
                )?;
                let range = SendMessageW(editor, EM_GETSEL, 0, 0) as u32;
                check(
                    range & 0xffff == 7 && range >> 16 == 13,
                    "Failed selection copy leaves the range intact",
                )?;
                check(
                    clipboard::read_text()? == "clipboard sentinel",
                    "Failed selection copy preserves the clipboard",
                )?;
            }
            let oversized = "a".repeat(4097);
            prepare(&oversized)?;
            SendMessageW(editor, EM_SETSEL, 0, -1);
            shortcut(VK_RETURN)?;
            pump_for(Duration::from_millis(850));
            check(window_text(editor) == oversized, "Selection length limit")?;
            prepare("ghbdtn")?;
            PostMessageW(app, WM_COMMAND, 101, 0);
            pump_for(Duration::from_millis(100));
            shortcut(VK_SPACE)?;
            pump_for(Duration::from_millis(300));
            check(window_text(editor) == "ghbdtn", "Pause disables shortcuts")?;
            PostMessageW(app, WM_COMMAND, 101, 0);
            pump_for(Duration::from_millis(100));
            SetFocus(password);
            SetWindowTextW(password, wide("ghbdtn").as_ptr());
            SendMessageW(password, EM_SETSEL, 0, -1);
            shortcut(VK_RETURN)?;
            pump_for(Duration::from_millis(850));
            check(
                window_text(password) == "ghbdtn",
                "Password field protection",
            )?;
            prepare("ghbdtn")?;
            SendMessageW(editor, EM_SETSEL, 0, -1);
            {
                // Clipboard capture runs off the editor thread so Ctrl+C can be dispatched.
                let owner = host as usize;
                let worker = thread::spawn(move || -> Result<(), String> {
                    let mut session = Session::capture(owner)?;
                    let _ = session.copy_selection()?;
                    clipboard::write_text(owner, "a newer clipboard value")?;
                    session.restore()?;
                    check(
                        clipboard::read_text()? == "a newer clipboard value",
                        "New clipboard values are not overwritten",
                    )
                });
                while !worker.is_finished() {
                    pump_for(Duration::from_millis(15));
                }
                worker
                    .join()
                    .map_err(|_| "Clipboard test worker panicked.")??;
            }
            // Verify singleton behavior and that the second invocation opens the existing window.
            let mut second = Command::new(binary).spawn().map_err(|e| e.to_string())?;
            wait_for(
                || matches!(second.try_wait(), Ok(Some(_))),
                "Second instance exits",
            )?;
            wait_for(
                || IsWindowVisible(app) != 0,
                "Second instance opens the existing window",
            )?;
            let mut original_bounds = RECT::default();
            GetClientRect(app, &mut original_bounds);
            record_binding(app, 0, VK_F8, &[VK_CONTROL], "F8")?;
            check(
                window_text(GetDlgItem(app, 107)).contains("already assigned"),
                "Duplicate action shortcut is rejected",
            )?;
            prepare("ghbdtn")?;
            trigger(
                VK_SPACE,
                "привет",
                "Previous shortcut survives duplicate rejection",
            )?;
            {
                struct ReservedShortcut(HWND);
                impl Drop for ReservedShortcut {
                    fn drop(&mut self) {
                        // SAFETY: this guard owns the test registration.
                        unsafe {
                            UnregisterHotKey(self.0, 88);
                        }
                    }
                }
                if RegisterHotKey(
                    host,
                    88,
                    MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                    VK_F23 as u32,
                ) == 0
                {
                    return Err("Could not reserve the test conflict shortcut.".into());
                }
                let _reserved = ReservedShortcut(host);
                record_binding(app, 0, VK_F23, &[VK_CONTROL, VK_MENU, VK_SHIFT], "F8")?;
                check(
                    window_text(GetDlgItem(app, 107)).contains("another app"),
                    "External global shortcut conflict is rejected",
                )?;
                prepare("ghbdtn")?;
                trigger(
                    VK_SPACE,
                    "привет",
                    "Previous shortcut survives external conflict",
                )?;
            }
            record_binding(app, 0, VK_F9, &[VK_CONTROL, VK_SHIFT], "Ctrl+Shift+F9")?;
            record_binding(app, 1, VK_F10, &[VK_CONTROL, VK_SHIFT], "Ctrl+Shift+F10")?;
            begin_recording(app, 0)?;
            native::chord(VK_ESCAPE, &[])?;
            wait_for(
                || window_text(GetDlgItem(app, 120)) == "Ctrl+Shift+F9",
                "Escape cancels shortcut capture",
            )?;
            begin_recording(app, 1)?;
            activate(host);
            SetFocus(editor);
            wait_for(
                || window_text(GetDlgItem(app, 121)) == "Ctrl+Shift+F10",
                "Leaving settings cancels shortcut capture",
            )?;
            prepare("ghbdtn")?;
            shortcut(VK_SPACE)?;
            pump_for(Duration::from_millis(250));
            check(
                window_text(editor) == "ghbdtn",
                "Old F8 binding is unregistered",
            )?;
            native::chord(VK_F9, &[VK_CONTROL, VK_SHIFT])?;
            wait_for(
                || window_text(editor) == "привет",
                "Unified custom shortcut corrects the last word",
            )?;
            pump_for(Duration::from_millis(500));
            check(
                window_text(GetDlgItem(app, 107)).contains("Ctrl+Shift+F10"),
                "Undo hint uses the configured shortcut",
            )?;
            native::chord(VK_F10, &[VK_CONTROL, VK_SHIFT])?;
            wait_for(
                || window_text(editor) == "ghbdtn",
                "Custom undo shortcut works",
            )?;
            pump_for(Duration::from_millis(500));
            prepare("Ghbdtn vbh!")?;
            SendMessageW(editor, EM_SETSEL, 0, -1);
            native::chord(VK_F9, &[VK_CONTROL, VK_SHIFT])?;
            wait_for(
                || window_text(editor) == "Привет мир!",
                "The same custom shortcut corrects selected text",
            )?;
            pump_for(Duration::from_millis(500));
            let mut final_bounds = RECT::default();
            GetClientRect(app, &mut final_bounds);
            check(
                original_bounds.right == final_bounds.right
                    && original_bounds.bottom == final_bounds.bottom,
                "Shortcut editing does not resize the settings window",
            )?;
            PostMessageW(app, WM_COMMAND, QUIT_ID, 0);
            wait_for(
                || matches!(process.0.try_wait(), Ok(Some(_))),
                "Exit before persistence check",
            )?;
            let legacy = std::fs::read_to_string(&settings_path)
                .map_err(|error| error.to_string())?
                .replace("hotkey_correct=", "hotkey_last_word=");
            std::fs::write(
                &settings_path,
                format!("{legacy}hotkey_selection=0001:0082\n"),
            )
            .map_err(|error| error.to_string())?;
            process.0 = spawn_app()?;
            wait_for(
                || !FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()).is_null(),
                "Restart with saved shortcuts",
            )?;
            app = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
            pump_for(Duration::from_millis(400));
            check(
                window_text(GetDlgItem(app, 120)) == "Ctrl+Shift+F9",
                "Legacy correction shortcut survives migration and restart",
            )?;
            prepare("ghbdtn")?;
            native::chord(VK_F9, &[VK_CONTROL, VK_SHIFT])?;
            wait_for(
                || window_text(editor) == "привет",
                "Saved global shortcut is registered after restart",
            )?;
            pump_for(Duration::from_millis(500));
            record_binding(app, 0, VK_BACK, &[], "F8")?;
            prepare("ghbdtn")?;
            native::chord(VK_F8, &[])?;
            wait_for(
                || window_text(editor) == "привет",
                "Backspace restores the default shortcut",
            )?;
            pump_for(Duration::from_millis(500));
            Ok(())
        })();
        let restored = original_clipboard.restore_current();
        result.and(restored)
    }
}

fn check_accessible_selection(expected: bool) -> Result<(), String> {
    let worker = thread::spawn(super::selection::accessible_selection);
    while !worker.is_finished() {
        pump_for(Duration::from_millis(15));
    }
    let actual = worker
        .join()
        .map_err(|_| "Selection probe worker panicked.")??;
    check(
        actual == Some(expected),
        "Windows accessibility distinguishes selection from the caret",
    )
}

/// Emulate editors that copy the current line with no selection. Unified correction
/// must inspect selection state first rather than accidentally replacing the whole line.
unsafe extern "system" fn copy_line_when_empty(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: this callback belongs to the test-owned edit on its creating thread.
    unsafe {
        if message == WM_COPY {
            if BLOCK_COPY.load(Ordering::Relaxed) {
                return 0;
            }
            let mut start = 0u32;
            let mut end = 0u32;
            SendMessageW(
                window,
                EM_GETSEL,
                &mut start as *mut u32 as usize,
                &mut end as *mut u32 as isize,
            );
            if start == end {
                SendMessageW(window, EM_SETSEL, 0, -1);
                let result = DefSubclassProc(window, message, wparam, lparam);
                SendMessageW(window, EM_SETSEL, start as usize, end as isize);
                return result;
            }
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}

fn activate(window: HWND) {
    // SAFETY: temporarily attach input queues only to foreground the known test target.
    unsafe {
        let other = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let current = GetCurrentThreadId();
        let attached = other != current && AttachThreadInput(current, other, 1) != 0;
        BringWindowToTop(window);
        SetForegroundWindow(window);
        if attached {
            AttachThreadInput(current, other, 0);
        }
    }
}

fn begin_recording(app: HWND, index: usize) -> Result<(), String> {
    // SAFETY: these messages open the real settings window and invoke its real shortcut button.
    unsafe {
        PostMessageW(app, WM_APP + 2, 0, 0);
        wait_for(
            || IsWindowVisible(app) != 0,
            "Open settings for shortcut editing",
        )?;
        activate(app);
        PostMessageW(app, WM_COMMAND, 120 + index, 0);
        wait_for(
            || window_text(GetDlgItem(app, 120 + index as i32)) == "Press keys...",
            "Shortcut capture started",
        )
    }
}

fn record_binding(
    app: HWND,
    index: usize,
    key: u16,
    modifiers: &[u16],
    expected: &str,
) -> Result<(), String> {
    begin_recording(app, index)?;
    native::chord(key, modifiers)?;
    // SAFETY: shortcut button IDs are fixed and created by this application.
    let button = unsafe { GetDlgItem(app, 120 + index as i32) };
    wait_for(
        || window_text(button) == expected,
        "Shortcut capture finished",
    )
    .map_err(|error| {
        // SAFETY: this is the existing status label of the known application window.
        let status = unsafe { window_text(GetDlgItem(app, 107)) };
        format!(
            "{error}; button: {:?}; status: {status}",
            window_text(button)
        )
    })?;
    pump_for(Duration::from_millis(180));
    Ok(())
}

fn select_pair(app: HWND, source_language: u16, destination_language: u16) -> Result<(), String> {
    // SAFETY: these messages query the app's real combo boxes and store only integer indexes.
    unsafe {
        for (control_id, language) in [(109, source_language), (110, destination_language)] {
            let control = GetDlgItem(app, control_id);
            let count = SendMessageW(control, CB_GETCOUNT, 0, 0);
            let choice = (1..count).find(|index| SendMessageW(control, CB_GETITEMDATA, *index as usize, 0) as u16 == language)
                .ok_or("These known-text desktop checks require installed US English and Russian layouts.")?;
            SendMessageW(control, CB_SETCURSEL, choice as usize, 0);
            PostMessageW(
                app,
                WM_COMMAND,
                control_id as usize | ((CBN_SELCHANGE as usize) << 16),
                control as isize,
            );
        }
    }
    Ok(())
}

fn shortcut(action_key: u16) -> Result<(), String> {
    let modifiers: &[u16] = if action_key == VK_BACK {
        &[VK_CONTROL]
    } else {
        &[]
    };
    native::chord(VK_F8, modifiers)
}

fn check(condition: bool, label: &str) -> Result<(), String> {
    if condition {
        println!("PASS: {label}");
        Ok(())
    } else {
        Err(format!("FAIL: {label}"))
    }
}

fn wait_for(mut condition: impl FnMut() -> bool, label: &str) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(4) {
        pump_for(Duration::from_millis(15));
        if condition() {
            println!("PASS: {label}");
            return Ok(());
        }
    }
    Err(format!("Timed out: {label}"))
}

fn pump_for(duration: Duration) {
    let started = Instant::now();
    while started.elapsed() < duration {
        // SAFETY: messages belong to the test thread; MSG is initialized before dispatch.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}
