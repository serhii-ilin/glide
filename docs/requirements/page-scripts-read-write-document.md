---
slug: "page-scripts-read-write-document"
kind: "requirement"
title: "Page scripts can read and write `document.cookie`, and those values stay in sync with the network cookie jar used for fetches"
status: "active"
milestone: "M3"
aliases:
  [
    "R-028"
  ]
features:
  [
    "document-cookie"
  ]
source: "HTML Living Standard (`document.cookie`); RFC 6265; product decision (Google Search interstitial uses `SG_SS`)"
---

Page scripts can read and write `document.cookie`, and those values stay in sync with the network cookie jar used for fetches
