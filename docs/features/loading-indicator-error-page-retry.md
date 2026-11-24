---
slug: "loading-indicator-error-page-retry"
kind: "feature"
title: "T4 Loading indicator, error page with retry, stop button"
status: "done"
aliases:
  [
    "F-023"
  ]
requirements:
  [
    "user-see-page-loading-failed"
  ]
gherkin: "backfill pending"
tests:
  [
    "`net::tests::*`",
    "`nav::tests::*`",
    "`app::tests::initial_iframe_load_uses_glide_net_error_page`",
    "`chrome::tests::*`",
    "`keyboard::tests::*`",
    "manual loading-indicator-error-page-retry#1–loading-indicator-error-page-retry#3"
  ]
mcp: "planned mcp-headless-session (`glide_page_info` loading/status)"
manual:
  [
    {
      "steps": "Open `https://nonexistent.invalid`",
      "expected": "Error page shows the URL, a reason, and **Retry**; **Retry** requests the same URL again (no `glide://` navigation)",
      "aliases": [
        "M-019"
      ]
    },
    {
      "steps": "Open a slow `https://` page (or throttle locally)",
      "expected": "Blue loading bar and **Stop** show only during the main document load; both clear when the load finishes",
      "aliases": [
        "M-020"
      ]
    },
    {
      "steps": "With a slow load in progress, press Esc or click Stop (■)",
      "expected": "Load stops; previous page stays visible; history and address bar match the displayed page",
      "aliases": [
        "M-021"
      ]
    }
  ]
---

A loading bar and stop control show in-flight navigation, and failed loads show a retryable error page.

## Notes

Retry is the failed URL; `NavTracker` fixes stop/history
