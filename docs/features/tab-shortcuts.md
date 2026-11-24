---
slug: "tab-shortcuts"
kind: "feature"
title: "Tab management and shortcuts"
status: "done"
aliases:
  [
    "F-046"
  ]
requirements:
  [
    "create-close-select-cycle-tabs"
  ]
gherkin: "tests/features/tab-shortcuts.feature"
tests:
  [
    "`tabs::tests::*`",
    "`keyboard::tests::tab_shortcut_table`",
    "`chrome::tests::classifies_tab_commands`",
    "manual tab-bar#1",
    "tab-shortcuts#1"
  ]
mcp: "planned mcp-tabs-devtools-state-js (`glide_tabs_*`)"
manual:
  [
    {
      "steps": "With three tabs open, press Cmd/Ctrl+Tab, Cmd/Ctrl+Shift+Tab and Cmd/Ctrl+1..3 (on macOS also Cmd+Shift+] and Cmd+Shift+[)",
      "expected": "Selection moves to the next, previous and numbered tab, wrapping at the ends",
      "aliases": [
        "M-033"
      ]
    }
  ]
---

Tabs can be created, closed, selected and cycled with the mouse and standard Cmd/Ctrl shortcuts, including Cmd/Ctrl+1..9.

## Notes

Cmd/Ctrl+W now closes the active tab and the window when the last tab closes. Cmd/Ctrl+Q still quits.
