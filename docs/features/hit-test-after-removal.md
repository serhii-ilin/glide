---
slug: "hit-test-after-removal"
kind: "feature"
title: "Hit testing after script-driven node removal"
status: "done"
aliases:
  [
    "F-052"
  ]
requirements:
  [
    "clicking-page-after-scripts-remove"
  ]
gherkin: "tests/features/hit-test-after-removal.feature"
tests:
  [
    "`js::host::tests::stale_paint_child_is_pruned_before_hit_test`",
    "`js::host::tests::paint_after_removed_positioned_node_does_not_panic`",
    "`js::paint_prune::tests::prune_drops_only_dead_ids`",
    "`js::integration::removed_positioned_child_stays_paintable`",
    "`chrome::tests::menu_rebuild_keeps_hit_testing_safe`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Clicks, hover and painting keep working after page scripts or chrome rebuilds remove rendered nodes; stale Blitz paint and stacking-context entries are pruned instead of panicking.

## Notes

`js::paint_prune` drops dead ids from `paint_children`, `layout_children`, `hoisted_children`, `stacking_context`, `sc_contribution_cache` and `anonymous_blocks`, keeping live paint order. It runs after every mirrored mutation batch, after chrome rebuilds in `Glide::sync` (the shell hit-tests before the next resolve), and inside `hit_test` before resolve; stale entries found there are logged as `dom.hit`. Reproduced as the startup panic on example.com, whose script replaces nodes while the chrome menu bar re-renders.
