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
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Threading::*},
    UI::{Controls::EM_SETSEL, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

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
        let child = Command::new(binary)
            .arg("--background")
            .env("MAGIC_LANGUAGE_SWITCHER_SMOKE", "1")
            .spawn()
            .map_err(|e| e.to_string())?;
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
        let _process = ProcessGuard(child);
        wait_for(
            || !FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()).is_null(),
            "Application startup",
        )?;
        pump_for(Duration::from_millis(350));
        let app = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
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
            prepare("ghbdtn")?;
            shortcut(VK_RETURN)?;
            pump_for(Duration::from_millis(1000));
            check(
                window_text(editor) == "ghbdtn",
                "Empty selection does not replace text",
            )?;
            check(
                clipboard::read_text()? == "clipboard sentinel",
                "Empty selection preserves clipboard",
            )?;
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
            Ok(())
        })();
        let restored = original_clipboard.restore_current();
        result.and(restored)
    }
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
    // Legacy test selectors describe word, selection, and undo; the product uses the F8 family.
    let modifiers: &[u16] = match action_key {
        VK_RETURN => &[VK_SHIFT],
        VK_BACK => &[VK_CONTROL],
        _ => &[],
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
