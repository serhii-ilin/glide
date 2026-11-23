//! Page `WebSocket` connections: one tokio task per socket, cookie-aware handshake.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use base64::Engine as _;
use blitz_traits::net::http::Method;
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{
    COOKIE, SEC_WEBSOCKET_EXTENSIONS, SEC_WEBSOCKET_PROTOCOL,
};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use url::Url;

use super::worker;
use crate::cookies::{CookieJar, CookieRequestKind, FetchCredentials};
use crate::devtools::network::NetworkInitiator;
use crate::js::console_feed::truncate_field;
use crate::logging;
use crate::network_feed;
use crate::tabs::TabId;

type ConnMap = HashMap<u64, Arc<SocketRecord>>;

static CONNECTIONS: OnceLock<Mutex<ConnMap>> = OnceLock::new();
type SenderMap = HashMap<u64, Sender<worker::WorkerCommand>>;
static COMMAND_SENDERS: OnceLock<Mutex<SenderMap>> = OnceLock::new();

fn connections() -> &'static Mutex<ConnMap> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn command_senders() -> &'static Mutex<SenderMap> {
    COMMAND_SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_connections() -> std::sync::MutexGuard<'static, ConnMap> {
    connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_command_senders() -> std::sync::MutexGuard<'static, SenderMap> {
    command_senders()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register the JS worker command channel for a DOM generation.
pub fn register_command_sender(dom_generation: u64, tx: Sender<worker::WorkerCommand>) {
    lock_command_senders().insert(dom_generation, tx);
}

/// Drop the command channel when a page worker retires.
pub fn unregister_command_sender(dom_generation: u64) {
    lock_command_senders().remove(&dom_generation);
}

#[derive(serde::Serialize)]
struct WireEvent<'a> {
    id: u64,
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    was_clean: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
}

fn deliver_event(event: WireEvent<'_>, dom_generation: u64) {
    let Ok(json) = serde_json::to_string(&event) else {
        return;
    };
    if let Some(tx) = lock_command_senders().get(&dom_generation) {
        let _ = tx.send(worker::WorkerCommand::WebSocketEvent { json });
    }
}

fn log_websocket(action: &str, url: &str, fields: &[(&str, String)]) {
    let truncated_url = truncate_field(url, 120);
    let mut out: Vec<(&str, String)> = fields
        .iter()
        .map(|(k, v)| {
            if *k == "detail" {
                (*k, truncate_field(v, 120))
            } else {
                (*k, v.clone())
            }
        })
        .collect();
    out.push(("url", truncated_url));
    out.push(("action", action.to_string()));
    logging::debug("net.websocket", "websocket", &out);
}

fn ws_url_for_api(url: &Url) -> String {
    let mut u = url.clone();
    if let Some(port) = u.port() {
        let default = if u.scheme() == "wss" { 443 } else { 80 };
        if port == default {
            let _ = u.set_port(None);
        }
    }
    u.to_string()
}

fn is_protocol_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E))
}

pub fn validate_protocols(protocols_json: &str) -> Result<Vec<String>, String> {
    let list: Vec<String> =
        serde_json::from_str(protocols_json).map_err(|_| "invalid protocols".to_string())?;
    let mut seen = HashSet::new();
    for entry in &list {
        if entry.is_empty() {
            return Err("empty protocol".into());
        }
        if !is_protocol_token(entry) {
            return Err("invalid protocol token".into());
        }
        if !seen.insert(entry.clone()) {
            return Err("duplicate protocol".into());
        }
    }
    Ok(list)
}

pub fn validate_connect_url(page_base: &str, raw_url: &str) -> Result<String, String> {
    let page = Url::parse(page_base).map_err(|_| "invalid document url")?;
    let resolved = Url::parse(raw_url)
        .or_else(|_| Url::parse(page_base).and_then(|b| b.join(raw_url)))
        .map_err(|_| "invalid url")?;
    if resolved.fragment().is_some() {
        return Err("url has fragment".into());
    }
    let scheme = resolved.scheme();
    if scheme != "ws" && scheme != "wss" {
        return Err("invalid url".into());
    }
    if page.scheme() == "https" && scheme == "ws" {
        return Err("mixed content blocked".into());
    }
    Ok(ws_url_for_api(&resolved))
}

