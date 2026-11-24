---
slug: "mcp-console-eval"
kind: "feature"
title: "MCP console and JavaScript evaluation"
status: "in-progress"
aliases:
  [
    "F-067"
  ]
requirements:
  [
    "agent-drive-headless-glide-session"
  ]
gherkin: "tests/features/mcp-console-eval.feature"
tests:
  [
    "`js::repl_eval::tests::json_eval_source_wraps_with_stringify`",
    "`js::integration::{repl_eval_reply_receives_result,repl_eval_reply_receives_error,repl_eval_reply_on_watchdog_timeout,repl_eval_respects_watchdog,throwing_eval_does_not_leave_watchdog_armed,compile_error_eval_does_not_leave_watchdog_armed,throwing_eval_still_mirrors_dom_mutations,watchdog_timeout_in_microtasks_still_flushes_dom,repl_eval_microtask_console_log_precedes_result,stale_route_eval_reply_is_error,eval_blocking_returns_value,eval_blocking_dom_mutation_is_mirrored,eval_blocking_with_js_disabled_is_error,json_eval_returns_json_text}`",
    "`devtools::console::tests::{since_returns_entries_after_cursor,since_respects_limit,since_reports_dropped_after_eviction,since_foreign_cursor_past_end_starts_at_oldest,since_stable_after_revoke_rejection}`"
  ]
mcp: "`glide_console_messages`, `glide_eval`"
manual: []
---

An agent can read the page console (logs, warnings, errors, uncaught exceptions) since a cursor and evaluate JavaScript in the page, getting the value or the error back.

## Notes

Eval reply channel and console cursor done; MCP tools pending. The MCP layer must apply `max_chars` to eval results. Deferred: (1) a caller that gives up on `eval_blocking` with nothing pumping afterwards can leave a late flush RPC queued past its 30 s timeout (the headless session pumps every 30 ms, so it is not exposed); (2) microtasks queued before an eval that itself hits the watchdog are drained without a bound.
