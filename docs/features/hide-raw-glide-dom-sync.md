---
slug: "hide-raw-glide-dom-sync"
kind: "feature"
title: "Hide the raw `__glide_dom_sync` RPC from page code"
status: "deferred"
aliases:
  [
    "F-040"
  ]
requirements:
  [
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests: []
mcp: "n/a (deferred)"
manual: []
---

The internal DOM-sync RPC is not callable directly by page scripts.

## Notes

`navigate`, `fetch`, and `batchMutate` are reachable as a plain global. Move the binding into a closure the page cannot call
