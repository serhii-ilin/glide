//! DevTools Elements tab: DOM tree snapshot and attribute inspection (UI thread).

use std::collections::HashSet;

use blitz_dom::BaseDocument;
use blitz_dom::node::NodeData;
use blitz_traits::node_id::NodeId;

use crate::find::page_iframe_id;

fn node_to_handle(id: NodeId) -> u64 {
    id.as_u64()
}

fn handle_to_node(raw: u64) -> NodeId {
    NodeId::from_u64(raw)
}

fn live_node_count(doc: &BaseDocument) -> usize {
    doc.tree().len()
}

pub const MAX_TREE_ROWS: usize = 500;
pub const MAX_TREE_DEPTH: usize = 64;
pub const MAX_ATTR_VALUE_LEN: usize = 120;
pub const MAX_TEXT_PREVIEW_LEN: usize = 80;
pub const MAX_INLINE_ATTRS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub node: Option<NodeId>,
    pub depth: usize,
    pub line: String,
    pub has_children: bool,
    pub expanded: bool,
    pub more_omitted: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct ElementsPanelState {
    pub expanded: HashSet<u64>,
    pub selected: Option<u64>,
    pub cursor_row: usize,
    pub ui_generation: u64,
    flat_handles: Vec<Option<u64>>,
}

impl ElementsPanelState {
    pub fn bump_ui(&mut self) {
        self.ui_generation = self.ui_generation.saturating_add(1);
    }

    pub fn reset_for_navigation(&mut self) {
        self.expanded.clear();
        self.selected = None;
        self.cursor_row = 0;
        self.flat_handles.clear();
        self.bump_ui();
    }

    pub fn apply_default_expansion(&mut self, doc: &BaseDocument) {
        self.expanded.insert(node_to_handle(doc.root_node().id));
        if let Some(html) = first_element_named(doc, "html") {
            self.expanded.insert(node_to_handle(html));
        }
        if let Some(body) = first_element_named(doc, "body") {
            self.expanded.insert(node_to_handle(body));
        }
    }

    pub fn toggle_expanded(&mut self, handle: u64) {
        if self.expanded.contains(&handle) {
            self.expanded.remove(&handle);
        } else {
            self.expanded.insert(handle);
        }
        self.bump_ui();
    }

    pub fn set_selected_handle(&mut self, handle: Option<u64>) {
        self.selected = handle;
        if let Some(h) = handle
            && let Some(idx) = self.flat_handles.iter().position(|row| *row == Some(h))
        {
            self.cursor_row = idx;
        }
        self.bump_ui();
    }

    pub fn flat_len(&self) -> usize {
        self.flat_handles.len()
    }

    pub fn expand_cursor(&mut self, rows: &[TreeRow]) {
        let Some(handle) = self.cursor_handle() else {
            return;
        };
        let Some(row) = rows.get(self.cursor_row) else {
            return;
        };
        if row.has_children && row.more_omitted.is_none() {
            self.expanded.insert(handle);
            self.bump_ui();
        }
    }

