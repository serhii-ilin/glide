---
slug: "cookies-persisted-disk"
kind: "feature"
title: "Cookies persisted to disk"
status: "done"
aliases:
  [
    "F-026"
  ]
requirements:
  [
    "settings-session-history-bookmarks-survive"
  ]
gherkin: "backfill pending"
tests:
  [
    "`cookies::tests::persist_round_trips_through_json`",
    "manual profile-dir-settings-session-history#4"
  ]
mcp: "n/a (headless MCP never opens the profile)"
manual: []
---

Cookies survive a browser restart.

## Notes

Implemented as `cookies.json` in document-cookie/profile-dir-settings-session-history; headless runs without profile use an in-memory jar only
