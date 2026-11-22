//! Pure page inspection helpers on a laid-out [`BaseDocument`].

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, local_name};
use blitz_traits::node_id::NodeId;

use super::protocol::{ElementInfo, Rect, truncate_chars};
use crate::headless;

const MAX_ELEMENT_TEXT_CHARS: usize = 200;

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

/// Centre of the first element matching `selector` in page coordinates.
///
/// Non-atomic inline elements have no layout box; their clickable area is the first
/// non-empty line fragment. A point that is covered by another element is rejected so a
/// click cannot silently activate the wrong target.
pub fn element_center(
    doc: &BaseDocument,
    selector: &str,
) -> Result<(f32, f32, ElementInfo), String> {
    let id = doc
        .query_selector(selector)
        .map_err(|_| format!("invalid selector `{selector}`"))?
        .ok_or_else(|| format!("no element matches selector `{selector}`"))?;
    let info = element_info(doc, id, true)
        .ok_or_else(|| format!("element `{selector}` is not visible"))?;
    let rect = first_non_empty_rect(doc, id)
        .or_else(|| first_descendant_rect(doc, id))
        .ok_or_else(|| format!("element `{selector}` is not visible"))?;
    let x = rect.x + rect.width / 2.0;
    let y = rect.y + rect.height / 2.0;
    ensure_not_covered(doc, id, selector, x, y)?;
    Ok((x, y, info))
}

/// Element under viewport point `(x, y)`, used to report the target of a coordinate click.
pub fn element_at(doc: &BaseDocument, x: f32, y: f32) -> Option<ElementInfo> {
    if let Some(id) = doc.element_from_point(x, y) {
        return element_info(doc, id, true);
    }
    let scroll = doc.viewport_scroll();
    let hit = doc.hit(x + scroll.x as f32, y + scroll.y as f32)?;
    let id = doc.nearest_non_anonymous_ancestor(hit.node_id)?;
    element_info(doc, id, true)
}

/// Element info for a known node, used to report the focused target when typing.
pub(crate) fn element_info_for(doc: &BaseDocument, id: NodeId) -> Option<ElementInfo> {
    element_info(doc, id, true)
}

fn page_rect(doc: &BaseDocument, rect: blitz_dom::BoundingRect) -> Rect {
    let scroll = doc.viewport_scroll();
    Rect {
        x: (rect.x + scroll.x) as f32,
        y: (rect.y + scroll.y) as f32,
        width: rect.width as f32,
        height: rect.height as f32,
    }
}

fn first_non_empty_rect(doc: &BaseDocument, id: NodeId) -> Option<Rect> {
    let node = doc.get_node(id)?;
    if !matches!(
        node.data,
        NodeData::Element(..) | NodeData::AnonymousBlock(..)
    ) {
        return None;
    }
    doc.node_client_rects(id)
        .into_iter()
        .map(|rect| page_rect(doc, rect))
        .find(|rect| rect.width > 0.0 && rect.height > 0.0)
}

fn first_descendant_rect(doc: &BaseDocument, id: NodeId) -> Option<Rect> {
    let node = doc.get_node(id)?;
    for child in node.children.iter() {
        if let Some(rect) = first_non_empty_rect(doc, *child) {
            return Some(rect);
        }
        if let Some(rect) = first_descendant_rect(doc, *child) {
            return Some(rect);
        }
    }
    None
}

fn ensure_not_covered(
    doc: &BaseDocument,
    target: NodeId,
    selector: &str,
    x: f32,
    y: f32,
) -> Result<(), String> {
    let Some(hit) = doc.hit(x, y) else {
        return Err(format!("element `{selector}` is not visible"));
    };
    let mut current = Some(hit.node_id);
    while let Some(id) = current {
        if id == target {
            return Ok(());
        }
        current = doc.get_node(id).and_then(|node| node.parent);
    }
    Err(format!(
        "element `{selector}` is covered by {}",
        covering_label(doc, hit.node_id)
    ))
}

