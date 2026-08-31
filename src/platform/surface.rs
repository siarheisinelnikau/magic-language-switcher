//! A persistent, CPU-addressable DIB. GDI only presents its invalid rectangle.
use std::{
    mem::size_of,
    ptr::{copy_nonoverlapping, null_mut},
};
use windows_sys::Win32::Graphics::Gdi::*;

pub(super) struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    width: i32,
    height: i32,
}
impl Surface {
    pub fn new(target: HDC, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: a top-down 32-bit DIB has width*height pixels. This guard retains
        // its memory and DC until all presentations finish, then releases both.
        unsafe {
            let dc = CreateCompatibleDC(target);
            if dc.is_null() {
                return None;
            }
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(target, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap);
            if previous.is_null() || previous as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(dc);
                return None;
            }
            Some(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }
    pub fn matches(&self, width: i32, height: i32) -> bool {
        self.width == width && self.height == height
    }
    pub fn upload(&mut self, pixels: &[u32]) {
        if pixels.len() != self.width as usize * self.height as usize {
            return;
        }
        // SAFETY: new allocated exactly this many pixels; the bitmap is not painted
        // or modified from any other thread. Flush before rewriting a prior GDI source.
        unsafe {
            GdiFlush();
            copy_nonoverlapping(pixels.as_ptr(), self.bits, pixels.len());
        }
    }
    pub fn present(&self, target: HDC, dirty: windows_sys::Win32::Foundation::RECT) -> bool {
        // SAFETY: both DCs remain live; BeginPaint clips the target to its update region.
        unsafe {
            BitBlt(
                target,
                dirty.left,
                dirty.top,
                dirty.right - dirty.left,
                dirty.bottom - dirty.top,
                self.dc,
                dirty.left,
                dirty.top,
                SRCCOPY,
            ) != 0
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: restore the DC's original bitmap before deleting the owned DIB and DC.
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::RECT;

    #[test]
    fn presents_only_dirty_pixels_with_correct_colors_and_top_down_rows() {
        let mut source = Surface::new(null_mut(), 4, 4).unwrap();
        let mut target = Surface::new(null_mut(), 4, 4).unwrap();
        source.upload(&[
            0xffff0000, 0xffff0000, 0xffff0000, 0xffff0000, 0xffff0000, 0xffff0000, 0xffff0000,
            0xffff0000, 0xff00ff00, 0xff00ff00, 0xff00ff00, 0xff00ff00, 0xff00ff00, 0xff00ff00,
            0xff00ff00, 0xff00ff00,
        ]);
        target.upload(&[0xffffffff; 16]);
        assert!(source.present(
            target.dc,
            RECT {
                left: 1,
                top: 1,
                right: 3,
                bottom: 3
            }
        ));
        // SAFETY: both DCs are test-owned, selected into their live 4x4 DIBs.
        unsafe {
            GdiFlush();
            assert_eq!(GetPixel(target.dc, 1, 1), 0x000000ff);
            assert_eq!(GetPixel(target.dc, 1, 2), 0x0000ff00);
            assert_eq!(GetPixel(target.dc, 0, 0), 0x00ffffff);
            assert_eq!(GetPixel(target.dc, 3, 3), 0x00ffffff);
        }
    }
}
