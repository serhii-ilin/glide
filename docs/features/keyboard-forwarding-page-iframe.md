---
slug: "keyboard-forwarding-page-iframe"
kind: "feature"
title: "Keyboard forwarding into the page iframe"
status: "done"
aliases:
  [
    "F-007"
  ]
requirements:
  [
    "documented-keyboard-shortcut-text-editing"
  ]
gherkin: "backfill pending"
tests:
  [
    "`keyboard::tests::headless_iframe_input_receives_keys`",
    "manual keyboard-forwarding-page-iframe#1",
    "keyboard-forwarding-page-iframe#2"
  ]
mcp: "planned mcp-page-input (`glide_type`)"
manual:
  [
    {
      "steps": "`cargo run --release -- tests/fixtures/input.html`. Click the input, type `abc`, Backspace",
      "expected": "Input shows `ab`",
      "aliases": [
        "M-002"
      ]
    },
    {
      "steps": "Open `tests/fixtures/input.html`, click the input (a first click on the page may only focus the iframe), type `x`",
      "expected": "Characters appear in the page input, not the address bar",
      "aliases": [
        "M-016"
      ]
    }
  ]
---

When the page iframe has focus, keyboard input goes to the page instead of the chrome.