enum Outbound {
    Send(Message),
    Close { code: u16, reason: String },
}

struct SocketRecord {
    dom_generation: u64,
    tab_id: TabId,
    network_epoch: u64,
    outbound: mpsc::UnboundedSender<Outbound>,
    buffered: Arc<AtomicUsize>,
    close_tx: watch::Sender<bool>,
}

pub fn start_connect(
    socket_id: u64,
    page_base: &str,
    url: &str,
    protocols: &str,
    dom_generation: u64,
) -> Result<String, String> {
    let ws_url_str = validate_connect_url(page_base, url)?;
    let ws_url = Url::parse(&ws_url_str).map_err(|_| "invalid url".to_string())?;
    validate_protocols(protocols)?;
    let page_url = Url::parse(page_base).ok();
    let stamp = network_feed::capture_worker_stamp();
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
    let (close_tx, close_rx) = watch::channel(false);
    let buffered = Arc::new(AtomicUsize::new(0));
    let record = Arc::new(SocketRecord {
        dom_generation,
        tab_id: stamp.tab_id,
        network_epoch: stamp.network_epoch,
        outbound: outbound_tx,
        buffered: Arc::clone(&buffered),
        close_tx,
    });
    {
        let mut map = lock_connections();
        map.insert(socket_id, Arc::clone(&record));
    }
    log_websocket(
        "connect",
        ws_url.as_str(),
        &[("socket_id", socket_id.to_string())],
    );
    let page_base = page_base.to_string();
    let protocols = protocols.to_string();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        remove_socket(socket_id);
        return Err("no async runtime".into());
    };
    handle.spawn(async move {
        run_socket(SocketIo {
            socket_id,
            dom_generation,
            ws_url,
            page_url,
            page_base,
            protocols,
            stamp,
            close_rx,
            outbound_rx,
            buffered,
        })
        .await;
        remove_socket(socket_id);
    });
    Ok(ws_url_str)
}

struct SocketIo {
    socket_id: u64,
    dom_generation: u64,
    ws_url: Url,
    page_url: Option<Url>,
    page_base: String,
    protocols: String,
    stamp: network_feed::NetworkStamp,
    close_rx: watch::Receiver<bool>,
    outbound_rx: mpsc::UnboundedReceiver<Outbound>,
    buffered: Arc<AtomicUsize>,
}

fn negotiated_protocol(
    requested: &[String],
    response: &tokio_tungstenite::tungstenite::http::HeaderMap,
) -> Result<String, String> {
    let Some(raw) = response
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(String::new());
    };
    let picked = raw.trim();
    if picked.is_empty() {
        return Ok(String::new());
    }
    if requested.is_empty() {
        return Err("server selected unrequested protocol".into());
    }
    if !requested.iter().any(|p| p == picked) {
        return Err("server selected unrequested protocol".into());
    }
    Ok(picked.to_string())
}

fn reject_handshake_extensions(
    response: &tokio_tungstenite::tungstenite::http::HeaderMap,
) -> Result<(), String> {
    if let Some(value) = response
        .get(SEC_WEBSOCKET_EXTENSIONS)
        .and_then(|v| v.to_str().ok())
        && !value.trim().is_empty()
    {
        return Err("server selected extensions".into());
    }
    Ok(())
}

