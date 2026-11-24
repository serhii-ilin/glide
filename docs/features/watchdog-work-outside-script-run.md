---
slug: "watchdog-work-outside-script-run"
kind: "feature"
title: "Watchdog for work outside a script run"
status: "deferred"
aliases:
  [
    "F-041"
  ]
requirements:
  [
    "crash-hang-panic-common-pages",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests: []
mcp: "n/a (deferred)"
manual: []
---

A watchdog bounds asynchronous JS work, not just a single script evaluation.

## Notes

The 5 s limit covers `run_script` only. An endless promise chain keeps the `glide-js` thread spinning with no watchdog
