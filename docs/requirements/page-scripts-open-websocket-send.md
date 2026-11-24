---
slug: "page-scripts-open-websocket-send"
kind: "requirement"
title: "Page scripts can open a `WebSocket`, send and receive text frames, and see connections close on navigation or page retirement; insecure `ws:` from an HTTPS page is blocked"
status: "active"
milestone: "M3"
aliases:
  [
    "R-037"
  ]
features:
  [
    "websocket"
  ]
source: "RFC 6455; HTML Living Standard (mixed content); product decision (cookie-aware handshake, one tokio task per socket)"
---

Page scripts can open a `WebSocket`, send and receive text frames, and see connections close on navigation or page retirement; insecure `ws:` from an HTTPS page is blocked
