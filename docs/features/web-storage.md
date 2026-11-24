---
slug: "web-storage"
kind: "feature"
title: "Origin-scoped persistent Web Storage"
status: "done"
aliases:
  [
    "F-081"
  ]
requirements:
  [
    "page-scripts-use-origin-scoped"
  ]
gherkin: "tests/features/web-storage.feature"
tests:
  [
    "`web_storage::tests::*`",
    "`js::storage_events::tests::*`",
    "`js::integration::web_storage_*`",
    "manual web-storage#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "Set `localStorage.setItem('persist','yes')` on `http://127.0.0.1` (or a `file://` fixture is insufficient — use a tuple origin). Quit Glide with Cmd/Ctrl+Q and relaunch with the same profile (no URL on the command line). Navigate to the same origin and run `localStorage.getItem('persist')` in the console",
      "expected": "The value reads `yes` after restart (`local_storage.json` under the profile dir). Cross-tab `storage` events are not asserted here: only the active tab runs a JS worker today, so a background tab does not receive `storage` until it is selected and its worker is live",
      "aliases": [
        "M-062"
      ]
    }
  ]
---

Page scripts can use `localStorage` and `sessionStorage` partitioned by origin; tuple-origin `localStorage` persists in `local_storage.json` beside `cookies.json`, enforces a ~5 MiB quota with `QuotaExceededError`, fires `storage` on other same-origin documents, and `sessionStorage` survives same-tab navigation within the tab.

## Notes

Rust-backed via DOM RPC; `data:` and other opaque origins use non-persistent per-document stores. Turso/SQLite considered overkill; serde JSON + atomic rename only. Only the active tab runs a JS worker today, so cross-tab `storage` events apply to same-origin peers in the same process when multiple workers are registered (e.g. tests), not background tabs.