async fn run_socket(io: SocketIo) {
    let socket_id = io.socket_id;
    let dom_generation = io.dom_generation;
    let ws_url = io.ws_url;
    let page_url = io.page_url;
    let page_base = io.page_base;
    let protocols_json = io.protocols;
    let requested_protocols = validate_protocols(&protocols_json).unwrap_or_default();
    let stamp = io.stamp;
    let mut close_rx = io.close_rx;
    let mut outbound_rx = io.outbound_rx;
    let buffered = io.buffered;
    let started = Instant::now();
    let http_url = match crate::cookies::ws_to_http_url(&ws_url) {
        Some(u) => u,
        None => {
            deliver_failure(socket_id, dom_generation);
            record_network(stamp, ws_url.as_str(), None, Some("invalid url"), started);
            return;
        }
    };
    let mut request = match ws_url.as_str().into_client_request() {
        Ok(req) => req,
        Err(err) => {
            deliver_failure(socket_id, dom_generation);
            record_network(
                stamp,
                ws_url.as_str(),
                None,
                Some(&err.to_string()),
                started,
            );
            return;
        }
    };
    if !requested_protocols.is_empty()
        && let Ok(v) = HeaderValue::from_str(&requested_protocols.join(", "))
    {
        request.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, v);
    }
    let mut cookie_headers = blitz_traits::net::http::HeaderMap::new();
    CookieJar::global().append_request_cookies(
        &http_url,
        page_url.as_ref().or(Url::parse(&page_base).ok().as_ref()),
        CookieRequestKind::WebSocket,
        FetchCredentials::Include,
        Method::GET,
        &mut cookie_headers,
    );
    if let Some(v) = cookie_headers.get(blitz_traits::net::http::header::COOKIE)
        && let Ok(s) = v.to_str()
        && let Ok(hv) = HeaderValue::from_str(s)
    {
        request.headers_mut().insert(COOKIE, hv);
    }
    let origin = page_url
        .or_else(|| Url::parse(&page_base).ok())
        .map(|u| u.origin().ascii_serialization());
    if let Some(origin) = origin
        && let Ok(hv) = HeaderValue::from_str(&origin)
    {
        request
            .headers_mut()
            .insert(tokio_tungstenite::tungstenite::http::header::ORIGIN, hv);
    }
    match tokio_tungstenite::connect_async(request).await {
        Ok((ws, response)) => {
            if let Err(err) = reject_handshake_extensions(response.headers()) {
                deliver_failure(socket_id, dom_generation);
                record_network(stamp, ws_url.as_str(), None, Some(&err), started);
                return;
            }
            let protocol = match negotiated_protocol(&requested_protocols, response.headers()) {
                Ok(p) => p,
                Err(err) => {
                    deliver_failure(socket_id, dom_generation);
                    record_network(stamp, ws_url.as_str(), None, Some(&err), started);
                    return;
                }
            };
            CookieJar::global().store_response_header_map(
                &http_url,
                &response_headers_to_blitz(response.headers()),
            );
            let status = response.status().as_u16();
            log_websocket(
                "open",
                ws_url.as_str(),
                &[
                    ("socket_id", socket_id.to_string()),
                    ("status", status.to_string()),
                ],
            );
            deliver_event(
                WireEvent {
                    id: socket_id,
                    kind: "open",
                    data: None,
                    binary: None,
                    code: None,
                    reason: None,
                    was_clean: None,
                    protocol: if protocol.is_empty() {
                        None
                    } else {
                        Some(protocol)
                    },
                },
                dom_generation,
            );
            record_network(stamp, ws_url.as_str(), Some(status), None, started);
            let (mut write, mut read) = ws.split();
            loop {
                tokio::select! {
                    changed = close_rx.changed() => {
                        if changed.is_ok() && *close_rx.borrow() {
                            let frame = CloseFrame {
                                code: CloseCode::Away,
                                reason: "".into(),
                            };
                            let _ = write.send(Message::Close(Some(frame))).await;
                            deliver_close(socket_id, dom_generation, 1001, "", false);
                            break;
                        }
                    }
                    outgoing = outbound_rx.recv() => {
                        match outgoing {
                            Some(Outbound::Send(msg)) => {
                                let len = match &msg {
                                    Message::Text(t) => t.len(),
                                    Message::Binary(b) => b.len(),
                                    _ => 0,
                                };
                                if write.send(msg).await.is_err() {
                                    buffered.fetch_sub(len, Ordering::Relaxed);
                                    deliver_failure(socket_id, dom_generation);
                                    break;
                                }
                                buffered.fetch_sub(len, Ordering::Relaxed);
                            }
                            Some(Outbound::Close { code, reason }) => {
                                let frame = CloseFrame {
                                    code: CloseCode::from(code),
                                    reason: reason.clone().into(),
                                };
                                let _ = write.send(Message::Close(Some(frame))).await;
                                deliver_close(socket_id, dom_generation, code, &reason, true);
                                break;
                            }
                            None => break,
                        }
                    }
                    incoming = read.next() => {
                        match incoming {
                            Some(Ok(Message::Text(text))) => {
                                deliver_event(WireEvent {
                                    id: socket_id,
                                    kind: "message",
                                    data: Some(text.to_string()),
                                    binary: Some(false),
                                    code: None,
                                    reason: None,
                                    was_clean: None,
                                    protocol: None,
                                }, dom_generation);
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                                deliver_event(WireEvent {
                                    id: socket_id,
                                    kind: "message",
                                    data: Some(b64),
                                    binary: Some(true),
                                    code: None,
                                    reason: None,
                                    was_clean: None,
                                    protocol: None,
                                }, dom_generation);
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                let _ = write.send(Message::Pong(payload)).await;
                            }
                            Some(Ok(Message::Pong(_))) => {}
                            Some(Ok(Message::Close(frame))) => {
                                let (code, reason) = frame
                                    .map(|f| (u16::from(f.code), f.reason.to_string()))
                                    .unwrap_or((1000, String::new()));
                                deliver_close(socket_id, dom_generation, code, &reason, true);
                                break;
                            }
                            Some(Ok(Message::Frame(_))) => {}
                            Some(Err(err)) => {
                                log_websocket(
                                    "error",
                                    ws_url.as_str(),
                                    &[
                                        ("socket_id", socket_id.to_string()),
                                        ("detail", err.to_string()),
                                    ],
                                );
                                deliver_failure(socket_id, dom_generation);
                                break;
                            }
                            None => {
                                deliver_close(socket_id, dom_generation, 1006, "", false);
                                break;
                            }
                        }
                    }
                }
            }
        }
        Err(err) => {
            log_websocket(
                "error",
                ws_url.as_str(),
                &[
                    ("socket_id", socket_id.to_string()),
                    ("detail", err.to_string()),
                ],
            );
            deliver_failure(socket_id, dom_generation);
            record_network(
                stamp,
                ws_url.as_str(),
                None,
                Some(&err.to_string()),
                started,
            );
        }
    }
}

