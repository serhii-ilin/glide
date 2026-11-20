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

    let buffer = render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| {
            scene.fill(
                Fill::NonZero,
                Default::default(),
                Color::WHITE,
                None,
                &Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f64, VIEWPORT_HEIGHT as f64),
            );
            paint_scene(
                scene,
                &mut document.0,
                1.0,
                VIEWPORT_WIDTH,
                VIEWPORT_HEIGHT,
                0,
                0,
            );
        },
        VIEWPORT_WIDTH,
        VIEWPORT_HEIGHT,
    );

    let file = std::fs::File::create(path).map_err(|err| err.to_string())?;
    let mut encoder = png::Encoder::new(file, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
    writer
        .write_image_data(&buffer)
        .map_err(|err| err.to_string())?;
    Ok(())
}
