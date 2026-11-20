//! Headless page load: fetch, parse, settle network and JavaScript (no window).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, DocumentConfig, local_name};
use blitz_net::Provider;
use blitz_traits::net::{NetProvider, Request};
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::{JsHost, JsTap};
use crate::parse;

pub const VIEWPORT_WIDTH: u32 = 1100;
pub const VIEWPORT_HEIGHT: u32 = 760;

/// Load a page headlessly and wait until subresources and scripts (when enabled) settle.
pub fn load(url: &str, js_enabled: bool) -> Result<HtmlPageDocument, String> {
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
        viewport: Some(Viewport::new(
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
            1.0,
            ColorScheme::Light,
        )),
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

    Ok(document)
}

/// Collapse whitespace in each line and drop empty lines (one block of text per line).
pub fn format_dump_text(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let raw = body_text_content(doc, scripting_enabled);
    raw.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn body_text_content(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let Some(body) = doc.find_body_node() else {
        return String::new();
    };
    let mut out = String::new();
    append_visible_text(doc, body.id, scripting_enabled, &mut out);
    out
}

fn append_visible_text(
    doc: &BaseDocument,
    node: NodeId,
    scripting_enabled: bool,
    out: &mut String,
) {
    let Some(node_ref) = doc.get_node(node) else {
        return;
    };
    match &node_ref.data {
        NodeData::Text(data) => out.push_str(&data.content),
        NodeData::Element(..) => {
            if node_ref
                .data
                .is_element_with_tag_name(&local_name!("script"))
                || node_ref
                    .data
                    .is_element_with_tag_name(&local_name!("style"))
            {
                return;
            }
            if scripting_enabled
                && node_ref
                    .data
                    .is_element_with_tag_name(&local_name!("noscript"))
            {
                return;
            }
            for child in node_ref.children.iter() {
                append_visible_text(doc, *child, scripting_enabled, out);
            }
        }
        NodeData::AnonymousBlock(..) => {
            for child in node_ref.children.iter() {
                append_visible_text(doc, *child, scripting_enabled, out);
            }
        }
        _ => {}
    }
}

pub fn dump_text(url: &str, js_enabled: bool) -> Result<String, String> {
    let doc = load(url, js_enabled)?;
    Ok(format_dump_text(&doc.0, js_enabled))
}

#[cfg(test)]
mod tests {
    #[test]
    fn dump_lines_collapses_whitespace() {
        let raw = "  hello   world\n\n\nfoo  bar  ";
        let got: String = raw
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(got, "hello world\nfoo bar");
    }
}
