Feature: F-047 Per-tab navigation state and multi-tab session restore
  As a user
  I want each tab to remember where I was, my find query and my zoom level
  So that restarting the browser puts every tab back the way I left it

  # Source / basis: HTML Living Standard (history traversal and browsing contexts);
  #                DOM Standard (one Document per browsing context); product session spec
  # Registry: docs/features.md -> F-047
  # Tests: tabs::tests::per_tab_state_is_independent,
  #        tabs::tests::session_round_trip_restores_find_query,
  #        profile::tests::session_v2_round_trip,
  #        profile::tests::legacy_session_migrates_to_one_tab,
  #        profile::tests::corrupt_session_falls_back, manual M-035

  Background:
    Given a profile directory for this run

  Scenario: Each tab owns its own navigation state
    Given two tabs are open
    When I navigate the first tab, find text and zoom in
    Then the second tab still has its original address, empty find query and default zoom
    And the first tab keeps its own history, address, find query and zoom

  Scenario: All tabs and their state survive a restart
    Given two tabs are open with different addresses, titles, find queries and zoom levels
    When I save the session and start a new browser with that profile
    Then both tabs are open with their original addresses, titles, find queries and zoom levels
    And the same tab is active

  Scenario: An older session opens as one tab
    Given a session file in the old entries-and-index format
    When the browser starts with that profile
    Then one tab is open at the saved entry
    And its saved position is restored

  Scenario: A corrupt session does not prevent startup
    Given a session file containing invalid data
    When the browser starts with that profile
    Then the browser starts with a fresh empty session
