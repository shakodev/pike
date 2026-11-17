//! A terminal-emitted color nonce proves that a readback contains the new scene.
//! Space-cell backgrounds avoid platform/font-dependent glyph OCR.
use std::io::{self, Write};
const A: [u8; 3] = [18, 69, 185];
const B: [u8; 3] = [231, 130, 19];
const ZERO: [u8; 3] = [35, 54, 73];
const ONE: [u8; 3] = [145, 196, 231];

fn colors(nonce: usize, child_pid: u32) -> Vec<[u8; 3]> {
    let mut colors = vec![A, B, A];
    let payload = (u64::from(child_pid) << 8) | nonce as u64;
    let alphabet = [ZERO, ONE, [199, 54, 161], [62, 189, 77]];
    for digit in 0..20 {
        colors.push(alphabet[((payload >> (digit * 2)) & 3) as usize]);
    }
    colors.extend([B, A, B]);
    colors
}

pub fn write_marker(out: &mut impl Write, nonce: usize, child_pid: u32) -> io::Result<()> {
    for [r, g, b] in colors(nonce, child_pid) {
        write!(out, "\x1b[48;2;{r};{g};{b}m ")?;
    }
    write!(out, "\x1b[0m")
}

fn near(pixel: &[u8], color: [u8; 3]) -> bool {
    pixel
        .iter()
        .zip(color)
        .all(|(actual, expected)| actual.abs_diff(expected) <= 3)
}

pub fn locate(
    shot: &iced::window::Screenshot,
    nonce: usize,
    child_pid: u32,
) -> Option<iced::Rectangle<u32>> {
    let width = shot.size.width as usize;
    let height = shot.size.height as usize;
    if nonce > 200 || width < 44 || height == 0 || shot.rgba.len() != width * height * 4 {
        return None;
    }
    let expected = colors(nonce, child_pid);
    // Scan complete native screenshot rows, bounded by the captured window.
    // A 14-DIP space-cell background is taller than this four-pixel stride.
    for y in (0..height).step_by(4) {
        let row = &shot.rgba[y * width * 4..(y + 1) * width * 4];
        let pixel = |x: usize| &row[x * 4..x * 4 + 3];
        let mut x = 0;
        while x < width {
            if !near(pixel(x), A) {
                x += 1;
                continue;
            }
            let start = x;
            let mut starts = [0usize; 3];
            let mut lengths = [0usize; 3];
            for (index, color) in [A, B, A].into_iter().enumerate() {
                // Native subpixel cell edges can blend one or two pixels.
                for _ in 0..2 {
                    if x < width && !near(pixel(x), color) {
                        x += 1;
                    }
                }
                starts[index] = x;
                while x < width && near(pixel(x), color) {
                    lengths[index] += 1;
                    x += 1;
                }
            }
            if lengths.iter().any(|n| *n < 2) {
                x = x.max(start + 1);
                continue;
            }
            let pitch = (starts[2] - starts[0]) as f64 / 2.0;
            // Infer fractional device-pixel pitch from the real prefix. Search
            // a small bounded range; sample cell centers, never blended edges.
            for offset in -12..=12 {
                let cell = pitch + f64::from(offset) / 16.0;
                if cell < 2.0 {
                    continue;
                }
                if expected.iter().enumerate().all(|(index, color)| {
                    let center = start + ((index as f64 + 0.5) * cell) as usize;
                    center < width && near(pixel(center), *color)
                }) {
                    let x = start as u32;
                    let y = y.saturating_sub(2) as u32;
                    return Some(iced::Rectangle {
                        x,
                        y,
                        width: ((cell * expected.len() as f64).ceil() as u32 + 2)
                            .min(shot.size.width - x),
                        height: 8.min(shot.size.height - y),
                    });
                }
            }
            x = x.max(start + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contains(image: &iced::window::Screenshot, nonce: usize) -> bool {
        locate(image, nonce, 12345).is_some()
    }
    fn shot(nonce: usize, cell: usize) -> iced::window::Screenshot {
        let width = cell * 28;
        let mut pixels = vec![0; width * 8 * 4];
        for y in 0..8 {
            for (index, color) in colors(nonce, 12345).iter().enumerate() {
                for x in (index + 1) * cell..(index + 2) * cell {
                    pixels[(y * width + x) * 4..(y * width + x) * 4 + 3].copy_from_slice(color);
                    pixels[(y * width + x) * 4 + 3] = 255;
                }
            }
        }
        iced::window::Screenshot::new(pixels, iced::Size::new(width as u32, 8), 1.0)
    }
    #[test]
    fn actual_nonce_accepts_multiple_cell_sizes_and_rejects_old_scene() {
        for cell in [3, 7, 19] {
            let image = shot(37, cell);
            assert!(contains(&image, 37));
            assert!(locate(&image, 37, 12346).is_none());
            assert!(!contains(&image, 36));
            assert!(!contains(&image, 0));
        }
    }
    #[test]
    fn blank_or_wrong_nonce_images_cannot_be_valid_latency_evidence() {
        let blank = iced::window::Screenshot::new(vec![0; 16], iced::Size::new(2, 2), 1.0);
        assert!(!contains(&blank, 1));
        assert!(!contains(&shot(0, 7), 1));
        assert!(!contains(&shot(1, 7), 201));
    }
    #[test]
    fn fractional_native_cell_edges_are_decoded_from_centers() {
        // Native 2× macOS readback has 16-pixel pure spans separated by
        // one blended edge pixel, with occasional 16-pixel pitches.
        let width = 460usize;
        let mut rgba = vec![0; width * 8 * 4];
        for y in 0..8 {
            for (index, color) in colors(1, 12345).iter().enumerate() {
                let left = 7 + (index as f64 * 16.85).round() as usize;
                let right = 7 + ((index + 1) as f64 * 16.85).round() as usize;
                for x in left..right {
                    let value = if x + 1 == right {
                        [82, 113, 137]
                    } else {
                        *color
                    };
                    rgba[(y * width + x) * 4..(y * width + x) * 4 + 3].copy_from_slice(&value);
                    rgba[(y * width + x) * 4 + 3] = 255;
                }
            }
        }
        let image = iced::window::Screenshot::new(rgba, iced::Size::new(width as u32, 8), 2.0);
        assert!(contains(&image, 1));
        assert!(!contains(&image, 2));
    }
}
