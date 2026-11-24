---
slug: "observers-idle"
kind: "feature"
title: "Observers, idle scheduling and feature-detect shims"
status: "done"
aliases:
  [
    "F-057"
  ]
requirements:
  [
    "page-scripts-observe-dom-mutations"
  ]
gherkin: "tests/features/observers-idle.feature"
tests:
  [
    "`js::integration::{mutation_observer_reports_child_and_attribute_changes,mutation_observer_honors_attribute_filter_and_character_data,mutation_observer_accepts_text_and_comment_targets,intersection_observer_fires_for_visible_and_hidden_elements,resize_observer_reports_initial_size_once,idle_callback_runs_with_a_deadline,css_supports_escape_and_custom_elements_registry}`",
    "`headless_js::observers_and_idle_work`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts can observe DOM mutations, intersection and size changes, schedule `requestIdleCallback` work, query `CSS.supports`, and register custom elements.

## Notes

`MutationObserver` accepts Text and Comment targets per the DOM Standard; unrooted `characterData` is delivered for direct targets, page-created nodes, and subtree descendants present at `observe`. Later CharacterData under an observed detached subtree is not hooked; childList/attribute mutations in fully detached trees are still not delivered. `oldValue` fields may be null; basic IntersectionObserver visibility is intersection-observer; ResizeObserver reports size changes only when re-measured; `CSS.supports` is a curated allow-list; upgrade and lifecycle are custom-element-upgrade.
