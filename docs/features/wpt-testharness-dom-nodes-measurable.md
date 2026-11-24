---
slug: "wpt-testharness-dom-nodes-measurable"
kind: "feature"
title: "WPT testharness `dom/nodes` measurable"
status: "done"
aliases:
  [
    "F-031"
  ]
requirements:
  [
    "run-page-javascript-allow-turning",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`docs/compat/REPORT.md`",
    "`docs/DECISIONS.md`",
    "`tests/compat_dom_nodes.rs`",
    "`tests/compat_runner.rs`",
    "`js::integration::*`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

The WPT `dom/nodes` harness runs against the domino DOM with measurable pass thresholds.

## Notes

Release at `504b190`: 12/28 files fully pass; 636/969 subtests (65.6%). The 8 files with no subtest results each count as one failed subtest. `tests/compat_dom_nodes.rs` gates the same set at ≥10 files and ≥50%
