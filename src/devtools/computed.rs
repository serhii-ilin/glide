//! DevTools Elements computed-style pane (UI thread, Stylo resolved values).

use blitz_dom::BaseDocument;
use blitz_dom::node::NodeData;
use blitz_traits::node_id::NodeId;

use crate::devtools::elements::{escape_elements_text, handle_to_node};

pub const MAX_COMPUTED_VALUE_LEN: usize = 512;

fn truncate_computed_value(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut end = 0;
    let mut count = 0;
    for (i, _) in value.char_indices() {
        if count >= max {
            break;
        }
        end = i;
        count += 1;
    }
    let cut = if count >= max {
        value.floor_char_boundary(end.saturating_add(1))
    } else {
        value.len()
    };
    format!("{}…", &value[..cut])
}

pub const COMPUTED_PROPERTIES: &[&str] = &[
    "display",
    "position",
    "box-sizing",
    "width",
    "height",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "color",
    "background-color",
    "font-family",
    "font-size",
    "font-weight",
    "line-height",
    "opacity",
    "visibility",
    "z-index",
    "overflow",
    "overflow-x",
    "overflow-y",
    "flex-direction",
    "flex-wrap",
    "justify-content",
    "align-items",
    "grid-template-columns",
    "grid-template-rows",
];

/// Curated computed style map for an element node after layout resolve.
pub fn computed_style_map(doc: &BaseDocument, node_id: NodeId) -> Option<Vec<(String, String)>> {
    let node = doc.get_node(node_id)?;
    if !matches!(
        &node.data,
        NodeData::Element(_) | NodeData::AnonymousBlock(_)
    ) {
        return None;
    }
    let mut rows = Vec::with_capacity(COMPUTED_PROPERTIES.len());
    for &name in COMPUTED_PROPERTIES {
        let mut value = doc.resolved_style_value(node_id, name);
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_COMPUTED_VALUE_LEN {
            value = truncate_computed_value(&value, MAX_COMPUTED_VALUE_LEN);
        }
        rows.push((name.to_string(), value));
    }
    Some(rows)
}

fn lookup_value<'a>(rows: &'a [(String, String)], name: &str) -> Option<&'a str> {
    rows.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn render_box_model_summary(rows: &[(String, String)]) -> String {
    let margin_t = lookup_value(rows, "margin-top").unwrap_or("0");
    let margin_r = lookup_value(rows, "margin-right").unwrap_or("0");
    let margin_b = lookup_value(rows, "margin-bottom").unwrap_or("0");
    let margin_l = lookup_value(rows, "margin-left").unwrap_or("0");
    let padding_t = lookup_value(rows, "padding-top").unwrap_or("0");
    let padding_r = lookup_value(rows, "padding-right").unwrap_or("0");
    let padding_b = lookup_value(rows, "padding-bottom").unwrap_or("0");
    let padding_l = lookup_value(rows, "padding-left").unwrap_or("0");
    let border_t = lookup_value(rows, "border-top-width").unwrap_or("0");
    let border_r = lookup_value(rows, "border-right-width").unwrap_or("0");
    let border_b = lookup_value(rows, "border-bottom-width").unwrap_or("0");
    let border_l = lookup_value(rows, "border-left-width").unwrap_or("0");
    let width = lookup_value(rows, "width").unwrap_or("auto");
    let height = lookup_value(rows, "height").unwrap_or("auto");
    format!(
        r#"<div class="devtools-box-model"><div class="bm-margin"><span class="bm-label">margin</span><span class="bm-t">{margin_t}</span><span class="bm-r">{margin_r}</span><span class="bm-b">{margin_b}</span><span class="bm-l">{margin_l}</span><div class="bm-border"><span class="bm-label">border</span><span class="bm-t">{border_t}</span><span class="bm-r">{border_r}</span><span class="bm-b">{border_b}</span><span class="bm-l">{border_l}</span><div class="bm-padding"><span class="bm-label">padding</span><span class="bm-t">{padding_t}</span><span class="bm-r">{padding_r}</span><span class="bm-b">{padding_b}</span><span class="bm-l">{padding_l}</span><div class="bm-content">{width} × {height}</div></div></div></div></div>"#
    )
}

