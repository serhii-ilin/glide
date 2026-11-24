---
slug: "remote-control-window"
kind: "feature"
title: "Remote control of a live window"
status: "in-progress"
aliases:
  [
    "F-069"
  ]
requirements:
  [
    "running-glide-window-controlled-agent"
  ]
gherkin: "tests/features/remote-control-window.feature"
tests:
  [
    "`remote_socket::tests::*`"
  ]
mcp: "all mcp-headless-session–mcp-page-input tools via `glide --mcp --attach [socket]`"
manual: []
---

`glide --remote-control` lets `glide --mcp --attach` drive the open window through a user-only local socket; the toolbar shows a Remote badge and every command is logged.

## Notes

Unix socket in `<profile>/run/` (dir 0700, socket 0600); opt-in per launch, never a saved setting. Socket transport (`src/remote_socket.rs`) done; window wiring and attach pending. Requests are capped at 1 MiB, responses at 64 MiB; the client keeps one deadline per call and discards late replies; `bind` refuses non-socket paths, symlinked run directories and overlong paths. Deferred: a lock file serialising two simultaneous `bind`s (the dev/ino guard already prevents removing the other server's socket).
