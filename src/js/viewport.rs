//! Viewport scroll state and scrolling, shared by the page-script RPC and the control session.
//!
//! Both callers move the same Blitz viewport offset through Blitz's single scroll primitive
//! (`BaseDocument::scroll_to`/`scroll_by` on the root element, clamped to the document, the
//! same path the window wheel takes), so a scroll from `window.scrollTo`, from an agent and
//! from the wheel all read back as one `window.scrollY`. Note `BaseDocument::scroll_viewport_by`
//! follows the wheel sign convention (positive moves the content down, i.e. scrolls up), so it
//! is not used here.

use blitz_dom::{BaseDocument, ScrollBehavior, ScrollLogicalPosition};
use blitz_traits::node_id::NodeId;

/// The viewport's scroll offset, the scrollable document size and the viewport size, all in
/// CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportState {
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub scroll_width: f64,
    pub scroll_height: f64,
    pub width: f64,
    pub height: f64,
}

impl ViewportState {
    /// JSON shape the page runtime reads (`scrollX`, `scrollY`, `scrollWidth`, ...).
    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "scrollX": self.scroll_x,
            "scrollY": self.scroll_y,
            "scrollWidth": self.scroll_width,
            "scrollHeight": self.scroll_height,
            "innerWidth": self.width,
            "innerHeight": self.height,
        })
    }
}

/// Current viewport state after bringing layout up to date.
pub fn state(doc: &mut BaseDocument) -> ViewportState {
    doc.resolve(0.0);
    let scroll = doc.viewport_scroll();
    let layout = *doc.root_element().final_layout();
    let content_width = f64::from(layout.size.width.max(layout.scrollable_overflow_rect.right));
    let content_height = f64::from(
        layout
            .size
            .height
            .max(layout.scrollable_overflow_rect.bottom),
    );
    let (width, height) = doc.viewport().logical_size();
    let (width, height) = (f64::from(width), f64::from(height));
    ViewportState {
        scroll_x: scroll.x,
        scroll_y: scroll.y,
        scroll_width: content_width.max(width),
        scroll_height: content_height.max(height),
        width,
        height,
    }
}

/// Scroll by a delta, clamped to the document, and return the new state.
pub fn scroll_by(doc: &mut BaseDocument, dx: f64, dy: f64) -> ViewportState {
    doc.resolve(0.0);
    let root = doc.root_element().id;
    doc.scroll_by(root, dx, dy, ScrollBehavior::Instant);
    state(doc)
}

/// Scroll to an absolute offset, clamped to the document, and return the new state.
pub fn scroll_to(doc: &mut BaseDocument, x: f64, y: f64) -> ViewportState {
    doc.resolve(0.0);
    let root = doc.root_element().id;
    doc.scroll_to(root, x, y, ScrollBehavior::Instant);
    state(doc)
}

/// Scroll so `node` is aligned per `block` (`start`, `center`, `end`, `nearest`).
pub fn scroll_into_view(doc: &mut BaseDocument, node: NodeId, block: &str) -> ViewportState {
    doc.resolve(0.0);
    let vertical = match block {
        "center" => ScrollLogicalPosition::Center,
        "end" => ScrollLogicalPosition::End,
        "nearest" => ScrollLogicalPosition::Nearest,
        _ => ScrollLogicalPosition::Start,
    };
    doc.scroll_into_view(
        node,
        ScrollBehavior::Instant,
        vertical,
        ScrollLogicalPosition::Nearest,
    );
    state(doc)
}

/// Scroll the first element matching `selector` into view (top-aligned).
pub fn scroll_to_selector(doc: &mut BaseDocument, selector: &str) -> Result<ViewportState, String> {
    doc.resolve(0.0);
    let id = doc
        .query_selector(selector)
        .map_err(|_| format!("invalid selector `{selector}`"))?
        .ok_or_else(|| format!("no element matches selector `{selector}`"))?;
    Ok(scroll_into_view(doc, id, "start"))
}

/// Scroll a non-root element to an absolute offset (clamped to its overflow) and return its
/// resulting `(scrollLeft, scrollTop)`. The root element scrolls the viewport instead.
pub fn scroll_element_to(doc: &mut BaseDocument, node: NodeId, x: f64, y: f64) -> (f64, f64) {
    doc.resolve(0.0);
    doc.scroll_to(node, x, y, ScrollBehavior::Instant);
    doc.get_node(node)
        .map(|n| {
            let offset = n.scroll_offset();
            (offset.x, offset.y)
        })
        .unwrap_or((0.0, 0.0))
}
