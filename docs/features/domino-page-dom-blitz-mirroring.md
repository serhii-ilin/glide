---
slug: "domino-page-dom-blitz-mirroring"
kind: "feature"
title: "Domino page DOM + Blitz mirroring"
status: "done"
aliases:
  [
    "F-036"
  ]
requirements:
  [
    "run-page-javascript-allow-turning",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`src/js/snapshot.rs`",
    "`src/js/batch.rs`",
    "`src/js/dom-glue.js`",
    "`src/js/vendor/domino.js`",
    "`js::integration::*`",
    "`tests/headless_js.rs`",
    "`tests/compat_dom_nodes.rs`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts see a domino DOM snapshot of Blitz, and mutations are mirrored back in per-task batches.

## Notes

One mutation batch per task (client ids for nodes created in the batch); SVG/MathML namespaces round-trip; `globalThis` is the window. Window capture sees `event.target`; `once` is stored on the listener record; a skipped insert is not queued twice
