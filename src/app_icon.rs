//! Owns the embedded Glide app icon and applies it to the platform: the window icon (Windows,
//! Linux) and the Dock icon when running the bare binary on macOS (F-079).

use winit::icon::{Icon, RgbaIcon};

use crate::logging;

/// 256 px render for window icons.
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../assets/icon/glide-256.png");
/// 1024 px render for the macOS Dock icon.
#[cfg(target_os = "macos")]
const DOCK_ICON_PNG: &[u8] = include_bytes!("../assets/icon/glide-1024.png");

/// Decode a PNG of any colour type or bit depth to RGBA8; returns `(pixels, width, height)`.
fn decode_png_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
    };
    Ok((rgba, info.width, info.height))
}

/// The window icon, or `None` (with one warning) if the embedded PNG cannot be used.
pub fn window_icon() -> Option<Icon> {
    let built = decode_png_rgba(WINDOW_ICON_PNG)
        .and_then(|(rgba, w, h)| RgbaIcon::new(rgba, w, h).map_err(|e| e.to_string()));
    match built {
        Ok(icon) => Some(icon.into()),
        Err(detail) => {
            warn_unavailable("window_icon", detail);
            None
        }
    }
}

fn warn_unavailable(error_kind: &str, detail: String) {
    logging::warn(
        "app.icon",
        "app icon unavailable",
        &[
            ("feature", "F-079".into()),
            ("error_kind", error_kind.into()),
            ("detail", detail),
            (
                "hint",
                "The window keeps the default icon; see F-079 in docs/features.md.".into(),
            ),
        ],
    );
}

/// Set the Dock icon. Must run on the main thread after the application has launched.
#[cfg(target_os = "macos")]
pub fn install_dock_icon() {
    use objc2::AnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        warn_unavailable("dock_icon", "not on the main thread".into());
        return;
    };
    let data = NSData::with_bytes(DOCK_ICON_PNG);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        warn_unavailable("dock_icon", "NSImage rejected the embedded PNG".into());
        return;
    };
    // SAFETY: `Some(&image)` is a valid NSImage; the only documented hazard is passing `None`,
    // which this call never does.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLIST_TEMPLATE: &str = include_str!("../assets/macos/Info.plist.in");

    // Basis: Apple Info.plist keys (CFBundleName/CFBundleIconFile), F-079.
    #[test]
    fn embedded_window_icon_decodes_to_256_square_rgba() {
        let (rgba, w, h) = decode_png_rgba(WINDOW_ICON_PNG).expect("embedded icon decodes");
        assert_eq!((w, h), (256, 256));
        assert_eq!(rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn decode_png_rgba_rejects_bytes_that_are_not_a_png() {
        assert!(decode_png_rgba(b"not a png").is_err());
    }

    #[test]
    fn decode_png_rgba_expands_rgb_and_palette_images() {
        let rgb = encode(png::ColorType::Rgb, None, &[10, 20, 30]);
        assert_eq!(decode_png_rgba(&rgb).unwrap().0, vec![10, 20, 30, 255]);
        let palette = encode(png::ColorType::Indexed, Some(vec![1, 2, 3]), &[0]);
        assert_eq!(decode_png_rgba(&palette).unwrap().0, vec![1, 2, 3, 255]);
    }

    fn encode(color: png::ColorType, palette: Option<Vec<u8>>, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 1, 1);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        if let Some(p) = palette {
            enc.set_palette(p);
        }
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn info_plist_template_names_the_app_and_icon_with_version_placeholder() {
        assert!(PLIST_TEMPLATE.contains("<key>CFBundleName</key>\n    <string>Glide</string>"));
        assert!(
            PLIST_TEMPLATE.contains("<key>CFBundleIconFile</key>\n    <string>AppIcon</string>")
        );
        assert!(PLIST_TEMPLATE.contains("@VERSION@"));
    }

    #[test]
    fn info_plist_template_is_balanced_xml() {
        assert!(PLIST_TEMPLATE.starts_with("<?xml"));
        for tag in ["plist", "dict"] {
            let open = PLIST_TEMPLATE.matches(&format!("<{tag}")).count();
            let close = PLIST_TEMPLATE.matches(&format!("</{tag}>")).count();
            assert_eq!(open, close, "unbalanced <{tag}>");
        }
        let keys = PLIST_TEMPLATE.matches("<key>").count();
        assert_eq!(keys, PLIST_TEMPLATE.matches("</key>").count());
    }
}
