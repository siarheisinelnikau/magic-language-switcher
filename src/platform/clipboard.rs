use std::{
    mem::size_of,
    ptr::{copy_nonoverlapping, null_mut},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HANDLE, HWND},
    Graphics::Gdi::{CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject},
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
        // SAFETY: duplicate_data transfers ownership of the duplicate to us. Each format
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

/// Eagerly duplicates payload formats before sending Ctrl+C. OLE's owner-specific
/// transport bookkeeping is excluded; its advertised payloads are copied separately.
/// The clipboard is
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
                let mut name = [0u16; 256];
                let length = GetClipboardFormatNameW(id, name.as_mut_ptr(), name.len() as i32)
                    .max(0) as usize;
                if matches!(
                    String::from_utf16_lossy(&name[..length]).as_str(),
                    "DataObject" | "Ole Private Data"
                ) {
                    id = EnumClipboardFormats(id);
                    continue;
                }
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
                let handle = duplicate_data(data, id);
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
        self.try_copy_selection()?.ok_or_else(|| {
            "No text to correct. Place the caret after a word, or select text in an editor.".into()
        })
    }

    pub fn try_copy_selection(&mut self) -> Result<Option<String>, String> {
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
                Ok(text) => return Ok(Some(text)),
                Err(_) => continue,
            }
        }
        if sequence() == previous {
            Ok(None)
        } else {
            Err("Could not read the copied text. Nothing was changed.".into())
        }
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

fn duplicate_data(data: HANDLE, id: u32) -> HANDLE {
    // SAFETY: capture holds the clipboard open. GDI formats need their matching clone
    // API; other payloads are copied only within the size of their locked HGLOBAL.
    unsafe {
        match id as u16 {
            CF_BITMAP | CF_DSPBITMAP => OleDuplicateData(data, CF_BITMAP, GMEM_MOVEABLE),
            CF_PALETTE => OleDuplicateData(data, CF_PALETTE, GMEM_MOVEABLE),
            CF_METAFILEPICT | CF_DSPMETAFILEPICT => {
                OleDuplicateData(data, CF_METAFILEPICT, GMEM_MOVEABLE)
            }
            CF_ENHMETAFILE | CF_DSPENHMETAFILE => CopyEnhMetaFileW(data, std::ptr::null()),
            _ => {
                let size = GlobalSize(data);
                if size == 0 || size > 64 * 1024 * 1024 {
                    return null_mut();
                }
                let source = GlobalLock(data);
                if source.is_null() {
                    return null_mut();
                }
                let copy = GlobalAlloc(GMEM_MOVEABLE, size);
                if copy.is_null() {
                    GlobalUnlock(data);
                    return null_mut();
                }
                let destination = GlobalLock(copy);
                if destination.is_null() {
                    GlobalUnlock(data);
                    GlobalFree(copy);
                    return null_mut();
                }
                copy_nonoverlapping(source as *const u8, destination as *mut u8, size);
                GlobalUnlock(copy);
                GlobalUnlock(data);
                copy
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{CloseEnhMetaFile, CreateBitmap, CreateEnhMetaFileW};

    #[test]
    fn clones_bitmap_and_enhanced_metafile_with_gdi_apis() {
        // SAFETY: the test creates and destroys its own GDI objects without touching the clipboard.
        unsafe {
            let bitmap = CreateBitmap(2, 2, 1, 1, [0u8; 4].as_ptr() as *const _);
            assert!(!bitmap.is_null());
            let copy = Format {
                id: CF_BITMAP as u32,
                handle: duplicate_data(bitmap, CF_BITMAP as u32),
            };
            assert!(!copy.handle.is_null());
            assert_ne!(copy.handle, bitmap);
            DeleteObject(bitmap);
            drop(copy);
            let dc = CreateEnhMetaFileW(
                null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            );
            assert!(!dc.is_null());
            let metafile = CloseEnhMetaFile(dc);
            assert!(!metafile.is_null());
            let copy = Format {
                id: CF_ENHMETAFILE as u32,
                handle: duplicate_data(metafile, CF_ENHMETAFILE as u32),
            };
            assert!(!copy.handle.is_null());
            assert_ne!(copy.handle, metafile);
            DeleteEnhMetaFile(metafile);
            drop(copy);
        }
    }
}
