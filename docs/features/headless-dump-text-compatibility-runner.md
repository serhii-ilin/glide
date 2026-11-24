---
slug: "headless-dump-text-compatibility-runner"
kind: "feature"
title: "Headless `--dump-text` (compatibility runner hook)"
status: "done"
aliases:
  [
    "F-029"
  ]
requirements:
  [
    "reproducible-html-css-js-compatibility",
    "run-page-javascript-allow-turning"
  ]
gherkin: "backfill pending"
tests:
  [
    "`headless_js::*`",
    "`args::tests::dump_text_*`"
  ]
mcp: "planned mcp-headless-session (`glide_snapshot_text`)"
manual: []
---

`--dump-text` extracts text from a headless page for compatibility checks.

## Notes

shares `headless::load`; settle loop in `headless::settle`.
