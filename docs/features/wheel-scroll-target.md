---
slug: "wheel-scroll-target"
kind: "feature"
title: "Wheel scroll targets the inner pane under the pointer"
status: "done"
aliases:
  [
    "F-075"
  ]
requirements:
  [
    "wheel-over-nested-scrollable-element"
  ]
gherkin: "tests/features/wheel-scroll-target.feature"
tests:
  [
    "`js::host::tests::{page_subdoc_hover_is_none_without_page_iframe,page_subdoc_hover_is_none_when_subdoc_has_no_hover,page_subdoc_hover_is_none_false_after_set_hover,wheel_scrolls_inner_pane_after_hover_refresh}`",
    "`app::tests::{wheel_should_refresh_page_hover_when_subdoc_hover_missing,wheel_should_not_refresh_page_hover_when_page_iframe_is_not_hovered,wheel_should_not_refresh_page_hover_when_subdoc_hover_set,wheel_should_not_refresh_page_hover_while_button_held,wheel_should_not_refresh_page_hover_for_non_wheel_events}`",
    "manual wheel-scroll-target#1"
  ]
mcp: "n/a (window input; headless MCP tools cannot dispatch a wheel)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/scroll-pane.html` in a window. Part 1: rest the pointer over the middle of the inner pane and wheel down — the pane scrolls and the page behind it does not jump. Part 2 (the post-navigation case): move the pointer onto the **Reload this pane page** link inside the pane, click it, do not move the mouse, then wheel down — the reloaded pane scrolls from the same pointer position. Part 3: scroll the pane to its bottom, wheel down again — the page viewport scrolls. Repeat part 2 on Reddit's right sidebar or a post drawer after navigation without moving the mouse",
      "expected": "The inner `overflow:auto` region scrolls under the wheel; immediately after the in-pane reload, the pane scrolls even though the pointer never moved; at the pane's bottom, leftover delta scrolls the page",
      "aliases": [
        "M-053"
      ]
    }
  ]
---

Moving the mouse wheel over a nested scrollable region scrolls that region first (including right after navigation when the pointer has not moved), and only passes leftover delta to the page.

## Notes

Blitz scrolls the stored hover node; when the page iframe subdocument has no hover, Glide forwards a synthetic `PointerMoved` from the last window coordinates before the wheel so hit-testing matches the pointer.
