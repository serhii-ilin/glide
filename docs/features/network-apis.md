---
slug: "network-apis"
kind: "feature"
title: "Network and binary Web APIs"
status: "done"
aliases:
  [
    "F-055"
  ]
requirements:
  [
    "page-scripts-build-formdata-including",
    "page-scripts-load-send-data"
  ]
gherkin: "tests/features/network-apis.feature"
tests:
  [
    "`js::apply::tests::{parse_data_url_base64_decodes_payload,js_headers_omit_set_cookie,build_fetch_request_carries_post_body}`",
    "`js::integration::xhr_sync_and_async_requests_work`",
    "`js::integration::fetch_request_and_response_objects_work`",
    "`js::integration::fetch_and_xhr_base64_data_urls_work`",
    "`js::integration::xhr_abort_stops_callbacks`",
    "`js::integration::xhr_headers_available_at_headers_received`",
    "`js::integration::fetch_binary_body_round_trips`",
    "`js::integration::fetch_request_init_overrides_input`",
    "`js::integration::send_beacon_returns_true_without_throwing`",
    "`js::integration::text_and_blob_helpers_work`",
    "`js::apply::tests::{start_fetch_returns_before_the_network_completes,start_fetch_answers_data_urls_inline}`",
    "`headless_js::network_apis_work`",
    "formdata tests for typed bodies"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts can call `fetch` (with `Request`/`Response`/`Headers`), `XMLHttpRequest`, and `navigator.sendBeacon`, and can build and read binary data with `Blob`, `File`, `FileReader`, `TextEncoder`, and `TextDecoder`, so app shells can feature-detect and use the network surface without silent fallback. Slow requests never freeze the window: the UI thread dispatches them and the response arrives when the network answers.

## Notes

The UI thread dispatches a page fetch/XHR to tokio and never waits for the network; the V8 worker still waits synchronously for the reply through the DOM RPC (a 25 s fetch timeout bounds it below the worker's 30 s wait, so the page sees `fetch timed out` rather than a generic RPC timeout). Replies carry raw bytes as `bodyBase64`. Async XHR runs the RPC inside a queued task; `sendBeacon` is fire-and-forget. `Set-Cookie` is stripped from script-visible response headers; CORS and credentials are not enforced. The shared jar is document-cookie. `AbortSignal` does not cancel work after the RPC starts. FormData / multipart is formdata.
