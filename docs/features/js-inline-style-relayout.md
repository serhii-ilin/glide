---
slug: "js-inline-style-relayout"
kind: "feature"
title: "Layout updates after page script changes inline style or class"
status: "deferred (found while writing mcp-scroll: an inline style or class change from page script left getBoundingClientRect and computed style unchanged in the V8 harness, while structural DOM changes relayout correctly; not a crash or hang, so logged per the compat fix policy)"
aliases: []
requirements: []
gherkin: "planned (spec added when promoted)"
tests: []
mcp: "n/a (engine behavior; observable via `glide_query` and `glide_computed_style`)"
manual: []
---

After page script sets `element.style.x`, `style.cssText`, the `style` attribute or `className`, the element's geometry and computed style reflect the change.

## Notes

Reproduced with `spacer.style.height = "10px"`, `setAttribute("style", ...)` and a class swap: the attribute value updates but `getBoundingClientRect` and `getComputedStyle` keep the old values; removing the element, or setting `hidden`, relayouts. The IntersectionObserver tests `intersection_observer_fires_after_dom_moves_target_into_view` and `lazy_img_fetches_only_after_near_viewport_layout_change` used `style.marginTop` and passed only because the root bounds equalled the document size; they now remove a spacer element instead.
