//! DOM operation protocol between the V8 worker and the UI thread.

use blitz_dom::BaseDocument;
use blitz_traits::node_id::NodeId;

/// Requests the UI thread applies against a page [`BaseDocument`](blitz_dom::BaseDocument).
#[derive(Debug, Clone)]
pub enum DomRequest {
    GetElementById {
        id: String,
        reply: u64,
    },
    QuerySelector {
        selector: String,
        reply: u64,
    },
    QuerySelectorAll {
        selector: String,
        reply: u64,
    },
    CreateElement {
        tag: String,
        reply: u64,
    },
    CreateTextNode {
        text: String,
        reply: u64,
    },
    AppendChild {
        parent: u64,
        child: u64,
        reply: u64,
    },
    RemoveChild {
        parent: u64,
        child: u64,
        reply: u64,
    },
    InsertBefore {
        parent: u64,
        child: u64,
        before: u64,
        reply: u64,
    },
    SetAttribute {
        node: u64,
        name: String,
        value: String,
        reply: u64,
    },
    GetAttribute {
        node: u64,
        name: String,
        reply: u64,
    },
    RemoveAttribute {
        node: u64,
        name: String,
        reply: u64,
    },
    SetInnerHtml {
        node: u64,
        html: String,
        reply: u64,
    },
    GetTextContent {
        node: u64,
        reply: u64,
    },
    SetTextContent {
        node: u64,
        text: String,
        reply: u64,
    },
    ClassListAdd {
        node: u64,
        token: String,
        reply: u64,
    },
    ClassListRemove {
        node: u64,
        token: String,
        reply: u64,
    },
    ClassListContains {
        node: u64,
        token: String,
        reply: u64,
    },
    StyleSet {
        node: u64,
        name: String,
        value: String,
        reply: u64,
    },
    ParentId {
        node: u64,
        reply: u64,
    },
    Fetch {
        url: String,
        reply: u64,
    },
    SnapshotTree {
        reply: u64,
    },
    BatchMutate {
        json: String,
        reply: u64,
    },
    BoundingRect {
        node: u64,
        reply: u64,
    },
    ComputedStyle {
        node: u64,
        prop: String,
        reply: u64,
    },
    LayoutField {
        node: u64,
        field: String,
        reply: u64,
    },
    PageUrl {
        reply: u64,
    },
    Navigate {
        url: String,
        reply: u64,
    },
    ListenTarget {
        node: u64,
        event_type: String,
        reply: u64,
    },
}

#[derive(Debug, Clone)]
pub enum DomValue {
    Null,
    Bool(bool),
    String(String),
    NodeId(u64),
    NodeList(Vec<u64>),
    Number(f64),
    Json(serde_json::Value),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DomResponse {
    pub value: DomValue,
}

impl DomResponse {
    pub fn ok(_reply: u64, value: DomValue) -> Self {
        Self { value }
    }

    pub fn err(_reply: u64, message: impl Into<String>) -> Self {
        Self {
            value: DomValue::Error(message.into()),
        }
    }
}

pub fn node_to_handle(id: NodeId) -> u64 {
    id.as_u64()
}

pub fn handle_to_node(raw: u64) -> NodeId {
    NodeId::from_u64(raw)
}

/// Returns `None` when the handle does not resolve in `doc` (stale slot / wrong generation).
pub fn resolve_handle(doc: &BaseDocument, raw: u64) -> Option<NodeId> {
    let id = handle_to_node(raw);
    doc.get_node(id).map(|_| id)
}

pub fn bubble_handles(doc: &BaseDocument, target: NodeId) -> Vec<u64> {
    let mut path = Vec::new();
    let mut cur = Some(target);
    while let Some(id) = cur {
        if doc.get_node(id).is_none() {
            break;
        }
        path.push(node_to_handle(id));
        cur = doc.get_node(id).and_then(|n| n.parent);
    }
    path
}
