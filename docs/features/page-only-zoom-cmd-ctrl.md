---
slug: "page-only-zoom-cmd-ctrl"
kind: "feature"
title: "T5 Page-only zoom (Cmd/Ctrl +/-/0) and find in page (Cmd/Ctrl+F)"
status: "done"
aliases:
  [
    "F-024"
  ]
requirements:
  [
    "page-zoom-find-page"
  ]
gherkin: "backfill pending"
tests:
  [
    "`zoom::tests::*`",
    "`find::tests::*`",
    "`keyboard::tests::readme_shortcut_table`",
    "`chrome::tests::toolbar_height_stable_across_page_zoom`",
    "manual page-only-zoom-cmd-ctrl#1–page-only-zoom-cmd-ctrl#4"
  ]
mcp: "n/a (zoom and find are chrome UI; page text via `glide_snapshot_text`)"
manual:
  [
    {
      "steps": "Open a long local page (`tests/fixtures/find.html`). Press Cmd/Ctrl+= several times, then Cmd/Ctrl+-, then Cmd/Ctrl+0",
      "expected": "Page zooms in 10% steps between 30% and 300%; reset returns to 100%; toolbar stays the same visual size",
      "aliases": [
        "M-022"
      ]
    },
    {
      "steps": "Press Cmd/Ctrl+F, type `alpha`",
      "expected": "Find bar opens with focus; matches highlight and scroll into view; count shows `n of m`",
      "aliases": [
        "M-023"
      ]
    },
    {
      "steps": "With find open, press Enter and Cmd/Ctrl+G, then Shift+Enter and Cmd/Ctrl+Shift+G",
      "expected": "Selection moves to next/previous match and wraps around",
      "aliases": [
        "M-024"
      ]
    },
    {
      "steps": "With find open, press Esc",
      "expected": "Find bar closes and page selection clears",
      "aliases": [
        "M-025"
      ]
    }
  ]
---

Page zoom changes only the page from 30% to 300%, and find highlights matches and scrolls them into view.

## Notes

Chrome viewport zoom; toolbar counter-scaled with `--ui`
