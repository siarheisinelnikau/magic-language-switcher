//! Bounded HTTPS downloads through the Windows TLS and proxy stack.
use super::native::wide;
use std::{
    ffi::c_void,
    io::Write,
    mem::size_of,
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

struct Internet(*mut c_void);
impl Drop for Internet {
    fn drop(&mut self) {
        // SAFETY: each guard owns one WinHTTP handle, closed in reverse dependency order.
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn failed(action: &str) -> String {
    // SAFETY: GetLastError has no pointer arguments.
    let code = unsafe { GetLastError() };
    format!("{action} Check your connection and try again. (Windows error {code})")
}

pub fn bytes(url: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    download(url, limit, &mut bytes, cancel)?;
    Ok(bytes)
}
pub fn download(
    url: &str,
    limit: u64,
    output: &mut impl Write,
    cancel: &AtomicBool,
) -> Result<u64, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or("Updates require HTTPS.")?;
    let slash = rest.find('/').ok_or("Invalid update URL.")?;
    let (host, path) = rest.split_at(slash);
    if !matches!(host, "api.github.com" | "github.com") || path.bytes().any(|byte| byte < 32) {
        return Err("Untrusted update URL.".into());
    }
    // SAFETY: every input string is terminated and remains live during the synchronous
    // call. TLS validation stays enabled; redirects may not downgrade HTTPS to HTTP.
    unsafe {
        let session = WinHttpOpen(
            wide(&format!(
                "MagicLanguageSwitcher/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        );
        if session.is_null() {
            return Err(failed("Could not initialize the update connection."));
        }
        let session = Internet(session);
        if WinHttpSetTimeouts(session.0, 5000, 5000, 15000, 15000) == 0 {
            return Err(failed("Could not set network timeouts."));
        }
        let connection = WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0);
        if connection.is_null() {
            return Err(failed("Could not connect to GitHub."));
        }
        let connection = Internet(connection);
        let request = WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return Err(failed("Could not request the update."));
        }
        let request = Internet(request);
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        if WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            &policy as *const _ as *const c_void,
            size_of::<u32>() as u32,
        ) == 0
        {
            return Err(failed("Could not secure the update connection."));
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("Update cancelled.".into());
        }
        let headers = wide(if host == "api.github.com" {
            "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
        } else {
            "Accept: application/octet-stream\r\n"
        });
        if WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0
        {
            return Err(failed("Could not download from GitHub."));
        }
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut _ as *mut c_void,
            &mut size,
            null_mut(),
        ) == 0
        {
            return Err(failed("Could not read GitHub's response."));
        }
        if status != 200 {
            return Err(match status {
                403 | 429 => "GitHub limited update requests. Try again later.".into(),
                404 => "No compatible release is available on GitHub yet.".into(),
                _ => format!("GitHub returned HTTP {status}. Try again later."),
            });
        }
        let mut buffer = [0u8; 65536];
        let mut total = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Update cancelled.".into());
            }
            let mut received = 0;
            if WinHttpReadData(
                request.0,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut received,
            ) == 0
            {
                return Err(failed("The update download was interrupted."));
            }
            if received == 0 {
                break;
            }
            total += received as u64;
            if total > limit {
                return Err("The update download exceeded its expected size.".into());
            }
            output
                .write_all(&buffer[..received as usize])
                .map_err(|error| format!("Could not save the update: {error}"))?;
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updates::{self, Version};
    use sha2::{Digest, Sha256};

    #[test]
    #[ignore = "downloads the latest public release from GitHub"]
    fn github_release_download_matches_both_checksums() {
        let cancel = AtomicBool::new(false);
        let json = bytes(updates::LATEST_API, 1024 * 1024, &cancel).unwrap();
        let update = updates::find_update(&json, Version(0, 0, 0))
            .unwrap()
            .unwrap();
        let checksum = bytes(&update.checksum_url, 65536, &cancel).unwrap();
        updates::verify_checksum_file(&checksum, &update).unwrap();
        let exe = bytes(&update.url, update.size, &cancel).unwrap();
        assert_eq!(exe.len() as u64, update.size);
        assert_eq!(format!("{:x}", Sha256::digest(&exe)), update.sha256);
        println!(
            "GitHub HTTPS and redirect download verified: v{}, {} bytes",
            update.version,
            exe.len()
        );
    }
}
