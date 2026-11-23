//! DOM operation protocol between the V8 worker and the UI thread.

use blitz_dom::BaseDocument;
use blitz_traits::node_id::NodeId;

/// Requests the UI thread applies against a page [`BaseDocument`](blitz_dom::BaseDocument).
///
/// Page scripts talk to Blitz through domino. The only mutation entry point is
/// [`DomRequest::BatchMutate`]. Per-node wrappers (`appendChild`, `setInnerHtml`, …)
/// are intentionally absent so page code cannot bypass domino and desync the trees.
#[derive(Debug, Clone)]
pub enum DomRequest {
    Fetch(FetchRequest),
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
    /// Submit a form the way the page's own `requestSubmit()` / `submit()`
    /// would: build the entry list and navigate through the document's
    /// navigation provider (the iframe's provider in the window).
    SubmitForm {
        form: u64,
        submitter: u64,
        reply: u64,
    },
    /// Live `value` / `checked` from Blitz for domino getters (F-039).
    ControlField {
        node: u64,
        field: String,
        reply: u64,
    },
    /// Blitz control state for every field in a form before submit (F-039).
    SyncFormControls {
        form: u64,
        reply: u64,
    },
    /// Domino control `value` strings applied to Blitz before form submit (F-078).
    PushFormValues {
        json: String,
        reply: u64,
    },
    /// Entry list for `new FormData(form)` (F-082).
    FormDataEntries {
        form: u64,
        submitter: u64,
        reply: u64,
    },
    ListenTarget {
        node: u64,
        event_type: String,
        reply: u64,
    },
    /// Cryptographically secure random bytes for `crypto.getRandomValues`.
    RandomBytes {
        size: usize,
        reply: u64,
    },
    /// `document.cookie` getter (F-058).
    CookieGet {
        reply: u64,
    },
    /// `document.cookie` setter (F-058).
    CookieSet {
        value: String,
        reply: u64,
    },
    /// Web Storage RPC (`localStorage` / `sessionStorage`, F-081).
    StorageCall {
        method: String,
        area: String,
        key: Option<String>,
        value: Option<String>,
        index: Option<u32>,
        page_url: String,
        tab_id: u64,
        opaque_document_id: u64,
        source_generation: u64,
        reply: u64,
    },
    WebSocketConnect {
        socket_id: u64,
        url: String,
        protocols: String,
        reply: u64,
    },
    WebSocketSend {
        socket_id: u64,
        data: String,
        binary: bool,
        reply: u64,
    },
    WebSocketClose {
        socket_id: u64,
        code: u16,
        reason: String,
        reply: u64,
    },
    WebSocketBuffered {
        socket_id: u64,
        reply: u64,
    },
    /// Test hook: number of live Blitz nodes. Dispatched only from debug builds.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    NodeCount {
        reply: u64,
    },
    /// Test hook: resize the viewport without a DOM mutation.
    /// Release builds do not construct this variant.
    #[cfg(debug_assertions)]
    TestResize {
        width: u32,
        height: u32,
        reply: u64,
    },
}

/// A page `fetch()` / dynamic script source as the JS worker sends it.
///
/// The host dispatches it on tokio rather than applying it synchronously, then
/// sends the [`DomResponse`] back through the worker's reply channel.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub credentials: String,
    pub reply: u64,
    pub initiator: crate::devtools::network::NetworkInitiator,
}

#[derive(Debug, Clone)]
pub enum DomValue {
    Null,
    String(String),
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
