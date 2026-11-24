---
slug: "mutationobserver"
kind: "feature"
title: "`MutationObserver`"
status: "done"
aliases:
  [
    "F-038"
  ]
requirements:
  [
    "page-scripts-observe-dom-mutations",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`js::integration::{mutation_observer_reports_child_and_attribute_changes,mutation_observer_honors_attribute_filter_and_character_data}`",
    "`headless_js::observers_and_idle_work`",
    "`tests/features/observers-idle.feature`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts can observe DOM mutations with `MutationObserver`.

## Notes

Implemented in observers-idle via domino `_setMutationHandler` fan-out in `dom-glue.js`; attribute/character `oldValue` is not tracked by domino.