fn covering_label(doc: &BaseDocument, id: NodeId) -> String {
    let mut current = Some(id);
    while let Some(node_id) = current {
        if let Some(element) = doc.get_node(node_id).and_then(|node| node.element_data()) {
            let tag = element.name.local.to_string();
            return match element.attr(local_name!("id")) {
                Some(element_id) => format!("<{tag}#{element_id}>"),
                None => format!("<{tag}>"),
            };
        }
        current = doc.get_node(node_id).and_then(|node| node.parent);
    }
    "<unknown>".to_string()
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
    let rect = doc
        .get_client_bounding_rect(id)
        .map(|rect| page_rect(doc, rect))
        .unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
    let visible = rect.width > 0.0 && rect.height > 0.0;
    Some(ElementInfo {
        tag,
        id: id_attr,
        class: class_attr,
        text,
        rect,
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
    fn element_center_uses_layout_box() {
        let mut doc = laid_out(
            r#"<!DOCTYPE html><html><body>
            <div id="on" style="width:50px;height:20px;margin-left:10px;margin-top:5px">Hi</div>
            </body></html>"#,
        );
        doc.resolve(0.0);
        let (x, y, info) = element_center(&doc, "#on").expect("centre");
        assert_eq!(info.id.as_deref(), Some("on"));
        assert!((x - (info.rect.x + info.rect.width / 2.0)).abs() < 0.01);
        assert!((y - (info.rect.y + info.rect.height / 2.0)).abs() < 0.01);
        assert!(info.visible);
    }

    #[test]
    fn hidden_element_has_no_center() {
        let mut doc = laid_out(
            r#"<!DOCTYPE html><html><body>
            <div id="off" style="display:none">Hide</div>
            </body></html>"#,
        );
        doc.resolve(0.0);
        let err = element_center(&doc, "#off").expect_err("hidden");
        assert!(err.contains("is not visible"), "{err}");
    }

    #[test]
    fn unknown_selector_has_no_center() {
        let doc = laid_out("<!DOCTYPE html><html><body></body></html>");
        let err = element_center(&doc, "#nope").expect_err("missing");
        assert!(err.contains("no element matches selector"), "{err}");
    }

    #[test]
    fn inline_link_center_uses_fragment_box() {
        let mut doc = laid_out(
            r##"<!DOCTYPE html><html><body>
            <div style="width:60px"><a id="l" href="#x">wrapped inline link text that spans lines</a></div>
            </body></html>"##,
        );
        doc.resolve(0.0);
        let id = doc.get_element_by_id("l").expect("link");
        let fragments: Vec<_> = doc
            .node_client_rects(id)
            .into_iter()
            .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
            .collect();
        assert!(fragments.len() >= 2, "link should wrap: {fragments:?}");
        let scroll = doc.viewport_scroll();
        let expected_x = (fragments[0].x + scroll.x + fragments[0].width / 2.0) as f32;
        let expected_y = (fragments[0].y + scroll.y + fragments[0].height / 2.0) as f32;
        let (x, y, info) = element_center(&doc, "#l").expect("centre");
        assert!((x - expected_x).abs() < 0.5, "{x} vs {expected_x}");
        assert!((y - expected_y).abs() < 0.5, "{y} vs {expected_y}");
        assert_eq!(info.id.as_deref(), Some("l"));
    }

    #[test]
    fn query_reports_inline_link_box() {
        let mut doc = laid_out(
            r##"<!DOCTYPE html><html><body>
            <p><a id="l" href="#x">plain inline link</a></p>
            </body></html>"##,
        );
        doc.resolve(0.0);
        let elements = query(&doc, "#l", DEFAULT_QUERY_LIMIT, true).expect("query");
        let link = elements.first().expect("link");
        assert!(link.visible, "{:?}", link.rect);
        assert!(
            link.rect.width > 0.0 && link.rect.height > 0.0,
            "{:?}",
            link.rect
        );
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