pub fn render_computed_pane_html(page: &BaseDocument, selected: Option<u64>) -> String {
    let Some(handle) = selected else {
        return "<p class=\"devtools-computed-empty\">Select an element to inspect computed styles</p>"
            .to_string();
    };
    let id = handle_to_node(handle);
    let Some(node) = page.get_node(id) else {
        return "<p class=\"devtools-computed-empty\">Node is no longer in the document</p>"
            .to_string();
    };
    match &node.data {
        NodeData::Element(_) | NodeData::AnonymousBlock(_) => {}
        NodeData::Text(_) => {
            return "<p class=\"devtools-computed-empty\">Computed styles are not available for text nodes</p>".to_string();
        }
        NodeData::Comment { .. } => {
            return "<p class=\"devtools-computed-empty\">Computed styles are not available for comment nodes</p>"
                .to_string();
        }
        NodeData::Document(_) => {
            return "<p class=\"devtools-computed-empty\">Computed styles are not available for the document node</p>"
                .to_string();
        }
    }
    let Some(rows) = computed_style_map(page, id) else {
        return "<p class=\"devtools-computed-empty\">Computed styles are not available</p>"
            .to_string();
    };
    if rows.is_empty() {
        return "<p class=\"devtools-computed-empty\">No computed styles resolved</p>".to_string();
    }
    let mut body = render_box_model_summary(&rows);
    body.push_str("<div class=\"devtools-computed-list\">");
    for (name, value) in rows {
        body.push_str(&format!(
            r#"<div class="devtools-computed-row"><span class="computed-name">{name}</span><span class="computed-colon">:</span> <span class="computed-value">{value}</span></div>
"#,
            name = escape_elements_text(&name),
            value = escape_elements_text(&value)
        ));
    }
    body.push_str("</div>");
    body
}

/// Selector-based map for MCP / control session (first match only).
pub fn computed_style_map_for_selector(
    doc: &BaseDocument,
    selector: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let id = doc
        .query_selector(selector)
        .map_err(|_| format!("invalid selector `{selector}`"))?
        .ok_or_else(|| format!("no element matches selector `{selector}`"))?;
    let rows = computed_style_map(doc, id)
        .ok_or_else(|| format!("no computed styles for selector `{selector}`"))?;
    let mut map = std::collections::BTreeMap::new();
    for (k, v) in rows {
        map.insert(k, v);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devtools::elements::node_to_handle;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    fn parse_page(html: &str) -> BaseDocument {
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        doc.resolve(0.0);
        doc
    }

    #[test]
    fn computed_rows_include_fixture_styles() {
        let html = include_str!("../../tests/fixtures/devtools-computed-sample.html");
        let doc = parse_page(html);
        let id = doc.get_element_by_id("box").expect("box");
        let rows = computed_style_map(&doc, id).expect("element");
        let display = rows
            .iter()
            .find(|(k, _)| k == "display")
            .map(|(_, v)| v.as_str());
        assert_eq!(display, Some("flex"));
        let color = rows
            .iter()
            .find(|(k, _)| k == "color")
            .map(|(_, v)| v.as_str());
        assert!(color.is_some_and(|c| c.contains("255")));
        assert!(rows.iter().any(|(k, _)| k == "padding-top"));
    }

    #[test]
    fn computed_empty_state_for_non_element_nodes() {
        let html = include_str!("../../tests/fixtures/devtools-computed-sample.html");
        let doc = parse_page(html);
        let p = doc.get_element_by_id("note").expect("p");
        let text = doc.get_node(p).expect("p node").children[0];
        let html_text = render_computed_pane_html(&doc, Some(node_to_handle(text)));
        assert!(html_text.contains("text nodes"));
        let html_comment = render_computed_pane_html(&doc, None);
        assert!(html_comment.contains("Select an element"));
    }

    #[test]
    fn computed_html_escapes_values_once() {
        let doc = parse_page(
            r#"<!DOCTYPE html><html><body><div id="x" style="font-family: 'A &amp; B', serif"></div></body></html>"#,
        );
        let id = doc.get_element_by_id("x").expect("x");
        let html = render_computed_pane_html(&doc, Some(node_to_handle(id)));
        assert!(!html.contains("&amp;amp;"));
    }

    #[test]
    fn computed_style_map_for_selector_errors_on_missing() {
        let doc = parse_page("<!DOCTYPE html><html><body></body></html>");
        let err = computed_style_map_for_selector(&doc, "#missing").unwrap_err();
        assert!(err.contains("no element"));
    }
}