fn deliver_close(socket_id: u64, dom_generation: u64, code: u16, reason: &str, was_clean: bool) {
    deliver_event(
        WireEvent {
            id: socket_id,
            kind: "close",
            data: None,
            binary: None,
            code: Some(code),
            reason: Some(reason.to_string()),
            was_clean: Some(was_clean),
            protocol: None,
        },
        dom_generation,
    );
}

fn deliver_failure(socket_id: u64, dom_generation: u64) {
    deliver_event(
        WireEvent {
            id: socket_id,
            kind: "error",
            data: None,
            binary: None,
            code: None,
            reason: None,
            was_clean: None,
            protocol: None,
        },
        dom_generation,
    );
    deliver_close(socket_id, dom_generation, 1006, "", false);
}

fn response_headers_to_blitz(
    headers: &tokio_tungstenite::tungstenite::http::HeaderMap,
) -> blitz_traits::net::http::HeaderMap {
    let mut out = blitz_traits::net::http::HeaderMap::new();
    for (name, value) in headers.iter() {
        if let Ok(s) = value.to_str()
            && let (Ok(n), Ok(v)) = (
                blitz_traits::net::http::HeaderName::from_bytes(name.as_str().as_bytes()),
                blitz_traits::net::http::HeaderValue::from_str(s),
            )
        {
            out.append(n, v);
        }
    }
    out
}

