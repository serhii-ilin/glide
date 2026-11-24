---
slug: "devtools-computed-styles"
kind: "feature"
title: "DevTools computed styles and highlight"
status: "done"
aliases:
  [
    "F-063"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-computed-styles.feature"
tests:
  [
    "`devtools::computed::tests::*`",
    "`devtools::highlight::tests::*`",
    "`devtools::tests::sync_elements_pane_includes_computed_styles`",
    "`chrome::tests::devtools_elements_computed_pane_markup`",
    "`control::page_ops::tests::computed_style_map_for_selector_reads_display`",
    "`tests/mcp_stdio.rs` (`computed_style_over_stdio`)",
    "manual devtools-computed-styles#1"
  ]
mcp: "`glide_computed_style`"
manual:
  [
    {
      "steps": "Open `tests/fixtures/devtools-computed-sample.html`, press F12, switch to **Elements**, select `#box`",
      "expected": "Computed pane lists `display: flex` and related properties; a blue highlight frames the box on the page; clearing selection or closing DevTools removes the highlight; another tab does not show the first tab's highlight",
      "aliases": [
        "M-048"
      ]
    }
  ]
---

With an element selected in Elements, the pane shows curated computed CSS (box model summary + common properties) and draws `#inspect-hl` over the node's border box in the page iframe; both refresh with each Elements sync and hide when DevTools closes, the tab changes, or selection clears.

## Notes

Selection-only highlight (no tree-row hover).
