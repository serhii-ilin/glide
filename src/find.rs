//! Find-in-page: text collection, case-insensitive search, and match navigation.

use blitz_dom::{BaseDocument, ScrollBehavior, ScrollLogicalPosition, local_name};
use blitz_traits::node_id::NodeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMatch {
    pub node_id: NodeId,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, Default)]
pub struct FindState {
    query: String,
    matches: Vec<FindMatch>,
    index: usize,
}

impl FindState {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn current_index(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.index + 1
        }
    }

    pub fn set_search(&mut self, texts: &[(NodeId, String)], query: &str) {
        self.query = query.to_string();
        self.matches = find_matches(texts, query);
        self.index = 0;
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.index = 0;
    }

    pub fn current(&self) -> Option<&FindMatch> {
        self.matches.get(self.index)
    }

    pub fn next_match(&mut self) -> Option<&FindMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.matches.len();
        self.matches.get(self.index)
    }

    pub fn prev_match(&mut self) -> Option<&FindMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.index = self.index.checked_sub(1).unwrap_or(self.matches.len() - 1);
        self.matches.get(self.index)
    }
}

/// Fold one Unicode scalar for case-insensitive find (same rule for query and haystack).
fn fold_case(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Case-insensitive substring search over inline-root texts; byte offsets are UTF-8 safe.
pub fn find_matches(texts: &[(NodeId, String)], query: &str) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle: Vec<char> = query.chars().map(fold_case).collect();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for &(node_id, ref text) in texts {
        out.extend(find_in_text(node_id, text, &needle));
    }
    out
}

fn find_in_text(node_id: NodeId, text: &str, needle: &[char]) -> Vec<FindMatch> {
    let text_folded: Vec<char> = text.chars().map(fold_case).collect();
    let byte_starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let text_len = text.len();

    let mut matches = Vec::new();
    let mut i = 0;
    while i + needle.len() <= text_folded.len() {
        if text_folded[i..i + needle.len()] == *needle {
            let start = byte_starts.get(i).copied().unwrap_or(0);
            let end = if i + needle.len() < byte_starts.len() {
                byte_starts[i + needle.len()]
            } else {
                text_len
            };
            matches.push(FindMatch {
                node_id,
                start_byte: start,
                end_byte: end,
            });
            i += needle.len();
        } else {
            i += 1;
        }
    }
    matches
}

fn skip_subtree_tag(node: &blitz_dom::node::Node) -> bool {
    node.data.is_element_with_tag_name(&local_name!("script"))
        || node.data.is_element_with_tag_name(&local_name!("style"))
        || node.data.is_element_with_tag_name(&local_name!("noscript"))
}

fn node_hidden(node: &blitz_dom::node::Node) -> bool {
    node.try_stylo_element_data()
        .and_then(|s| s.get())
        .is_some_and(|s| s.styles.is_display_none())
}

/// Collect laid-out inline-root text from a page document for searching.
pub fn collect_searchable_texts(doc: &BaseDocument) -> Vec<(NodeId, String)> {
    let Some(body) = doc.find_body_node() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_from_node(doc, body.id, &mut out);
    out
}

fn collect_from_node(doc: &BaseDocument, node_id: NodeId, out: &mut Vec<(NodeId, String)>) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    if skip_subtree_tag(node) {
        return;
    }
    if node_hidden(node) {
        return;
    }
    if node.flags.is_inline_root()
        && let Some(text) = inline_layout_text(node)
        && !text.is_empty()
    {
        out.push((node_id, text));
    }
    let child_ids: Vec<NodeId> = node
        .layout_children
        .borrow()
        .as_ref()
        .map(|ids| ids.iter().copied().collect())
        .unwrap_or_else(|| node.children.iter().copied().collect());
    for child in child_ids {
        collect_from_node(doc, child, out);
    }
}

fn inline_layout_text(node: &blitz_dom::node::Node) -> Option<String> {
    let data = node.element_data()?;
    let layout = data.inline_layout_data.as_ref()?;
    Some(layout.text.clone())
}

pub fn page_iframe_id(chrome: &BaseDocument) -> Option<NodeId> {
    chrome.get_element_by_id("page")
}