fn record_network(
    stamp: network_feed::NetworkStamp,
    url: &str,
    status: Option<u16>,
    error: Option<&str>,
    started: Instant,
) {
    network_feed::publish_completed(
        stamp,
        network_feed::NetworkCompleted {
            method: "GET".into(),
            url: url.to_string(),
            initiator: NetworkInitiator::WebSocket,
            status,
            error: error.map(str::to_string),
            duration_ms: started.elapsed().as_millis() as u64,
            size_bytes: None,
            response_headers: Vec::new(),
        },
    );
}

fn remove_socket(socket_id: u64) {
    lock_connections().remove(&socket_id);
}

fn with_socket<F, T>(socket_id: u64, f: F) -> Result<T, String>
where
    F: FnOnce(&Arc<SocketRecord>) -> Result<T, String>,
{
    let map = lock_connections();
    let Some(rec) = map.get(&socket_id) else {
        return Err("unknown socket".into());
    };
    f(rec)
}

pub fn send_message(socket_id: u64, data: &str, binary: bool) -> Result<usize, String> {
    with_socket(socket_id, |rec| {
        let msg = if binary {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "invalid binary payload")?;
            Message::Binary(bytes.into())
        } else {
            Message::Text(data.into())
        };
        let len = match &msg {
            Message::Text(t) => t.len(),
            Message::Binary(b) => b.len(),
            _ => 0,
        };
        rec.outbound
            .send(Outbound::Send(msg))
            .map_err(|_| "socket closed")?;
        rec.buffered.fetch_add(len, Ordering::Relaxed);
        Ok(rec.buffered.load(Ordering::Relaxed))
    })
}

pub fn close_socket(socket_id: u64, code: u16, reason: &str) -> Result<(), String> {
    with_socket(socket_id, |rec| {
        rec.outbound
            .send(Outbound::Close {
                code,
                reason: reason.to_string(),
            })
            .map_err(|_| "socket closed")?;
        Ok(())
    })
}

pub fn buffered_amount(socket_id: u64) -> Result<usize, String> {
    with_socket(socket_id, |rec| Ok(rec.buffered.load(Ordering::Relaxed)))
}

pub fn close_for_dom_generation(dom_generation: u64) {
    let ids: Vec<(u64, watch::Sender<bool>)> = {
        let map = lock_connections();
        map.iter()
            .filter(|(_, r)| r.dom_generation == dom_generation)
            .map(|(id, r)| (*id, r.close_tx.clone()))
            .collect()
    };
    for (_id, tx) in ids {
        let _ = tx.send(true);
    }
}

