---
slug: "url-schemes"
kind: "feature"
title: "`file://`, `data:`, http(s), and `about:blank`"
status: "done"
aliases:
  [
    "F-011"
  ]
requirements:
  [
    "render-html-css-flex-floats"
  ]
gherkin: "tests/features/url-schemes.feature"
tests:
  [
    "`about::tests::*`",
    "`location::tests::keeps_about_urls`",
    "`control::session::tests::resolve_nav_url_keeps_about_scheme`",
    "`net::tests::{about_blank_delivers_empty_document,unsupported_about_urls_serve_local_error_page_without_provider}`",
    "`headless::tests::{fetch_about_blank_without_network,fetch_unsupported_about_urls_local_error_without_provider}`",
    "`tabs::tests::new_tab_starts_on_about_blank`",
    "`history::tests::back_from_second_page_returns_about_blank`",
    "`js::apply::tests::{navigate_rpc_requests_the_target_url,fetch_about_blank_returns_local_html_without_provider}`",
    "`control_session::{navigate_about_blank_reports_empty_document,navigate_about_blank_history_back_round_trips}`"
  ]
mcp: "`glide_navigate` (`about:blank` and other schemes)"
manual:
  [
    {
      "steps": "Open a new tab (Cmd/Ctrl+T) or type `about:blank` in the address bar; open `about:src`",
      "expected": "New tab shows a blank white page; the address bar keeps `about:blank` (not a DuckDuckGo search). `about:src` shows Glide's \"Can't open page\" error locally",
      "aliases": [
        "M-054"
      ]
    }
  ]
---

Local files, data URLs, web pages, and `about:blank` load with their stylesheets, images and fonts; `about:blank` is an empty document with no network I/O. Other `about:` URLs show a local error page without network I/O.

## Notes

HTTP document loads use `src/http.rs` (redirects)
