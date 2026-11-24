---
slug: "intersection-observer"
kind: "feature"
title: "IntersectionObserver and native lazy images"
status: "done"
aliases:
  [
    "F-083"
  ]
requirements:
  [
    "page-scripts-receive-intersectionobserver"
  ]
gherkin: "tests/features/intersection-observer.feature"
tests:
  [
    "`js::integration::{intersection_observer_fires_after_dom_moves_target_into_view,intersection_observer_fires_for_visible_and_hidden_elements,intersection_observer_threshold_crossing_fires,intersection_observer_take_records_buffers_entries,lazy_img_fetches_only_after_near_viewport_layout_change}`",
    "manual intersection-observer#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "Open a tall `file://tests/fixtures/` page with a below-the-fold box and an intersection observer in the console, scroll with Page Down, and repeat with a `loading=\"lazy\"` image below the fold",
      "expected": "Intersection callbacks fire after scrolling/layout moves targets into view; the lazy image requests only after it approaches the viewport",
      "aliases": [
        "M-063"
      ]
    }
  ]
---

IntersectionObserver re-evaluates after scroll, layout/DOM updates and rAF, delivers threshold crossings with full entry fields, and deferred fetches for `loading="lazy"` images when they approach the viewport.

## Notes

Layout rects come from Blitz/Taffy via `getBoundingClientRect`. Checks are rAF-throttled and also scheduled after `__glide_flush_dom` / `__glide_flush_raf`. `window.scrollTo` maps to viewport scroll over DOM RPC. Native lazy images use a root-margin observer plus `LoadImage` RPC.
