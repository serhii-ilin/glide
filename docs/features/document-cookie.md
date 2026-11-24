---
slug: "document-cookie"
kind: "feature"
title: "`document.cookie` / cookie-jar bridge"
status: "done"
aliases:
  [
    "F-058"
  ]
requirements:
  [
    "page-scripts-read-write-document"
  ]
gherkin: "tests/features/document-cookie.feature"
tests:
  [
    "`cookies::tests::*`",
    "`http::tests::set_cookie_echoes_on_follow_up_request`",
    "`js::apply::tests::cookie_rpc_get_hides_httponly`",
    "`js::integration::{document_cookie_get_set_visible_to_scripts,document_cookie_expired_assignment_does_not_throw,document_cookie_rejected_assignment_does_not_throw}`",
    "manual profile-dir-settings-session-history#4"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts can read and write cookies through `document.cookie`, and script-visible cookies match the jar used for navigation and `fetch`. HttpOnly cookies never appear in the getter.

## Notes

One process-wide `CookieJar` (`get_or_init` singleton; headless/MCP/`--dump-text` share it via `ensure_process_global`). `cookie_store` 0.22.1 with embedded MPL `public_suffix_list.dat` (`publicsuffix` 2.3) rejects invalid `Domain=` values. SameSite, credentials (`omit`/`same-origin`/`include` on fetch/XHR RPC), and HttpOnly document setter guards. The setter is best-effort: matching expired assignments delete the cookie; non-matching expired assignments and rejected parses are ignored without throwing. `cookies.json` saves on persistent Set-Cookie / `document.cookie` when a profile path is set.
