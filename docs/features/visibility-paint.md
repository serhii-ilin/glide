---
slug: "visibility-paint"
kind: "feature"
title: "Paint visible descendants of hidden ancestors"
status: "done"
aliases:
  [
    "F-089"
  ]
requirements:
  [
    "element-visibility-visible-inside-visibility"
  ]
gherkin: "tests/features/visibility-paint.feature"
tests:
  [
    "`shot::tests::{visible_child_of_hidden_body_is_painted,visibility_hidden_element_is_not_painted}`",
    "manual visibility-paint#1"
  ]
mcp: "n/a (engine painting; observable via `glide_screenshot`)"
manual:
  [
    {
      "steps": "Run `cargo run --release -- --screenshot https://www.reddit.com reddit.png` (network needed) and open the PNG; then open `https://www.reddit.com` in the window",
      "expected": "The PNG and the window show Reddit's header, feed cards and sidebar, not a blank white page",
      "aliases": [
        "M-068"
      ]
    }
  ]
---

A page that hides its shell with `visibility: hidden` on `body` but marks the content `visibility: visible` is drawn instead of blank, as in other browsers; the hidden elements themselves stay invisible. Fixes the blank Reddit page.

## Notes

Upstream `blitz-paint` returned early for any element with `visibility != visible`, skipping its whole subtree. Vendored as `third_party/blitz-paint` (patch in `render_element` / `render_hidden_element_children`, marked `Glide patch (visibility-paint)`). Limits: inline text laid out by the hidden element stays hidden, and the hidden element's own overflow clip and scroll offset are not applied to its descendants.
