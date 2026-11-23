//! Apply [`DomRequest`] values on the UI thread.
//!
//! [`DomRequest::Fetch`] is the exception: [`start_fetch`] dispatches it on tokio so
//! the UI thread never waits for the network.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use blitz_dom::BaseDocument;
use blitz_net::Provider;
use blitz_traits::net::http::{self, HeaderMap, Method};
use blitz_traits::net::{Body, Bytes, Request};
use url::Url;

const FORBIDDEN_RESPONSE_HEADERS: &[&str] = &["set-cookie", "set-cookie2"];
const FORBIDDEN_REQUEST_HEADERS: &[&str] = &[
    "cookie",
    "host",
    "content-length",
    "connection",
    "user-agent",
    "referer",
    "origin",
];

use super::bridge::{DomRequest, DomResponse, DomValue, FetchRequest};

pub fn apply_request(doc: &mut BaseDocument, base_url: &str, req: DomRequest) -> DomResponse {
    let reply = reply_id(&req);
    let result = panic::catch_unwind(AssertUnwindSafe(|| apply_inner(doc, base_url, req)));
    match result {
        Ok(resp) => resp,
        Err(_) => DomResponse::err(reply, "dom operation panicked"),
    }
}

pub(crate) fn reply_id(req: &DomRequest) -> u64 {
    match req {
        DomRequest::Fetch(req) => req.reply,
        DomRequest::SnapshotTree { reply }
        | DomRequest::BatchMutate { reply, .. }
        | DomRequest::BoundingRect { reply, .. }
        | DomRequest::ComputedStyle { reply, .. }
        | DomRequest::LayoutField { reply, .. }
        | DomRequest::PageUrl { reply }
        | DomRequest::Navigate { reply, .. }
        | DomRequest::SubmitForm { reply, .. }
        | DomRequest::ControlField { reply, .. }
        | DomRequest::SyncFormControls { reply, .. }
        | DomRequest::PushFormValues { reply, .. }
        | DomRequest::FormDataEntries { reply, .. }
        | DomRequest::ListenTarget { reply, .. }
        | DomRequest::RandomBytes { reply, .. }
        | DomRequest::CookieGet { reply }
        | DomRequest::CookieSet { reply, .. }
        | DomRequest::StorageCall { reply, .. }
        | DomRequest::WebSocketConnect { reply, .. }
        | DomRequest::WebSocketSend { reply, .. }
        | DomRequest::WebSocketClose { reply, .. }
        | DomRequest::WebSocketBuffered { reply, .. }
        | DomRequest::NodeCount { reply } => *reply,
        #[cfg(debug_assertions)]
        DomRequest::TestResize { reply, .. } => *reply,
    }
}

