//! PNG pixel comparison for WPT reftests and Acid2 diff.

use std::path::Path;

const CHANNEL_TOLERANCE: u8 = 16;
/// Maximum fraction of pixels that may exceed [`CHANNEL_TOLERANCE`] per channel.
pub const MAX_DIFF_FRACTION: f64 = 0.001;

/// Decode a PNG file to RGBA8 pixels `(width, height, bytes)`.
pub fn decode_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let decoder = png::Decoder::new(std::io::Cursor::new(file));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let buf_len = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG output buffer size unknown".to_string())?;
    let mut buf = vec![0; buf_len];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let color_type = info.color_type;
    let width = info.width;
    let height = info.height;
    match color_type {
        png::ColorType::Rgba => Ok((width, height, buf)),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in buf.chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            Ok((width, height, rgba))
        }
        other => Err(format!("unsupported PNG color type: {other:?}")),
    }
}

fn channel_exceeds(a: u8, b: u8) -> bool {
    a.abs_diff(b) > CHANNEL_TOLERANCE
}

/// Compare two RGBA buffers of the same size; return `(diff_pixel_count, total_pixels)`.
pub fn diff_pixels(a: &[u8], b: &[u8]) -> Result<(u64, u64), String> {
    if a.len() != b.len() {
        return Err(format!(
            "buffer length mismatch: {} vs {}",
            a.len(),
            b.len()
        ));
    }
    if !a.len().is_multiple_of(4) {
        return Err("RGBA buffer length must be a multiple of 4".into());
    }
    let pixels = (a.len() / 4) as u64;
    let mut diff = 0_u64;
    for chunk in 0..a.len() / 4 {
        let i = chunk * 4;
        if channel_exceeds(a[i], b[i])
            || channel_exceeds(a[i + 1], b[i + 1])
            || channel_exceeds(a[i + 2], b[i + 2])
            || channel_exceeds(a[i + 3], b[i + 3])
        {
            diff += 1;
        }
    }
    Ok((diff, pixels))
}

/// Compare two PNG files; returns diff fraction in `[0, 1]`.
pub fn png_diff_fraction(left: &Path, right: &Path) -> Result<f64, String> {
    let (w1, h1, a) = decode_png(left)?;
    let (w2, h2, b) = decode_png(right)?;
    if w1 != w2 || h1 != h2 {
        return Err(format!("dimension mismatch: {w1}x{h1} vs {w2}x{h2}"));
    }
    let (diff, total) = diff_pixels(&a, &b)?;
    Ok(diff as f64 / total as f64)
}

/// True when at most [`MAX_DIFF_FRACTION`] of pixels exceed the channel tolerance.
pub fn reftest_images_match(left: &Path, right: &Path) -> Result<bool, String> {
    let frac = png_diff_fraction(left, right)?;
    Ok(frac <= MAX_DIFF_FRACTION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};

    fn write_rgba_png(path: &Path, w: u32, h: u32, pixels: &[u8]) {
        let parent = path.parent().expect("parent");
        std::fs::create_dir_all(parent).ok();
        let file = std::fs::File::create(path).expect("create png");
        let mut enc = Encoder::new(file, w, h);
        enc.set_color(ColorType::Rgba);
        enc.set_depth(BitDepth::Eight);
        let mut writer = enc.write_header().expect("header");
        writer.write_image_data(pixels).expect("data");
    }

    #[test]
    fn identical_images_pass() {
        let dir = std::env::temp_dir().join("glide_compat_png_identical");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        let pixels = vec![255_u8; 4 * 10 * 10];
        write_rgba_png(&a, 10, 10, &pixels);
        write_rgba_png(&b, 10, 10, &pixels);
        assert!(reftest_images_match(&a, &b).unwrap());
        let frac = png_diff_fraction(&a, &b).unwrap();
        assert!(frac <= MAX_DIFF_FRACTION);
    }

    #[test]
    fn one_pixel_over_threshold_counts() {
        let dir = std::env::temp_dir().join("glide_compat_png_diff");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        let pixels_a = vec![100_u8; 4 * 10 * 10];
        let mut pixels_b = pixels_a.clone();
        // One pixel differs by more than CHANNEL_TOLERANCE on R (1% of 10×10).
        pixels_b[0] = pixels_a[0].saturating_add(CHANNEL_TOLERANCE + 1);
        write_rgba_png(&a, 10, 10, &pixels_a);
        write_rgba_png(&b, 10, 10, &pixels_b);
        let frac = png_diff_fraction(&a, &b).unwrap();
        assert!((frac - 0.01).abs() < 1e-6);
        assert!(!reftest_images_match(&a, &b).unwrap());
    }
}
