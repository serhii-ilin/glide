//! Render one page to a PNG without opening a window.
//!
//! Used by `glide --screenshot <url> <file.png>` and by the screenshot in the
//! README. The same `HtmlDocument` path the window uses, rasterized with
//! Vello's CPU renderer so it does not need a display.

use std::path::Path;

use anyrender::{PaintScene as _, render_to_buffer};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::util::Color;
use blitz_paint::paint_scene;
use peniko::Fill;
use peniko::kurbo::Rect;

use crate::headless::{VIEWPORT_HEIGHT, VIEWPORT_WIDTH, load};

pub fn render_url(url: &str, path: &Path) -> Result<(), String> {
    render_url_inner(url, path, true)
}

pub fn render_url_no_js(url: &str, path: &Path) -> Result<(), String> {
    render_url_inner(url, path, false)
}

fn render_url_inner(url: &str, path: &Path, js_enabled: bool) -> Result<(), String> {
    let mut document = load(url, js_enabled)?;
    let png_bytes = encode_png(
        &paint_rgba(&mut document.0, VIEWPORT_WIDTH, VIEWPORT_HEIGHT),
        VIEWPORT_WIDTH,
        VIEWPORT_HEIGHT,
    )?;
    std::fs::write(path, png_bytes).map_err(|err| err.to_string())?;
    Ok(())
}

/// Rasterize a laid-out document to straight RGBA8 pixels (`width * height * 4` bytes).
pub fn paint_rgba(doc: &mut blitz_dom::BaseDocument, width: u32, height: u32) -> Vec<u8> {
    render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| {
            scene.fill(
                Fill::NonZero,
                Default::default(),
                Color::WHITE,
                None,
                &Rect::new(0.0, 0.0, width as f64, height as f64),
            );
            paint_scene(scene, doc, 1.0, width, height, 0, 0);
        },
        width,
        height,
    )
}

/// Wrap an RGBA8 buffer in a PNG without writing to disk.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
    writer
        .write_image_data(rgba)
        .map_err(|err| err.to_string())?;
    writer.finish().map_err(|err| err.to_string())?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::DocumentConfig;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use crate::parse;

    #[test]
    fn encode_png_roundtrip() {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(64, 64, 1.0, ColorScheme::Light)),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(
            "<!DOCTYPE html><html><body><p>PNG</p></body></html>",
            config,
            false,
        );
        doc.resolve(0.0);
        let rgba = paint_rgba(&mut doc, 64, 64);
        let bytes = encode_png(&rgba, 64, 64).expect("encode");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let reader = decoder.read_info().expect("decode");
        assert_eq!(reader.info().width, 64);
        assert_eq!(reader.info().height, 64);
    }
}
