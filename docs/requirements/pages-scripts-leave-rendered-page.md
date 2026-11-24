---
slug: "pages-scripts-leave-rendered-page"
kind: "requirement"
title: "When a page's scripts leave the rendered page blank, Glide shows the page's server-rendered content (parsed with scripting off, `<noscript>` content included) instead of an empty page, and records the switch"
status: "active"
milestone: "M3"
aliases:
  [
    "R-046"
  ]
features:
  [
    "fail-safe-rendering-live-window",
    "failsafe-static-render"
  ]
source: "HTML Living Standard (scripting-disabled parsing, `noscript`); product decision (rendering fails safe)"
---

When a page's scripts leave the rendered page blank, Glide shows the page's server-rendered content (parsed with scripting off, `<noscript>` content included) instead of an empty page, and records the switch
