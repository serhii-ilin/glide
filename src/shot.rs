//! Render one page to a PNG without opening a window.
//!
//! Used by `glide --screenshot <url> <file.png>` and by the screenshot in the
//! README. The same `HtmlDocument` path the window uses, rasterized with
//! Vello's CPU renderer so it does not need a display.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use anyrender::{PaintScene as _, render_to_buffer};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_dom::util::Color;
use blitz_net::Provider;
use blitz_paint::paint_scene;
use blitz_traits::net::{NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use peniko::Fill;
use peniko::kurbo::Rect;

use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::{JsHost, JsTap};
use crate::parse;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 760;

pub fn render_url(url: &str, path: &Path) -> Result<(), String> {
    render_url_inner(url, path, true)
}

pub fn render_url_no_js(url: &str, path: &Path) -> Result<(), String> {
    render_url_inner(url, path, false)
}

fn render_url_inner(url: &str, path: &Path, js_enabled: bool) -> Result<(), String> {
    let net_impl = Arc::new(Provider::new(None));
    let handle = tokio::runtime::Handle::current();
    let (final_url, bytes) = if url.starts_with("file://") {
        let path = url.trim_start_matches("file://");
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        (url.to_string(), bytes)
    } else {
        let parsed = url::Url::parse(url).map_err(|err| err.to_string())?;
        let (final_url, bytes) = handle
            .block_on(net_impl.fetch_async(Request::get(parsed)))
            .map_err(|err| format!("fetch {url}: {err}"))?;
        (final_url, bytes.into())
    };
    let html = String::from_utf8_lossy(&bytes);

    let net_provider: Arc<dyn NetProvider> = net_impl.clone();
    let tap = if js_enabled {
        JsTap::new_enabled()
    } else {
        JsTap::new_disabled()
    };
    let html_provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(AtomicBool::new(js_enabled)),
        js_tap: tap.clone(),
    };
    let mut js_host = JsHost::new(js_enabled, tap, Some(net_impl.clone()));

    let config = DocumentConfig {
        base_url: Some(final_url.clone()),
        net_provider: Some(net_provider),
        html_parser_provider: Some(Arc::new(html_provider)),
        viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
        ..Default::default()
    };
    let mut document = HtmlPageDocument(parse::document_from_html(&html, config, js_enabled));

    let start = Instant::now();
    js_host.maybe_start_page(&document.0, &final_url, &handle);
    let mut js_quiet_ticks = 0_u32;
    loop {
        document.0.resolve(0.0);
        let js_redraw = js_host.pump(&mut document.0, Instant::now());
        if js_redraw {
            document.0.resolve(0.0);
            js_quiet_ticks = 0;
        } else if js_enabled {
            js_quiet_ticks = js_quiet_ticks.saturating_add(1);
        }
        let net_done = net_impl.is_empty();
        let js_done = !js_enabled || js_quiet_ticks > 20;
        let min_js = if js_enabled {
            Duration::from_secs(1)
        } else {
            Duration::ZERO
        };
        if (net_done && js_done && start.elapsed() >= min_js)
            || start.elapsed() > Duration::from_secs(12)
        {
            document.0.resolve(0.0);
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }

    if js_enabled {
        js_host.flush_dom_rpc(&mut document.0, Duration::from_secs(4));
        document.0.resolve(0.0);
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
            paint_scene(scene, &mut document.0, 1.0, WIDTH, HEIGHT, 0, 0);
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
