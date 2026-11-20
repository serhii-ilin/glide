//! DOM operation protocol between the V8 worker and the UI thread.

use std::collections::HashMap;

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
    DocumentUrl {
        reply: u64,
    },
    Fetch {
        url: String,
        reply: u64,
    },
}

#[derive(Debug, Clone)]
pub enum DomValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    NodeId(u64),
    NodeList(Vec<u64>),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DomResponse {
    pub id: u64,
    pub value: DomValue,
}

impl DomResponse {
    pub fn ok(id: u64, value: DomValue) -> Self {
        Self { id, value }
    }

    pub fn err(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
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

/// In-memory listener registry on the worker thread (node handle, event type).
#[derive(Default)]
pub struct ListenerTable {
    next_id: u64,
    pub by_id: HashMap<u64, (u64, String)>,
}

impl ListenerTable {
    pub fn register(&mut self, node: u64, event: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.by_id.insert(id, (node, event));
        id
    }
}