---
slug: "devtools-elements-tree"
kind: "feature"
title: "DevTools Elements tree"
status: "done"
aliases:
  [
    "F-062"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-elements-tree.feature"
tests:
  [
    "`devtools::elements::tests::*`",
    "`devtools::tests::sync_elements_pane_renders_tree`",
    "`chrome::tests::{devtools_elements_pane_markup,devtools_elements_tree_focused_includes_descendant}`",
    "`keyboard::tests::{dispatch_devtools_elements_tree_keys,dispatch_devtools_elements_keys_after_row_select_focus}`",
    "`tabs::tests::duplicate_tab_resets_elements_expansion`",
    "manual devtools-elements-tree#1"
  ]
mcp: "n/a (in-window UI; MCP `glide_dom` / `glide_query` are separate)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/counter.html`, press F12, switch to **Elements**. Click a row label to select it; use Up/Down for the selection; click the **▶/▼** twisty (or Right/Left when the tree container is focused) to expand/collapse; switch to a second tab with a different page, then back",
      "expected": "Tree shows tags with escaped text; the detail pane lists attributes; arrow keys work after clicking a row; twisty click toggles children without losing keyboard focus; after DOM edits or navigation the tree updates; another tab does not show the first tab's expansion or selection",
      "aliases": [
        "M-047"
      ]
    }
  ]
---

The Elements tab shows the active page DOM as an expandable tree (≤500 rows) with a detail pane for attributes; tree text is HTML-escaped and refreshes on navigation or DOM changes. Click the twisty to expand/collapse; click a row to select and focus the tree for arrow keys.

## Notes

Per-tab expansion/selection is in-memory.
