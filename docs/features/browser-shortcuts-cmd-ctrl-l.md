---
slug: "browser-shortcuts-cmd-ctrl-l"
kind: "feature"
title: "Browser shortcuts (Cmd/Ctrl+L/R/W/Q/[/], Alt+arrows, F5, scroll keys)"
status: "done"
aliases:
  [
    "F-005"
  ]
requirements:
  [
    "documented-keyboard-shortcut-text-editing"
  ]
gherkin: "backfill pending"
tests:
  [
    "`keyboard::tests::readme_shortcut_table`",
    "`keyboard::tests::{scroll_down_keys_move_the_page_down_by_a_page_step,scroll_up_keys_move_the_page_up_and_clamp_at_the_top,scroll_home_goes_to_top_and_end_to_the_clamped_bottom}`",
    "manual browser-shortcuts-cmd-ctrl-l#1–browser-shortcuts-cmd-ctrl-l#10"
  ]
mcp: "n/a (chrome UI)"
manual:
  [
    {
      "steps": "Focus the address bar, type `example.com`, press Enter",
      "expected": "Page navigates to Example Domain",
      "aliases": [
        "M-004"
      ]
    },
    {
      "steps": "Open a long page. Click the page (not the address bar). Press Space several times",
      "expected": "Viewport scrolls down",
      "aliases": [
        "M-005"
      ]
    },
    {
      "steps": "Same page, press Shift+Space, PgUp, PgDn, Home, End, and arrow keys with no text field focused",
      "expected": "Page scrolls in the matching direction",
      "aliases": [
        "M-006"
      ]
    },
    {
      "steps": "With the address bar focused, press Space",
      "expected": "A space is typed; the page does not scroll",
      "aliases": [
        "M-007"
      ]
    },
    {
      "steps": "Press Cmd/Ctrl+L",
      "expected": "Address bar receives focus and its text is selected; the next typed character replaces the URL",
      "aliases": [
        "M-008"
      ],
      "covers": [
        "text-editing-macos-standard-keybindings"
      ]
    },
    {
      "steps": "Visit two URLs, press Alt+Left and Alt+Right (Option+arrows on macOS)",
      "expected": "Back and forward navigate history",
      "aliases": [
        "M-009"
      ]
    },
    {
      "steps": "Press Cmd/Ctrl+[ then Cmd/Ctrl+]",
      "expected": "Back then forward",
      "aliases": [
        "M-010"
      ]
    },
    {
      "steps": "Press Ctrl+R, F5, and on macOS Cmd+R",
      "expected": "Page reloads each time",
      "aliases": [
        "M-011"
      ]
    },
    {
      "steps": "Press Cmd/Ctrl+W with one tab open",
      "expected": "The tab closes and the window closes",
      "aliases": [
        "M-012"
      ],
      "covers": [
        "tab-shortcuts"
      ]
    },
    {
      "steps": "Press Cmd/Ctrl+Q",
      "expected": "The process exits",
      "aliases": [
        "M-013"
      ]
    }
  ]
---

Documented shortcuts navigate, reload, close, quit, find, zoom and scroll the browser.
