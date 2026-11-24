---
slug: "websocket"
kind: "feature"
title: "WebSocket"
status: "done"
aliases:
  [
    "F-077"
  ]
requirements:
  [
    "page-scripts-open-websocket-send"
  ]
gherkin: "tests/features/websocket.feature"
tests:
  [
    "`js::websocket::tests::*`",
    "`js::integration::websocket_*`",
    "`http::tests::cross_port_post_uses_initiating_document_for_origin_and_cookies`",
    "`cookies::tests::ws_to_http_url_maps_schemes`"
  ]
mcp: "planned mcp-network-requests (`glide_network_requests`); Manual: websocket#1"
manual:
  [
    {
      "steps": "Open a local echo page using `new WebSocket('ws://127.0.0.1:…')` against `websocat` or `python -m websockets`; send a small `Blob` and a ~200 KiB `Uint8Array`; call `close(1000,'bye')` and try invalid `close(1005)`",
      "expected": "Binary echoes respect `binaryType` (`blob`/`arraybuffer`); invalid close codes throw without changing `readyState`; clean close reports code 1000 and reason; tab close/navigation ends the socket with code 1001",
      "aliases": [
        "M-057"
      ]
    }
  ]
---

Page scripts can construct `WebSocket`, exchange text and binary frames with a server, and receive `open`, `message`, `error`, and `close` events (`MessageEvent` / `CloseEvent`). The handshake attaches profile cookies and `Origin`, negotiated `protocol` must be requested, mixed `ws:` from HTTPS is rejected, and open sockets close when the page is retired or the tab network epoch advances.

## Notes

One tokio task per socket via `tokio-tungstenite` 0.30.0; `futures-util` drives `SinkExt`/`StreamExt` in the socket task. Events route to the owning worker by DOM generation. `binaryType` selects `Blob` vs `ArrayBuffer`; `send(Blob)` sends bytes. `bufferedAmount` tracks queued payload bytes.
