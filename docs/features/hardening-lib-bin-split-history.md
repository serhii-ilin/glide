---
slug: "hardening-lib-bin-split-history"
kind: "feature"
title: "T1 Hardening: lib/bin split, `History` module + tests, tested CLI args, clippy clean, CI"
status: "done"
aliases:
  [
    "F-020"
  ]
requirements:
  [
    "navigate-back-forward-reload-history",
    "quality-gates-fmt-clippy-d"
  ]
gherkin: "backfill pending"
tests:
  [
    "`history::tests::*`",
    "`args::tests::*`",
    "CI workflow"
  ]
mcp: "n/a (process/tooling)"
manual: []
---

The crate is split into library and binaries, core logic is unit-tested, and CI enforces the quality gates.

## Notes

lib/bin split, `src/app.rs`
