---
slug: "devtools-inspect"
kind: "feature"
title: "DevTools click-to-inspect"
status: "done"
aliases:
  [
    "F-064"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-inspect.feature"
tests:
  [
    "`devtools::inspect::tests::*`",
    "`devtools::tests::inspect_exits_when_devtools_tab_changes`",
    "`chrome::tests::classifies_devtools_inspect_toggle`",
    "`keyboard::tests::readme_shortcut_table`",
    "manual devtools-inspect#1"
  ]
mcp: "n/a (UI mode; agents use `glide_query` / `glide_computed_style`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/devtools-inspect-sample.html`, press F12 (or Cmd/Ctrl+Shift+C to open DevTools on Elements). Click the element-picker icon or press Cmd/Ctrl+Shift+C again. Hover `#inner` and `#link`; click the link",
      "expected": "Picker button shows pressed state; hover draws `#inspect-hl` and a tag label; click selects the link in Elements with ancestors expanded and does not navigate or follow the href; scrolling the page clears the hover overlay; Esc exits inspect mode but leaves DevTools open; switching to Console exits inspect mode",
      "aliases": [
        "M-049"
      ]
    }
  ]
---

The user can toggle inspect mode (toolbar picker or Cmd/Ctrl+Shift+C), hover to highlight a page element, and click to select it in Elements without delivering the click to the page.

## Notes

Esc exits inspect mode before closing DevTools. Chrome UI outside `#page` is not pickable.
