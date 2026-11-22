//! Apply [`DomRequest`] values on the UI thread.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use blitz_dom::BaseDocument;
use blitz_net::Provider;
use blitz_traits::net::http::{self, HeaderMap, Method};
use blitz_traits::net::{Body, Bytes, Request};
use url::Url;

use super::bridge::{DomRequest, DomResponse, DomValue};

pub fn apply_request(
    doc: &mut BaseDocument,
    net: Option<&Arc<Provider>>,
    base_url: &str,
    req: DomRequest,
) -> DomResponse {
    let reply = reply_id(&req);
    let result = panic::catch_unwind(AssertUnwindSafe(|| apply_inner(doc, net, base_url, req)));
    match result {
        Ok(resp) => resp,
        Err(_) => DomResponse::err(reply, "dom operation panicked"),
    }
}

fn reply_id(req: &DomRequest) -> u64 {
    match req {
        DomRequest::Fetch { reply, .. }
        | DomRequest::SnapshotTree { reply }
        | DomRequest::BatchMutate { reply, .. }
        | DomRequest::BoundingRect { reply, .. }
        | DomRequest::ComputedStyle { reply, .. }
        | DomRequest::LayoutField { reply, .. }
        | DomRequest::PageUrl { reply }
        | DomRequest::Navigate { reply, .. }
        | DomRequest::ListenTarget { reply, .. }
        | DomRequest::NodeCount { reply } => *reply,
        #[cfg(debug_assertions)]
        DomRequest::TestResize { reply, .. } => *reply,
    }
}

fn apply_inner(
    doc: &mut BaseDocument,
    net: Option<&Arc<Provider>>,
    base_url: &str,
    req: DomRequest,
) -> DomResponse {
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
            let _ = url;
            DomResponse::ok(reply, DomValue::Null)
        }
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
        DomRequest::Fetch {
            url,
            method,
            headers,
            body,
            reply,
        } => apply_fetch(net, base_url, &url, &method, &headers, &body, reply),
    }
}

fn apply_fetch(
    net: Option<&Arc<Provider>>,
    base_url: &str,
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: &str,
    reply: u64,
) -> DomResponse {
    if url.starts_with("data:") {
        return DomResponse::ok(
            reply,
            DomValue::Json(data_url_response(url, method, headers, body)),
        );
    }
    let Some(net) = net else {
        return DomResponse::err(reply, "no network provider");
    };
    let resolved = Url::parse(url)
        .or_else(|_| Url::parse(base_url).and_then(|b| b.join(url)))
        .map_err(|e| e.to_string());
    let Ok(parsed) = resolved else {
        return DomResponse::err(reply, "invalid url");
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return DomResponse::err(reply, "no async runtime");
    };
    let request = match build_fetch_request(parsed, method, headers, body) {
        Ok(req) => req,
        Err(err) => return DomResponse::err(reply, err),
    };
    if request.url.scheme() == "http" || request.url.scheme() == "https" {
        let fetched = handle.block_on(crate::http::fetch_followed(request));
        return match fetched {
            Ok(resp) => DomResponse::ok(reply, fetch_json(&resp)),
            Err(err) => DomResponse::err(reply, format!("fetch failed: {err}")),
        };
    }
    let net = Arc::clone(net);
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("glide-fetch".into())
        .spawn(move || {
            let result = handle.block_on(async {
                tokio::time::timeout(Duration::from_secs(30), net.fetch_async(request)).await
            });
            let _ = tx.send(result);
        })
        .ok();
    let Ok(result) = rx.recv_timeout(Duration::from_secs(31)) else {
        return DomResponse::err(reply, "fetch timeout");
    };
    match result {
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
        if name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.clone());
        }
        if let (Ok(header_name), Ok(header_value)) = (
            http::header::HeaderName::from_bytes(name.as_bytes()),
            http::header::HeaderValue::from_str(value),
        ) {
            headers.append(header_name, header_value);
        }
    }
    let body = if body.is_empty() {
        Body::Empty
    } else {
        Body::Bytes(Bytes::from(body.as_bytes().to_vec()))
    };
    let mut request = Request::get(url);
    request.method = method;
    request.content_type = content_type;
    request.headers = headers;
    request.body = body;
    Ok(request)
}

fn status_text(status: u16) -> String {
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
        .unwrap_or("")
        .to_string()
}

fn headers_json(pairs: &[(String, String)]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (name, value) in pairs {
        map.insert(name.clone(), serde_json::Value::String(value.clone()));
    }
    map
}

fn fetch_json(resp: &crate::http::Followed) -> DomValue {
    let ok = (200..300).contains(&resp.status);
    let mut headers = headers_json(&resp.headers);
    if !resp.content_type.is_empty() && !headers.contains_key("content-type") {
        headers.insert(
            "content-type".into(),
            serde_json::Value::String(resp.content_type.clone()),
        );
    }
    DomValue::Json(serde_json::json!({
        "status": resp.status,
        "ok": ok,
        "statusText": status_text(resp.status),
        "url": resp.url,
        "redirected": resp.redirected,
        "headers": headers,
        "body": String::from_utf8_lossy(&resp.body),
    }))
}

fn data_url_response(
    url: &str,
    method: &str,
    req_headers: &[(String, String)],
    body: &str,
) -> serde_json::Value {
    let (content_type, payload) = parse_data_url(url);
    let is_get = method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD");
    let response_body = if is_get { payload } else { body.to_string() };
    let mut headers: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    headers.insert(
        "content-type".into(),
        serde_json::Value::String(content_type),
    );
    for (name, value) in req_headers {
        headers.insert(
            name.to_lowercase(),
            serde_json::Value::String(value.clone()),
        );
    }
    serde_json::json!({
        "status": 200,
        "ok": true,
        "statusText": "OK",
        "url": url,
        "redirected": false,
        "headers": headers,
        "body": response_body,
    })
}

fn parse_data_url(url: &str) -> (String, String) {
    let mut content_type = "text/plain;charset=utf-8".to_string();
    let mut payload = String::new();
    let Some(rest) = url.strip_prefix("data:") else {
        return (content_type, payload);
    };
    let Some((meta, data)) = rest.split_once(',') else {
        return (content_type, payload);
    };
    if !meta.contains(";base64") {
        payload = data.to_string();
    }
    if let Some(mime) = meta.split(';').next().filter(|m| !m.is_empty()) {
        content_type = mime.to_string();
    }
    (content_type, payload)
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
}
