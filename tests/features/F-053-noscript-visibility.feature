Feature: F-053 Noscript visibility follows the scripting mode
  As a user
  I want `<noscript>` fallback markup hidden when page scripts are enabled
  So that fallback meta tags and messages do not appear as page text

  # Source / basis: HTML Living Standard (the noscript element); CSS Display;
  #                Blitz default UA stylesheet `@media (scripting)`
  # Registry: docs/features.md -> F-053
  # Tests: parse::tests::noscript_is_hidden_when_scripting_is_enabled,
  #        parse::tests::noscript_is_visible_when_scripting_is_disabled, manual M-039

  Background:
    Given a page contains a noscript element with fallback markup

  Scenario: Fallback is hidden when scripting is enabled
    When the page is parsed with scripting enabled
    Then the noscript element has computed display "none"
    And the rest of the page remains visible

  Scenario: Fallback is visible when scripting is disabled
    When the page is parsed with scripting disabled
    Then the noscript element is not display "none"
