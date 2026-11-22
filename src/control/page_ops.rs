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
