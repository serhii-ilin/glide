---
slug: "super-critical-compatibility-fixes"
kind: "feature"
title: "T8 Super-critical compatibility fixes"
status: "done"
aliases:
  [
    "F-028"
  ]
requirements:
  [
    "crash-hang-panic-common-pages",
    "reproducible-html-css-js-compatibility"
  ]
gherkin: "backfill pending"
tests:
  [
    "`docs/compat/REPORT.md`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

No super-critical crash, hang or panic was found in the first compatibility run.

## Notes

T7 run (191 checks) found no crash, panic or hang; nothing to fix. Non-critical gaps are wpt-css-reftest-pixel-diffs, wpt-testharness-dom-nodes-measurable, missing-web-apis-dom-bindings, acid2-acid3-layout-js-test
