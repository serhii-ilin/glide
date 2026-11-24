---
slug: "unified-session-history"
kind: "feature"
title: "Unified session history stack"
status: "done"
aliases:
  [
    "F-086"
  ]
requirements:
  [
    "page-scripts-read-performance-timing"
  ]
gherkin: "tests/features/unified-session-history.feature"
tests:
  [
    "`nav::tests::chrome_back_after_push_state_is_same_document_traversal`",
    "`js::integration::{history_unified_stack_length_matches_rust,history_hashchange_fires_on_fragment_only_push}`",
    "`headless_js::unified_history_fixture_works`",
    "`history::tests::*`",
    "`js::history_rpc::tests::*`",
    "manual unified-session-history#1"
  ]
mcp: "`glide_back`, `glide_forward` (headless traverses the same stack as page `history`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/unified-history.html`, then replace the page body with a tiny SPA script that calls `history.pushState` twice and `history.back()` once. Use toolbar Back and Forward, then `history.back()` / `history.forward()` in the Console REPL",
      "expected": "Toolbar and page `history` stay in sync: back over a pushState entry does not reload the document (popstate only); forward returns; full navigations still load a new document",
      "aliases": [
        "M-067"
      ]
    }
  ]
---

`history.pushState`/`replaceState` and chrome or MCP back/forward share one Rust-owned stack per tab: same-document entries update location and fire `popstate` without reloading; document entries still fetch; `hashchange` fires on fragment-only moves; cross-origin pushes throw `SecurityError`.

## Notes

State is JSON-cloned on the host. Stack is capped at 100 entries. Tab session persistence still stores URLs only for saved stacks.