fn apply_inner(doc: &mut BaseDocument, base_url: &str, req: DomRequest) -> DomResponse {
    match req {
        DomRequest::SnapshotTree { reply } => {
            let tree = super::snapshot::snapshot_tree(doc);
            DomResponse::ok(reply, DomValue::Json(tree))
        }
        DomRequest::BatchMutate { json, reply } => super::batch::apply_batch(doc, &json, reply),
        DomRequest::BoundingRect { node, reply } => {
            super::batch::reply_bounding_rect(doc, node, reply)
        }
        DomRequest::ComputedStyle { node, prop, reply } => {
            super::batch::reply_computed_style(doc, node, &prop, reply)
        }
        DomRequest::LayoutField { node, field, reply } => {
            super::batch::reply_layout_number(doc, node, &field, reply)
        }
        DomRequest::PageUrl { reply } => {
            DomResponse::ok(reply, DomValue::String(base_url.to_string()))
        }
        DomRequest::Navigate { url, reply } => {
            navigate(doc, base_url, &url);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::SubmitForm {
            form,
            submitter,
            reply,
        } => {
            if let (Some(form), Some(submitter)) = (
                super::bridge::resolve_handle(doc, form),
                super::bridge::resolve_handle(doc, submitter),
            ) && doc.get_node(form).is_some_and(|node| {
                node.data
                    .is_element_with_tag_name(&blitz_dom::local_name!("form"))
            }) {
                doc.submit_form(form, submitter);
            }
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::ControlField { node, field, reply } => {
            super::form_controls::reply_control_field(doc, node, &field, reply)
        }
        DomRequest::SyncFormControls { form, reply } => {
            let Some(form_id) = super::bridge::resolve_handle(doc, form) else {
                return DomResponse::ok(reply, DomValue::Json(serde_json::json!([])));
            };
            let rows = super::form_controls::sync_form_controls(doc, form_id);
            let json: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(id, value, checked)| {
                    serde_json::json!({
                        "id": id,
                        "value": value,
                        "checked": checked,
                    })
                })
                .collect();
            DomResponse::ok(reply, DomValue::Json(serde_json::Value::Array(json)))
        }
        DomRequest::PushFormValues { json, reply } => {
            let pairs: Vec<(u64, String)> = serde_json::from_str(&json).unwrap_or_default();
            super::form_controls::push_form_values(doc, &pairs);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::FormDataEntries {
            form,
            submitter,
            reply,
        } => {
            let Some(form_id) = super::bridge::resolve_handle(doc, form) else {
                return DomResponse::err(reply, "invalid form");
            };
            let submitter_id = super::bridge::resolve_handle(doc, submitter).unwrap_or(form_id);
            super::form_data::reply_form_data_entries(doc, form_id, submitter_id, reply)
        }
        DomRequest::RandomBytes { size, reply } => apply_random_bytes(size, reply),
        DomRequest::CookieGet { reply } => apply_cookie_get(base_url, reply),
        DomRequest::CookieSet { value, reply } => apply_cookie_set(base_url, &value, reply),
        DomRequest::StorageCall {
            method,
            area,
            key,
            value,
            index,
            page_url,
            tab_id,
            opaque_document_id,
            source_generation,
            reply,
        } => apply_storage_call(
            &method,
            &area,
            key.as_deref(),
            value.as_deref(),
            index,
            &page_url,
            tab_id,
            opaque_document_id,
            source_generation,
            reply,
        ),
        DomRequest::ListenTarget {
            node,
            event_type,
            reply,
        } => {
            let _ = (node, event_type);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::NodeCount { reply } => DomResponse::ok(
            reply,
            DomValue::Number(super::batch::live_node_count(doc) as f64),
        ),
        #[cfg(debug_assertions)]
        DomRequest::TestResize {
            width,
            height,
            reply,
        } => {
            {
                let mut viewport = doc.viewport_mut();
                viewport.window_size = (width, height);
            }
            DomResponse::ok(reply, DomValue::Null)
        }
        // `Fetch` never reaches this point: [`start_fetch`] dispatches it on tokio
        // from [`JsHost`](super::host::JsHost) so the UI thread never waits for
        // the network.
        DomRequest::Fetch(req) => {
            DomResponse::err(req.reply, "fetch must be dispatched off the UI thread")
        }
        DomRequest::WebSocketConnect { reply, .. }
        | DomRequest::WebSocketSend { reply, .. }
        | DomRequest::WebSocketClose { reply, .. }
        | DomRequest::WebSocketBuffered { reply, .. } => {
            DomResponse::err(reply, "websocket rpc must be dispatched off the UI thread")
        }
    }
}

pub fn start_websocket_connect(
    page_base: &str,
    socket_id: u64,
    url: String,
    protocols: String,
    dom_generation: u64,
    reply: u64,
    reply_tx: crossbeam_channel::Sender<DomResponse>,
) {
    let send = |value: DomResponse| {
        let _ = reply_tx.send(value);
    };
    match super::websocket::start_connect(socket_id, page_base, &url, &protocols, dom_generation) {
        Ok(serialized_url) => send(DomResponse::ok(reply, DomValue::String(serialized_url))),
        Err(err) => send(DomResponse::err(reply, err)),
    }
}

pub fn apply_websocket_send(socket_id: u64, data: &str, binary: bool, reply: u64) -> DomResponse {
    match super::websocket::send_message(socket_id, data, binary) {
        Ok(n) => DomResponse::ok(reply, DomValue::Number(n as f64)),
        Err(err) => DomResponse::err(reply, err),
    }
}

pub fn apply_websocket_close(socket_id: u64, code: u16, reason: &str, reply: u64) -> DomResponse {
    match super::websocket::close_socket(socket_id, code, reason) {
        Ok(()) => DomResponse::ok(reply, DomValue::Null),
        Err(err) => DomResponse::err(reply, err),
    }
}

pub fn apply_websocket_buffered(socket_id: u64, reply: u64) -> DomResponse {
    match super::websocket::buffered_amount(socket_id) {
        Ok(n) => DomResponse::ok(reply, DomValue::Number(n as f64)),
        Err(err) => DomResponse::err(reply, err),
    }
}

/// Hand a page-initiated navigation (a `location` assignment, a form submit)
/// to the document's [`NavigationProvider`](blitz_traits::navigation::NavigationProvider).
/// In the window the page document's provider forwards the URL to the chrome
/// iframe; a headless document may have no provider.
fn navigate(doc: &BaseDocument, base_url: &str, raw: &str) {
    let parsed =
        url::Url::parse(raw).or_else(|_| url::Url::parse(base_url).and_then(|base| base.join(raw)));
    let Ok(parsed) = parsed else {
        return;
    };
    if parsed.scheme() == "javascript" {
        return;
    }
    doc.navigation_provider
        .navigate_to(blitz_traits::navigation::NavigationOptions::new(
            parsed,
            None,
            doc.id(),
        ));
}

const MAX_RANDOM_BYTES: usize = 65_536;

#[allow(clippy::too_many_arguments)] // one DOM RPC maps many storage methods
fn apply_storage_call(
    method: &str,
    area: &str,
    key: Option<&str>,
    value: Option<&str>,
    index: Option<u32>,
    page_url: &str,
    tab_id: u64,
    opaque_document_id: u64,
    source_generation: u64,
    reply: u64,
) -> DomResponse {
    let store = crate::web_storage::WebStorage::global();
    let area = match crate::web_storage::parse_area(area) {
        Ok(a) => a,
        Err(err) => return DomResponse::err(reply, err),
    };
    if page_url.is_empty() {
        return DomResponse::err(reply, "missing page url");
    }
    let page = page_url.to_string();
    match method {
        "attach" => {
            let Ok(parsed) = Url::parse(&page) else {
                return DomResponse::ok(reply, DomValue::Null);
            };
            let origin = if parsed.scheme() == "data" || !parsed.origin().is_tuple() {
                return DomResponse::ok(reply, DomValue::Null);
            } else {
                parsed.origin().ascii_serialization()
            };
            crate::js::storage_events::attach(source_generation, origin, page, tab_id);
            DomResponse::ok(reply, DomValue::Null)
        }
        "snapshot" => match store.snapshot_json(&page, tab_id, opaque_document_id) {
            Ok(json) => DomResponse::ok(reply, DomValue::Json(json)),
            Err(err) => DomResponse::err(reply, err),
        },
        "getItem" => {
            let Some(key) = key else {
                return DomResponse::err(reply, "missing key");
            };
            match store.get_item(area, key, &page, tab_id, opaque_document_id) {
                Ok(None) => DomResponse::ok(reply, DomValue::Null),
                Ok(Some(v)) => DomResponse::ok(reply, DomValue::String(v)),
                Err(err) => DomResponse::err(reply, err),
            }
        }
        "setItem" => {
            let Some(key) = key else {
                return DomResponse::err(reply, "missing key");
            };
            let value = value.unwrap_or("");
            match store.set_item(
                area,
                key,
                value,
                &page,
                tab_id,
                opaque_document_id,
                source_generation,
            ) {
                Ok(()) => DomResponse::ok(reply, DomValue::Null),
                Err(err) if err == "QuotaExceededError" => DomResponse::err(reply, err),
                Err(err) => DomResponse::err(reply, err),
            }
        }
        "removeItem" => {
            let Some(key) = key else {
                return DomResponse::err(reply, "missing key");
            };
            match store.remove_item(
                area,
                key,
                &page,
                tab_id,
                opaque_document_id,
                source_generation,
            ) {
                Ok(()) => DomResponse::ok(reply, DomValue::Null),
                Err(err) => DomResponse::err(reply, err),
            }
        }
        "clear" => match store.clear(area, &page, tab_id, opaque_document_id, source_generation) {
            Ok(()) => DomResponse::ok(reply, DomValue::Null),
            Err(err) => DomResponse::err(reply, err),
        },
        "length" => match store.length(area, &page, tab_id, opaque_document_id) {
            Ok(n) => DomResponse::ok(reply, DomValue::Number(n as f64)),
            Err(err) => DomResponse::err(reply, err),
        },
        "key" => {
            let index = index.unwrap_or(0);
            match store.key_at(area, index, &page, tab_id, opaque_document_id) {
                Ok(None) => DomResponse::ok(reply, DomValue::Null),
                Ok(Some(k)) => DomResponse::ok(reply, DomValue::String(k)),
                Err(err) => DomResponse::err(reply, err),
            }
        }
        other => DomResponse::err(reply, format!("unknown storage method: {other}")),
    }
}

fn apply_cookie_get(base_url: &str, reply: u64) -> DomResponse {
    let Ok(page_url) = Url::parse(base_url) else {
        return DomResponse::ok(reply, DomValue::String(String::new()));
    };
    let value = crate::cookies::CookieJar::global().document_cookie(&page_url);
    DomResponse::ok(reply, DomValue::String(value))
}

fn apply_cookie_set(base_url: &str, assignment: &str, reply: u64) -> DomResponse {
    let Ok(page_url) = Url::parse(base_url) else {
        return DomResponse::err(reply, "invalid document url");
    };
    crate::cookies::CookieJar::global().set_document_cookie(&page_url, assignment);
    DomResponse::ok(reply, DomValue::Null)
}

fn apply_random_bytes(size: usize, reply: u64) -> DomResponse {
    if size > MAX_RANDOM_BYTES {
        return DomResponse::err(reply, "QuotaExceededError");
    }
    let mut buf = vec![0u8; size];
    if let Err(e) = getrandom::fill(&mut buf) {
        return DomResponse::err(reply, format!("random failed: {e}"));
    }
    let bytes: Vec<serde_json::Value> = buf.iter().map(|b| serde_json::Value::from(*b)).collect();
    DomResponse::ok(reply, DomValue::Json(serde_json::Value::Array(bytes)))
}

/// Bound a page fetch so a stalled connection cannot hold the JS thread past the
/// worker's own DOM-RPC wait. It is deliberately shorter than the worker's 30 s
/// `recv_timeout`, so the page sees "fetch timed out" instead of a generic RPC
/// timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(25);

/// Answer a page `fetch()` / dynamic script source without blocking the UI thread.
///
/// Data URLs are answered inline because they need no I/O. Every other scheme is
/// fetched on tokio and replies through `reply` when it completes, so the winit
/// event loop keeps pumping while the page waits. The caller (the JS worker) is
/// still synchronous: it blocks in `__glide_dom_sync` until this reply arrives.
pub fn start_fetch(
    net: Option<Arc<Provider>>,
    base_url: &str,
    fetch: FetchRequest,
    reply_tx: crossbeam_channel::Sender<DomResponse>,
) {
    let FetchRequest {
        url,
        method,
        headers,
        body,
        credentials,
        reply,
        initiator,
    } = fetch;
    let stamp = crate::network_feed::capture_worker_stamp();
    let started = std::time::Instant::now();
    let send_now = |value: DomResponse| {
        let _ = reply_tx.send(value);
    };
    if is_data_url(&url) {
        let response = match data_url_response_json(&url) {
            Ok(value) => DomResponse::ok(reply, DomValue::Json(value)),
            Err(reason) => DomResponse::err(reply, reason),
        };
        record_js_network(stamp, &method, &url, initiator, &response, started);
        send_now(response);
        return;
    }
    let resolved = Url::parse(&url)
        .or_else(|_| Url::parse(base_url).and_then(|b| b.join(&url)))
        .map_err(|e| e.to_string());
    let Ok(parsed) = resolved else {
        let response = DomResponse::err(reply, "invalid url");
        record_js_network(stamp, &method, &url, initiator, &response, started);
        send_now(response);
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        send_now(DomResponse::err(reply, "no async runtime"));
        return;
    };
    let mut request = match build_fetch_request(parsed, &method, &headers, &body) {
        Ok(req) => req,
        Err(err) => {
            send_now(DomResponse::err(reply, err));
            return;
        }
    };
    let page_url = Url::parse(base_url).ok();
    let creds = crate::cookies::FetchCredentials::parse(&credentials);
    crate::cookies::CookieJar::global().append_request_cookies(
        &request.url,
        page_url.as_ref(),
        crate::cookies::CookieRequestKind::Fetch,
        creds,
        request.method.clone(),
        &mut request.headers,
    );
    let method_for_log = method.clone();
    let url_for_log = url.clone();
    let fetch_mode = crate::http::fetch_api_sec_mode(&request.url, page_url.as_ref(), creds);
    let fetch_metadata = crate::http::FetchMetadata::fetch_api(page_url.clone(), fetch_mode);
    handle.spawn(async move {
        let response = fetch_response(net, request, reply, page_url, creds, fetch_metadata).await;
        record_js_network(
            stamp,
            &method_for_log,
            &url_for_log,
            initiator,
            &response,
            started,
        );
        let _ = reply_tx.send(response);
    });
}

fn record_js_network(
    stamp: crate::network_feed::NetworkStamp,
    method: &str,
    url: &str,
    initiator: crate::devtools::network::NetworkInitiator,
    response: &DomResponse,
    started: std::time::Instant,
) {
    let duration_ms = started.elapsed().as_millis() as u64;
    let (status, error, size_bytes, response_headers) = match &response.value {
        DomValue::Json(v) => {
            let status = v.get("status").and_then(|s| s.as_u64()).map(|s| s as u16);
            let body = v
                .get("bodyBase64")
                .and_then(|b| b.as_str())
                .and_then(|s| {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD
                        .decode(s)
                        .ok()
                        .map(|bytes| bytes.len() as u64)
                })
                .or_else(|| {
                    v.get("body")
                        .and_then(|b| b.as_str())
                        .map(|s| s.len() as u64)
                });
            let hdrs = v
                .get("headers")
                .and_then(|h| h.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            (status, None, body, hdrs)
        }
        DomValue::Error(msg) => (None, Some(msg.clone()), None, Vec::new()),
        _ => (None, None, None, Vec::new()),
    };
    crate::network_feed::publish_completed(
        stamp,
        crate::network_feed::NetworkCompleted {
            method: method.to_string(),
            url: url.to_string(),
            initiator,
            status,
            error,
            duration_ms,
            size_bytes,
            response_headers,
        },
    );
}

async fn fetch_response(
    net: Option<Arc<Provider>>,
    request: Request,
    reply: u64,
    page_url: Option<Url>,
    creds: crate::cookies::FetchCredentials,
    fetch_metadata: crate::http::FetchMetadata,
) -> DomResponse {
    if request.url.scheme() == "about" {
        if let Some(html) = crate::about::about_document(&request.url) {
            return DomResponse::ok(
                reply,
                fetch_json(&crate::http::Followed {
                    url: request.url.to_string(),
                    status: 200,
                    body: Bytes::from(html),
                    redirected: false,
                    content_type: "text/html".to_string(),
                    headers: Vec::new(),
                }),
            );
        }
        return DomResponse::err(reply, "fetch failed: A local network error occurred");
    }
    if request.url.scheme() == "http" || request.url.scheme() == "https" {
        let fetch = crate::http::fetch_followed_with_cookie_context(
            request,
            page_url,
            crate::cookies::CookieRequestKind::Fetch,
            creds,
            fetch_metadata,
        );
        return match tokio::time::timeout(FETCH_TIMEOUT, fetch).await {
            Ok(Ok(resp)) => DomResponse::ok(reply, fetch_json(&resp)),
            Ok(Err(err)) => DomResponse::err(reply, format!("fetch failed: {err}")),
            Err(_) => DomResponse::err(reply, "fetch timed out"),
        };
    }
    let Some(net) = net else {
        return DomResponse::err(reply, "no network provider");
    };
    match tokio::time::timeout(FETCH_TIMEOUT, net.fetch_async(request)).await {
        Ok(Ok((final_url, bytes))) => DomResponse::ok(
            reply,
            fetch_json(&crate::http::Followed {
                url: final_url,
                status: 200,
                body: bytes,
                redirected: false,
                content_type: String::new(),
                headers: Vec::new(),
            }),
        ),
        Ok(Err(err)) => DomResponse::err(reply, format!("fetch failed: {err}")),
        Err(_) => DomResponse::err(reply, "fetch timed out"),
    }
}

fn is_data_url(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

fn is_forbidden_request_header(name: &str) -> bool {
    let key = name.trim().to_ascii_lowercase();
    FORBIDDEN_REQUEST_HEADERS
        .iter()
        .any(|blocked| key == *blocked)
}

fn header_name_valid(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.bytes().all(|b| {
            b == b'!'
                || b == b'#'
                || b == b'$'
                || b == b'&'
                || b == b'+'
                || b == b'-'
                || b == b'.'
                || b == b'^'
                || b == b'_'
                || b == b'|'
                || b == b'~'
                || b.is_ascii_alphanumeric()
        })
}

fn header_value_valid(value: &str) -> bool {
    !value.bytes().any(|b| b == b'\r' || b == b'\n')
}

fn build_fetch_request(
    url: Url,
    method: &str,
    header_pairs: &[(String, String)],
    body: &str,
) -> Result<Request, String> {
    let method: Method = method
        .parse()
        .map_err(|_| "invalid request method".to_string())?;
    let mut headers = HeaderMap::new();
    let mut content_type = None;
    for (name, value) in header_pairs {
        if is_forbidden_request_header(name) {
            continue;
        }
        if !header_name_valid(name) || !header_value_valid(value) {
            return Err("invalid header".to_string());
        }
        if name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.clone());
        }
        let header_name = http::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "invalid header".to_string())?;
        let header_value =
            http::header::HeaderValue::from_str(value).map_err(|_| "invalid header".to_string())?;
        headers.append(header_name, header_value);
    }
    let mut request = Request::get(url);
    request.method = method;
    request.content_type = content_type;
    request.headers = headers;
    request.body = Body::Empty;
    super::fetch_body::apply_fetch_body(&mut request, body)?;
    Ok(request)
}

fn status_text(status: u16) -> String {
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
        .unwrap_or("")
        .to_string()
}

fn headers_json_for_js(pairs: &[(String, String)]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (name, value) in pairs {
        if FORBIDDEN_RESPONSE_HEADERS
            .iter()
            .any(|blocked| name.eq_ignore_ascii_case(blocked))
        {
            continue;
        }
        map.insert(name.clone(), serde_json::Value::String(value.clone()));
    }
    map
}

fn fetch_reply_json(
    status: u16,
    url: &str,
    redirected: bool,
    header_pairs: &[(String, String)],
    body: &[u8],
) -> serde_json::Value {
    let ok = (200..300).contains(&status);
    let body_base64 = base64::engine::general_purpose::STANDARD.encode(body);
    serde_json::json!({
        "status": status,
        "ok": ok,
        "statusText": status_text(status),
        "url": url,
        "redirected": redirected,
        "headers": headers_json_for_js(header_pairs),
        "body": String::from_utf8_lossy(body),
        "bodyBase64": body_base64,
    })
}

fn fetch_json(resp: &crate::http::Followed) -> DomValue {
    let mut header_pairs = resp.headers.clone();
    if !resp.content_type.is_empty()
        && !header_pairs
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        header_pairs.push(("content-type".into(), resp.content_type.clone()));
    }
    DomValue::Json(fetch_reply_json(
        resp.status,
        &resp.url,
        resp.redirected,
        &header_pairs,
        &resp.body,
    ))
}

fn data_url_response_json(url: &str) -> Result<serde_json::Value, String> {
    let (content_type, bytes) = parse_data_url_bytes(url)?;
    let headers = vec![("content-type".into(), content_type)];
    Ok(fetch_reply_json(200, url, false, &headers, &bytes))
}

fn data_url_content_type(meta: &str) -> String {
    let trimmed = meta.trim();
    if trimmed.is_empty() {
        return "text/plain;charset=utf-8".into();
    }
    trimmed
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("base64"))
        .collect::<Vec<_>>()
        .join(";")
}

