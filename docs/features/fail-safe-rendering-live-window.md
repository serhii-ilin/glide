---
slug: "fail-safe-rendering-live-window"
kind: "feature"
title: "Fail-safe rendering in the live window"
status: "planned"
aliases:
  [
    "F-088"
  ]
requirements:
  [
    "pages-scripts-leave-rendered-page"
  ]
gherkin: "planned (spec added when promoted)"
tests: []
mcp: "n/a (window UI; MCP live-window parity follows remote-control-window)"
manual: []
---

The same fallback in the live window: if a tab's scripts leave it blank after they settle, the tab reloads its server-rendered content with scripting off for that document only, shows a small notice, and the JS toggle can retry.

## Notes

Window tabs render pages in an iframe of the chrome document and share one global scripting flag, so this needs a per-document scripting override and a settle hook in `src/app.rs`; needs a manual check row when built.
