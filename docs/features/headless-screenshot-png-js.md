---
slug: "headless-screenshot-png-js"
kind: "feature"
title: "Headless `--screenshot` to PNG (with and without JS)"
status: "done"
aliases:
  [
    "F-010"
  ]
requirements:
  [
    "render-page-png-window"
  ]
gherkin: "backfill pending"
tests:
  [
    "manual"
  ]
mcp: "planned mcp-headless-session (`glide_screenshot`)"
manual:
  [
    {
      "steps": "`cargo run --release -- --screenshot https://example.com /tmp/x.png`",
      "expected": "PNG shows the page",
      "aliases": [
        "M-003"
      ]
    }
  ]
---

`--screenshot` paints a loaded page to a PNG without opening a window.

## Notes

Raster path split into `shot::paint_rgba` / `shot::encode_png`.
