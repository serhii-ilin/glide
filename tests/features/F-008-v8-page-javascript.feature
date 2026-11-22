Feature: F-008 V8 page JavaScript lifecycle
  As a page author
  I want timers and scripts to behave predictably across navigation and idle time
  So that pages stay responsive without stale work corrupting the next document

  # Source / basis: ECMA-262 (timers); DOM Standard; HTML Living Standard (script execution)
  # Registry: docs/features.md -> F-008
  # Tests: js::host::tests::{stale_dom_rpc_is_rejected,current_dom_rpc_is_applied,page_with_timer_reports_next_wake,page_without_timers_reports_no_wake}, app::tests::pump_wait_*, manual M-049, M-050
  # MCP: planned F-067 (glide_eval)

  Background:
    Given page JavaScript is enabled
    And a local fixture under tests/fixtures/

  Scenario: timers fire while the window is idle
  # Covered by manual M-049 and js::host::tests::page_with_timer_reports_next_wake
    Given the page tests/fixtures/timers.html is loaded in a window
    When the user does not interact with the window for at least two seconds
    Then the tick counter in the page advances without keyboard or pointer input
    And the delayed timeout line shows that the timeout fired

  Scenario: navigating away stops the previous page's scripts
  # Covered by manual M-050 and js::host::tests::stale_dom_rpc_is_rejected
    Given a page with active interval and timeout timers is loaded
    When the browser navigates to another URL
    Then the previous page's worker is retired
    And its uncaught errors and console output are not published to DevTools or stderr

  Scenario: stale mutations from the previous page are not applied
  # Covered by js::host::tests::stale_dom_rpc_is_rejected
    Given a page has started its JavaScript worker
    When a DOM RPC from the previous generation arrives after navigation
    Then the host replies with an error and does not mutate the current document