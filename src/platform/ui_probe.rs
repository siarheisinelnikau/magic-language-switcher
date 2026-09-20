//! Release-profile responsiveness checks without global input or clipboard changes.
use super::{
    diagnostics::REPORT_MESSAGE,
    native::wide,
    ui::{QUIT_ID, WINDOW_CLASS, window_text},
};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Child, Command},
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Input::KeyboardAndMouse::IsWindowEnabled, WindowsAndMessaging::*},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn send(window: HWND, message: u32, parameter: usize) -> Result<f64, String> {
    let started = Instant::now();
    let mut result = 0;
    // SAFETY: the probe owns this app instance; these messages have no pointer payloads.
    if unsafe {
        SendMessageTimeoutW(
            window,
            message,
            parameter,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            5000,
            &mut result,
        )
    } == 0
    {
        return Err(format!("UI message {message} timed out."));
    }
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}
fn snapshot(window: HWND, path: &Path) -> Result<serde_json::Value, String> {
    send(window, REPORT_MESSAGE, 0)?;
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn paints(report: &serde_json::Value) -> u64 {
    report["paint"]["count"].as_u64().unwrap_or(0)
}

pub fn run(binary: &Path, output: &Path) -> Result<(), String> {
    // SAFETY: read-only lookup prevents driving another running instance.
    if !unsafe { FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()) }.is_null() {
        return Err("Close Magic Language Switcher before profiling its UI.".into());
    }
    let temporary = std::env::temp_dir().join(format!(
        "MagicLanguageSwitcher-ui-profile-{}",
        std::process::id()
    ));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let report = temporary.join("timings.json");
    let preferences = temporary.join("settings.conf");
    fs::write(&preferences, "check_updates_on_startup=false\n").map_err(|e| e.to_string())?;
    let mut process = Process(
        Command::new(binary)
            .arg("--background")
            .env("MAGIC_LANGUAGE_SWITCHER_SETTINGS", &preferences)
            .env("MAGIC_LANGUAGE_SWITCHER_UI_PROFILE", &report)
            .env("MAGIC_LANGUAGE_SWITCHER_UI_NO_ACTIVATE", "1")
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let window = loop {
            // SAFETY: the class belongs to the single instance started above.
            let window = unsafe { FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()) };
            if !window.is_null() {
                let mut pid = 0;
                // SAFETY: Windows fills the process ID for this HWND.
                unsafe {
                    GetWindowThreadProcessId(window, &mut pid);
                }
                if pid != process.0.id() {
                    return Err("Another instance appeared during the UI profile.".into());
                }
                break window;
            }
            if Instant::now() > deadline {
                return Err("UI profile startup timed out.".into());
            }
            thread::sleep(Duration::from_millis(10));
        };
        let hidden_before = snapshot(window, &report)?;
        thread::sleep(Duration::from_secs(1));
        let hidden_after = snapshot(window, &report)?;
        let mut latency: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        for _ in 0..20 {
            latency
                .entry("open")
                .or_default()
                .push(send(window, WM_APP + 2, 0)?);
            latency
                .entry("pause")
                .or_default()
                .push(send(window, WM_COMMAND, 101)?);
            let started = Instant::now();
            // SAFETY: invalidate our test instance and let Windows issue WM_PAINT normally.
            unsafe {
                RedrawWindow(
                    window,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN,
                );
            }
            latency
                .entry("repaint")
                .or_default()
                .push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let visible_before = snapshot(window, &report)?;
        thread::sleep(Duration::from_secs(1));
        let visible_after = snapshot(window, &report)?;
        // Hide and quit through the app's normal command path.
        send(window, WM_COMMAND, 2)?;
        send(window, WM_COMMAND, QUIT_ID)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.0.try_wait().map_err(|e| e.to_string())?.is_none() {
            if Instant::now() > deadline {
                return Err("Hidden UI quit timed out.".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let final_report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let latency=latency.into_iter().map(|(name,mut values)| {
            values.sort_by(f64::total_cmp); let n=values.len();
            (name,serde_json::json!({"count":n,"median_ms":values[n/2],"p95_ms":values[((n-1)as f64*0.95).round()as usize],"max_ms":values.last()}))
        }).collect::<BTreeMap<_,_>>();
        let result = serde_json::json!({"callback_roundtrip":latency,"ui_thread":final_report,"hidden_idle_paints":paints(&hidden_after).saturating_sub(paints(&hidden_before)),"visible_idle_paints":paints(&visible_after).saturating_sub(paints(&visible_before))});
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(output, serde_json::to_vec_pretty(&result).unwrap())
            .map_err(|e| e.to_string())?;
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        Ok(())
    })();
    drop(process);
    for path in [&report, &preferences] {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_dir(temporary);
    result
}

/// Exercise the actual startup worker with deterministic release responses and no network.
pub fn startup_checks(binary: &Path) -> Result<(), String> {
    // SAFETY: never send commands to an app instance the probe does not own.
    if !unsafe { FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()) }.is_null() {
        return Err("Close Magic Language Switcher before checking startup updates.".into());
    }
    let root = std::env::temp_dir().join(format!(
        "MagicLanguageSwitcher-startup-update-test-{}",
        std::process::id()
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let preferences = root.join("settings.conf");
    let response = root.join("release.json");
    let request = root.join("request");
    let result = (|| {
        for mode in ["disabled", "default", "error", "available"] {
            fs::write(
                &preferences,
                if mode == "disabled" {
                    "check_updates_on_startup=false\n"
                } else {
                    ""
                },
            )
            .map_err(|e| e.to_string())?;
            let json=match mode {
                "error"=>"invalid JSON".into(),
                "available"=>serde_json::json!({"tag_name":"v99.0.0","html_url":"https://github.com/siarheisinelnikau/magic-language-switcher/releases/tag/v99.0.0","draft":false,"prerelease":false,"assets":[
                    {"name":"MagicLanguageSwitcher-v99.0.0.exe","browser_download_url":"https://github.com/siarheisinelnikau/magic-language-switcher/releases/download/v99.0.0/MagicLanguageSwitcher-v99.0.0.exe","size":4096,"state":"uploaded","digest":format!("sha256:{}","ab".repeat(32))},
                    {"name":"SHA256SUMS.txt","browser_download_url":"https://github.com/siarheisinelnikau/magic-language-switcher/releases/download/v99.0.0/SHA256SUMS.txt","size":100,"state":"uploaded","digest":null}
                ]}).to_string(),
                _=>serde_json::json!({"tag_name":format!("v{}",env!("CARGO_PKG_VERSION")),"html_url":format!("https://github.com/siarheisinelnikau/magic-language-switcher/releases/tag/v{}",env!("CARGO_PKG_VERSION")),"draft":false,"prerelease":false,"assets":[]}).to_string(),
            };
            fs::write(&response, json).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&request);
            let mut process = Process(
                Command::new(binary)
                    .arg("--background")
                    .env("MAGIC_LANGUAGE_SWITCHER_SETTINGS", &preferences)
                    .env("MAGIC_LANGUAGE_SWITCHER_UPDATE_FIXTURE", &response)
                    .env("MAGIC_LANGUAGE_SWITCHER_UPDATE_REQUEST", &request)
                    .spawn()
                    .map_err(|e| e.to_string())?,
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            let window = loop {
                // SAFETY: this is our dedicated class; verify its owner before use.
                let window = unsafe { FindWindowW(wide(WINDOW_CLASS).as_ptr(), null()) };
                if !window.is_null() {
                    let mut pid = 0;
                    // SAFETY: GetWindowThreadProcessId writes one DWORD.
                    unsafe {
                        GetWindowThreadProcessId(window, &mut pid);
                    }
                    if pid != process.0.id() {
                        return Err("A different app instance appeared.".into());
                    }
                    break window;
                }
                if Instant::now() > deadline {
                    return Err("Startup-check test timed out.".into());
                }
                thread::sleep(Duration::from_millis(10));
            };
            send(window, WM_NULL, 0)?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline && (mode == "disabled" || !request.exists()) {
                thread::sleep(Duration::from_millis(20));
            }
            if request.exists() != (mode != "disabled") {
                return Err(format!("Startup update request policy failed: {mode}"));
            }
            // Give the worker result a timer turn; no-update and failure must stay quiet.
            thread::sleep(Duration::from_millis(250));
            send(window, WM_NULL, 0)?;
            // SAFETY: a modal result dialog would disable the verified owner window.
            if unsafe { IsWindowEnabled(window) } == 0 {
                return Err(format!("Startup update showed a blocking dialog: {mode}"));
            }
            if mode == "available" {
                // SAFETY: control 107 belongs to the verified test instance.
                let subtitle = unsafe { GetDlgItem(window, 107) };
                if !window_text(subtitle).contains("Version 99.0.0 is available.") {
                    return Err("Startup did not publish the available-update state.".into());
                }
            }
            send(window, WM_COMMAND, QUIT_ID)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while process.0.try_wait().map_err(|e| e.to_string())?.is_none() {
                if Instant::now() > deadline {
                    return Err("Startup test app did not quit.".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            println!(
                "PASS: startup update mode {mode}, request={}, no blocking dialog",
                request.exists()
            );
        }
        Ok(())
    })();
    for path in [&preferences, &response, &request] {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_dir(root);
    result
}
