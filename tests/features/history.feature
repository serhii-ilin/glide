Feature: history Back, forward and reload with redirect-aware history
  As a user
  I want back, forward and reload to follow the pages I visited
  So that I can navigate without retyping URLs

  # Source / basis: HTML Living Standard - navigation and history traversal;
  #                RFC 9110 - redirect handling
  # Registry: docs/features/history.md
  # Tests: history::tests::*, nav::tests::*, http::tests::*

  Scenario: Back at the first entry does not change the page
    Given a browsing session with one page loaded
    When I go back
    Then the current page stays the same

  Scenario: Following a link pushes a new history entry
    Given a local page with a link to another local fixture
    When I follow the link
    Then the linked page is current
    And going back returns to the original page

  Scenario: A redirect replaces the redirected entry
    Given a local fixture that redirects to another local fixture
    When the redirect completes
    Then the final URL is current
    And going back skips the redirected entry

  Scenario: Reload keeps the current history entry
    Given a loaded page with a back entry
    When I reload the page
    Then the same URL remains current
    And the back entry still returns to the previous page
