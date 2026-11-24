---
slug: "compatibility-runner-compat-bin-docs"
kind: "feature"
title: "T7 Compatibility runner (`compat` bin) and `docs/compat/REPORT.md`"
status: "done"
aliases:
  [
    "F-027"
  ]
requirements:
  [
    "reproducible-html-css-js-compatibility"
  ]
gherkin: "backfill pending"
tests:
  [
    "`compat::*`",
    "`tests/compat_runner.rs`",
    "`docs/compat/results.json`"
  ]
mcp: "n/a (process/tooling)"
manual: []
---

The `compat` binary generates a reproducible report from Acid, WPT and API probes.

## Notes

Acid1-3 live URLs, ~100 WPT reftests + 30 `dom/nodes` harness; `scripts/vendor-wpt.sh`
