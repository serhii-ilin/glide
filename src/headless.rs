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
use crate::logging;
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
        if parsed.scheme() == "http" || parsed.scheme() == "https" {
            match handle.block_on(crate::http::fetch_document(
                Request::get(parsed),
                js_enabled,
            )) {
                crate::http::DocumentFetch::Page { url, body } => (url, body.to_vec()),
                crate::http::DocumentFetch::Failed { url, reason } => {
                    return Err(format!("fetch {url}: {reason}"));
                }
            }
        } else {
            let (final_url, bytes) = handle
                .block_on(net_impl.fetch_async(Request::get(parsed)))
                .map_err(|err| format!("fetch {url}: {err}"))?;
            (final_url, bytes.into())
        }
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
        if !html.trim().is_empty() && !has_visible_content(&document.0) {
            logging::warn(
                "js.page",
                "page rendered no visible text",
                &[
                    ("page_url", final_url.clone()),
                    (
                        "hint",
                        "Scripts ran without errors; the app may depend on a capability listed by js.caps (GLIDE_LOG=debug) or on an unimplemented navigation API."
                            .into(),
                    ),
                ],
            );
        }
    }

    Ok(document)
}

/// Whether the laid-out page shows any text or replaced content.
///
/// Hidden subtrees (`display:none`) have no layout box, so a shell whose only text
/// sits in a hidden fallback counts as blank. Used to report script apps that take
/// a silent fallback path instead of rendering.
pub fn has_visible_content(doc: &BaseDocument) -> bool {
    let Some(body) = doc.find_body_node() else {
        return false;
    };
    let size = body.final_layout().size;
    let laid_out = size.width > 0.0 && size.height > 0.0;
    body.children
        .iter()
        .any(|child| visible_in_subtree(doc, *child, laid_out))
}

/// `parent_laid_out` carries the nearest box ancestor: text nodes have no layout
/// data of their own, and hidden subtrees (`display:none`) lay out to zero size.
fn visible_in_subtree(doc: &BaseDocument, id: NodeId, parent_laid_out: bool) -> bool {
    let Some(node) = doc.get_node(id) else {
        return false;
    };
    match &node.data {
        NodeData::Text(data) => parent_laid_out && !data.content.trim().is_empty(),
        NodeData::Element(..) | NodeData::AnonymousBlock(..) => {
            let size = node.final_layout().size;
            let laid_out = size.width > 0.0 && size.height > 0.0;
            let replaced = node.data.is_element_with_tag_name(&local_name!("img"))
                || node.data.is_element_with_tag_name(&local_name!("svg"))
                || node.data.is_element_with_tag_name(&local_name!("canvas"))
                || node.data.is_element_with_tag_name(&local_name!("video"))
                || node.data.is_element_with_tag_name(&local_name!("iframe"));
            (laid_out && replaced)
                || node
                    .children
                    .iter()
                    .any(|child| visible_in_subtree(doc, *child, laid_out))
        }
        _ => false,
    }
}

/// Collapse whitespace in each line, drop blank lines, and append a trailing newline.
pub fn format_dump_text(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let raw = body_text_content(doc, scripting_enabled);
    let lines: Vec<String> = raw
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "\n".to_string();
    }
    format!("{}\n", lines.join("\n"))
}

fn body_text_content(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let Some(body) = doc.find_body_node() else {
        return String::new();
    };
    let mut out = String::new();
    append_visible_text(doc, body.id, scripting_enabled, &mut out);
    out
}

fn push_line_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn is_block_element(node: &blitz_dom::node::Node) -> bool {
    node.data.is_element_with_tag_name(&local_name!("p"))
        || node.data.is_element_with_tag_name(&local_name!("div"))
        || node.data.is_element_with_tag_name(&local_name!("li"))
        || node.data.is_element_with_tag_name(&local_name!("ul"))
        || node.data.is_element_with_tag_name(&local_name!("ol"))
        || node.data.is_element_with_tag_name(&local_name!("h1"))
        || node.data.is_element_with_tag_name(&local_name!("h2"))
        || node.data.is_element_with_tag_name(&local_name!("h3"))
        || node.data.is_element_with_tag_name(&local_name!("h4"))
        || node.data.is_element_with_tag_name(&local_name!("h5"))
        || node.data.is_element_with_tag_name(&local_name!("h6"))
        || node.data.is_element_with_tag_name(&local_name!("section"))
        || node.data.is_element_with_tag_name(&local_name!("article"))
        || node.data.is_element_with_tag_name(&local_name!("header"))
        || node.data.is_element_with_tag_name(&local_name!("footer"))
        || node.data.is_element_with_tag_name(&local_name!("nav"))
        || node.data.is_element_with_tag_name(&local_name!("main"))
        || node.data.is_element_with_tag_name(&local_name!("table"))
        || node.data.is_element_with_tag_name(&local_name!("tr"))
        || node.data.is_element_with_tag_name(&local_name!("pre"))
        || node
            .data
            .is_element_with_tag_name(&local_name!("blockquote"))
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
            if node_ref.data.is_element_with_tag_name(&local_name!("br")) {
                push_line_break(out);
                return;
            }
            let block = is_block_element(node_ref);
            if block {
                push_line_break(out);
            }
            for child in node_ref.children.iter() {
                append_visible_text(doc, *child, scripting_enabled, out);
            }
            if block {
                push_line_break(out);
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
    use super::*;
    use blitz_dom::DocumentConfig;

    fn doc_from_html(html: &str, scripting: bool) -> BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), scripting)
    }

    fn laid_out_doc(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(html, config, true);
        doc.resolve(0.0);
        doc
    }

    #[test]
    fn visible_content_ignores_display_none_text() {
        let hidden = laid_out_doc(
            r#"<!DOCTYPE html><html><body><div style="display:none">hidden</div></body></html>"#,
        );
        assert!(!has_visible_content(&hidden));

        let shown = laid_out_doc("<!DOCTYPE html><html><body><p>shown</p></body></html>");
        assert!(has_visible_content(&shown));
    }

    #[test]
    fn visible_content_counts_a_sized_image() {
        let doc = laid_out_doc(
            r#"<!DOCTYPE html><html><body><img width="10" height="10"></body></html>"#,
        );
        assert!(has_visible_content(&doc));
    }

    #[test]
    fn dump_lines_collapses_whitespace() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>  hello   world</p><p>foo  bar</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "hello world\nfoo bar\n");
    }

    #[test]
    fn block_elements_are_separated() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>a</p><p>b</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "a\nb\n");
    }

    #[test]
    fn br_breaks_lines() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>a<br>b</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "a\nb\n");
    }
}
