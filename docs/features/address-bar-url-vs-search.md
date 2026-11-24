---
slug: "address-bar-url-vs-search"
kind: "feature"
title: "Address bar: URL vs. search resolution (bare host → https, phrase → DuckDuckGo HTML)"
status: "done"
aliases:
  [
    "F-001"
  ]
requirements:
  [
    "open-url-search-phrase-typed"
  ]
gherkin: "backfill pending"
tests:
  [
    "`location::tests::*`",
    "`chrome::tests::reads_the_address_form`"
  ]
mcp: "planned mcp-headless-session (`glide_navigate`)"
manual: []
---

Typing a bare host navigates to its HTTPS page, typing a phrase searches DuckDuckGo's HTML frontend, and a full URL navigates directly.
