# One file per feature ID in docs/features.md.
# Keep the Feature line, Source / basis comment and Registry comment in sync with the row.
Feature: F-067 MCP console and JavaScript evaluation
  As an agent driving Glide over MCP
  I want to read the page console and evaluate JavaScript in the page
  So that I can inspect and drive a page without a human at the window

  # Source / basis: ECMA-262 (eval, PerformEval, %JSON.stringify%),
  #                WHATWG Console Standard (log levels), Chrome DevTools console UX,
  #                Model Context Protocol specification (tools)
  # Registry: docs/features.md -> F-067
  # Tests: js::repl_eval::tests::json_eval_source_wraps_with_stringify,
  #        js::integration::{repl_eval_reply_receives_result,repl_eval_reply_receives_error,
  #        repl_eval_reply_on_watchdog_timeout,repl_eval_respects_watchdog,
  #        throwing_eval_does_not_leave_watchdog_armed,
  #        compile_error_eval_does_not_leave_watchdog_armed,
  #        throwing_eval_still_mirrors_dom_mutations,
  #        watchdog_timeout_in_microtasks_still_flushes_dom,
  #        repl_eval_microtask_console_log_precedes_result,
  #        stale_route_eval_reply_is_error,eval_blocking_returns_value,
  #        eval_blocking_dom_mutation_is_mirrored,eval_blocking_with_js_disabled_is_error,
  #        json_eval_returns_json_text},
  #        devtools::console::tests::{since_returns_entries_after_cursor,since_respects_limit,
  #        since_reports_dropped_after_eviction,since_foreign_cursor_past_end_starts_at_oldest,
  #        since_stable_after_revoke_rejection}
  # MCP: glide_console_messages, glide_eval (server tools pending F-066/F-067;
  #      MCP stdio scenario pending tests/mcp_stdio.rs)

  Background:
    Given JavaScript is enabled and a page has loaded in the active tab

  Scenario: Eval returns the formatted value of an expression
    When the agent evaluates `1 + 1`
    Then the reply is the DevTools-style preview `2`

  Scenario: Eval with JSON output returns JSON text
    When the agent evaluates `({ a: 1 })` asking for JSON output
    Then the reply is the JSON text `{"a":1}`

  Scenario: A throwing expression returns its error
    When the agent evaluates `throw new Error('boom')`
    Then the reply is an error describing `boom`

  Scenario: An infinite loop is stopped by the watchdog and the page keeps working
    When the agent evaluates `while (true) {}`
    Then the reply is a timeout error after the REPL watchdog
    And a later eval `1 + 1` still returns `2`

  Scenario: Eval with JavaScript disabled is an error
    Given page JavaScript is disabled
    When the agent evaluates `1 + 1`
    Then the reply is an error saying JavaScript is disabled

  Scenario: An eval that changes the DOM is visible to the next read
    When the agent evaluates a snippet that appends `<p id="added">mirrored</p>`
    Then reading the page DOM finds the new paragraph with text `mirrored`

  Scenario: Console messages since a cursor return only newer entries
    Given the journal holds `a`, `b` and `c`
    When the agent asks for console messages since the cursor that follows `a`
    Then only `b` and `c` are returned
    And the next cursor follows `c`

  Scenario: Evicted console entries are reported as dropped
    Given more than 500 console entries were pushed
    And the agent's cursor predates the oldest retained entry
    When the agent asks for console messages since that cursor
    Then the reply reports how many entries were evicted before being read

  # Pending tests/mcp_stdio.rs: the MCP server lands with F-066/F-067.
  @mcp-stdio
  Scenario: glide_eval and glide_console_messages over MCP
    Given `glide --mcp` serves a headless page over stdio
    When the agent calls `glide_eval` with `console.log('hi'); 6 * 7`
    And the agent calls `glide_console_messages` since cursor 0
    Then `glide_eval` returns `42` and the console row `hi`
    And `glide_console_messages` returns that row with `dropped` and `next_cursor`
