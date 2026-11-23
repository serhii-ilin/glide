//! Page `WebSocket` connections: one tokio task per socket, cookie-aware handshake.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use base64::Engine as _;
use blitz_traits::net::http::Method;
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{COOKIE, SEC_WEBSOCKET_PROTOCOL};
use url::Url;

use super::worker;
use crate::cookies::{CookieJar, CookieRequestKind, FetchCredentials};
use crate::devtools::network::NetworkInitiator;
use crate::logging;
use crate::network_feed;
use crate::tabs::TabId;

type ConnMap = HashMap<u64, Arc<SocketRecord>>;

static CONNECTIONS: OnceLock<Mutex<ConnMap>> = OnceLock::new();
static COMMAND_TX: OnceLock<Mutex<Option<Sender<worker::WorkerCommand>>>> = OnceLock::new();

fn connections() -> &'static Mutex<ConnMap> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register the JS worker command channel used to deliver socket events.
pub fn set_command_sender(tx: Sender<worker::WorkerCommand>) {
    *COMMAND_TX
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("command tx lock") = Some(tx);
}

fn command_tx() -> Option<Sender<worker::WorkerCommand>> {
    COMMAND_TX
        .get()
        .and_then(|m| m.lock().ok().and_then(|g| g.clone()))
}

enum Outbound {
    Send(Message),
    Close,
}

struct SocketRecord {
    dom_generation: u64,
    tab_id: TabId,
    network_epoch: u64,
    outbound: mpsc::UnboundedSender<Outbound>,
    buffered: Arc<AtomicUsize>,
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
}

fn deliver_event(event: WireEvent<'_>) {
    let Ok(json) = serde_json::to_string(&event) else {
        return;
    };
    if let Some(tx) = command_tx() {
        let _ = tx.send(worker::WorkerCommand::WebSocketEvent { json });
    }
}

fn log_websocket(action: &str, url: &str, fields: &[(&str, String)]) {
    let mut out = fields.to_vec();
    out.push(("url", url.to_string()));
    out.push(("action", action.to_string()));
    logging::debug("net.websocket", "websocket", &out);
}

pub fn validate_connect_url(page_base: &str, raw_url: &str) -> Result<Url, String> {
    let page = Url::parse(page_base).map_err(|_| "invalid document url")?;
    let resolved = Url::parse(raw_url)
        .or_else(|_| Url::parse(page_base).and_then(|b| b.join(raw_url)))
        .map_err(|_| "invalid url")?;
    let scheme = resolved.scheme();
    if scheme != "ws" && scheme != "wss" {
        return Err("invalid url".into());
    }
    if page.scheme() == "https" && scheme == "ws" {
        return Err("mixed content blocked".into());
    }
    Ok(resolved)
}

pub fn start_connect(
    socket_id: u64,
    page_base: &str,
    url: &str,
    protocols: &str,
    dom_generation: u64,
) -> Result<(), String> {
    let ws_url = validate_connect_url(page_base, url)?;
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
    });
    {
        let mut map = connections().lock().expect("connections lock");
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
    let _ = (close_tx, dom_generation);
    Ok(())
}

struct SocketIo {
    socket_id: u64,
    ws_url: Url,
    page_url: Option<Url>,
    page_base: String,
    protocols: String,
    stamp: network_feed::NetworkStamp,
    close_rx: watch::Receiver<bool>,
    outbound_rx: mpsc::UnboundedReceiver<Outbound>,
    buffered: Arc<AtomicUsize>,
}

