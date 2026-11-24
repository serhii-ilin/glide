---
slug: "runtime-js-toggle-toolbar-cmd"
kind: "feature"
title: "T3 Runtime JS toggle (toolbar + Cmd/Ctrl+Shift+J) and `--dump-text` headless mode"
status: "done"
aliases:
  [
    "F-022"
  ]
requirements:
  [
    "javascript-turned-off-runtime-page",
    "run-page-javascript-allow-turning"
  ]
gherkin: "backfill pending"
tests:
  [
    "`args::tests::dump_text_*`",
    "`headless_js::*`",
    "`html::tests::set_js_runtime_mode_updates_shared_flags`",
    "`chrome::tests::classifies_chrome_actions`",
    "`keyboard::tests::*`"
  ]
mcp: "planned mcp-tabs-devtools-state-js (`glide_set_js`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/js-toggle.html`. Press Cmd/Ctrl+Shift+J twice",
      "expected": "After first toggle: toolbar **JS off**, page shows `static` and `no-js-fallback`. After second: **JS on**, page shows `scripted`",
      "aliases": [
        "M-017"
      ]
    },
    {
      "steps": "Open `tests/fixtures/counter.html`. Confirm `#count` shows `ready`. Toggle JS off then on",
      "expected": "With JS off, count stays `0`; after turning JS on, count shows `ready` again",
      "aliases": [
        "M-018"
      ]
    }
  ]
---

Users can switch page JavaScript on or off at runtime, and `--dump-text` exposes page text headlessly.

## Notes

manual runtime-js-toggle-toolbar-cmd#1–runtime-js-toggle-toolbar-cmd#2
