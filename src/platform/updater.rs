//! Manual GitHub updates and a same-EXE helper for replacement after shutdown.
use super::http;
use crate::updates::{self, Update, Version};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    ptr::null,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::ReplaceFileW, System::Threading::*};

const PREFIX: &str = ".MagicLanguageSwitcher-update-";
const FILES: [&str; 5] = ["helper.exe", "new.exe", "backup.exe", "plan.json", "ready"];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    pid: u32,
    target: PathBuf,
    original_hash: String,
    new_hash: String,
    #[cfg(feature = "desktop-tests")]
    fixture_dir: Option<PathBuf>,
}

struct Transaction {
    directory: PathBuf,
    handed_off: bool,
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.handed_off {
            remove_transaction(&self.directory);
        }
    }
}
fn remove_transaction(directory: &Path) {
    // Remove only known files; never recurse into user-supplied directories.
    for name in FILES {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(directory);
}
fn file_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn error(action: &str, error: impl std::fmt::Display) -> String {
    format!("{action}: {error}")
}
fn transaction(target: &Path) -> Result<Transaction, String> {
    let parent = target.parent().ok_or("The EXE has no parent folder.")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = parent.join(format!("{PREFIX}{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).map_err(|e| {
        error(
            "Cannot write to the EXE folder. Move the app to a writable folder and try again",
            e,
        )
    })?;
    Ok(Transaction {
        directory,
        handed_off: false,
    })
}

pub fn check(cancel: &AtomicBool) -> Result<Option<Update>, String> {
    let json = http::bytes(updates::LATEST_API, 1024 * 1024, cancel)?;
    updates::find_update(&json, Version::parse(env!("CARGO_PKG_VERSION"))?)
}

pub fn install(update: &Update, cancel: &AtomicBool) -> Result<(), String> {
    let target = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let mut transaction = transaction(&target)?;
    let checksum = http::bytes(&update.checksum_url, 65536, cancel)?;
    updates::verify_checksum_file(&checksum, update)?;
    let staged = transaction.directory.join("new.exe");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|e| e.to_string())?;
    let size = http::download(&update.url, update.size, &mut file, cancel)?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    verify_payload(&staged, size, update)?;
    let plan = Plan {
        pid: std::process::id(),
        original_hash: updates::hash_file(&target)?,
        target,
        new_hash: update.sha256.clone(),
        #[cfg(feature = "desktop-tests")]
        fixture_dir: None,
    };
    handoff(&mut transaction, &plan, cancel)
}

fn verify_payload(staged: &Path, size: u64, update: &Update) -> Result<(), String> {
    if size != update.size || updates::hash_file(staged)? != update.sha256 {
        return Err(
            "The downloaded EXE failed its SHA-256 check. The update was not installed.".into(),
        );
    }
    validate_pe(staged)
}

fn validate_pe(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| "Incomplete update EXE.")?;
    if &dos[..2] != b"MZ" {
        return Err("Invalid update EXE.".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    if offset as u64
        > file
            .metadata()
            .map_err(|e| e.to_string())?
            .len()
            .saturating_sub(26)
    {
        return Err("Invalid update PE header.".into());
    }
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|e| e.to_string())?;
    let mut header = [0u8; 26];
    file.read_exact(&mut header)
        .map_err(|_| "Incomplete update PE header.")?;
    if &header[..6] != b"PE\0\0\x64\x86" || &header[24..26] != b"\x0b\x02" {
        return Err("The update is not a Windows x64 EXE.".into());
    }
    Ok(())
}

