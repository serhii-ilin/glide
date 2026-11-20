//! Render one page to a PNG without opening a window.
//!
//! Used by `glide --screenshot <url> <file.png>` and by the screenshot in the
//! README. The same `HtmlDocument` path the window uses, rasterized with
//! Vello's CPU renderer so it does not need a display.

use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyrender::{PaintScene as _, render_to_buffer};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_dom::util::Color;
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_net::Provider;
use blitz_paint::paint_scene;
use blitz_traits::net::{NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use peniko::Fill;
use peniko::kurbo::Rect;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 760;

pub fn render_url(url: &str, path: &Path) -> Result<(), String> {
    let net = Arc::new(Provider::new(None));
    let handle = tokio::runtime::Handle::current();
    let parsed = url::Url::parse(url).map_err(|err| err.to_string())?;
    let (final_url, bytes) = handle
        .block_on(net.fetch_async(Request::get(parsed)))
        .map_err(|err| format!("fetch {url}: {err}"))?;
    let html = String::from_utf8_lossy(&bytes);

    let net_provider: Arc<dyn NetProvider> = net.clone();
    let mut document = HtmlDocument::from_html(
        &html,
        DocumentConfig {
            base_url: Some(final_url),
            net_provider: Some(net_provider),
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
            ..Default::default()
        },
    );

    let start = Instant::now();
    loop {
        document.resolve(0.0);
        if net.is_empty() || start.elapsed() > Duration::from_secs(12) {
            document.resolve(0.0);
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }

    let buffer = render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| {
            scene.fill(
                Fill::NonZero,
                Default::default(),
                Color::WHITE,
                None,
                &Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64),
            );
            paint_scene(scene, &mut document, 1.0, WIDTH, HEIGHT, 0, 0);
        },
        WIDTH,
        HEIGHT,
    );

    let file = std::fs::File::create(path).map_err(|err| err.to_string())?;
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
    writer
        .write_image_data(&buffer)
        .map_err(|err| err.to_string())?;
    Ok(())
}
