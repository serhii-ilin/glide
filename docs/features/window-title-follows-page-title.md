---
slug: "window-title-follows-page-title"
kind: "feature"
title: "Window title follows page `<title>`"
status: "done"
aliases:
  [
    "F-004"
  ]
requirements:
  [
    "navigate-back-forward-reload-history"
  ]
gherkin: "backfill pending"
tests:
  [
    "manual"
  ]
mcp: "planned mcp-headless-session (`glide_page_info`)"
manual:
  [
    {
      "steps": "Open `https://example.com`",
      "expected": "Window title reads \"Example Domain — Glide\"",
      "aliases": [
        "M-001"
      ]
    }
  ]
---

The window title shows the loaded page's document title.
