---
slug: "history"
kind: "feature"
title: "Back / forward / reload with redirect-aware history"
status: "done"
aliases:
  [
    "F-003"
  ]
requirements:
  [
    "navigate-back-forward-reload-history"
  ]
gherkin: "tests/features/history.feature"
tests:
  [
    "`history::tests::*`"
  ]
mcp: "planned mcp-headless-session (`glide_back`, `glide_forward`, `glide_reload`)"
manual: []
---

Back, forward and reload follow the visited pages, with link clicks adding entries and redirects replacing the redirected entry.

## Notes

`History` in `src/history.rs`
