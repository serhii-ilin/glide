---
slug: "headless-scroll-observable-by-page-scripts"
kind: "requirement"
title: "An agent can scroll the headless page by delta or to a selector, and page scripts observe the scroll (scroll offsets, scroll events, IntersectionObserver) so infinite-scroll content loads"
status: "active"
milestone: "M5"
aliases: []
features:
  [
    "mcp-scroll"
  ]
source: "CSSOM View Module (Window.scrollX/scrollY/scrollTo/scrollBy, Element.scrollIntoView); HTML Living Standard (scroll event); Intersection Observer; product decision (infinite-scroll sites such as Reddit)"
---

An agent can scroll the headless page by delta or to a selector, and page scripts observe the scroll (scroll offsets, scroll events, IntersectionObserver) so infinite-scroll content loads
