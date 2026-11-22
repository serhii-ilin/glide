Feature: F-008 V8 page JavaScript lifecycle
  As a page author
  I want timers and scripts to behave predictably across navigation and idle time
  So that pages stay responsive without stale work corrupting the next document

  # Source / basis: HTML Living Standard (timers, script execution); DOM Standard
  # Registry: docs/features.md -> F-008
  # Tests: js::host::tests::{stale_dom_rpc_is_rejected,current_dom_rpc_is_applied,maybe_start_without_navigation_bumps_generation,page_with_timer_reports_next_wake,page_without_timers_reports_no_wake,settimeout_zero_reports_immediate_wake,settimeout_negative_delay_reports_immediate_wake,worker_wake_on_dom_rpc_without_host_pump,earlier_timer_shortens_next_wake_and_wakes_the_host}, js::integration::timers_fixture_reports_wake_and_advances_with_pump, app::tests::pump_wait_*, manual M-049, M-050
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
  # Covered by manual M-050, js::host::tests::stale_dom_rpc_is_rejected, and js::host::tests::maybe_start_without_navigation_bumps_generation
    Given a page with active interval and timeout timers is loaded
    When the browser navigates to another URL (address bar, link click, or page-initiated location.assign)
    Then the previous page's worker is retired
    And its uncaught errors and console output are not published to DevTools or stderr

  Scenario: stale mutations from the previous page are not applied
  # Covered by js::host::tests::stale_dom_rpc_is_rejected and js::host::tests::maybe_start_without_navigation_bumps_generation
    Given a page has started its JavaScript worker
    When a DOM RPC from the previous generation arrives after navigation
    Then the host replies with an error and does not mutate the current document

  Scenario: a settled page arms no wake
  # Covered by js::host::tests::page_without_timers_reports_no_wake
    Given a page with no timers or animation frames has finished loading
    Then the host reports no next wake deadline

  Scenario: a later timer wakes the host with an earlier deadline
  # Covered by js::host::tests::earlier_timer_shortens_next_wake
    Given a page already has a long-pending timer
    When a script schedules a shorter timer
    Then the host wake deadline moves earlier

  Scenario: asynchronous worker DOM work reaches the window without input
  # Covered by js::host::tests::worker_wake_on_dom_rpc_without_host_pump
    Given an active page worker
    When page script queues a DOM mutation on the host channel
    Then the host is woken to drain and apply the mutation without pointer or keyboard input
