//! Adapter for the native update UI. Networking, verification and installation are shared.
use crate::updates::{self, Update};
#[cfg(feature = "desktop-tests")]
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Update cancelled.".into())
    } else {
        Ok(())
    }
}

pub fn check(cancel: &AtomicBool) -> Result<Option<Update>, String> {
    cancelled(cancel)?;
    #[cfg(feature = "desktop-tests")]
    if let Some(fixture) = std::env::var_os("MAGIC_LANGUAGE_SWITCHER_UPDATE_FIXTURE") {
        if let Some(counter) = std::env::var_os("MAGIC_LANGUAGE_SWITCHER_UPDATE_REQUEST") {
            std::fs::write(counter, b"checked").map_err(|e| e.to_string())?;
        }
        let json = std::fs::read_to_string(fixture).map_err(|e| e.to_string())?;
        return rust_auto_updator::parse_release(&json, &updates::config()?)
            .map(|release| release.map(Box::new));
    }
    let release = rust_auto_updator::check_latest(&updates::config()?)?;
    cancelled(cancel)?;
    Ok(release.map(Box::new))
}

#[derive(Default)]
pub struct Progress {
    bytes: AtomicU64,
    total: AtomicU64,
}
impl Progress {
    pub fn text(&self, language: crate::i18n::Language) -> String {
        let total = self.total.load(Ordering::Acquire);
        let bytes = self.bytes.load(Ordering::Relaxed).min(total);
        if total == 0 {
            return language.text("Connecting to GitHub...").into();
        }
        language.format(
            "Downloading and verifying update: {percent}% ({received}/{total} MB)",
            &[
                ("percent", &(bytes.saturating_mul(100) / total).to_string()),
                ("received", &format!("{:.2}", bytes as f64 / 1048576.0)),
                ("total", &format!("{:.2}", total as f64 / 1048576.0)),
            ],
        )
    }
}

pub fn install(update: &Update, cancel: &AtomicBool, progress: &Progress) -> Result<(), String> {
    cancelled(cancel)?;
    let mut prepared = rust_auto_updator::prepare_download_with_cancel(
        update,
        &updates::config()?,
        cancel,
        |received, total| {
            progress.bytes.store(received, Ordering::Relaxed);
            progress.total.store(total, Ordering::Release);
        },
    )?;
    cancelled(cancel)?;
    rust_auto_updator::handoff(&mut prepared)
}

/// Route helper commands before the single-instance mutex or GUI can start.
pub fn command_mode() -> Result<bool, String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let config = updates::config()?;
    if arguments.first().is_some_and(|arg| arg == "--apply-update") {
        return rust_auto_updator::apply_update(&config).map(|()| true);
    }
    if let Some(index) = arguments.iter().position(|arg| arg == "--cleanup-update") {
        let directory = arguments
            .get(index + 1)
            .ok_or("Missing updater cleanup directory.")?;
        let directory = std::path::PathBuf::from(directory);
        if !cleanup_legacy(&directory) {
            rust_auto_updator::cleanup_after_update(directory, config.install.clone());
        }
    }
    #[cfg(feature = "desktop-tests")]
    if rust_auto_updator::smoke_cli(&arguments, &config)? {
        return Ok(true);
    }
    Ok(false)
}

// Released v0.1.x used the old executable as helper and passed its transaction
// directory to the new host. Keep this narrow cleanup bridge for that upgrade.
fn cleanup_legacy(directory: &std::path::Path) -> bool {
    let Ok(directory) = std::fs::canonicalize(directory) else {
        return false;
    };
    let Ok(target) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return false;
    };
    if directory.parent() != target.parent()
        || !directory.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with(".MagicLanguageSwitcher-update-")
        })
    {
        return false;
    }
    let Ok(file) = std::fs::File::open(directory.join("plan.json")) else {
        return true;
    };
    let Ok(plan) =
        serde_json::from_reader::<_, serde_json::Value>(std::io::Read::take(file, 16385))
    else {
        return true;
    };
    if plan
        .get("target")
        .and_then(serde_json::Value::as_str)
        .map(std::path::Path::new)
        != Some(target.as_path())
    {
        return true;
    }
    let _ = std::thread::Builder::new()
        .name("legacy-update-cleanup".into())
        .spawn(move || {
            for _ in 0..100 {
                let helper = directory.join("helper.exe");
                if std::fs::remove_file(&helper).is_ok() || !helper.exists() {
                    for file in ["new.exe", "backup.exe", "plan.json", "ready"] {
                        let _ = std::fs::remove_file(directory.join(file));
                    }
                    let _ = std::fs::remove_dir(&directory);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
    true
}

#[cfg(feature = "desktop-tests")]
pub fn smoke(executable: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new(executable)
        .arg("--test-updater")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Shared updater smoke test failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_release_cleanup_keeps_unrelated_files_and_rejects_foreign_directories() {
        let target = std::env::current_exe()
            .and_then(std::fs::canonicalize)
            .unwrap();
        let directory = target.parent().unwrap().join(format!(
            ".MagicLanguageSwitcher-update-cleanup-test-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("plan.json"), serde_json::json!({"target":target, "pid":123, "original_hash":"ab".repeat(32), "new_hash":"cd".repeat(32)}).to_string()).unwrap();
        std::fs::write(directory.join("helper.exe"), b"old helper").unwrap();
        std::fs::write(directory.join("backup.exe"), b"old backup").unwrap();
        std::fs::write(directory.join("unrelated.txt"), b"keep").unwrap();
        assert!(cleanup_legacy(&directory));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while directory.join("plan.json").exists() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!directory.join("plan.json").exists());
        assert!(!directory.join("helper.exe").exists());
        assert!(!directory.join("backup.exe").exists());
        assert_eq!(
            std::fs::read(directory.join("unrelated.txt")).unwrap(),
            b"keep"
        );
        assert!(!cleanup_legacy(target.parent().unwrap()));
        std::fs::remove_file(directory.join("unrelated.txt")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
