---
slug: "keyboard-verification-table-tests-readme"
kind: "feature"
title: "T2 Keyboard verification: table tests for every README shortcut, manual checklist"
status: "done"
aliases:
  [
    "F-021"
  ]
requirements:
  [
    "documented-keyboard-shortcut-text-editing"
  ]
gherkin: "backfill pending"
tests:
  [
    "`keyboard::tests::*`",
    "`docs/manual-tests.md` browser-shortcuts-cmd-ctrl-l#1–keyboard-forwarding-page-iframe#2"
  ]
mcp: "n/a (process/tooling)"
manual: []
---

Every README shortcut has an automated table test and a manual verification row.
