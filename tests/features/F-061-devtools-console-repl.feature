Feature: F-061 DevTools Console REPL
  As a page author
  I want to evaluate JavaScript in the Console input line
  So that I can inspect values and run snippets in the page context

  # Source / basis: Chrome DevTools Console command line; product decision (R-029)
  # Registry: docs/features.md -> F-061
  # Tests: devtools::repl::tests::*, devtools::console::tests::clear_keeps_repl_history,
  #        js::repl_eval::tests::*, js::integration::{repl_eval_returns_formatted_result,repl_eval_throws_error_row,repl_eval_respects_watchdog},
  #        keyboard::tests::dispatch_devtools_console_submit_and_history,
  #        chrome::tests::devtools_console_input_exists_when_console_tab_active, manual M-045

  Background:
    Given JavaScript is enabled and a page has loaded in the active tab

  Scenario: Submitting an expression echoes input and shows the result
    Given the DevTools Console input contains `1 + 1`
    When the user submits the input
    Then the console journal lists a row beginning with `> 1 + 1`
    And the console journal lists a row beginning with `< 2`

  Scenario: Thrown errors appear as error rows
    When the user evaluates `throw new Error('repl-fail')`
    Then an error row describes the thrown error

  Scenario: Clear removes output but not input history
    Given prior expressions were submitted on this tab
    When the user activates Clear
    Then the journal is empty
    And pressing Up in the input recalls the prior expression

  Scenario: Input history is scoped per browser tab
    Given expressions were submitted on tab A
    When the user switches to tab B
    Then Up in the input does not show tab A history