fn handoff(transaction: &mut Transaction, plan: &Plan, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Update cancelled.".into());
    }
    let helper = transaction.directory.join("helper.exe");
    fs::copy(&plan.target, &helper).map_err(|e| error("Could not prepare the update helper", e))?;
    let plan_path = transaction.directory.join("plan.json");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&plan_path)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(plan).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let mut child = Command::new(&helper)
        .arg("--apply-update")
        .arg(&plan_path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| error("Could not start the update helper", e))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if transaction.directory.join("ready").is_file() {
            transaction.handed_off = true;
            return Ok(());
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("The update helper could not prepare the replacement. The current app is unchanged.".into());
        }
        if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The update was cancelled before replacement.".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: this guard owns the OpenProcess result.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn load_plan(path: &Path) -> Result<(Plan, PathBuf, Process), String> {
    let helper = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let directory = helper
        .parent()
        .ok_or("Invalid update helper path.")?
        .to_path_buf();
    if helper.file_name().and_then(|v| v.to_str()) != Some("helper.exe")
        || !directory
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with(PREFIX))
        || fs::canonicalize(path).map_err(|e| e.to_string())? != directory.join("plan.json")
    {
        return Err("Invalid update plan location.".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("Invalid update plan size.".into());
    }
    let plan: Plan = serde_json::from_slice(&bytes).map_err(|_| "Invalid update plan.")?;
    let target = fs::canonicalize(&plan.target).map_err(|e| e.to_string())?;
    if target != plan.target
        || target.parent() != directory.parent()
        || !updates::valid_digest(&plan.original_hash)
        || !updates::valid_digest(&plan.new_hash)
        || updates::hash_file(&helper)? != plan.original_hash
        || updates::hash_file(&target)? != plan.original_hash
        || updates::hash_file(&directory.join("new.exe"))? != plan.new_hash
    {
        return Err("Update files changed before replacement.".into());
    }
    validate_pe(&directory.join("new.exe"))?;
    // SAFETY: query and synchronize rights only, no memory access or process modification.
    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            plan.pid,
        )
    };
    if process.is_null() {
        return Err("The app exited before the update was prepared.".into());
    }
    let process = Process(process);
    let mut image = vec![0u16; 32768];
    let mut length = image.len() as u32;
    // SAFETY: output buffer is allocated to the supplied capacity; the process handle is live.
    if unsafe { QueryFullProcessImageNameW(process.0, 0, image.as_mut_ptr(), &mut length) } == 0 {
        return Err("Could not verify the running app.".into());
    }
    if fs::canonicalize(PathBuf::from(String::from_utf16_lossy(
        &image[..length as usize],
    )))
    .map_err(|e| e.to_string())?
        != target
    {
        return Err("The update process does not match this EXE.".into());
    }
    Ok((plan, directory, process))
}

fn restart(plan: &Plan, directory: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(&plan.target);
    command
        .current_dir(plan.target.parent().unwrap())
        .arg("--cleanup-update")
        .arg(directory);
    #[cfg(feature = "desktop-tests")]
    if let Some(fixture) = &plan.fixture_dir {
        command.arg("--update-fixture-done").arg(fixture);
    }
    command.creation_flags(CREATE_NO_WINDOW).spawn()
}

