//! DevTools element picker: page hit-test, selection, and inspect overlay label.

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, qual_name};
use blitz_traits::node_id::NodeId;

use crate::devtools::elements::{self, node_to_handle};
use crate::devtools::highlight::{self, HighlightRect};
use crate::find::{page_iframe_id, with_page_document_mut};
use crate::nav::ObserveEffect;
use crate::tabs::Tab;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InspectPointerState {
    pub inspect_mode: bool,
    pub swallow_pointer_up: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectPointerButtonDecision {
    PassThrough,
    Consume,
    ConsumePickPress,
    ConsumeSwallowedRelease,
}

/// Pure gate for whether an inspect-mode pointer button event is consumed (see unit tests).
pub fn inspect_pointer_button_decision(
    state: InspectPointerState,
    over_page: bool,
    left_button: bool,
    pressed: bool,
) -> (InspectPointerButtonDecision, InspectPointerState) {
    let mut next = state;
    if state.swallow_pointer_up && left_button && !pressed {
        next.swallow_pointer_up = false;
        return (InspectPointerButtonDecision::ConsumeSwallowedRelease, next);
    }
    if state.inspect_mode && over_page {
        if pressed && left_button {
            next.swallow_pointer_up = true;
            return (InspectPointerButtonDecision::ConsumePickPress, next);
        }
        return (InspectPointerButtonDecision::Consume, next);
    }
    (InspectPointerButtonDecision::PassThrough, state)
}

pub fn navigation_exits_inspect_mode(effect: ObserveEffect) -> bool {
    matches!(
        effect,
        ObserveEffect::Completed | ObserveEffect::LinkNavigation | ObserveEffect::Redirect
    )
}

/// Whether `(x, y)` in chrome client coordinates lies inside the page iframe.
pub fn chrome_pointer_targets_page(chrome: &BaseDocument, x: f32, y: f32) -> bool {
    let Some(iframe) = page_iframe_id(chrome) else {
        return false;
    };
    let Some(border) = chrome.get_client_bounding_rect(iframe) else {
        return false;
    };
    let left = border.x as f32;
    let top = border.y as f32;
    x >= left && y >= top && x <= left + border.width as f32 && y <= top + border.height as f32
}

fn page_client_point(chrome: &BaseDocument, x: f32, y: f32) -> Option<(f32, f32)> {
    if !chrome_pointer_targets_page(chrome, x, y) {
        return None;
    }
    let iframe = page_iframe_id(chrome)?;
    let border = chrome.get_client_bounding_rect(iframe)?;
    Some((x - border.x as f32, y - border.y as f32))
}

/// Hit-test the page subdocument at a chrome client coordinate (reuses Blitz `hit`).
pub fn page_hit_at_chrome_point(chrome: &mut BaseDocument, x: f32, y: f32) -> Option<NodeId> {
    let (page_x, page_y) = page_client_point(chrome, x, y)?;
    with_page_document_mut(chrome, |page| {
        let pruned = crate::js::prune_stale_paint_children(page);
        if pruned > 0 {
            page.resolve(0.0);
        }
        page.resolve(0.0);
        page.hit(page_x, page_y).map(|hit| hit.node_id)
    })?
}

/// Map a hit node to the element shown in Elements (text/comment -> parent element).
pub fn inspectable_element_for_hit(chrome: &BaseDocument, hit: NodeId) -> Option<NodeId> {
    elements::with_page_document(chrome, |page| inspectable_element_on_page(page, hit))?
}

fn inspectable_element_on_page(page: &BaseDocument, hit: NodeId) -> Option<NodeId> {
    let mut current = Some(hit);
    while let Some(id) = current {
        let node = page.get_node(id)?;
        match &node.data {
            NodeData::Element(_) | NodeData::AnonymousBlock(_) => return Some(id),
            NodeData::Text(_) | NodeData::Comment { .. } => current = node.parent,
            NodeData::Document(_) => return None,
        }
    }
    None
}

pub fn expand_ancestors_for_element(
    page: &BaseDocument,
    expanded: &mut std::collections::HashSet<u64>,
    element: NodeId,
) {
    let mut current = Some(element);
    while let Some(id) = current {
        expanded.insert(node_to_handle(id));
        current = page.get_node(id).and_then(|n| n.parent);
    }
}

pub fn highlight_for_page_node(
    chrome: &BaseDocument,
    page: &BaseDocument,
    node_id: NodeId,
) -> Option<HighlightRect> {
    highlight::highlight_for_selected(chrome, page, Some(node_to_handle(node_id)))
}

pub fn inspect_element_label(page: &BaseDocument, element: NodeId) -> String {
    let Some(node) = page.get_node(element) else {
        return String::new();
    };
    let size = page
        .get_client_bounding_rect(element)
        .map(|r| format!("{} × {}", r.width, r.height))
        .unwrap_or_else(|| "0 × 0".to_string());
    match &node.data {
        NodeData::Element(_) | NodeData::AnonymousBlock(_) => {
            let tag = node
                .element_data()
                .map(|e| e.name.local.as_str())
                .unwrap_or("?");
            let id = node.attr(blitz_dom::local_name!("id")).unwrap_or_default();
            let class = node
                .attr(blitz_dom::local_name!("class"))
                .unwrap_or_default();
            let mut label = tag.to_string();
            if !id.is_empty() {
                label.push('#');
                label.push_str(id.as_ref());
            }
            if !class.is_empty() {
                label.push('.');
                label.push_str(class.split_whitespace().next().unwrap_or(""));
            }
            format!("{label}  {size}")
        }
        _ => size,
    }
}

pub fn apply_inspect_label(
    chrome: &mut BaseDocument,
    label: Option<&str>,
    rect: Option<HighlightRect>,
) {
    let Some(label_id) = chrome.get_element_by_id("inspect-label") else {
        return;
    };
    let (style, text) = match (label.filter(|s| !s.is_empty()), rect) {
        (Some(l), Some(r)) => {
            let ui = crate::chrome::chrome_ui_scale(chrome);
            let gap = 22.0 * ui;
            (
                format!(
                    "display:block;left:{}px;top:{}px;",
                    r.left,
                    (r.top - gap).max(0.0)
                ),
                l.to_string(),
            )
        }
        _ => ("display:none;".to_string(), String::new()),
    };
    let mut m = chrome.mutate();
    m.set_attribute(label_id, qual_name!("style"), &style);
    for child in m.child_ids(label_id) {
        m.remove_and_drop_node(child);
    }
    if !text.is_empty() {
        let tn = m.create_text_node(&text);
        m.append_children(label_id, &[tn]);
    }
}

/// Select the element under the pointer, expand ancestors, and return its handle.
pub fn pick_at_chrome_point(
    chrome: &mut BaseDocument,
    tab: &mut Tab,
    x: f32,
    y: f32,
) -> Option<u64> {
    let hit = page_hit_at_chrome_point(chrome, x, y)?;
    let element = inspectable_element_for_hit(chrome, hit)?;
    let handle = node_to_handle(element);
    elements::with_page_document(chrome, |page| {
        expand_ancestors_for_element(page, &mut tab.elements.expanded, element);
    });
    tab.elements.set_selected_handle(Some(handle));
    tab.elements_epoch = tab.elements_epoch.saturating_add(1);
    Some(handle)
}

pub fn exit_inspect_visuals(chrome: &mut BaseDocument) {
    highlight::apply_inspect_highlight(chrome, None);
    apply_inspect_label(chrome, None, None);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::chrome::{apply_devtools_state, chrome_document, set_inspect_toggle_active};
    use crate::devtools::{DevToolsDock, DevToolsState, DevToolsTab};
    use crate::html::GlideHtmlProvider;
    use crate::parse;
    use blitz_dom::{DocumentConfig, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};

    const FIXTURE: &str = include_str!("../../tests/fixtures/devtools-inspect-sample.html");

    fn layout(doc: &mut BaseDocument) {
        doc.resolve(0.0);
    }

    fn chrome_with_fixture() -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1100, 760, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut chrome =
            parse::document_from_html(&chrome_document("about:blank", true, false), config, false);
        layout(&mut chrome);
        let state = DevToolsState {
            open: true,
            tab: DevToolsTab::Elements,
            dock: DevToolsDock::Bottom,
            size_px: 240,
        };
        apply_devtools_state(&mut chrome, &state);
        layout(&mut chrome);
        let page_id = chrome.get_element_by_id("page").expect("iframe");
        chrome
            .mutate()
            .set_attribute(page_id, qual_name!("srcdoc"), FIXTURE);
        layout(&mut chrome);
        chrome
    }

    fn point_over(chrome: &BaseDocument, id: &str) -> (f32, f32) {
        let (cx, cy) = elements::with_page_document(chrome, |page| {
            let element = page.get_element_by_id(id).expect("element");
            let r = page.get_client_bounding_rect(element).expect("rect");
            (
                r.x as f32 + r.width as f32 / 2.0,
                r.y as f32 + r.height as f32 / 2.0,
            )
        })
        .expect("coords");
        let iframe = page_iframe_id(chrome).expect("iframe");
        let border = chrome
            .get_client_bounding_rect(iframe)
            .expect("iframe rect");
        (border.x as f32 + cx, border.y as f32 + cy)
    }

    fn hit_inset_over(chrome: &BaseDocument, id: &str, inset_x: f32, inset_y: f32) -> (f32, f32) {
        let (cx, cy) = elements::with_page_document(chrome, |page| {
            let element = page.get_element_by_id(id).expect("element");
            let r = page.get_client_bounding_rect(element).expect("rect");
            (r.x as f32 + inset_x, r.y as f32 + inset_y)
        })
        .expect("coords");
        let iframe = page_iframe_id(chrome).expect("iframe");
        let border = chrome
            .get_client_bounding_rect(iframe)
            .expect("iframe rect");
        (border.x as f32 + cx, border.y as f32 + cy)
    }

    #[test]
    fn hit_test_maps_nested_element() {
        let mut chrome = chrome_with_fixture();
        let (x, y) = hit_inset_over(&chrome, "inner", 4.0, 4.0);
        let hit = page_hit_at_chrome_point(&mut chrome, x, y).expect("hit");
        let element = inspectable_element_for_hit(&chrome, hit).expect("element");
        let tag = with_page_document_mut(&mut chrome, |page| {
            page.get_node(element)
                .and_then(|n| n.element_data())
                .map(|e| e.name.local.to_string())
        })
        .flatten()
        .expect("tag");
        assert_eq!(tag, "div");
        let is_inner = with_page_document_mut(&mut chrome, |page| {
            page.get_element_by_id("inner")
                .map(|id| id == element)
                .unwrap_or(false)
        })
        .expect("inner id");
        assert!(
            is_inner,
            "expected hit to map to #inner, got handle {element:?}"
        );
    }

    #[test]
    fn text_hit_maps_to_parent_element() {
        let mut chrome = chrome_with_fixture();
        let (x, y) = point_over(&chrome, "note");
        let hit = page_hit_at_chrome_point(&mut chrome, x, y).expect("hit");
        let element = inspectable_element_for_hit(&chrome, hit).expect("element");
        let is_p = with_page_document_mut(&mut chrome, |page| {
            page.get_element_by_id("note").map(|id| id == element)
        })
        .flatten()
        .expect("page");
        assert!(is_p);
    }

    #[test]
    fn pick_expands_ancestors_and_selects() {
        let mut chrome = chrome_with_fixture();
        let mut tab = crate::tabs::Tab::new(1, "about:blank".into());
        let (x, y) = point_over(&chrome, "link");
        let before_url = tab.url().to_string();
        let handle = pick_at_chrome_point(&mut chrome, &mut tab, x, y).expect("pick");
        assert_eq!(tab.url(), before_url);
        assert!(tab.elements.expanded.contains(&handle));
        let outer_expanded = with_page_document_mut(&mut chrome, |page| {
            page.get_element_by_id("outer")
                .map(|id| tab.elements.expanded.contains(&node_to_handle(id)))
        })
        .flatten()
        .expect("outer expanded");
        assert!(
            outer_expanded,
            "expected #outer in expanded set after picking #link"
        );
        assert_eq!(tab.elements.selected, Some(handle));
    }

    #[test]
    fn pick_does_not_follow_link() {
        let mut chrome = chrome_with_fixture();
        let mut tab = crate::tabs::Tab::new(1, "about:blank".into());
        let (x, y) = point_over(&chrome, "link");
        let mut session = crate::devtools::DevToolsSession {
            inspect_mode: true,
            ..Default::default()
        };
        let over_page = chrome_pointer_targets_page(&chrome, x, y);
        let (decision, next) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: true,
                swallow_pointer_up: false,
            },
            over_page,
            true,
            true,
        );
        assert_eq!(decision, InspectPointerButtonDecision::ConsumePickPress);
        assert!(next.swallow_pointer_up);
        pick_at_chrome_point(&mut chrome, &mut tab, x, y).expect("pick");
        session.exit_inspect_mode_after_pick(&mut chrome);
        session.swallow_pointer_up = true;
        let (release, _) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: false,
                swallow_pointer_up: true,
            },
            false,
            true,
            false,
        );
        assert_eq!(
            release,
            InspectPointerButtonDecision::ConsumeSwallowedRelease
        );
        assert!(tab.url().contains("about:blank") || tab.url().starts_with("file:"));
    }

    #[test]
    fn pointer_gate_cases() {
        let (d, s) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: true,
                swallow_pointer_up: false,
            },
            true,
            true,
            true,
        );
        assert_eq!(d, InspectPointerButtonDecision::ConsumePickPress);
        assert!(s.swallow_pointer_up);

        let (d, _) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: false,
                swallow_pointer_up: true,
            },
            false,
            true,
            false,
        );
        assert_eq!(d, InspectPointerButtonDecision::ConsumeSwallowedRelease);

        let (d, _) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: true,
                swallow_pointer_up: false,
            },
            true,
            true,
            false,
        );
        assert_eq!(d, InspectPointerButtonDecision::Consume);

        let (d, _) = inspect_pointer_button_decision(
            InspectPointerState {
                inspect_mode: false,
                swallow_pointer_up: false,
            },
            true,
            true,
            false,
        );
        assert_eq!(d, InspectPointerButtonDecision::PassThrough);
    }

    #[test]
    fn navigation_exits_inspect_mode_on_commit_effects() {
        use crate::nav::ObserveEffect;
        assert!(navigation_exits_inspect_mode(ObserveEffect::Completed));
        assert!(navigation_exits_inspect_mode(ObserveEffect::LinkNavigation));
        assert!(navigation_exits_inspect_mode(ObserveEffect::Redirect));
        assert!(!navigation_exits_inspect_mode(ObserveEffect::Unchanged));
    }

    #[test]
    fn inspect_label_stores_raw_text_not_escaped() {
        let mut chrome = chrome_with_fixture();
        let (x, y) = point_over(&chrome, "a&b<c\"");
        let hit = page_hit_at_chrome_point(&mut chrome, x, y).expect("hit");
        let element = inspectable_element_for_hit(&chrome, hit).expect("element");
        let label =
            elements::with_page_document(&chrome, |page| inspect_element_label(page, element))
                .expect("label");
        assert!(label.contains("a&b<c\""));
        let rect = elements::with_page_document(&chrome, |page| {
            highlight_for_page_node(&chrome, page, element)
        })
        .flatten()
        .expect("rect");
        apply_inspect_label(&mut chrome, Some(&label), Some(rect));
        let label_el = chrome
            .get_element_by_id("inspect-label")
            .expect("label node");
        let text = chrome
            .get_node(label_el)
            .and_then(|n| n.children.first().copied())
            .and_then(|id| chrome.get_node(id))
            .map(|n| n.text_content())
            .unwrap_or_default();
        assert_eq!(text, label);
    }

    #[test]
    fn chrome_ui_outside_page_is_not_targeted() {
        let chrome = chrome_with_fixture();
        assert!(!chrome_pointer_targets_page(&chrome, 4.0, 4.0));
        let point = |id: &str| {
            let el = chrome.get_element_by_id(id).expect(id);
            let r = chrome.get_client_bounding_rect(el).expect("rect");
            (
                r.x as f32 + r.width as f32 / 2.0,
                r.y as f32 + r.height as f32 / 2.0,
            )
        };
        let (x, y) = point("devtools");
        assert!(!chrome_pointer_targets_page(&chrome, x, y));
        let (x, y) = point("devtools-split");
        assert!(!chrome_pointer_targets_page(&chrome, x, y));

        let mut right = chrome_with_fixture();
        let state = DevToolsState {
            open: true,
            tab: DevToolsTab::Elements,
            dock: DevToolsDock::Right,
            size_px: 240,
        };
        apply_devtools_state(&mut right, &state);
        layout(&mut right);
        let (x, y) = {
            let el = right.get_element_by_id("devtools").expect("devtools");
            let r = right.get_client_bounding_rect(el).expect("rect");
            (
                r.x as f32 + r.width as f32 / 2.0,
                r.y as f32 + r.height as f32 / 2.0,
            )
        };
        assert!(!chrome_pointer_targets_page(&right, x, y));
    }

    #[test]
    fn overlay_uses_layout_mapping() {
        let mut chrome = chrome_with_fixture();
        let (x, y) = point_over(&chrome, "inner");
        let hit = page_hit_at_chrome_point(&mut chrome, x, y).expect("hit");
        let element = inspectable_element_for_hit(&chrome, hit).expect("element");
        let rect = elements::with_page_document(&chrome, |page| {
            highlight_for_page_node(&chrome, page, element)
        })
        .flatten()
        .expect("highlight");
        assert!(rect.is_visible());
        let iframe = highlight::iframe_highlight_origin(&chrome).expect("iframe");
        assert!(rect.top >= iframe.top);
    }

    #[test]
    fn toggle_sets_button_pressed_state() {
        let mut chrome = chrome_with_fixture();
        set_inspect_toggle_active(&mut chrome, true);
        let btn = chrome
            .get_element_by_id("devtools-inspect-toggle")
            .expect("toggle");
        let class = chrome
            .get_node(btn)
            .and_then(|n| n.attr(blitz_dom::local_name!("class")))
            .unwrap_or_default();
        assert!(class.contains("active"));
    }

    #[test]
    fn exit_clears_overlay() {
        let mut chrome = chrome_with_fixture();
        highlight::apply_inspect_highlight(
            &mut chrome,
            Some(HighlightRect {
                left: 1.0,
                top: 2.0,
                width: 10.0,
                height: 10.0,
            }),
        );
        apply_inspect_label(
            &mut chrome,
            Some("div#x"),
            Some(HighlightRect {
                left: 1.0,
                top: 2.0,
                width: 10.0,
                height: 10.0,
            }),
        );
        exit_inspect_visuals(&mut chrome);
        let hl = chrome.get_element_by_id("inspect-hl").expect("hl");
        let hl_style = chrome
            .get_node(hl)
            .and_then(|n| n.attr(blitz_dom::local_name!("style")))
            .unwrap_or_default();
        assert!(hl_style.contains("display:none"), "{hl_style}");
        let label = chrome.get_element_by_id("inspect-label").expect("label");
        let label_style = chrome
            .get_node(label)
            .and_then(|n| n.attr(blitz_dom::local_name!("style")))
            .unwrap_or_default();
        assert!(label_style.contains("display:none"), "{label_style}");
    }

    #[test]
    fn pointer_left_clears_hover_while_inspect_mode_stays() {
        let mut chrome = chrome_with_fixture();
        let session = crate::devtools::DevToolsSession {
            inspect_mode: true,
            state: DevToolsState {
                open: true,
                tab: DevToolsTab::Elements,
                ..Default::default()
            },
            ..Default::default()
        };
        highlight::apply_inspect_highlight(
            &mut chrome,
            Some(HighlightRect {
                left: 5.0,
                top: 5.0,
                width: 20.0,
                height: 10.0,
            }),
        );
        exit_inspect_visuals(&mut chrome);
        assert!(session.inspect_mode);
        let hl = chrome.get_element_by_id("inspect-hl").expect("hl");
        let style = chrome
            .get_node(hl)
            .and_then(|n| n.attr(blitz_dom::local_name!("style")))
            .unwrap_or_default();
        assert!(style.contains("display:none"));
    }
}
