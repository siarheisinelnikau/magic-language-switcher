use std::{
    mem::size_of,
    ptr::{copy_nonoverlapping, null_mut},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HANDLE, HWND},
    Graphics::Gdi::{DeleteEnhMetaFile, DeleteMetaFile, DeleteObject},
    System::{DataExchange::*, Memory::*, Ole::*},
};

struct OpenClipboardGuard;
impl OpenClipboardGuard {
    fn open(owner: HWND) -> Result<Self, String> {
        for _ in 0..10 {
            // SAFETY: the caller supplies a live owner window or null for a read operation.
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(15));
        }
        Err("The clipboard is busy. Try again in a moment.".into())
    }
}
impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: this guard is constructed only after a successful OpenClipboard on this thread.
        unsafe {
            CloseClipboard();
        }
    }
}

struct Format {
    id: u32,
    handle: HANDLE,
}
impl Drop for Format {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        // SAFETY: OleDuplicateData transfers ownership of the duplicate to us. Each format
        // requires its corresponding Win32 destructor until SetClipboardData takes ownership.
        unsafe {
            match self.id as u16 {
                CF_BITMAP | CF_DSPBITMAP | CF_PALETTE => {
                    DeleteObject(self.handle);
                }
                CF_ENHMETAFILE | CF_DSPENHMETAFILE => {
                    DeleteEnhMetaFile(self.handle);
                }
                CF_METAFILEPICT | CF_DSPMETAFILEPICT => {
                    let pointer = GlobalLock(self.handle);
                    if !pointer.is_null() {
                        if GlobalSize(self.handle) >= size_of::<METAFILEPICT>() {
                            DeleteMetaFile((*(pointer as *const METAFILEPICT)).hMF);
                        }
                        GlobalUnlock(self.handle);
                    }
                    GlobalFree(self.handle);
                }
                _ => {
                    GlobalFree(self.handle);
                }
            }
        }
    }
}

/// Eagerly duplicates every clipboard format before sending Ctrl+C. The clipboard is
/// never left open while an editor handles input. Newer clipboard values take priority.
pub(super) struct Session {
    formats: Vec<Format>,
    owner: usize,
    owned_sequence: Option<u32>,
}

impl Session {
    pub fn capture(owner: usize) -> Result<Self, String> {
        let _guard = OpenClipboardGuard::open(null_mut())?;
        let mut session = Self {
            formats: Vec::new(),
            owner,
            owned_sequence: None,
        };
        // SAFETY: the clipboard remains open throughout enumeration and duplication.
        unsafe {
            let mut id = EnumClipboardFormats(0);
            while id != 0 {
                if (CF_PRIVATEFIRST as u32..=CF_GDIOBJLAST as u32).contains(&id)
                    || id == CF_OWNERDISPLAY as u32
                {
                    return Err("The clipboard contains an app-specific format that cannot be preserved. Copy plain text first.".into());
                }
                let data = GetClipboardData(id);
                if data.is_null() {
                    return Err(
                        "Could not preserve the current clipboard. Try copying it again.".into(),
                    );
                }
                if GlobalSize(data) > 64 * 1024 * 1024 {
                    return Err(
                        "The clipboard is too large to preserve. Copy a smaller item first.".into(),
                    );
                }
                let handle = OleDuplicateData(data, id as u16, GMEM_MOVEABLE);
                if handle.is_null() {
                    return Err(
                        "Could not preserve the current clipboard format. Copy plain text first."
                            .into(),
                    );
                }
                session.formats.push(Format { id, handle });
                id = EnumClipboardFormats(id);
            }
        }
        Ok(session)
    }

    pub fn copy_selection(&mut self) -> Result<String, String> {
        let previous = sequence();
        super::native::chord(0x43, &[0x11])?;
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(15));
            let current = sequence();
            if current == previous {
                continue;
            }
            self.owned_sequence = Some(current);
            match read_text() {
                Ok(text) => return Ok(text),
                Err(_) => continue,
            }
        }
        Err("No text to correct. Place the caret after a word, or select text in an editor.".into())
    }

    pub fn paste(&mut self, text: &str) -> Result<(), String> {
        write_text(self.owner, text)?;
        self.owned_sequence = Some(sequence());
        super::native::chord(0x56, &[0x11])?;
        thread::sleep(Duration::from_millis(300));
        Ok(())
    }

    pub fn restore(&mut self) -> Result<(), String> {
        let Some(expected) = self.owned_sequence else {
            return Ok(());
        };
        if sequence() != expected {
            self.owned_sequence = None;
            return Ok(());
        }
        let _guard = OpenClipboardGuard::open(self.owner as HWND)?;
        if sequence() != expected {
            self.owned_sequence = None;
            return Ok(());
        }
        // SAFETY: the clipboard is open, and every Format owns a valid duplicated handle.
        unsafe {
            if EmptyClipboard() == 0 {
                return Err("Windows could not restore the clipboard.".into());
            }
            let mut failed = false;
            for format in &mut self.formats {
                if SetClipboardData(format.id, format.handle).is_null() {
                    failed = true;
                } else {
                    format.handle = null_mut();
                }
            }
            self.owned_sequence = None;
            if failed {
                return Err("Some clipboard formats could not be restored.".into());
            }
        }
        Ok(())
    }

    #[cfg(feature = "desktop-tests")]
    pub fn restore_current(&mut self) -> Result<(), String> {
        self.owned_sequence = Some(sequence());
        self.restore()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(super) fn sequence() -> u32 {
    // SAFETY: this is an atomic OS-owned counter with no pointer arguments.
    unsafe { GetClipboardSequenceNumber() }
}

pub(super) fn read_text() -> Result<String, String> {
    let _guard = OpenClipboardGuard::open(null_mut())?;
    // SAFETY: the clipboard is open; size bounds the UTF-16 slice, and the lock is released
    // before closing the clipboard. Untrusted clipboard strings are decoded lossily.
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT as u32);
        if handle.is_null() {
            return Err("The selection does not contain plain text.".into());
        }
        let length = GlobalSize(handle) / 2;
        if length > 1024 * 1024 {
            return Err("Select up to 4,096 characters at a time.".into());
        }
        let data = GlobalLock(handle) as *const u16;
        if data.is_null() {
            return Err("Could not read the selected text.".into());
        }
        let units = std::slice::from_raw_parts(data, length);
        let end = units.iter().position(|unit| *unit == 0).unwrap_or(length);
        let result = String::from_utf16_lossy(&units[..end]);
        GlobalUnlock(handle);
        Ok(result)
    }
}

pub(super) fn write_text(owner: usize, text: &str) -> Result<(), String> {
    let text = super::native::wide(text);
    // SAFETY: the allocation has exactly enough space for the NUL-terminated UTF-16 buffer.
    // On successful SetClipboardData Windows takes ownership; all error paths free it.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2);
        if memory.is_null() {
            return Err("Could not allocate clipboard memory.".into());
        }
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            GlobalFree(memory);
            return Err("Could not lock clipboard memory.".into());
        }
        copy_nonoverlapping(text.as_ptr(), pointer as *mut u16, text.len());
        GlobalUnlock(memory);
        let guard = OpenClipboardGuard::open(owner as HWND);
        if let Err(error) = guard {
            GlobalFree(memory);
            return Err(error);
        }
        let _guard = guard?;
        if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT as u32, memory).is_null() {
            GlobalFree(memory);
            return Err("Could not write the clipboard.".into());
        }
    }
    Ok(())
}
