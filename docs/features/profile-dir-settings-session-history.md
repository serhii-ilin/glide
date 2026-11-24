---
slug: "profile-dir-settings-session-history"
kind: "feature"
title: "T6 Profile dir, settings, session, history file, bookmarks, cookies, Web Storage"
status: "done"
aliases:
  [
    "F-025"
  ]
requirements:
  [
    "document-address-reflected-url-attributes",
    "settings-session-history-bookmarks-survive"
  ]
gherkin: "backfill pending"
tests:
  [
    "`profile::tests::*`",
    "`history::tests::from_parts_restores_position`",
    "`args::tests::*`",
    "`keyboard::tests::readme_shortcut_table`",
    "`net::tests::{provider_with_cookies_feature_builds,glide_bookmarks_page_delivers_html}`",
    "`cookies::tests::persist_round_trips_through_json`",
    "`web_storage::tests::persist_round_trips_through_json`",
    "manual profile-dir-settings-session-history#1–profile-dir-settings-session-history#4"
  ]
mcp: "n/a (headless MCP never opens the profile)"
manual:
  [
    {
      "steps": "Visit two `https://` pages, quit (Cmd/Ctrl+Q), relaunch with no URL argument",
      "expected": "Same page as at quit; back returns to the previous page",
      "aliases": [
        "M-026"
      ]
    },
    {
      "steps": "Toggle JS off (toolbar or Cmd/Ctrl+Shift+J), quit, relaunch with no URL",
      "expected": "JS stays off until toggled again",
      "aliases": [
        "M-027"
      ]
    },
    {
      "steps": "Bookmark a page (star or Cmd/Ctrl+D). In the address bar open `glide://bookmarks` and click a link",
      "expected": "Bookmarks page lists the entry; the link navigates the page iframe",
      "aliases": [
        "M-028"
      ]
    },
    {
      "steps": "Log into a cookie-based site, navigate to another page on that host, confirm still logged in; quit Glide and relaunch with the same profile",
      "expected": "Session cookies survive for the process; persistent cookies reload from `cookies.json` when the profile dir is available",
      "aliases": [
        "M-029"
      ],
      "covers": [
        "document-cookie"
      ]
    }
  ]
---

A profile directory persists settings, the last session, history, bookmarks, the cookie jar (`cookies.json`), and tuple-origin `localStorage` (`local_storage.json`).

## Notes

document-cookie owns jar semantics; web-storage owns Web Storage; profile under platform data dir or `GLIDE_PROFILE_DIR`