pub fn with_page_document_mut<R>(
    chrome: &mut BaseDocument,
    f: impl FnOnce(&mut BaseDocument) -> R,
) -> Option<R> {
    let page_id = page_iframe_id(chrome)?;
    let sub = chrome
        .get_node_mut(page_id)
        .and_then(|node| node.subdoc_mut())?;
    let mut inner = sub.inner_mut();
    Some(f(&mut inner))
}

pub fn apply_current_match(chrome: &mut BaseDocument, m: &FindMatch) -> bool {
    with_page_document_mut(chrome, |page| {
        page.set_text_selection(m.node_id, m.start_byte, m.node_id, m.end_byte);
        page.scroll_into_view(
            m.node_id,
            ScrollBehavior::Auto,
            ScrollLogicalPosition::Center,
            ScrollLogicalPosition::Nearest,
        );
    })
    .is_some()
}

pub fn clear_page_selection(chrome: &mut BaseDocument) {
    let _ = with_page_document_mut(chrome, |page| page.clear_text_selection());
}

/// Chrome find UI state for one window.
#[derive(Debug, Default)]
pub struct FindSession {
    pub open: bool,
    synced_query: String,
    pub state: FindState,
}

impl FindSession {
    pub fn open_bar(&mut self, chrome: &mut BaseDocument) {
        self.open = true;
        crate::chrome::set_find_bar_open(chrome, true);
        crate::chrome::focus_find_input(chrome);
        self.sync_from_input(chrome);
    }

    pub fn close_bar(&mut self, chrome: &mut BaseDocument) {
        self.open = false;
        self.synced_query.clear();
        self.state.clear();
        crate::chrome::set_find_bar_open(chrome, false);
        crate::chrome::set_find_count(chrome, 0, 0);
        clear_page_selection(chrome);
        if let Some(page_id) = page_iframe_id(chrome) {
            chrome.set_focus_to(page_id);
        }
    }

    /// Re-collect page text and refresh matches after the page finishes loading.
    pub fn rerun_search_after_load(&mut self, chrome: &mut BaseDocument) {
        if !self.open || self.state.query().is_empty() {
            return;
        }
        self.synced_query.clear();
        self.sync_from_input(chrome);
    }

    pub fn sync_from_input(&mut self, chrome: &mut BaseDocument) {
        if !self.open {
            return;
        }
        let query = crate::chrome::find_input_value(chrome).unwrap_or_default();
        if query == self.synced_query {
            return;
        }
        let texts = with_page_document_mut(chrome, |page| collect_searchable_texts(page))
            .unwrap_or_default();
        self.state.set_search(&texts, &query);
        self.synced_query = query;
        crate::chrome::set_find_count(chrome, self.state.current_index(), self.state.match_count());
        if let Some(m) = self.state.current() {
            apply_current_match(chrome, m);
        } else {
            clear_page_selection(chrome);
        }
    }

