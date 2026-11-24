//! Pure page inspection helpers on a laid-out [`BaseDocument`].

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, local_name};
use blitz_traits::node_id::NodeId;

use super::protocol::{ElementInfo, Rect, ScrollInfo, truncate_chars};
use crate::headless::{self, VIEWPORT_HEIGHT, VIEWPORT_WIDTH};
use crate::js::viewport::{self, ViewportState};

const MAX_ELEMENT_TEXT_CHARS: usize = 200;
/// Tallest full-page screenshot (CSS pixels); longer documents are cut here.
pub const MAX_FULL_PAGE_HEIGHT: u32 = 16_384;

pub fn snapshot_text(
    doc: &BaseDocument,
    scripting_enabled: bool,
    max_chars: usize,
) -> (String, bool) {
    let raw = headless::format_dump_text(doc, scripting_enabled);
    truncate_chars(&raw, max_chars)
}

pub fn dom_html(
    doc: &BaseDocument,
    selector: Option<&str>,
    all: bool,
    max_chars: usize,
) -> Result<(String, usize, bool), String> {
    let sel = selector.unwrap_or("html");
    if all {
        let ids = doc
            .query_selector_all(sel)
            .map_err(|_| format!("invalid selector `{sel}`"))?;
        if ids.is_empty() {
            return Err(format!("no element matches selector `{sel}`"));
        }
        let matched = ids.len();
        let parts: Result<Vec<String>, String> = ids
            .iter()
            .map(|id| {
                let node = doc
                    .get_node(*id)
                    .ok_or_else(|| format!("stale node for selector `{sel}`"))?;
                Ok(node.outer_html())
            })
            .collect();
        let joined = parts?.join("\n");
        let (html, truncated) = truncate_chars(&joined, max_chars);
        return Ok((html, matched, truncated));
    }
    let id = doc
        .query_selector(sel)
        .map_err(|_| format!("invalid selector `{sel}`"))?
        .ok_or_else(|| format!("no element matches selector `{sel}`"))?;
    let node = doc
        .get_node(id)
        .ok_or_else(|| format!("stale node for selector `{sel}`"))?;
    let outer = node.outer_html();
    let (html, truncated) = truncate_chars(&outer, max_chars);
    Ok((html, 1, truncated))
}

pub fn query(
    doc: &BaseDocument,
    selector: &str,
    limit: usize,
    scripting_enabled: bool,
) -> Result<Vec<ElementInfo>, String> {
    let ids = doc
        .query_selector_all(selector)
        .map_err(|_| format!("invalid selector `{selector}`"))?;
    let mut out = Vec::new();
    for id in ids.into_iter().take(limit) {
        if let Some(info) = element_info(doc, id, scripting_enabled) {
            out.push(info);
        }
    }
    Ok(out)
}

pub fn page_title(doc: &BaseDocument) -> String {
    let Ok(Some(title_id)) = doc.query_selector("title") else {
        return String::new();
    };
    if doc.get_node(title_id).is_none() {
        return String::new();
    }
    let mut text = String::new();
    collect_text(doc, title_id, &mut text);
    collapse_whitespace(&text)
}

pub fn paint_png(doc: &mut BaseDocument, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let rgba = crate::shot::paint_rgba(doc, width, height);
    crate::shot::encode_png(&rgba, width, height)
}

/// Paint the whole document height (capped at [`MAX_FULL_PAGE_HEIGHT`]) from the top, then put
/// the viewport size and scroll offset back so the page is unchanged.
pub fn paint_full_page_png(doc: &mut BaseDocument) -> Result<Vec<u8>, String> {
    let before = viewport::state(doc);
    let tall = (before.scroll_height.ceil() as u32).clamp(VIEWPORT_HEIGHT, MAX_FULL_PAGE_HEIGHT);
    let saved_size = doc.viewport().window_size;
    doc.viewport_mut().window_size = (VIEWPORT_WIDTH, tall);
    viewport::scroll_to(doc, 0.0, 0.0);
    let png = paint_png(doc, VIEWPORT_WIDTH, tall);
    doc.viewport_mut().window_size = saved_size;
    viewport::scroll_to(doc, before.scroll_x, before.scroll_y);
    png
}

/// Convert viewport state to the control result shape.
pub fn scroll_info(state: ViewportState, settled: bool) -> ScrollInfo {
    ScrollInfo {
        x: state.scroll_x,
        y: state.scroll_y,
        scroll_width: state.scroll_width,
        scroll_height: state.scroll_height,
        viewport_width: state.width.round() as u32,
        viewport_height: state.height.round() as u32,
        settled,
    }
}

/// Apply a scroll request to the document: first `to_selector` (when given), then the deltas.
pub fn apply_scroll(
    doc: &mut BaseDocument,
    to_selector: Option<&str>,
    dx: f64,
    dy: f64,
) -> Result<ViewportState, String> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err("dx and dy must be finite numbers".to_string());
    }
    let mut state = viewport::state(doc);
    if let Some(selector) = to_selector {
        state = viewport::scroll_to_selector(doc, selector)?;
    }
    if dx != 0.0 || dy != 0.0 {
        state = viewport::scroll_by(doc, dx, dy);
    }
    Ok(state)
}

