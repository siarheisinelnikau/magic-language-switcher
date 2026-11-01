//! Bounded capture of the app-owned client area, using its real native paint path.
use std::{mem::size_of, path::Path};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    Storage::Xps::{PW_CLIENTONLY, PrintWindow},
    UI::WindowsAndMessaging::*,
};

pub fn save(window: HWND, path: &Path) -> Result<(), String> {
    // SAFETY: the host supplies its own window; all DCs and bitmaps are released locally.
    unsafe {
        RedrawWindow(
            window,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        let mut rect = RECT::default();
        if GetClientRect(window, &mut rect) == 0 {
            return Err("Could not read capture bounds.".into());
        }
        let (width, height) = (rect.right, rect.bottom);
        if width <= 0 || height <= 0 || width > 4096 || height > 4096 {
            return Err("Invalid capture bounds.".into());
        }
        let screen = GetDC(window);
        let dc = CreateCompatibleDC(screen);
        ReleaseDC(window, screen);
        if dc.is_null() {
            return Err("Could not allocate capture DC.".into());
        }
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut pixels = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            dc,
            &info,
            DIB_RGB_COLORS,
            &mut pixels,
            std::ptr::null_mut(),
            0,
        );
        if bitmap.is_null() {
            DeleteDC(dc);
            return Err("Could not allocate capture bitmap.".into());
        }
        let old = SelectObject(dc, bitmap);
        let result = if PrintWindow(window, dc, PW_CLIENTONLY) == 0 {
            Err("Native client capture failed.".into())
        } else {
            GdiFlush();
            let pixels =
                std::slice::from_raw_parts(pixels.cast::<u8>(), (width * height * 4) as usize);
            write_png(path, width as u32, height as u32, pixels)
        };
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        result
    }
}

// PNG with lossless stored DEFLATE blocks keeps this developer-only path dependency-free.
fn write_png(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<(), String> {
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for row in bgra.chunks_exact(width as usize * 4) {
        raw.push(0);
        for pixel in row.as_chunks::<4>().0 {
            raw.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    let mut compressed = vec![0x78, 0x01];
    let mut remaining = raw.as_slice();
    while !remaining.is_empty() {
        let count = remaining.len().min(65535);
        compressed.push(u8::from(count == remaining.len()));
        let count = count as u16;
        compressed.extend_from_slice(&count.to_le_bytes());
        compressed.extend_from_slice(&(!count).to_le_bytes());
        compressed.extend_from_slice(&remaining[..count as usize]);
        remaining = &remaining[count as usize..];
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    compressed.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::from(width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &compressed);
    chunk(&mut png, b"IEND", &[]);
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, png).map_err(|error| error.to_string())
}
fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let mut crc = !0u32;
    for byte in kind.iter().chain(data) {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