pub fn close_for_tab_network_epoch(tab_id: TabId, network_epoch: u64) {
    let ids: Vec<(u64, watch::Sender<bool>)> = {
        let map = lock_connections();
        map.iter()
            .filter(|(_, r)| r.tab_id == tab_id && r.network_epoch < network_epoch)
            .map(|(id, r)| (*id, r.close_tx.clone()))
            .collect()
    };
    for (_id, tx) in ids {
        let _ = tx.send(true);
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use tokio::net::TcpListener;

    /// Echo WebSocket test server on `127.0.0.1:0`, optionally replying with `Sec-WebSocket-Protocol`.
    pub async fn test_server(protocol: Option<&str>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test ws listener");
        let addr = listener.local_addr().expect("local addr");
        let url = format!("ws://{addr}");
        let protocol = protocol.map(str::to_string);
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let protocol = protocol.clone();
                tokio::spawn(async move {
                    #[allow(clippy::result_large_err)]
                    let Ok(ws_stream) = tokio_tungstenite::accept_hdr_async(
                        stream,
                        |_: &tokio_tungstenite::tungstenite::handshake::server::Request,
                         mut resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
                            if let Some(proto) = &protocol
                                && let Ok(v) = HeaderValue::from_str(proto)
                            {
                                resp.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, v);
                            }
                            Ok(resp)
                        },
                    )
                    .await
                    else {
                        return;
                    };
                    let (mut write, mut read) = ws_stream.split();
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(t)) => {
                                if write.send(Message::Text(t)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(Message::Binary(b)) => {
                                if write.send(Message::Binary(b)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(Message::Close(_)) => break,
                            Ok(Message::Ping(p)) => {
                                let _ = write.send(Message::Pong(p)).await;
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                });
            }
        });
        (url, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Duration;

    #[test]
    fn mixed_content_rejects_ws_from_https_document() {
        let err =
            validate_connect_url("https://example.test/", "ws://127.0.0.1:9/echo").unwrap_err();
        assert!(err.contains("mixed content"), "{err}");
    }

    #[test]
    fn rejects_non_ws_scheme() {
        let err =
            validate_connect_url("https://example.test/", "https://example.test/x").unwrap_err();
        assert!(err.contains("invalid url"), "{err}");
    }

    #[test]
    fn rejects_url_fragment() {
        let err = validate_connect_url("http://example.test/", "ws://example.test/#x").unwrap_err();
        assert!(err.contains("fragment"), "{err}");
    }

    #[test]
    fn serializes_url_without_default_port() {
        let resolved =
            validate_connect_url("http://example.test/", "ws://example.test:80/a").expect("url");
        assert_eq!(resolved, "ws://example.test/a");
    }

    #[test]
    fn rejects_duplicate_protocols() {
        let err = validate_protocols(r#"["a","a"]"#).unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn rejects_empty_protocol_token() {
        let err = validate_protocols(r#"[""]"#).unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn buffered_amount_accumulates_two_sends() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (tx, _rx) = unbounded();
        register_command_sender(1, tx);
        rt.block_on(async {
            let (url, _server) = test_support::test_server(None).await;
            let socket_id = 9_u64;
            start_connect(socket_id, "http://example.test/", &url, "[]", 1).expect("connect");
            tokio::time::sleep(Duration::from_millis(100)).await;
            let first = send_message(socket_id, "aa", false).expect("send1");
            let second = send_message(socket_id, "bbb", false).expect("send2");
            assert_eq!(first + second - second, first);
            assert!(buffered_amount(socket_id).unwrap() >= 5);
        });
        unregister_command_sender(1);
    }

    #[test]
    fn retire_delivers_code_1001() {
        let (tx, rx) = unbounded();
        register_command_sender(7, tx);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let (url, _server) = test_support::test_server(None).await;
            let socket_id = 12_u64;
            start_connect(socket_id, "http://example.test/", &url, "[]", 7).expect("connect");
            tokio::time::sleep(Duration::from_millis(150)).await;
            close_for_dom_generation(7);
            tokio::time::sleep(Duration::from_millis(150)).await;
        });
        let mut close_code = None;
        while let Ok(worker::WorkerCommand::WebSocketEvent { json }) = rx.try_recv() {
            if json.contains("\"close\"") && json.contains("\"code\":1001") {
                close_code = Some(1001);
            }
        }
        assert_eq!(close_code, Some(1001));
        unregister_command_sender(7);
    }

    #[test]
    fn log_websocket_truncates_long_fields() {
        let long = "x".repeat(200);
        log_websocket("connect", &long, &[("socket_id", "1".into())]);
    }

    #[test]
    fn test_server_echoes_text() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let (url, _server) = test_support::test_server(None).await;
            let (mut ws, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("client connect");
            ws.send(Message::Text("ping".into())).await.expect("send");
            let reply = ws.next().await.expect("frame").expect("ok");
            assert_eq!(reply, Message::Text("ping".into()));
        });
    }
}
