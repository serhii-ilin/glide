---
slug: "link-opening"
kind: "feature"
title: "Link opening with modifier keys"
status: "partial"
aliases:
  [
    "F-048"
  ]
requirements:
  [
    "modified-link-activation-opens-tab"
  ]
gherkin: "tests/features/link-opening.feature"
tests:
  [
    "`tabs::tests::link_target_from_modifiers`",
    "manual link-opening#1"
  ]
mcp: "n/a (modifier-click is window input)"
manual:
  [
    {
      "steps": "Open a local link fixture. Plain-click a link, then Cmd/Ctrl-click another link, then Cmd/Ctrl+Shift-click a third link",
      "expected": "Plain click replaces the active page; Cmd/Ctrl click opens a background tab; Cmd/Ctrl+Shift click opens a foreground tab",
      "aliases": [
        "M-036"
      ]
    }
  ]
---

A plain link click navigates the current tab, Cmd/Ctrl click opens a background tab and Cmd/Ctrl+Shift click opens a foreground tab.

## Notes

Blitz `DocumentEvent::NavigateIframe` carries only `node_id` and `url`; modifier flags are dropped before reaching the host.