fn parse_data_url_bytes(url: &str) -> Result<(String, Vec<u8>), String> {
    if !is_data_url(url) {
        return Err("invalid data url".to_string());
    }
    let rest = &url[5..];
    let Some((meta, data)) = rest.split_once(',') else {
        return Ok((data_url_content_type(""), Vec::new()));
    };
    let is_base64 = meta
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"));
    let content_type = data_url_content_type(meta);
    let bytes = if is_base64 {
        if data.is_empty() {
            Vec::new()
        } else {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "invalid base64 data url".to_string())?
        }
    } else {
        percent_decode(data)
    };
    Ok((content_type, bytes))
}

/// URL Standard data URL body decoding: `%XX` escapes, with `+` left literal.
fn percent_decode(input: &str) -> Vec<u8> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

pub(crate) fn camel_to_kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js::bridge::node_to_handle;
    use crate::parse;
    use blitz_dom::DocumentConfig;
    use serde_json::json;

    fn sample_doc(html: &str) -> blitz_dom::BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), true)
    }

    #[test]
    fn dom_mutation_from_apply_commands() {
        let mut doc =
            sample_doc("<!DOCTYPE html><html><body><div id=\"root\"></div></body></html>");
        let root = node_to_handle(doc.get_element_by_id("root").unwrap());
        let resp = super::super::batch::apply_batch(
            &mut doc,
            &json!([{
                "t": "insert",
                "parent": root,
                "cid": "c1",
                "kind": "element",
                "name": "span"
            }])
            .to_string(),
            1,
        );
        let crate::js::bridge::DomValue::Json(v) = resp.value else {
            panic!("json");
        };
        let child = v["ids"]["c1"].as_u64().unwrap();
        super::super::batch::apply_batch(
            &mut doc,
            &json!([{ "t": "text", "id": child, "value": "hello" }]).to_string(),
            2,
        );
        let id = crate::js::bridge::handle_to_node(child);
        assert_eq!(
            doc.get_node(id)
                .map(|n| n.text_content())
                .unwrap_or_default(),
            "hello"
        );
    }

    #[test]
    fn counter_fixture_ids() {
        let html = std::fs::read_to_string("tests/fixtures/counter.html").unwrap();
        let doc = sample_doc(&html);
        assert!(doc.get_element_by_id("count").is_some());
        assert!(doc.get_element_by_id("inc").is_some());
    }

    #[test]
    fn set_text_content_via_rpc() {
        let mut doc =
            sample_doc(r#"<!DOCTYPE html><html><body><p id="t">before</p></body></html>"#);
        let id = doc.get_element_by_id("t").unwrap();
        let handle = node_to_handle(id);
        super::super::batch::apply_batch(
            &mut doc,
            &json!([{ "t": "text", "id": handle, "value": "after" }]).to_string(),
            1,
        );
        let text = doc
            .get_node(id)
            .map(|n| n.text_content())
            .unwrap_or_default();
        assert_eq!(text, "after");
    }

    #[test]
    fn create_after_text_clear_preserves_generation() {
        let mut doc = sample_doc(r#"<!DOCTYPE html><html><body><p id="t">x</p></body></html>"#);
        let handle = node_to_handle(doc.get_element_by_id("t").unwrap());
        super::super::batch::apply_batch(
            &mut doc,
            &json!([
                { "t": "text", "id": handle, "value": "" },
                {
                    "t": "insert",
                    "parent": handle,
                    "cid": "c1",
                    "kind": "element",
                    "name": "span"
                }
            ])
            .to_string(),
            1,
        );
        let child = doc
            .get_node(doc.get_element_by_id("t").unwrap())
            .and_then(|n| n.children.first().copied());
        assert!(child.is_some());
        assert!(doc.get_node(child.unwrap()).is_some());
    }

    #[test]
    fn camel_case_style_maps_to_css() {
        assert_eq!(camel_to_kebab("backgroundColor"), "background-color");
    }

    #[test]
    fn parse_data_url_percent_decodes_plain_payload() {
        let (_, bytes) = parse_data_url_bytes("data:text/javascript,a%20b%2Bc").expect("data");
        assert_eq!(bytes, b"a b+c");
    }

    #[test]
    fn parse_data_url_base64_decodes_payload() {
        let (_, bytes) = parse_data_url_bytes("data:text/plain;base64,aGVsbG8=").unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn parse_data_url_base64_rejects_garbage() {
        assert!(parse_data_url_bytes("data:text/plain;base64,%%%").is_err());
    }

    #[test]
    fn data_url_keeps_full_content_type() {
        let (ctype, _) = parse_data_url_bytes("data:text/html;charset=utf-8,hi").unwrap();
        assert_eq!(ctype, "text/html;charset=utf-8");
    }

    #[test]
    fn build_fetch_request_carries_post_body() {
        let url = Url::parse("https://example.test/").unwrap();
        let req = build_fetch_request(
            url,
            "POST",
            &[("content-type".into(), "text/plain".into())],
            r#"{"kind":"text","data":"payload"}"#,
        )
        .unwrap();
        assert_eq!(req.method, Method::POST);
        let Body::Bytes(bytes) = req.body else {
            panic!("expected bytes body");
        };
        assert_eq!(&bytes[..], b"payload");
    }

    #[test]
    fn build_fetch_request_drops_forbidden_request_headers() {
        let url = Url::parse("https://example.test/").unwrap();
        let req = build_fetch_request(
            url,
            "GET",
            &[("Cookie".into(), "a=1".into()), ("X-Ok".into(), "1".into())],
            "",
        )
        .unwrap();
        assert!(!req.headers.contains_key(http::header::COOKIE));
        assert!(req.headers.contains_key("x-ok"));
    }

    #[test]
    fn build_fetch_request_formdata_multipart_posts_to_loopback() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Captured {
            content_type: String,
            body: Vec<u8>,
        }

        let captured = Arc::new(Mutex::new(Captured::default()));
        let captured2 = captured.clone();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).expect("read");
            let req = &buf[..n];
            let header_end = req
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|i| i + 4)
                .unwrap_or(0);
            let headers = String::from_utf8_lossy(&req[..header_end]);
            let mut cap = captured2.lock().expect("lock");
            for line in headers.lines() {
                let lower = line.to_ascii_lowercase();
                if let Some(ct) = lower.strip_prefix("content-type: ") {
                    cap.content_type = ct.trim().to_string();
                }
            }
            cap.body = req[header_end..].to_vec();
            let body = "ok";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write");
        });

        let body_json = serde_json::json!({
            "kind": "formData",
            "entries": [
                {"name": "q", "valueKind": "text", "data": "a\r\nb"},
                {
                    "name": "f",
                    "valueKind": "file",
                    "dataBase64": "aGVsbG8=",
                    "filename": "x\"y.txt",
                    "type": "text/plain"
                }
            ]
        })
        .to_string();
        let url = Url::parse(&format!("http://{addr}/post")).unwrap();
        let req = build_fetch_request(url, "POST", &[], &body_json).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let followed = rt
            .block_on(crate::http::fetch_followed(req))
            .expect("fetch");
        assert_eq!(followed.status, 200);
        server.join().expect("join");
        let cap = captured.lock().expect("lock");
        assert!(
            cap.content_type
                .starts_with("multipart/form-data; boundary=")
        );
        let boundary = cap
            .content_type
            .split("boundary=")
            .nth(1)
            .expect("boundary")
            .trim()
            .trim_matches('"');
        let body_text = String::from_utf8_lossy(&cap.body);
        assert!(body_text.contains(&format!("--{boundary}")));
        assert!(body_text.contains("name=\"q\""));
        assert!(body_text.contains("a\r\nb"));
        assert!(body_text.contains("filename=\"x%22y.txt\""));
        assert!(body_text.contains("hello"));
        assert!(body_text.contains(&format!("--{boundary}--")));
    }

    #[test]
    fn build_fetch_request_formdata_respects_request_content_type() {
        let url = Url::parse("http://127.0.0.1/").unwrap();
        let body_json =
            r#"{"kind":"formData","entries":[{"name":"x","valueKind":"text","data":"1"}]}"#;
        let req = build_fetch_request(
            url,
            "POST",
            &[("Content-Type".into(), "text/plain".into())],
            body_json,
        )
        .unwrap();
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .expect("ct");
        assert_eq!(ct, "text/plain");
        let Body::Bytes(bytes) = req.body else {
            panic!("expected multipart bytes");
        };
        assert!(bytes.starts_with(b"--"));
    }

    #[test]
    fn build_fetch_request_url_search_params_sets_urlencoded() {
        let url = Url::parse("http://127.0.0.1/").unwrap();
        let req = build_fetch_request(
            url,
            "POST",
            &[],
            r#"{"kind":"urlSearchParams","pairs":[["a","1"]]}"#,
        )
        .unwrap();
        let Body::Bytes(bytes) = req.body else {
            panic!("bytes");
        };
        assert_eq!(&bytes[..], b"a=1");
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("application/x-www-form-urlencoded"));
    }

    #[test]
    fn js_headers_omit_set_cookie() {
        let map = headers_json_for_js(&[
            ("content-type".into(), "text/plain".into()),
            ("Set-Cookie".into(), "secret=1".into()),
        ]);
        assert!(map.contains_key("content-type"));
        assert!(!map.contains_key("Set-Cookie"));
        assert!(!map.contains_key("set-cookie"));
    }

    use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String)>>);

    impl NavigationProvider for Recorder {
        fn navigate_to(&self, options: NavigationOptions) {
            self.0
                .lock()
                .expect("recorder")
                .push((options.method.to_string(), options.url.to_string()));
        }
    }

    fn page_with_recorder(html: &str) -> (BaseDocument, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let config = DocumentConfig {
            base_url: Some("https://example.test/".into()),
            navigation_provider: Some(recorder.clone()),
            ..DocumentConfig::default()
        };
        let doc = crate::parse::document_from_html(html, config, false);
        (doc, recorder)
    }

    fn recorded(recorder: &Recorder) -> Vec<(String, String)> {
        recorder.0.lock().expect("recorder").clone()
    }

    #[test]
    fn submit_form_rpc_navigates_with_the_entry_list() {
        let (mut doc, recorder) = page_with_recorder(
            "<!DOCTYPE html><html><body><form action=\"/search\" method=\"get\">\
         <input name=\"q\" value=\"glide\"></form></body></html>",
        );
        let form = doc.query_selector("form").expect("selector").expect("form");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(form),
                reply: 1,
            },
        );
        assert_eq!(
            recorded(&recorder),
            vec![("GET".into(), "https://example.test/search?q=glide".into())]
        );
    }

    #[test]
    fn post_form_submit_navigates_with_post_method() {
        use blitz_traits::net::http::Method;
        let (mut doc, recorder) = page_with_recorder(
            "<!DOCTYPE html><html><body><form action=\"/login\" method=\"post\">\
         <input name=\"user\" value=\"a\"><input type=\"submit\" name=\"go\" value=\"1\"></form></body></html>",
        );
        let form = doc.query_selector("form").expect("selector").expect("form");
        let submit = doc
            .query_selector("input[type=submit]")
            .expect("selector")
            .expect("submit");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(submit),
                reply: 1,
            },
        );
        let nav = recorded(&recorder);
        assert_eq!(nav.len(), 1);
        assert_eq!(nav[0].0, Method::POST.to_string());
        assert_eq!(nav[0].1, "https://example.test/login");
    }

    #[test]
    fn post_form_entry_list_includes_named_submit_button() {
        use blitz_traits::net::{Body, FormData};
        #[derive(Default)]
        struct BodyRecorder(Mutex<Vec<FormData>>);
        impl NavigationProvider for BodyRecorder {
            fn navigate_to(&self, options: NavigationOptions) {
                if let Body::Form(form) = options.document_resource {
                    self.0.lock().expect("recorder").push(form);
                }
            }
        }
        let recorder = Arc::new(BodyRecorder::default());
        let config = DocumentConfig {
            base_url: Some("https://example.test/".into()),
            navigation_provider: Some(recorder.clone()),
            ..DocumentConfig::default()
        };
        let mut doc = crate::parse::document_from_html(
            "<!DOCTYPE html><html><body><form action=\"/login\" method=\"post\">\
         <input name=\"user\" value=\"a\"><input type=\"submit\" name=\"go\" value=\"1\"></form></body></html>",
            config,
            false,
        );
        let form = doc.query_selector("form").expect("selector").expect("form");
        let submit = doc
            .query_selector("input[type=submit]")
            .expect("selector")
            .expect("submit");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(submit),
                reply: 1,
            },
        );
        let bodies = recorder.0.lock().expect("recorder");
        let names: Vec<_> = bodies[0].iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"go"));
    }

    #[test]
    fn hidden_input_entry_list_uses_value_attribute_not_stale_editor() {
        use blitz_traits::net::{Body, EntryValue, FormData};
        #[derive(Default)]
        struct BodyRecorder(Mutex<Vec<FormData>>);
        impl NavigationProvider for BodyRecorder {
            fn navigate_to(&self, options: NavigationOptions) {
                if let Body::Form(form) = options.document_resource {
                    self.0.lock().expect("recorder").push(form);
                }
            }
        }
        let recorder = Arc::new(BodyRecorder::default());
        let config = DocumentConfig {
            base_url: Some("https://example.test/".into()),
            navigation_provider: Some(recorder.clone()),
            ..DocumentConfig::default()
        };
        let mut doc = crate::parse::document_from_html(
            "<!DOCTYPE html><html><body><form action=\"/search\" method=\"post\">\
         <input type=\"hidden\" name=\"gbv\" value=\"1\"></form></body></html>",
            config,
            false,
        );
        let form = doc.query_selector("form").expect("selector").expect("form");
        let hidden = doc
            .query_selector("input[type=hidden]")
            .expect("selector")
            .expect("hidden");
        let mut m = doc.mutate();
        m.set_attribute(hidden, blitz_dom::qual_name!("value"), "2");
        drop(m);
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(form),
                reply: 1,
            },
        );
        let bodies = recorder.0.lock().expect("recorder");
        let gbv = bodies[0]
            .iter()
            .find(|e| e.name == "gbv")
            .expect("gbv field");
        assert_eq!(
            gbv.value,
            EntryValue::String("2".into()),
            "hidden gbv must reflect updated value attribute"
        );
    }

    #[test]
    fn post_form_select_multiple_selected_options() {
        use blitz_traits::net::{Body, FormData};
        #[derive(Default)]
        struct BodyRecorder(Mutex<Vec<FormData>>);
        impl NavigationProvider for BodyRecorder {
            fn navigate_to(&self, options: NavigationOptions) {
                if let Body::Form(form) = options.document_resource {
                    self.0.lock().expect("recorder").push(form);
                }
            }
        }
        let recorder = Arc::new(BodyRecorder::default());
        let config = DocumentConfig {
            base_url: Some("https://example.test/".into()),
            navigation_provider: Some(recorder.clone()),
            ..DocumentConfig::default()
        };
        let html = "<!DOCTYPE html><html><body><form action=\"/save\" method=\"post\">\
            <select name=\"c\" multiple>\
            <option value=\"a\" selected>a</option>\
            <option value=\"b\" selected>b</option>\
            </select>\
            <input type=\"submit\" value=\"ok\">\
            </form></body></html>";
        let mut doc = crate::parse::document_from_html(html, config, false);
        let form = doc.query_selector("form").expect("selector").expect("form");
        let submit = doc
            .query_selector("input[type=submit]")
            .expect("selector")
            .expect("submit");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(submit),
                reply: 1,
            },
        );
        let bodies = recorder.0.lock().expect("recorder");
        let c_values: Vec<String> = bodies[0]
            .iter()
            .filter(|e| e.name == "c")
            .map(|e| match &e.value {
                blitz_traits::net::EntryValue::String(s) => s.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(c_values, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn post_form_formaction_targets_other_url() {
        use blitz_traits::net::http::Method;
        let (mut doc, recorder) = page_with_recorder(
            "<!DOCTYPE html><html><body><form action=\"/default\" method=\"post\">\
            <input name=\"x\" value=\"1\">\
            <input type=\"submit\" formaction=\"/other\" value=\"go\">\
            </form></body></html>",
        );
        let form = doc.query_selector("form").expect("selector").expect("form");
        let submit = doc
            .query_selector("input[type=submit]")
            .expect("selector")
            .expect("submit");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::SubmitForm {
                form: node_to_handle(form),
                submitter: node_to_handle(submit),
                reply: 1,
            },
        );
        let nav = recorded(&recorder);
        assert_eq!(nav.len(), 1);
        assert_eq!(nav[0].0, Method::POST.to_string());
        assert_eq!(nav[0].1, "https://example.test/other");
    }

    #[test]
    fn navigate_rpc_requests_the_target_url() {
        let (mut doc, recorder) = page_with_recorder("<!DOCTYPE html><html><body></body></html>");
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::Navigate {
                url: "/next?x=1".into(),
                reply: 1,
            },
        );
        assert_eq!(
            recorded(&recorder),
            vec![("GET".into(), "https://example.test/next?x=1".into())]
        );
        apply_request(
            &mut doc,
            "https://example.test/",
            DomRequest::Navigate {
                url: "about:blank".into(),
                reply: 1,
            },
        );
        assert_eq!(
            recorded(&recorder),
            vec![
                ("GET".into(), "https://example.test/next?x=1".into()),
                ("GET".into(), "about:blank".into()),
            ]
        );
    }

    #[tokio::test]
    async fn fetch_about_blank_returns_local_html_without_provider() {
        let request = Request::get(url::Url::parse("about:blank").expect("url"));
        let meta = crate::http::FetchMetadata::fetch_api(None, crate::http::SecFetchMode::Cors);
        let response = fetch_response(
            None,
            request,
            1,
            None,
            crate::cookies::FetchCredentials::Include,
            meta,
        )
        .await;
        match response.value {
            DomValue::Json(value) => {
                assert_eq!(value["status"], 200);
                assert!(
                    value["body"]
                        .as_str()
                        .is_some_and(|b| b.contains("<body></body>"))
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn cookie_rpc_get_hides_httponly() {
        use std::sync::Arc;
        let _jar_guard = crate::cookies::CookieJar::lock_global_for_test();
        crate::cookies::CookieJar::replace_global(Arc::new(crate::cookies::CookieJar::empty()));
        let page = url::Url::parse("http://example.test/").unwrap();
        crate::cookies::CookieJar::global().store_response_headers(
            &page,
            &[("Set-Cookie".into(), "hidden=1; Path=/; HttpOnly".into())],
        );
        crate::cookies::CookieJar::global()
            .store_response_headers(&page, &[("Set-Cookie".into(), "shown=2; Path=/".into())]);
        let mut doc = blitz_dom::BaseDocument::new(blitz_dom::DocumentConfig::default());
        let resp = apply_request(
            &mut doc,
            "http://example.test/",
            DomRequest::CookieGet { reply: 1 },
        );
        match resp.value {
            DomValue::String(s) => {
                assert!(s.contains("shown=2"));
                assert!(!s.contains("hidden"));
            }
            other => panic!("unexpected cookie get: {other:?}"),
        }
    }

    #[test]
    fn start_fetch_returns_before_the_network_completes() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::time::Instant;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (requested_tx, requested_rx) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut head = [0u8; 1024];
            let _ = stream.read(&mut head).expect("read request");
            requested_tx.send(()).expect("signal request");
            release_rx.recv().expect("wait for release");
            let body = "slow response";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let _guard = rt.enter();
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let started = Instant::now();
        start_fetch(
            None,
            "http://127.0.0.1/",
            FetchRequest {
                url: format!("http://{addr}/slow.js"),
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 1,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            },
            reply_tx,
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "start_fetch waited for the network: {:?}",
            started.elapsed()
        );
        requested_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("server received the request");
        assert!(
            reply_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "the reply arrived before the server answered"
        );
        release_tx.send(()).expect("release server");
        let response = reply_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fetch reply");
        match response.value {
            DomValue::Json(value) => {
                assert_eq!(value["status"], 200);
                assert_eq!(value["body"], "slow response");
            }
            other => panic!("unexpected fetch reply: {other:?}"),
        }
        server.join().expect("server thread");
    }

    #[test]
    fn start_fetch_records_network_entry() {
        crate::network_feed::set_worker_route(7, 1);
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        start_fetch(
            None,
            "https://example.test/",
            FetchRequest {
                url: "data:,hello".into(),
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 1,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            },
            reply_tx,
        );
        let _ = reply_rx.recv().expect("reply");
        let batch = crate::network_feed::drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].tab_id, 7);
        assert_eq!(batch[0].url, "data:,hello");
        assert_eq!(
            batch[0].initiator,
            crate::devtools::network::NetworkInitiator::Fetch
        );
        assert!(batch[0].started_ms > 0);
    }

    #[test]
    fn start_fetch_xhr_records_xhr_initiator() {
        let _ = crate::network_feed::drain_pending();
        crate::network_feed::set_worker_route(3, 1);
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        start_fetch(
            None,
            "https://example.test/",
            FetchRequest {
                url: "data:,xhr".into(),
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 1,
                initiator: crate::devtools::network::NetworkInitiator::Xhr,
            },
            reply_tx,
        );
        let _ = reply_rx.recv().expect("reply");
        let batch = crate::network_feed::drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(
            batch[0].initiator,
            crate::devtools::network::NetworkInitiator::Xhr
        );
    }

    #[test]
    fn start_fetch_invalid_url_records_error_before_reply() {
        let _ = crate::network_feed::drain_pending();
        crate::network_feed::set_worker_route(2, 1);
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        start_fetch(
            None,
            "https://example.test/",
            FetchRequest {
                url: "http://".into(),
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 1,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            },
            reply_tx,
        );
        let batch = crate::network_feed::drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].error.as_deref(), Some("invalid url"));
        let _ = reply_rx.recv().expect("reply still sent");
    }

    #[test]
    fn start_fetch_file_uses_stamp_captured_at_start() {
        let _ = crate::network_feed::drain_pending();
        crate::network_feed::set_worker_route(1, 1);
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.html");
        let net = std::sync::Arc::new(blitz_net::Provider::new(None));
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let file_url = url::Url::from_file_path(&path)
            .expect("file url")
            .to_string();
        start_fetch(
            Some(net),
            "file:///unused",
            FetchRequest {
                url: file_url,
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 1,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            },
            reply_tx,
        );
        let _ = reply_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("reply");
        crate::network_feed::set_worker_route(9, 9);
        let batch = crate::network_feed::drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].tab_id, 1);
        assert_eq!(batch[0].network_epoch, 1);
    }

    #[test]
    fn start_fetch_answers_data_urls_inline() {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        start_fetch(
            None,
            "https://example.test/",
            FetchRequest {
                url: "data:text/plain,inline".into(),
                method: "GET".into(),
                headers: Vec::new(),
                body: String::new(),
                credentials: "same-origin".into(),
                reply: 2,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            },
            reply_tx,
        );
        let response = reply_rx.try_recv().expect("data URL reply is immediate");
        match response.value {
            DomValue::Json(value) => assert_eq!(value["body"], "inline"),
            other => panic!("unexpected fetch reply: {other:?}"),
        }
    }
}