    pub fn step(&mut self, chrome: &mut BaseDocument, forward: bool) {
        if !self.open {
            return;
        }
        let m = if forward {
            self.state.next_match().cloned()
        } else {
            self.state.prev_match().cloned()
        };
        crate::chrome::set_find_count(chrome, self.state.current_index(), self.state.match_count());
        if let Some(m) = m {
            apply_current_match(chrome, &m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use blitz_dom::{DocumentConfig, qual_name};
    use blitz_traits::node_id::NodeId;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use crate::chrome::chrome_document;
    use crate::html::GlideHtmlProvider;
    use crate::parse;

    fn node_id(n: u64) -> NodeId {
        NodeId::from_u64(n)
    }

    #[test]
    fn finds_all_case_insensitive() {
        let texts = vec![(node_id(1), "Hello HELLO hello".to_string())];
        let matches = find_matches(&texts, "hel");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn no_matches_for_empty_query() {
        let texts = vec![(node_id(1), "hello".to_string())];
        assert!(find_matches(&texts, "").is_empty());
    }

    #[test]
    fn non_ascii_offsets_are_char_boundaries() {
        let texts = vec![(node_id(1), "Café résumé".to_string())];
        let matches = find_matches(&texts, "café");
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        let text = &texts[0].1;
        assert!(text.is_char_boundary(m.start_byte));
        assert!(text.is_char_boundary(m.end_byte));
        assert_eq!(&text[m.start_byte..m.end_byte], "Café");
    }

    #[test]
    fn matches_across_multiple_nodes() {
        let a = node_id(1);
        let b = node_id(2);
        let texts = vec![(a, "foo bar".to_string()), (b, "foo baz".to_string())];
        let matches = find_matches(&texts, "foo");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].node_id, a);
        assert_eq!(matches[1].node_id, b);
    }

    #[test]
    fn turkish_dotted_i_matches_itself() {
        let texts = vec![(node_id(1), "İstanbul".to_string())];
        let matches = find_matches(&texts, "istanbul");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &texts[0].1[matches[0].start_byte..matches[0].end_byte],
            "İstanbul"
        );
    }

    #[test]
    fn greek_sigma_matches_itself() {
        let texts = vec![(node_id(1), "ΟΔΟΣ".to_string())];
        let matches = find_matches(&texts, "οδοσ");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn german_sharp_s_does_not_panic() {
        let texts = vec![(node_id(1), "groß".to_string())];
        let _ = find_matches(&texts, "ß");
        let _ = find_matches(&texts, "GROß");
    }

    #[test]
    fn next_prev_wrap() {
        let mut state = FindState::default();
        let texts = vec![(node_id(1), "aa bb aa".to_string())];
        state.set_search(&texts, "aa");
        assert_eq!(state.match_count(), 2);
        assert_eq!(state.current_index(), 1);
        state.next_match();
        assert_eq!(state.current_index(), 2);
        state.next_match();
        assert_eq!(state.current_index(), 1);
        state.prev_match();
        assert_eq!(state.current_index(), 2);
        state.prev_match();
        assert_eq!(state.current_index(), 1);
    }

    fn test_document_config() -> DocumentConfig {
        DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        }
    }

    fn layout_doc(doc: &mut BaseDocument) {
        doc.resolve(0.0);
    }

    #[test]
    fn headless_find_selection_on_fixture() {
        const FIXTURE: &str = include_str!("../tests/fixtures/find.html");
        let html = chrome_document("about:blank", true);
        let mut chrome = parse::document_from_html(&html, test_document_config(), false);
        layout_doc(&mut chrome);
        let page_id = chrome.get_element_by_id("page").expect("page iframe");
        {
            let mut m = chrome.mutate();
            m.set_attribute(page_id, qual_name!("srcdoc"), FIXTURE);
        }
        layout_doc(&mut chrome);

        let texts = with_page_document_mut(&mut chrome, |page| collect_searchable_texts(page))
            .expect("page subdoc");
        let combined: String = texts.iter().map(|(_, t)| t.as_str()).collect();
        assert!(
            !combined.contains("alpha hidden"),
            "display:none text is skipped"
        );
        assert!(
            !texts.iter().any(|(_, t)| t.contains("var secret")),
            "script text is skipped"
        );

        let script_only = find_matches(&texts, "xyzzy");
        assert!(
            script_only.is_empty(),
            "script-only tokens have no inline layout; skip is defensive"
        );

        let before_matches = find_matches(&texts, "before");
        let after_matches = find_matches(&texts, "after");
        assert!(
            !before_matches.is_empty() && !after_matches.is_empty(),
            "anonymous layout children must be searchable"
        );

        let matches = find_matches(&texts, "alpha");
        assert!(!matches.is_empty(), "expected alpha matches");
        let first = &matches[0];
        apply_current_match(&mut chrome, first);
        layout_doc(&mut chrome);

        let (ranges, selected) = with_page_document_mut(&mut chrome, |page| {
            (page.get_text_selection_ranges(), page.get_selected_text())
        })
        .expect("page subdoc");
        assert_eq!(ranges.len(), 1);
        let (node, start, end) = ranges[0];
        assert_eq!(node, first.node_id);
        assert_eq!(start, first.start_byte);
        assert_eq!(end, first.end_byte);
        assert_eq!(selected.as_deref(), Some("alpha"));

        let cafe_matches = find_matches(&texts, "café");
        assert!(!cafe_matches.is_empty(), "expected café match");
        let cafe_match = &cafe_matches[0];
        apply_current_match(&mut chrome, cafe_match);
        layout_doc(&mut chrome);
        let cafe_selected =
            with_page_document_mut(&mut chrome, |page| page.get_selected_text()).expect("page");
        let cafe_inline = texts
            .iter()
            .find(|(id, _)| *id == cafe_match.node_id)
            .map(|(_, t)| &t[cafe_match.start_byte..cafe_match.end_byte]);
        assert_eq!(cafe_selected.as_deref(), cafe_inline);
    }
}
