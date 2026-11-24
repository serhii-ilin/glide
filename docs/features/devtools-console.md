---
slug: "devtools-console"
kind: "feature"
title: "DevTools Console output"
status: "done"
aliases:
  [
    "F-060"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-console.feature"
tests:
  [
    "`devtools::console::tests::*`",
    "`js::console_feed::tests::*`",
    "`js::integration::{console_log_reaches_console_feed,console_log_includes_line_number_from_stack,uncaught_exception_reaches_console_feed,unhandled_rejection_reaches_console_feed,handled_rejection_revokes_console_row_same_turn,handled_rejection_revokes_after_settimeout_catch,two_unhandled_rejections_revoking_one_leaves_one_row}`",
    "`tabs::tests::{selecting_active_tab_is_a_noop,next_tab_on_single_tab_is_a_noop}`",
    "`headless::tests::headless_load_drains_console_queue`",
    "`chrome::tests::classifies_devtools_commands`",
    "manual devtools-console#1"
  ]
mcp: "planned mcp-console-eval (`glide_console_messages`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/counter.html`, press F12, click Console if needed. In the page, use a bookmarklet or reload after adding a script that calls `console.log('hello')`, `console.warn('warn')`, and `throw new Error('x')`",
      "expected": "Console rows show level labels (LOG/WARN/EXCEPTION) with timestamps and message text; **Clear** empties the list; switching to another tab and back shows that tab’s own buffer",
      "aliases": [
        "M-044"
      ]
    }
  ]
---

The Console tab shows page `console` messages, uncaught exceptions and unhandled rejections with level styling, a timestamp, source when known, and Clear per active tab (500-entry ring buffer). Structured `js.console` / `js.uncaught` logging is unchanged.

## Notes

Per-tab buffer is in-memory only (not session-persisted).
