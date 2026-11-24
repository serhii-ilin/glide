---
slug: "element-visibility-visible-inside-visibility"
kind: "requirement"
title: "An element with `visibility: visible` inside a `visibility: hidden` ancestor is painted, while the hidden ancestor itself paints nothing"
status: "active"
milestone: "M3"
aliases:
  [
    "R-047"
  ]
features:
  [
    "visibility-paint"
  ]
source: "CSS 2.1 section 11.2 (visibility); product decision (Reddit hides `body` until hydration)"
---

An element with `visibility: visible` inside a `visibility: hidden` ancestor is painted, while the hidden ancestor itself paints nothing