async fn run_socket(io: SocketIo) {
    let socket_id = io.socket_id;
    let ws_url = io.ws_url;
    let page_url = io.page_url;
    let page_base = io.page_base;
    let protocols = io.protocols;
    let stamp = io.stamp;
    let mut close_rx = io.close_rx;
    let mut outbound_rx = io.outbound_rx;
    let buffered = io.buffered;
    let started = Instant::now();
    let http_url = match crate::cookies::ws_to_http_url(&ws_url) {
        Some(u) => u,
        None => {
            deliver_failure(socket_id);
            record_network(stamp, ws_url.as_str(), None, Some("invalid url"), started);
            return;
        }
    };
    let mut request = match ws_url.as_str().into_client_request() {
        Ok(req) => req,
        Err(err) => {
            deliver_failure(socket_id);
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
    if let Ok(list) = serde_json::from_str::<Vec<String>>(&protocols)
        && !list.is_empty()
        && let Ok(v) = HeaderValue::from_str(&list.join(", "))
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
            deliver_event(WireEvent {
                id: socket_id,
                kind: "open",
                data: None,
                binary: None,
                code: None,
                reason: None,
                was_clean: None,
            });
            record_network(stamp, ws_url.as_str(), Some(status), None, started);
            let (mut write, mut read) = ws.split();
            loop {
                tokio::select! {
                    changed = close_rx.changed() => {
                        if changed.is_ok() && *close_rx.borrow() {
                            let _ = write.send(Message::Close(None)).await;
                            deliver_close(socket_id, 1001, "", false);
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
                                buffered.store(len, Ordering::Relaxed);
                                if write.send(msg).await.is_err() {
                                    deliver_failure(socket_id);
                                    break;
                                }
                                buffered.store(0, Ordering::Relaxed);
                            }
                            Some(Outbound::Close) => {
                                let _ = write.send(Message::Close(None)).await;
                                deliver_close(socket_id, 1000, "", true);
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
                                });
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
                                });
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                let _ = write.send(Message::Pong(payload)).await;
                            }
                            Some(Ok(Message::Pong(_))) => {}
                            Some(Ok(Message::Close(frame))) => {
                                let (code, reason) = frame
                                    .map(|f| (u16::from(f.code), f.reason.to_string()))
                                    .unwrap_or((1000, String::new()));
                                deliver_close(socket_id, code, &reason, true);
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
                                deliver_failure(socket_id);
                                break;
                            }
                            None => {
                                deliver_close(socket_id, 1006, "", false);
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
            deliver_failure(socket_id);
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

fn deliver_close(socket_id: u64, code: u16, reason: &str, was_clean: bool) {
    deliver_event(WireEvent {
        id: socket_id,
        kind: "close",
        data: None,
        binary: None,
        code: Some(code),
        reason: Some(reason.to_string()),
        was_clean: Some(was_clean),
    });
}

fn deliver_failure(socket_id: u64) {
    deliver_event(WireEvent {
        id: socket_id,
        kind: "error",
        data: None,
        binary: None,
        code: None,
        reason: None,
        was_clean: None,
    });
    deliver_close(socket_id, 1006, "", false);
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
            out.insert(n, v);
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
    let mut map = connections().lock().expect("connections lock");
    map.remove(&socket_id);
}

fn with_socket<F, T>(socket_id: u64, f: F) -> Result<T, String>
where
    F: FnOnce(&Arc<SocketRecord>) -> Result<T, String>,
{
    let map = connections().lock().expect("connections lock");
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
        rec.buffered.store(len, Ordering::Relaxed);
        Ok(len)
    })
}

pub fn close_socket(socket_id: u64, _code: u16, _reason: &str) -> Result<(), String> {
    with_socket(socket_id, |rec| {
        rec.outbound
            .send(Outbound::Close)
            .map_err(|_| "socket closed")?;
        Ok(())
    })
}

pub fn buffered_amount(socket_id: u64) -> Result<usize, String> {
    with_socket(socket_id, |rec| Ok(rec.buffered.load(Ordering::Relaxed)))
}

pub fn close_for_dom_generation(dom_generation: u64) {
    let ids: Vec<u64> = {
        let map = connections().lock().expect("connections lock");
        map.iter()
            .filter(|(_, r)| r.dom_generation == dom_generation)
            .map(|(id, _)| *id)
            .collect()
    };
    for id in ids {
        let _ = close_socket(id, 1001, "page detached");
    }
}

pub fn close_for_tab_network_epoch(tab_id: TabId, network_epoch: u64) {
    let ids: Vec<u64> = {
        let map = connections().lock().expect("connections lock");
        map.iter()
            .filter(|(_, r)| r.tab_id == tab_id && r.network_epoch < network_epoch)
            .map(|(id, _)| *id)
            .collect()
    };
    for id in ids {
        let _ = close_socket(id, 1001, "navigation");
    }
}

/// Echo WebSocket test server on `127.0.0.1:0`.
pub async fn test_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test ws listener");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("ws://{addr}");
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await else {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_server_echoes_text() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let (url, _server) = test_server().await;
            let (mut ws, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("client connect");
            ws.send(Message::Text("ping".into())).await.expect("send");
            let reply = ws.next().await.expect("frame").expect("ok");
            assert_eq!(reply, Message::Text("ping".into()));
        });
    }
}
