---
slug: "mcp-page-input"
kind: "feature"
title: "MCP page input"
status: "planned"
aliases:
  [
    "F-068"
  ]
requirements:
  [
    "agent-drive-headless-glide-session"
  ]
gherkin: "—"
tests: []
mcp: "`glide_click`, `glide_type`"
manual: []
---

An agent can click an element by CSS selector or coordinates and type text (optionally pressing Enter), with page scripts and link navigation reacting as for a user.

## Notes

Hidden or unmatched elements return a tool error.