fn apply(path: &Path) -> Result<(), String> {
    let (plan, directory, process) = load_plan(path)?;
    fs::write(directory.join("ready"), b"ready").map_err(|e| e.to_string())?;
    // SAFETY: the handle is retained while waiting; the parent has signaled safe shutdown.
    if unsafe { WaitForSingleObject(process.0, 60000) } != WAIT_OBJECT_0 {
        return Err("The app did not close in time. The update was not installed.".into());
    }
    if updates::hash_file(&plan.target)? != plan.original_hash
        || updates::hash_file(&directory.join("new.exe"))? != plan.new_hash
    {
        return Err("Update files changed before replacement.".into());
    }
    let target = file_wide(&plan.target);
    let staged = file_wide(&directory.join("new.exe"));
    let backup = file_wide(&directory.join("backup.exe"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: paths are terminated, distinct, and on the same volume. Windows retains a backup.
        if unsafe {
            ReplaceFileW(
                target.as_ptr(),
                staged.as_ptr(),
                backup.as_ptr(),
                0,
                null(),
                null(),
            )
        } != 0
        {
            break;
        }
        // Some ReplaceFile failures can move the original into the backup slot.
        if !plan.target.exists() && directory.join("backup.exe").exists() {
            let _ = fs::rename(directory.join("backup.exe"), &plan.target);
        }
        let current_hash = updates::hash_file(&plan.target).ok();
        if current_hash.as_deref() == Some(&plan.new_hash) {
            break;
        }
        if current_hash.as_deref() != Some(&plan.original_hash) {
            return Err(format!(
                "Windows could not finish the replacement. The original EXE backup is in {}.",
                directory.display()
            ));
        }
        if Instant::now() >= deadline {
            let _ = restart(&plan, &directory);
            return Err("Windows could not replace the EXE. The previous version was kept; try again after closing other copies.".into());
        }
        thread::sleep(Duration::from_millis(200));
    }
    if let Err(failure) = restart(&plan, &directory) {
        // SAFETY: reverse the same replacement, restoring the old EXE from its backup.
        let restored =
            unsafe { ReplaceFileW(target.as_ptr(), backup.as_ptr(), null(), 0, null(), null()) }
                != 0;
        if restored {
            let _ = restart(&plan, &directory);
        }
        return Err(error("Could not restart the updated app", failure));
    }
    Ok(())
}

fn cleanup(directory: PathBuf) {
    let Ok(target) = std::env::current_exe().and_then(fs::canonicalize) else {
        return;
    };
    let Ok(directory) = fs::canonicalize(directory) else {
        return;
    };
    if directory.parent() != target.parent()
        || !directory
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with(PREFIX))
    {
        return;
    }
    let Ok(bytes) = fs::read(directory.join("plan.json")) else {
        return;
    };
    let Ok(plan) = serde_json::from_slice::<Plan>(&bytes) else {
        return;
    };
    if plan.target != target {
        return;
    }
    let _ = thread::Builder::new()
        .name("update-cleanup".into())
        .spawn(move || {
            for _ in 0..100 {
                // The helper is still mapped briefly after starting the new process.
                if fs::remove_file(directory.join("helper.exe")).is_ok()
                    || !directory.join("helper.exe").exists()
                {
                    remove_transaction(&directory);
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
}

/// Run internal modes before acquiring the application's single-instance mutex.
pub fn command_mode() -> Result<bool, String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|v| v == "--apply-update") {
        let path = args.get(2).ok_or("Missing update plan.")?;
        return apply(Path::new(path)).map(|()| true);
    }
    if let Some(index) = args.iter().position(|v| v == "--cleanup-update")
        && let Some(path) = args.get(index + 1)
    {
        cleanup(PathBuf::from(path));
    }
    #[cfg(feature = "desktop-tests")]
    for mode in ["--update-fixture-wait", "--update-fixture-done"] {
        if let Some(index) = args.iter().position(|v| v == mode)
            && let Some(path) = args.get(index + 1)
        {
            let directory = Path::new(path);
            fs::write(
                directory.join(if mode.ends_with("done") {
                    "done"
                } else {
                    "started"
                }),
                std::process::id().to_string(),
            )
            .map_err(|e| e.to_string())?;
            if mode.ends_with("wait") {
                while !directory.join("stop").exists() {
                    thread::sleep(Duration::from_millis(50));
                }
            }
            if mode.ends_with("done")
                && let Some(index) = args.iter().position(|v| v == "--cleanup-update")
                && let Some(path) = args.get(index + 1)
            {
                let deadline = Instant::now() + Duration::from_secs(15);
                while Path::new(path).exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(50));
                }
            }
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(feature = "desktop-tests")]
pub fn smoke(executable: &Path) -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!(
        "MagicLanguageSwitcher-updater-test-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let target = directory.join("app.exe");
    fs::copy(executable, &target).map_err(|e| e.to_string())?;
    let target = fs::canonicalize(target).map_err(|e| e.to_string())?;
    let mut child = Command::new(&target)
        .arg("--update-fixture-wait")
        .arg(&directory)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    let outcome = (|| {
        let mut transaction = transaction(&target)?;
        fs::copy(executable, transaction.directory.join("new.exe")).map_err(|e| e.to_string())?;
        // A valid PE overlay makes the candidate distinguishable from the original.
        OpenOptions::new()
            .append(true)
            .open(transaction.directory.join("new.exe"))
            .and_then(|mut file| file.write_all(b"update-fixture"))
            .map_err(|e| e.to_string())?;
        let plan = Plan {
            pid: child.id(),
            original_hash: updates::hash_file(&target)?,
            new_hash: updates::hash_file(&transaction.directory.join("new.exe"))?,
            target: target.clone(),
            fixture_dir: Some(directory.clone()),
        };
        if handoff(&mut transaction, &plan, &AtomicBool::new(true)).is_ok()
            || updates::hash_file(&target)? != plan.original_hash
            || transaction.directory.join("helper.exe").exists()
        {
            return Err("Cancellation changed the original app or started a helper.".into());
        }
        handoff(&mut transaction, &plan, &AtomicBool::new(false))?;
        fs::write(directory.join("stop"), b"stop").map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while !directory.join("done").exists() || transaction.directory.exists() {
            if Instant::now() > deadline {
                return Err(format!(
                    "Update handoff, restart, or cleanup timed out. Restart marker: {}; transaction remains: {}; target hash: {:?}; files: {:?}",
                    directory.join("done").exists(),
                    transaction.directory.exists(),
                    updates::hash_file(&target),
                    fs::read_dir(&transaction.directory).map(|items| items
                        .filter_map(Result::ok)
                        .map(|entry| entry.file_name())
                        .collect::<Vec<_>>())
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
        if updates::hash_file(&target)? != plan.new_hash {
            return Err("Update did not replace the original EXE.".into());
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    for name in ["app.exe", "started", "stop", "done"] {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(directory);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_tampered_truncated_and_non_executable_downloads() {
        let path = std::env::temp_dir().join(format!(
            "MagicLanguageSwitcher-payload-test-{}",
            std::process::id()
        ));
        fs::write(&path, b"invalid executable").unwrap();
        let mut update = Update {
            version: Version(0, 1, 3),
            filename: String::new(),
            url: String::new(),
            checksum_url: String::new(),
            sha256: "ab".repeat(32),
            size: 18,
        };
        assert!(
            verify_payload(&path, 18, &update)
                .unwrap_err()
                .contains("SHA-256")
        );
        update.sha256 = updates::hash_file(&path).unwrap();
        assert!(
            verify_payload(&path, 17, &update)
                .unwrap_err()
                .contains("SHA-256")
        );
        assert!(
            verify_payload(&path, 18, &update)
                .unwrap_err()
                .contains("EXE")
        );
        fs::remove_file(path).unwrap();
    }
}
