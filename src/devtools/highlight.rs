//! DevTools element highlight overlay (`#inspect-hl`) in chrome coordinates.

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, qual_name};
use blitz_traits::node_id::NodeId;

use crate::devtools::elements::handle_to_node;
use crate::find::page_iframe_id;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl HighlightRect {
    pub fn is_visible(self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

/// Map a page border box (document/layout coordinates), viewport scroll, iframe
/// placement, and zoom scales to the fixed `#inspect-hl` overlay rect in chrome.
///
/// `ui_scale` is the chrome `--ui` counter-scale (`1.0` at 100% page zoom). `page_zoom`
/// is the active page zoom factor applied to the chrome viewport (iframe content).
pub fn overlay_rect_from_parts(
    layout_box: HighlightRect,
    scroll_x: f32,
    scroll_y: f32,
    iframe_origin: HighlightRect,
    ui_scale: f32,
    page_zoom: f32,
) -> HighlightRect {
    let ui = ui_scale.max(f32::EPSILON);
    let zoom = page_zoom.max(f32::EPSILON);
    let client_left = layout_box.left - scroll_x;
    let client_top = layout_box.top - scroll_y;
    let content_scale = zoom / ui;
    HighlightRect {
        left: iframe_origin.left + client_left * content_scale,
        top: iframe_origin.top + client_top * content_scale,
        width: layout_box.width * content_scale,
        height: layout_box.height * content_scale,
    }
}

/// Map a viewport-relative page client rect into chrome overlay coordinates.
pub fn map_page_rect_to_chrome(
    page_client_rect: HighlightRect,
    iframe_origin: HighlightRect,
) -> HighlightRect {
    overlay_rect_from_parts(page_client_rect, 0.0, 0.0, iframe_origin, 1.0, 1.0)
}

pub fn page_layout_border_box(page: &BaseDocument, node_id: NodeId) -> Option<HighlightRect> {
    let client = page.get_client_bounding_rect(node_id)?;
    let scroll = page.viewport_scroll();
    Some(HighlightRect {
        left: client.x as f32 + scroll.x as f32,
        top: client.y as f32 + scroll.y as f32,
        width: client.width as f32,
        height: client.height as f32,
    })
}

pub fn page_element_highlight_rect(page: &BaseDocument, node_id: NodeId) -> Option<HighlightRect> {
    let node = page.get_node(node_id)?;
    if !matches!(
        &node.data,
        NodeData::Element(_) | NodeData::AnonymousBlock(_)
    ) {
        return None;
    }
    let border = page.get_client_bounding_rect(node_id)?;
    Some(HighlightRect {
        left: border.x as f32,
        top: border.y as f32,
        width: border.width as f32,
        height: border.height as f32,
    })
}

pub fn iframe_highlight_origin(chrome: &BaseDocument) -> Option<HighlightRect> {
    let iframe = page_iframe_id(chrome)?;
    let border = chrome.get_client_bounding_rect(iframe)?;
    Some(HighlightRect {
        left: border.x as f32,
        top: border.y as f32,
        width: border.width as f32,
        height: border.height as f32,
    })
}

pub fn highlight_for_selected(
    chrome: &BaseDocument,
    page: &BaseDocument,
    selected: Option<u64>,
) -> Option<HighlightRect> {
    let handle = selected?;
    let node_id = handle_to_node(handle);
    page.get_node(node_id)?;
    let layout = page_layout_border_box(page, node_id)?;
    let iframe = iframe_highlight_origin(chrome)?;
    let scroll = page.viewport_scroll();
    let page_zoom = chrome.viewport().zoom();
    let ui_scale = 1.0 / page_zoom.max(f32::EPSILON);
    Some(overlay_rect_from_parts(
        layout,
        scroll.x as f32,
        scroll.y as f32,
        iframe,
        ui_scale,
        page_zoom,
    ))
}

pub fn highlight_css_style(rect: HighlightRect) -> String {
    format!(
        "display:block;left:{}px;top:{}px;width:{}px;height:{}px;",
        rect.left, rect.top, rect.width, rect.height
    )
}

pub fn apply_inspect_highlight(chrome: &mut BaseDocument, rect: Option<HighlightRect>) {
    let Some(hl_id) = chrome.get_element_by_id("inspect-hl") else {
        return;
    };
    let (style, class) = match rect.filter(|r| r.is_visible()) {
        Some(r) => (highlight_css_style(r), "active"),
        None => (String::from("display:none;"), ""),
    };
    let mut m = chrome.mutate();
    m.set_attribute(hl_id, qual_name!("style"), &style);
    m.set_attribute(hl_id, qual_name!("class"), class);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::chrome::{apply_devtools_state, chrome_document, chrome_document_from_state};
    use crate::devtools::{DevToolsDock, DevToolsState, DevToolsTab};
    use crate::html::GlideHtmlProvider;
    use crate::parse;
    use blitz_dom::{DocumentConfig, local_name};
    use blitz_traits::shell::{ColorScheme, Viewport};

    fn sample_page(html: &str) -> BaseDocument {
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        doc.resolve(0.0);
        doc
    }

    fn parse_chrome(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1100, 760, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        parse::document_from_html(html, config, false)
    }

    fn chrome_with_devtools(dock: DevToolsDock, size_px: i32) -> BaseDocument {
        let html = chrome_document("about:blank", true, false);
        let mut chrome = parse_chrome(&html);
        chrome.resolve(0.0);
        apply_devtools_state(
            &mut chrome,
            &DevToolsState {
                open: true,
                tab: DevToolsTab::Elements,
                dock,
                size_px,
            },
        );
        chrome.resolve(0.0);
        chrome
    }

    #[test]
    fn overlay_scroll_offset_moves_top_up() {
        let layout = HighlightRect {
            left: 30.0,
            top: 400.0,
            width: 40.0,
            height: 20.0,
        };
        let iframe = HighlightRect {
            left: 10.0,
            top: 90.0,
            width: 800.0,
            height: 500.0,
        };
        let at_rest = overlay_rect_from_parts(layout, 0.0, 0.0, iframe, 1.0, 1.0);
        let scrolled = overlay_rect_from_parts(layout, 0.0, 350.0, iframe, 1.0, 1.0);
        assert_eq!(at_rest.top, 490.0);
        assert_eq!(scrolled.top, 140.0);
        assert!(scrolled.top < at_rest.top);
        assert_eq!(at_rest.left, scrolled.left);
    }

    #[test]
    fn overlay_page_zoom_scales_size() {
        let layout = HighlightRect {
            left: 0.0,
            top: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let iframe = HighlightRect {
            left: 0.0,
            top: 0.0,
            width: 500.0,
            height: 500.0,
        };
        let zoomed = overlay_rect_from_parts(layout, 0.0, 0.0, iframe, 0.5, 2.0);
        assert_eq!(zoomed.width, 400.0);
        assert_eq!(zoomed.height, 200.0);
    }

    #[test]
    fn map_page_rect_adds_iframe_offset() {
        let page = HighlightRect {
            left: 10.0,
            top: 20.0,
            width: 50.0,
            height: 30.0,
        };
        let iframe = HighlightRect {
            left: 100.0,
            top: 200.0,
            width: 800.0,
            height: 600.0,
        };
        let mapped = map_page_rect_to_chrome(page, iframe);
        assert_eq!(mapped.left, 110.0);
        assert_eq!(mapped.top, 220.0);
    }

    #[test]
    fn highlight_wiring_matches_overlay_from_parts() {
        let page_html = r#"<!DOCTYPE html><html><body><div id="target" style="margin-top:80px;width:40px;height:20px"></div></body></html>"#;
        let page = sample_page(page_html);
        let target = page.get_element_by_id("target").expect("target");
        let layout = page_layout_border_box(&page, target).expect("layout");
        let scroll = page.viewport_scroll();
        let chrome = chrome_with_devtools(DevToolsDock::Bottom, 280);
        let iframe = iframe_highlight_origin(&chrome).expect("iframe");
        let zoom = chrome.viewport().zoom();
        let ui = 1.0 / zoom;
        let expected =
            overlay_rect_from_parts(layout, scroll.x as f32, scroll.y as f32, iframe, ui, zoom);
        let wired = highlight_for_selected(
            &chrome,
            &page,
            Some(crate::devtools::elements::node_to_handle(target)),
        );
        assert_eq!(wired, Some(expected));
    }

    #[test]
    fn iframe_origin_differs_bottom_vs_right_dock() {
        const PANEL_PX: i32 = 300;
        let bottom = chrome_with_devtools(DevToolsDock::Bottom, PANEL_PX);
        let right = chrome_with_devtools(DevToolsDock::Right, PANEL_PX);
        let iframe_bottom = iframe_highlight_origin(&bottom).expect("bottom iframe");
        let iframe_right = iframe_highlight_origin(&right).expect("right iframe");
        assert!(iframe_bottom.is_visible() && iframe_right.is_visible());
        assert!(
            iframe_bottom.height < iframe_right.height,
            "bottom dock shrinks iframe height: bottom={iframe_bottom:?} right={iframe_right:?}"
        );
        assert!(
            iframe_bottom.width > iframe_right.width,
            "right dock shrinks iframe width: bottom={iframe_bottom:?} right={iframe_right:?}"
        );
        let height_delta = iframe_right.height - iframe_bottom.height;
        let width_delta = iframe_bottom.width - iframe_right.width;
        assert!(
            (height_delta - PANEL_PX as f32).abs() < 40.0,
            "bottom panel consumes vertical space: delta={height_delta}"
        );
        assert!(
            (width_delta - PANEL_PX as f32).abs() < 40.0,
            "right panel consumes horizontal space: delta={width_delta}"
        );
        let (layout_w_bottom, layout_h_bottom) =
            crate::chrome::node_layout_size(&bottom, "page").expect("bottom page layout");
        let (layout_w_right, layout_h_right) =
            crate::chrome::node_layout_size(&right, "page").expect("right page layout");
        assert!((layout_w_bottom - iframe_bottom.width).abs() < 2.0);
        assert!((layout_h_bottom - iframe_bottom.height).abs() < 2.0);
        assert!((layout_w_right - iframe_right.width).abs() < 2.0);
        assert!((layout_h_right - iframe_right.height).abs() < 2.0);
        let page_client = HighlightRect {
            left: 8.0,
            top: 12.0,
            width: 40.0,
            height: 20.0,
        };
        let hl_bottom = map_page_rect_to_chrome(page_client, iframe_bottom);
        let hl_right = map_page_rect_to_chrome(page_client, iframe_right);
        assert_eq!(hl_bottom.left, iframe_bottom.left + page_client.left);
        assert_eq!(hl_bottom.top, iframe_bottom.top + page_client.top);
        assert_eq!(hl_right.left, iframe_right.left + page_client.left);
        assert_eq!(hl_right.top, iframe_right.top + page_client.top);
    }

    #[test]
    fn overlay_hidden_when_inactive() {
        let chrome_html = chrome_document_from_state(&crate::chrome::ChromeViewState::default());
        let mut chrome = parse_chrome(&chrome_html);
        chrome.resolve(0.0);
        apply_inspect_highlight(&mut chrome, None);
        let hl = chrome.get_element_by_id("inspect-hl").expect("hl");
        let style = chrome
            .get_node(hl)
            .and_then(|n| n.element_data())
            .and_then(|e| e.attr(local_name!("style")))
            .unwrap_or("");
        assert!(style.contains("display:none"));
    }
}
