---
slug: "devtools-network"
kind: "feature"
title: "DevTools Network tab"
status: "done"
aliases:
  [
    "F-065"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-network.feature"
tests:
  [
    "`devtools::network::tests::*`",
    "`network_feed::tests::*`",
    "`net::tests::{network_log_records_subresource,document_post_records_method_post,http_subresource_records_status_and_headers,http_subresource_no_content_records_204}`",
    "`js::apply::tests::{start_fetch_records_network_entry,start_fetch_xhr_records_xhr_initiator,start_fetch_invalid_url_records_error_before_reply,start_fetch_file_uses_stamp_captured_at_start}`",
    "`chrome::tests::classifies_devtools_network_clear`"
  ]
mcp: "planned mcp-network-requests (`glide_network_requests`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/network-subresources.html` (or any local page with a CSS/JS link), open DevTools → Network, reload",
      "expected": "Table shows document and subresource rows with status, type, size and time; filter narrows rows; row click shows URL and headers in the detail pane without navigating; Clear empties the table until new loads",
      "aliases": [
        "M-050"
      ]
    }
  ]
---

The Network tab lists completed document, subresource, `fetch()` and `XMLHttpRequest` requests for the active page with a filterable table and per-row detail.

## Notes

Per-tab ring buffer (200); document epoch bumps at fetch start; link navigation prunes stale rows. Document POST navigations record method `POST`.