fn element_info(doc: &BaseDocument, id: NodeId, scripting_enabled: bool) -> Option<ElementInfo> {
    let node = doc.get_node(id)?;
    let el = node.element_data()?;
    let tag = el.name.local.to_string();
    let id_attr = el.attr(local_name!("id")).map(str::to_string);
    let class_attr = el.attr(local_name!("class")).map(str::to_string);
    let mut text = String::new();
    for child in node.children.iter() {
        append_element_text(doc, *child, scripting_enabled, &mut text);
    }
    let text = collapse_whitespace(&text);
    let (text, _) = truncate_chars(&text, MAX_ELEMENT_TEXT_CHARS);
    let pos = node.absolute_position(0.0, 0.0);
    let size = node.final_layout().size;
    let width = size.width;
    let height = size.height;
    let visible = width > 0.0 && height > 0.0;
    Some(ElementInfo {
        tag,
        id: id_attr,
        class: class_attr,
        text,
        rect: Rect {
            x: pos.x,
            y: pos.y,
            width,
            height,
        },
        visible,
    })
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_text(doc: &BaseDocument, id: NodeId, out: &mut String) {
    let Some(node) = doc.get_node(id) else {
        return;
    };
    match &node.data {
        NodeData::Text(data) => out.push_str(&data.content),
        NodeData::Element(..) | NodeData::AnonymousBlock(..) => {
            for child in node.children.iter() {
                collect_text(doc, *child, out);
            }
        }
        _ => {}
    }
}

fn append_element_text(doc: &BaseDocument, id: NodeId, scripting_enabled: bool, out: &mut String) {
    let Some(node) = doc.get_node(id) else {
        return;
    };
    match &node.data {
        NodeData::Text(data) => out.push_str(&data.content),
        NodeData::Element(..) => {
            if node.data.is_element_with_tag_name(&local_name!("script"))
                || node.data.is_element_with_tag_name(&local_name!("style"))
            {
                return;
            }
            if scripting_enabled && node.data.is_element_with_tag_name(&local_name!("noscript")) {
                return;
            }
            for child in node.children.iter() {
                append_element_text(doc, *child, scripting_enabled, out);
            }
        }
        NodeData::AnonymousBlock(..) => {
            for child in node.children.iter() {
                append_element_text(doc, *child, scripting_enabled, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use blitz_dom::DocumentConfig;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use super::*;
    use crate::control::protocol::{DEFAULT_MAX_CHARS, DEFAULT_QUERY_LIMIT};
    use crate::headless::{VIEWPORT_HEIGHT, VIEWPORT_WIDTH};
    use crate::parse;

    fn laid_out(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(
                VIEWPORT_WIDTH,
                VIEWPORT_HEIGHT,
                1.0,
                ColorScheme::Light,
            )),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(html, config, true);
        doc.resolve(0.0);
        doc
    }

    #[test]
    fn page_title_reads_head_title() {
        let doc = laid_out(
            "<!DOCTYPE html><html><head><title>  Hello   World </title></head><body></body></html>",
        );
        assert_eq!(page_title(&doc), "Hello World");
    }

    #[test]
    fn page_title_empty_without_title_element() {
        let doc = laid_out("<!DOCTYPE html><html><body><p>x</p></body></html>");
        assert_eq!(page_title(&doc), "");
    }

    #[test]
    fn apply_scroll_clamps_and_reports_new_state() {
        let mut doc = laid_out(
            r#"<!DOCTYPE html><html><body style="margin:0"><div style="height:3000px"></div></body></html>"#,
        );
        let moved = apply_scroll(&mut doc, None, 0.0, 500.0).expect("scroll");
        assert_eq!(moved.scroll_y, 500.0);
        let end = apply_scroll(&mut doc, None, 0.0, 1.0e9).expect("scroll to end");
        assert_eq!(end.scroll_y, end.scroll_height - end.height);
    }

    #[test]
    fn apply_scroll_rejects_non_finite_delta() {
        let mut doc = laid_out("<!DOCTYPE html><html><body></body></html>");
        let err = apply_scroll(&mut doc, None, f64::NAN, 0.0).expect_err("nan");
        assert!(err.contains("finite"), "{err}");
    }

    #[test]
    fn dom_no_match_is_error() {
        let doc = laid_out("<!DOCTYPE html><html><body></body></html>");
        let err = dom_html(&doc, Some("#nope"), false, DEFAULT_MAX_CHARS).expect_err("err");
        assert!(err.contains("no element matches selector"));
    }

    #[test]
    fn dom_invalid_selector_is_error() {
        let doc = laid_out("<!DOCTYPE html><html><body></body></html>");
        let err = dom_html(&doc, Some("[[["), false, DEFAULT_MAX_CHARS).expect_err("err");
        assert!(err.contains("invalid selector `[[[`"), "{err}");
    }

    #[test]
    fn computed_style_map_for_selector_reads_display() {
        let doc = laid_out(
            r#"<!DOCTYPE html><html><body><div id="box" style="display:flex;color:rgb(255,0,0)"></div></body></html>"#,
        );
        let map = crate::devtools::computed::computed_style_map_for_selector(&doc, "#box")
            .expect("styles");
        assert_eq!(map.get("display").map(String::as_str), Some("flex"));
        assert!(map.get("color").is_some_and(|c| c.contains("255")));
    }

    #[test]
    fn query_on_small_document_returns_tag_and_visibility() {
        let doc = laid_out(
            r#"<!DOCTYPE html><html><body>
            <div id="on" style="width:50px;height:20px">Hi</div>
            <div id="off" style="display:none">Hide</div>
            </body></html>"#,
        );
        let elements = query(&doc, "#on, #off", DEFAULT_QUERY_LIMIT, true).expect("query");
        let on = elements
            .iter()
            .find(|e| e.id.as_deref() == Some("on"))
            .expect("on");
        assert_eq!(on.tag, "div");
        assert!(on.visible);
        let off = elements.iter().find(|e| e.id.as_deref() == Some("off"));
        if let Some(o) = off {
            assert!(!o.visible);
        }
    }
}
