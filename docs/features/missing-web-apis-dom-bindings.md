---
slug: "missing-web-apis-dom-bindings"
kind: "feature"
title: "Missing Web APIs and DOM bindings"
status: "partial"
aliases:
  [
    "F-032"
  ]
requirements:
  [
    "reproducible-html-css-js-compatibility"
  ]
gherkin: "backfill pending"
tests:
  [
    "`tests/compat/probe.html`",
    "`docs/compat/REPORT.md`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Missing Web APIs and DOM bindings are tracked and implemented in priority order.

## Notes

network-apis, timing-history-crypto, observers-idle, document-cookie, websocket and formdata implement parts of the app-shell slice. Remaining gaps are `Range` (range-acid3), `Notification`, `Worker`, `EventSource`, `indexedDB`, `WebAssembly`, `crypto.subtle`, `structuredClone`, and shadow DOM. Autonomous custom-element upgrade is custom-element-upgrade. Compat probe counts in `docs/compat/REPORT.md` are stale until the next network run.
