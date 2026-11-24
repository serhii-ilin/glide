---
slug: "per-tab-session"
kind: "feature"
title: "Per-tab navigation state and multi-tab session restore"
status: "done"
aliases:
  [
    "F-047"
  ]
requirements:
  [
    "tab-keeps-independent-history-address"
  ]
gherkin: "tests/features/per-tab-session.feature"
tests:
  [
    "`tabs::tests::per_tab_state_is_independent`",
    "`tabs::tests::session_round_trip_restores_find_query`",
    "`profile::tests::session_v2_round_trip`",
    "`profile::tests::legacy_session_migrates_to_one_tab`",
    "`profile::tests::corrupt_session_falls_back`",
    "manual per-tab-session#1"
  ]
mcp: "planned mcp-tabs-devtools-state-js (`glide_tabs_list`)"
manual:
  [
    {
      "steps": "Open two tabs with different pages, zoom the first to 130% and run a find query, then quit and relaunch with no URL",
      "expected": "Both tabs return with their addresses, titles and per-tab state; the same tab is active",
      "aliases": [
        "M-035"
      ]
    }
  ]
---

Every tab keeps its own history, address, title, loading, find and zoom state, and all open tabs come back after a restart.

## Notes

MVP keeps one live iframe and reloads it on tab switch; all per-tab state still persists.
