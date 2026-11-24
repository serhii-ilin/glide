---
slug: "devtools-console-repl"
kind: "feature"
title: "DevTools Console REPL"
status: "done"
aliases:
  [
    "F-061"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-console-repl.feature"
tests:
  [
    "`devtools::repl::tests::*`",
    "`devtools::console::tests::clear_keeps_repl_history`",
    "`devtools::tests::sync_console_pane_resets_repl_ui_on_tab_change`",
    "`js::repl_eval::tests::*`",
    "`js::integration::{repl_eval_returns_formatted_result,repl_eval_throws_error_row,repl_eval_respects_watchdog,repl_eval_preview_symbol_then_followup_addition,repl_eval_preview_accessor_without_calling_then_followup,repl_eval_preview_infinite_accessor_without_hang_then_followup,repl_eval_preview_proxy_without_traps_then_followup,repl_eval_global_var_visible_on_next_eval}`",
    "`chrome::tests::devtools_console_input_exists_when_console_tab_active`",
    "`keyboard::tests::{dispatch_devtools_console_submit_and_history,dispatch_devtools_console_ignores_enter_while_ime_composing}`",
    "manual devtools-console-repl#1"
  ]
mcp: "planned mcp-console-eval (`glide_eval`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/counter.html`, press F12, focus the Console input at the bottom. Type `document.title` and press Enter; press Up to recall the line; type `throw 1` and Enter; click **Clear** and press Up again. In a second tab, submit a different expression, switch tabs, and confirm Up recalls only that tab's history",
      "expected": "Input echoes with `>`; results show with `<`; errors appear as error rows; **Clear** removes output but Up still recalls history; per-tab history does not leak across tabs; Tab/arrow shortcuts do not scroll the page while the input is focused",
      "aliases": [
        "M-045"
      ]
    }
  ]
---

The Console tab includes an input line that evaluates JavaScript in the active page V8 context, echoes `>` input and `<` results or errors in the per-tab journal, keeps per-tab input history (100 entries) across Clear, and reuses the five-second script watchdog.

## Notes

History is in-memory only (not session-persisted).
