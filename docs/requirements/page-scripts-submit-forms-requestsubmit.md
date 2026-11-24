---
slug: "page-scripts-submit-forms-requestsubmit"
kind: "requirement"
title: "Page scripts can submit forms (`requestSubmit`, `submit`, the `onsubmit` handler) and navigate the browsing context (`location.assign`/`replace`), and a headless load follows those navigations for a bounded number of hops"
status: "active"
milestone: "M3"
aliases:
  [
    "R-032"
  ]
features:
  [
    "page-navigation"
  ]
source: "HTML Living Standard (form submission algorithm, `requestSubmit`, `location`); product decision (headless follows page navigation, Reddit's JavaScript challenge)"
---

Page scripts can submit forms (`requestSubmit`, `submit`, the `onsubmit` handler) and navigate the browsing context (`location.assign`/`replace`), and a headless load follows those navigations for a bounded number of hops
