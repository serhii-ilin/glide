---
slug: "acid2-acid3-layout-js-test"
kind: "feature"
title: "Acid2/Acid3 layout and JS test gaps"
status: "partial"
aliases:
  [
    "F-033"
  ]
requirements:
  [
    "render-html-css-flex-floats",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`docs/compat/REPORT.md`",
    "`tests/compat_dom_nodes.rs`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Acid2/Acid3 layout and JavaScript gaps are tracked against the wpt-dom-nodes-15-subtests thresholds.

## Notes

Acid3 with JS is 34/100 (wpt-dom-nodes-15-subtests floor is 15). Acid2 reftest still differs (~3.7% pixel diff). Acid1 stays unreachable (HTTP 403)
