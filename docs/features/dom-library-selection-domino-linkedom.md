---
slug: "dom-library-selection-domino-linkedom"
kind: "feature"
title: "T9a DOM library selection (domino / linkedom / happy-dom spike)"
status: "done"
aliases:
  [
    "F-035"
  ]
requirements:
  [
    "reproducible-html-css-js-compatibility"
  ]
gherkin: "backfill pending"
tests:
  [
    "`docs/dom-library-spike.md`",
    "`scripts/dom-spike/run.mjs`",
    "`scripts/dom-spike/tests/fork-timeout.test.mjs`",
    "`tests/dom_spike.rs`"
  ]
mcp: "n/a (process/tooling)"
manual: []
---

A spike compared DOM libraries and selected domino for vendoring.

## Notes

Recommends **domino** (multi-file vendoring); T9b in domino-page-dom-blitz-mirroring