    pub fn collapse_or_parent(&mut self, rows: &[TreeRow]) {
        let Some(handle) = self.cursor_handle() else {
            return;
        };
        let Some(row) = rows.get(self.cursor_row) else {
            return;
        };
        if row.expanded {
            self.expanded.remove(&handle);
            self.bump_ui();
            return;
        }
        let depth = row.depth;
        for i in (0..self.cursor_row).rev() {
            if rows[i].more_omitted.is_some() {
                continue;
            }
            if rows[i].depth < depth {
                self.cursor_row = i;
                self.selected = rows[i].node.map(node_to_handle);
                self.bump_ui();
                return;
            }
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.flat_handles.is_empty() {
            return;
        }
        let len = self.flat_handles.len();
        let row = self.cursor_row as isize + delta;
        let row = row.clamp(0, (len - 1) as isize) as usize;
        self.cursor_row = row;
        self.selected = self.flat_handles[row];
        self.bump_ui();
    }

    pub fn cursor_handle(&self) -> Option<u64> {
        self.flat_handles.get(self.cursor_row).copied().flatten()
    }

    pub fn sync_flat_handles(&mut self, rows: &[TreeRow]) {
        self.flat_handles = rows
            .iter()
            .filter(|r| r.more_omitted.is_none())
            .map(|r| r.node.map(node_to_handle))
            .collect();
        if self.cursor_row >= self.flat_handles.len() {
            self.cursor_row = self.flat_handles.len().saturating_sub(1);
        }
        if self.selected.is_none() {
            self.selected = self.cursor_handle();
        }
    }

    pub fn prune_expansion(&mut self, doc: &BaseDocument) {
        self.expanded
            .retain(|h| doc.get_node(handle_to_node(*h)).is_some());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTreeStamp {
    pub doc_id: usize,
    pub node_count: usize,
    pub epoch: u64,
}

pub fn page_tree_stamp(doc: &BaseDocument, epoch: u64) -> PageTreeStamp {
    PageTreeStamp {
        doc_id: doc.id(),
        node_count: live_node_count(doc),
        epoch,
    }
}

pub fn escape_elements_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate_preview(value: &str, max: usize) -> String {
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

fn first_element_named(doc: &BaseDocument, local: &str) -> Option<NodeId> {
    let root = doc.root_node().id;
    walk_find_tag(doc, root, local)
}

fn walk_find_tag(doc: &BaseDocument, id: NodeId, name: &str) -> Option<NodeId> {
    let node = doc.get_node(id)?;
    if let NodeData::Element(el) = &node.data
        && el.name.local.as_str() == name
    {
        return Some(id);
    }
    let children: Vec<NodeId> = node.children.iter().copied().collect();
    for child in children {
        if let Some(found) = walk_find_tag(doc, child, name) {
            return Some(found);
        }
    }
    None
}

pub fn flatten_dom_tree(doc: &BaseDocument, state: &ElementsPanelState) -> Vec<TreeRow> {
    let root = doc.root_node().id;
    let mut rows = Vec::new();
    let mut omitted = 0usize;
    visit_node(doc, root, 0, state, &mut rows, &mut omitted, true);
    if omitted > 0 && rows.len() < MAX_TREE_ROWS {
        rows.push(TreeRow {
            node: None,
            depth: 0,
            line: format!("… {omitted} more"),
            has_children: false,
            expanded: false,
            more_omitted: Some(omitted),
        });
    } else if omitted > 0
        && let Some(last) = rows.last_mut()
    {
        last.line = format!("{} … +{omitted} more", last.line);
    }
    rows
}

fn visit_node(
    doc: &BaseDocument,
    id: NodeId,
    depth: usize,
    state: &ElementsPanelState,
    rows: &mut Vec<TreeRow>,
    omitted: &mut usize,
    include_self: bool,
) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    if rows.len() >= MAX_TREE_ROWS {
        *omitted = omitted.saturating_add(1);
        return;
    }
    let Some(node) = doc.get_node(id) else {
        return;
    };
    if include_self {
        let handle = node_to_handle(id);
        let (line, has_children) = row_label(doc, id, node);
        let expanded = has_children && state.expanded.contains(&handle);
        rows.push(TreeRow {
            node: Some(id),
            depth,
            line,
            has_children,
            expanded,
            more_omitted: None,
        });
        if !expanded {
            return;
        }
    }
    let child_ids: Vec<NodeId> = node.children.iter().copied().collect();
    for child in child_ids {
        visit_node(doc, child, depth + 1, state, rows, omitted, true);
    }
}

fn row_label(_doc: &BaseDocument, _id: NodeId, node: &blitz_dom::node::Node) -> (String, bool) {
    match &node.data {
        NodeData::Document(_) => ("#document".to_string(), !node.children.is_empty()),
        NodeData::Element(el) | NodeData::AnonymousBlock(el) => {
            let tag = el.name.local.as_str();
            let attrs = format_element_attrs(el.attrs());
            let line = if attrs.is_empty() {
                format!("<{tag}>")
            } else {
                format!("<{tag} {attrs}>")
            };
            (line, !node.children.is_empty())
        }
        NodeData::Text(data) => {
            let preview = truncate_preview(data.content.trim(), MAX_TEXT_PREVIEW_LEN);
            (format!("\"{preview}\""), false)
        }
        NodeData::Comment { contents } => {
            let preview = truncate_preview(contents.trim(), MAX_TEXT_PREVIEW_LEN);
            (format!("<!-- {preview} -->"), false)
        }
    }
}

fn format_element_attrs(attrs: &[blitz_dom::node::Attribute]) -> String {
    let mut parts = Vec::new();
    for (i, a) in attrs.iter().enumerate() {
        if i >= MAX_INLINE_ATTRS {
            parts.push("…".to_string());
            break;
        }
        let name = a.name.local.as_str();
        let value = truncate_preview(&a.value, MAX_ATTR_VALUE_LEN);
        parts.push(format!("{name}=\"{value}\""));
    }
    parts.join(" ")
}

pub fn render_detail_html(doc: &BaseDocument, selected: Option<u64>) -> String {
    let Some(handle) = selected else {
        return "<p class=\"devtools-elements-detail-empty\">No node selected</p>".to_string();
    };
    let id = handle_to_node(handle);
    let Some(node) = doc.get_node(id) else {
        return "<p class=\"devtools-elements-detail-empty\">Node is no longer in the document</p>"
            .to_string();
    };
    match &node.data {
        NodeData::Element(el) | NodeData::AnonymousBlock(el) => {
            let mut rows = String::new();
            for a in el.attrs() {
                let name = escape_elements_text(a.name.local.as_ref());
                let value = truncate_preview(&a.value, MAX_ATTR_VALUE_LEN);
                let value = escape_elements_text(&value);
                rows.push_str(&format!(
                    r#"<div class="devtools-elements-attr"><span class="attr-name">{name}</span><span class="attr-eq">=</span><span class="attr-value">"{value}"</span></div>
"#
                ));
            }
            if rows.is_empty() {
                format!(
                    "<p class=\"devtools-elements-detail-tag\">&lt;{}&gt;</p>",
                    escape_elements_text(el.name.local.as_str())
                )
            } else {
                format!(
                    "<p class=\"devtools-elements-detail-tag\">&lt;{tag}&gt;</p>{rows}",
                    tag = escape_elements_text(el.name.local.as_str())
                )
            }
        }
        NodeData::Text(data) => format!(
            "<p class=\"devtools-elements-detail-text\">{}</p>",
            escape_elements_text(&truncate_preview(&data.content, MAX_TEXT_PREVIEW_LEN * 2))
        ),
        NodeData::Comment { contents } => format!(
            "<p class=\"devtools-elements-detail-comment\">&lt;!-- {} --&gt;</p>",
            escape_elements_text(&truncate_preview(contents, MAX_TEXT_PREVIEW_LEN * 2))
        ),
        NodeData::Document(_) => {
            "<p class=\"devtools-elements-detail-tag\">#document</p>".to_string()
        }
    }
}

pub fn render_tree_rows_html(rows: &[TreeRow], selected: Option<u64>) -> String {
    let mut html = String::new();
    for row in rows {
        if let Some(count) = row.more_omitted {
            html.push_str(&format!(
                r#"<div class="devtools-elements-row more"><span class="indent" style="padding-left: calc({}px * var(--ui))"></span><span class="label">{}</span></div>
"#,
                row.depth * 12,
                escape_elements_text(&format!("… {count} more"))
            ));
            continue;
        }
        let Some(id) = row.node else {
            continue;
        };
        let handle = node_to_handle(id);
        let selected_class = if selected == Some(handle) {
            " selected"
        } else {
            ""
        };
        let twist = if row.has_children {
            if row.expanded { "▼" } else { "▶" }
        } else {
            " "
        };
        let twist_href = if row.has_children {
            format!(r#"glide://devtools-elements-toggle?node={handle}"#)
        } else {
            "#".to_string()
        };
        html.push_str(&format!(
            r#"<div class="devtools-elements-row{selected_class}"><a class="twist-link" href="{twist_href}"><span class="twist">{twist}</span></a><a class="devtools-elements-row-link" href="glide://devtools-elements-select?node={handle}"><span class="indent" style="padding-left: calc({}px * var(--ui))"></span><span class="label">{line}</span></a></div>
"#,
            row.depth * 12,
            line = escape_elements_text(&row.line)
        ));
    }
    html
}

pub fn with_page_document<R>(
    chrome: &BaseDocument,
    f: impl FnOnce(&BaseDocument) -> R,
) -> Option<R> {
    let page_id = page_iframe_id(chrome)?;
    let sub = chrome.get_node(page_id)?.subdoc()?;
    Some(f(&sub.inner()))
}

pub struct ElementsRenderPlan {
    pub stamp: PageTreeStamp,
    pub rows: Vec<TreeRow>,
    pub tree_html: String,
    pub detail_html: String,
}

pub fn prepare_elements_render(
    page: &BaseDocument,
    state: &mut ElementsPanelState,
    stamp: PageTreeStamp,
    last_stamp: &Option<PageTreeStamp>,
    _last_ui_generation: u64,
    _force_full: bool,
) -> Option<ElementsRenderPlan> {
    state.prune_expansion(page);
    let _ = last_stamp;
    let rows = flatten_dom_tree(page, state);
    state.sync_flat_handles(&rows);
    if state.selected.is_none() {
        state.selected = state.cursor_handle();
    }
    let tree_html = render_tree_rows_html(&rows, state.selected);
    let detail_html = render_detail_html(page, state.selected);
    Some(ElementsRenderPlan {
        stamp,
        rows,
        tree_html,
        detail_html,
    })
}

pub fn paint_elements_render(chrome: &mut BaseDocument, plan: &ElementsRenderPlan) -> bool {
    let Some(tree_id) = chrome.get_element_by_id("devtools-elements-tree") else {
        return false;
    };
    let Some(detail_id) = chrome.get_element_by_id("devtools-elements-detail") else {
        return false;
    };
    let mut m = chrome.mutate();
    replace_inner_html_tree(&mut m, tree_id, &plan.tree_html);
    replace_inner_html_tree(&mut m, detail_id, &plan.detail_html);
    true
}

fn replace_inner_html_tree(m: &mut blitz_dom::DocumentMutator<'_>, parent: NodeId, html: &str) {
    let children: Vec<_> = m.child_ids(parent).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    if html.is_empty() {
        return;
    }
    crate::parse::parse_inner_html_into_mutator(m, parent, html, false);
}

pub(crate) fn reset_tab_elements_for_navigation(tab: &mut crate::tabs::Tab) {
    tab.elements.reset_for_navigation();
    tab.elements_epoch = tab.elements_epoch.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    fn parse_page(html: &str) -> BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), true)
    }

    #[test]
    fn escape_elements_text_neutralizes_html_injection() {
        let raw = r#"<img src=x onerror="alert(1)">"#;
        let escaped = escape_elements_text(raw);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(escaped.contains("&lt;"));
        assert!(escaped.contains("&quot;"));
        let doc = parse_page(&format!(
            r#"<!DOCTYPE html><html><body><p id="t" data-x="{raw}"></p></body></html>"#
        ));
        let p = doc.get_element_by_id("t").unwrap();
        let detail = render_detail_html(&doc, Some(node_to_handle(p)));
        assert!(!detail.contains("<img"), "detail must be escaped: {detail}");
        assert!(detail.contains("&lt;"));
    }

    #[test]
    fn flatten_tree_respects_row_cap_and_more_indicator() {
        let mut inner = String::from("<!DOCTYPE html><html><body>");
        for i in 0..600 {
            inner.push_str(&format!("<span id=\"s{i}\"></span>"));
        }
        inner.push_str("</body></html>");
        let doc = parse_page(&inner);
        let mut state = ElementsPanelState::default();
        state.apply_default_expansion(&doc);
        state.expanded.insert(node_to_handle(doc.root_node().id));
        // Expand body so all spans are visible candidates
        if let Some(body) = first_element_named(&doc, "body") {
            state.expanded.insert(node_to_handle(body));
        }
        let rows = flatten_dom_tree(&doc, &state);
        assert!(rows.len() <= MAX_TREE_ROWS);
        assert!(
            rows.iter()
                .any(|r| r.more_omitted.is_some() || r.line.contains("more")),
            "expected overflow indicator: {:?}",
            rows.last()
        );
    }

    #[test]
    fn truncation_shortens_long_attribute_values() {
        let long = "z".repeat(200);
        let doc = parse_page(&format!(
            r#"<!DOCTYPE html><html><body><div data-v="{long}"></div></body></html>"#
        ));
        let div = first_element_named(&doc, "div").unwrap();
        let (line, _) = row_label(&doc, div, doc.get_node(div).unwrap());
        assert!(line.contains('…'));
        assert!(line.len() < long.len());
    }

    #[test]
    fn expansion_state_prunes_stale_node_ids() {
        let doc = parse_page(
            r#"<!DOCTYPE html><html><body><div id="a"><span></span></div></body></html>"#,
        );
        let mut state = ElementsPanelState::default();
        state.expanded.insert(999_999);
        let div = doc.get_element_by_id("a").unwrap();
        state.expanded.insert(node_to_handle(div));
        state.prune_expansion(&doc);
        assert!(!state.expanded.contains(&999_999));
        assert!(state.expanded.contains(&node_to_handle(div)));
    }

    #[test]
    fn detail_pane_lists_element_attributes() {
        let doc =
            parse_page(r#"<!DOCTYPE html><html><body><p id="t" data-x="abc">hi</p></body></html>"#);
        let p = doc.get_element_by_id("t").unwrap();
        let html = render_detail_html(&doc, Some(node_to_handle(p)));
        assert!(html.contains("id"));
        assert!(html.contains("data-x"));
        assert!(html.contains("abc"));
    }

    #[test]
    fn render_tree_rows_html_escapes_line_content() {
        let rows = vec![TreeRow {
            node: Some(NodeId::from_u64(1)),
            depth: 0,
            line: "<script>".to_string(),
            has_children: false,
            expanded: false,
            more_omitted: None,
        }];
        let html = render_tree_rows_html(&rows, None);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    fn first_text_child(doc: &BaseDocument, parent: NodeId) -> Option<NodeId> {
        let parent_node = doc.get_node(parent)?;
        for &child in &parent_node.children {
            if matches!(doc.get_node(child)?.data, NodeData::Text(_)) {
                return Some(child);
            }
        }
        None
    }

    #[test]
    fn prepare_elements_render_reflects_text_and_attribute_edits_without_node_count_change() {
        let mut doc = parse_page(
            r#"<!DOCTYPE html><html><body><p id="t" data-v="old">hello</p></body></html>"#,
        );
        let mut state = ElementsPanelState::default();
        state.apply_default_expansion(&doc);
        let p = doc.get_element_by_id("t").unwrap();
        state.expanded.insert(node_to_handle(p));
        let stamp = page_tree_stamp(&doc, 1);
        let plan1 =
            prepare_elements_render(&doc, &mut state, stamp, &None, u64::MAX, false).expect("plan");
        assert!(
            plan1.rows.iter().any(|r| r.line.contains("hello")),
            "rows: {:?}",
            plan1.rows
        );
        let text_id = first_text_child(&doc, p).unwrap();
        let count_before = doc.tree().len();
        {
            let mut m = doc.mutate();
            m.set_node_text(text_id, "Tom & Jerry");
            m.set_attribute(
                p,
                blitz_dom::QualName::new(None, blitz_dom::ns!(html), "data-v".into()),
                "new\"quote",
            );
        }
        assert_eq!(doc.tree().len(), count_before);
        let stamp2 = page_tree_stamp(&doc, 1);
        assert_eq!(stamp, stamp2, "stamp must not detect text/attr edits");
        let plan2 = prepare_elements_render(&doc, &mut state, stamp2, &Some(stamp), 0, false)
            .expect("updated plan after DOM edit");
        assert!(
            plan2
                .rows
                .iter()
                .any(|r| r.line.contains("Tom & Jerry")),
            "raw row: {:?}",
            plan2.rows
        );
        assert!(
            plan2.tree_html.contains("Tom &amp; Jerry"),
            "tree: {}",
            plan2.tree_html
        );
        assert!(!plan2.tree_html.contains("&amp;amp;"), "{:?}", plan2.tree_html);
        state.set_selected_handle(Some(node_to_handle(p)));
        let detail = render_detail_html(&doc, Some(node_to_handle(p)));
        assert!(detail.contains("new&quot;quote"), "detail: {detail}");
    }

    #[test]
    fn tree_row_escapes_text_and_attributes_once() {
        let doc =
            parse_page(r#"<!DOCTYPE html><html><body><p id="t">Tom & Jerry</p></body></html>"#);
        let mut state = ElementsPanelState::default();
        state.apply_default_expansion(&doc);
        if let Some(body) = first_element_named(&doc, "body") {
            state.expanded.insert(node_to_handle(body));
        }
        let p = doc.get_element_by_id("t").unwrap();
        state.expanded.insert(node_to_handle(p));
        let rows = flatten_dom_tree(&doc, &state);
        let text_row = rows
            .iter()
            .find(|r| r.line.contains("Tom"))
            .expect("text row");
        assert_eq!(text_row.line, r#""Tom & Jerry""#);
        let html = render_tree_rows_html(&rows, None);
        assert!(
            html.contains("Tom &amp; Jerry"),
            "single escape in HTML: {html}"
        );
        assert!(!html.contains("&amp;amp;"));
        let doc2 =
            parse_page(r#"<!DOCTYPE html><html><body><script>alert(1)</script></body></html>"#);
        let mut state2 = ElementsPanelState::default();
        state2.apply_default_expansion(&doc2);
        if let Some(body) = first_element_named(&doc2, "body") {
            state2.expanded.insert(node_to_handle(body));
        }
        let rows2 = flatten_dom_tree(&doc2, &state2);
        let script = rows2
            .iter()
            .find(|r| r.line.starts_with("<script"))
            .expect("script row");
        let html2 = render_tree_rows_html(std::slice::from_ref(script), None);
        assert!(!html2.contains("<script>"));
        assert!(html2.contains("&lt;script&gt;"));
    }

    #[test]
    fn toggle_expanded_via_state_matches_toggle_command_behavior() {
        let doc = parse_page(
            r#"<!DOCTYPE html><html><body><div id="a"><span></span></div></body></html>"#,
        );
        let div = doc.get_element_by_id("a").unwrap();
        let handle = node_to_handle(div);
        let mut state = ElementsPanelState::default();
        state.apply_default_expansion(&doc);
        state.expanded.insert(node_to_handle(doc.root_node().id));
        if let Some(body) = first_element_named(&doc, "body") {
            state.expanded.insert(node_to_handle(body));
        }
        assert!(!state.expanded.contains(&handle));
        state.toggle_expanded(handle);
        assert!(state.expanded.contains(&handle));
        let rows = flatten_dom_tree(&doc, &state);
        assert!(
            rows.iter().any(|r| r.line.contains("<span")),
            "span visible when expanded: {:?}",
            rows
        );
        state.toggle_expanded(handle);
        assert!(
            !flatten_dom_tree(&doc, &state)
                .iter()
                .any(|r| r.line.contains("<span")),
            "span hidden when collapsed"
        );
    }
}
