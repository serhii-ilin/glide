---
slug: "tab-bar"
kind: "feature"
title: "Tab strip UI"
status: "done"
aliases:
  [
    "F-044"
  ]
requirements:
  [
    "create-close-select-cycle-tabs",
    "provide-safari-like-chrome-tab"
  ]
gherkin: "tests/features/tab-bar.feature"
tests:
  [
    "`chrome::tests::tab_strip_renders_tabs_and_controls`",
    "`chrome::tests::active_tab_is_marked_in_tab_strip`",
    "`chrome::tests::tab_close_buttons_carry_tab_ids`",
    "`app::tests::titles_changed_detects_a_page_title_update`",
    "manual tab-bar#1"
  ]
mcp: "planned mcp-tabs-devtools-state-js (`glide_tabs_list`, `glide_tab_select`, `glide_tab_new`, `glide_tab_close`)"
manual:
  [
    {
      "steps": "Open two local `file://` fixtures. Click a tab, then its close button (the × at the tab's leading edge), then the `+` new-tab button at the right end of the tab row",
      "expected": "The clicked tab becomes active and its page is shown; the close button removes that tab and selects a neighbour; new-tab adds and activates a tab",
      "aliases": [
        "M-032"
      ],
      "covers": [
        "tab-shortcuts"
      ]
    }
  ]
---

The window shows a strip of tabs with a new-tab button, an active tab, titles and a close control on each tab.

## Notes

Safari-style separate tab bar below the toolbar: equal-width flat segments with hairline dividers, the active tab lighter, the close control at the leading edge on hover/active, an accent dot while loading, centered titles clipped to the tab, and a fixed-width `+` new-tab button at the end. Blitz has no `text-overflow: ellipsis`, so long titles are clipped rather than ellipsized.
