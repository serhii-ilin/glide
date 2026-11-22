Feature: F-060 DevTools Console output
  As a page author
  I want console messages and page errors in the DevTools Console tab
  So that I can debug scripts without leaving the browser window

  # Source / basis: Chrome DevTools Console UX; product decision (R-029)
  # Registry: docs/features.md -> F-060
  # Tests: devtools::console::tests::*, js::console_feed::tests::*, js::integration::console_*,
  #        js::integration::uncaught_* , js::integration::unhandled_*,
  #        chrome::tests::classifies_devtools_commands, manual M-044

  Background:
  Given JavaScript is enabled for the active tab

  Scenario: Page console calls appear in the per-tab journal
    Given a local page whose script calls console.log and console.warn
    When the page scripts run
    Then the active tab console journal records both messages with distinct levels

  Scenario: Uncaught exceptions and unhandled rejections are logged
    Given a page that throws or returns a rejected promise without a handler
    When the error surfaces
    Then the console journal records an exception or rejection entry

  Scenario: The Console tab lists messages with level and Clear
    Given DevTools is open on the Console tab
    When messages are present in the journal
    Then each row shows a timestamp, level label and message text
    And Clear removes all rows for the active tab only