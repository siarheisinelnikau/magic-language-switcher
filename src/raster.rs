//! Retained CPU pixels for the settings background. No graphics device or render loop.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Card {
    pub rect: [i32; 4],
    pub radius: i32,
    pub color: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scene {
    pub background: u32,
    pub cards: [Card; 2],
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    width: i32,
    height: i32,
    scale: u32,
    scene: Scene,
}

#[derive(Default)]
pub(crate) struct Frame {
    key: Option<Key>,
    pixels: Vec<u32>,
    #[cfg(any(test, feature = "desktop-tests"))]
    generations: u64,
}
impl Frame {
    /// None means an invalid or oversized surface; false reuses the existing pixels.
    pub fn prepare(&mut self, width: i32, height: i32, scale: f32, scene: Scene) -> Option<bool> {
        if width <= 0 || height <= 0 || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let count = (width as usize).checked_mul(height as usize)?;
        if count > 16 * 1024 * 1024 {
            return None;
        }
        let key = Key {
            width,
            height,
            scale: scale.to_bits(),
            scene,
        };
        if self.key == Some(key) {
            return Some(false);
        }
        self.pixels
            .try_reserve(count.saturating_sub(self.pixels.len()))
            .ok()?;
        self.pixels.resize(count, 0);
        self.pixels.fill(pixel(scene.background));
        for card in scene.cards {
            self.card(width, height, scale, card);
        }
        self.key = Some(key);
        #[cfg(any(test, feature = "desktop-tests"))]
        {
            self.generations += 1;
        }
        Some(true)
    }
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
    #[cfg(any(test, feature = "desktop-tests"))]
    pub fn generations(&self) -> u64 {
        self.generations
    }

    fn card(&mut self, width: i32, height: i32, scale: f32, card: Card) {
        let edge = |value: i32| (value as f32 * scale).round();
        let [x, y, w, h] = card.rect;
        let left = edge(x);
        let top = edge(y);
        let right = edge(x + w);
        let bottom = edge(y + h);
        let half_w = (right - left) * 0.5;
        let half_h = (bottom - top) * 0.5;
        let radius = (card.radius as f32 * scale)
            .min(half_w)
            .min(half_h)
            .max(0.0);
        let center_x = (left + right) * 0.5;
        let center_y = (top + bottom) * 0.5;
        let color = pixel(card.color);
        for row in (top as i32).max(0)..(bottom as i32).min(height) {
            let start = (left as i32).max(0);
            let end = (right as i32).min(width);
            if start >= end {
                continue;
            }
            let offset = row as usize * width as usize;
            let qy = ((row as f32 + 0.5) - center_y).abs() - (half_h - radius);
            if qy <= 0.0 {
                self.pixels[offset + start as usize..offset + end as usize].fill(color);
                continue;
            }
            for column in start..end {
                let qx = ((column as f32 + 0.5) - center_x).abs() - (half_w - radius);
                let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                let index = offset + column as usize;
                self.pixels[index] = blend(self.pixels[index], color, coverage);
            }
        }
    }
}
// COLORREF is 0x00BBGGRR; a little-endian, 32-bit top-down DIB stores BGRA pixels.
fn pixel(color: u32) -> u32 {
    0xff000000 | ((color & 0xff) << 16) | (color & 0xff00) | ((color >> 16) & 0xff)
}
fn blend(background: u32, foreground: u32, coverage: f32) -> u32 {
    let channel = |shift: u32| {
        let old = ((background >> shift) & 255u32) as f32;
        let new = ((foreground >> shift) & 255u32) as f32;
        ((old + (new - old) * coverage).round() as u32) << shift
    };
    0xff000000 | channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Scene {
        Scene {
            background: 0x00ffffff,
            cards: [
                Card {
                    rect: [4, 4, 20, 20],
                    radius: 5,
                    color: 0x000000ff,
                },
                Card {
                    rect: [28, 4, 20, 20],
                    radius: 5,
                    color: 0x0000ff00,
                },
            ],
        }
    }
    #[test]
    fn unchanged_exposes_reuse_the_frame_but_size_dpi_and_theme_rebuild_it() {
        let mut frame = Frame::default();
        assert_eq!(frame.prepare(52, 28, 1.0, scene()), Some(true));
        let first = frame.pixels().as_ptr();
        for _ in 0..100 {
            assert_eq!(frame.prepare(52, 28, 1.0, scene()), Some(false));
        }
        assert_eq!(frame.pixels().as_ptr(), first);
        assert_eq!(frame.generations(), 1);
        assert_eq!(frame.prepare(65, 35, 1.25, scene()), Some(true));
        assert_eq!(frame.prepare(65, 35, 1.5, scene()), Some(true));
        let mut changed = scene();
        changed.background = 0;
        assert_eq!(frame.prepare(65, 35, 1.5, changed), Some(true));
        assert_eq!(frame.generations(), 4);
    }
    #[test]
    fn pixels_keep_color_order_round_corners_and_antialiasing_at_common_dpis() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut frame = Frame::default();
            let width = (52.0 * scale) as usize;
            frame
                .prepare(width as i32, (28.0 * scale) as i32, scale, scene())
                .unwrap();
            let at = |x: f32, y: f32| {
                frame.pixels()[(y * scale) as usize * width + (x * scale) as usize]
            };
            assert_eq!(at(0.0, 0.0), 0xffffffff);
            assert_eq!(at(12.0, 12.0), 0xffff0000);
            assert_eq!(at(36.0, 12.0), 0xff00ff00);
            assert_eq!(at(4.0, 4.0), 0xffffffff);
            assert!(
                frame.pixels().iter().any(|&value| value != 0xffffffff
                    && value != 0xffff0000
                    && value != 0xff00ff00)
            );
        }
    }
    #[test]
    fn invalid_sizes_do_not_replace_a_good_frame() {
        let mut frame = Frame::default();
        frame.prepare(52, 28, 1.0, scene()).unwrap();
        let original = frame.pixels().to_vec();
        for (width, height, scale) in [
            (0, 10, 1.0),
            (-1, 10, 1.0),
            (i32::MAX, i32::MAX, 1.0),
            (10, 10, f32::NAN),
        ] {
            assert_eq!(frame.prepare(width, height, scale, scene()), None);
        }
        assert_eq!(frame.pixels(), original);
        assert_eq!(frame.generations(), 1);
    }
}
