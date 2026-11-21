Feature: F-050 Structured diagnostics
  As a developer
  I want browser and page-script failures logged with context
  So that I can identify the error and its root cause from a terminal log

  # Source / basis: product observability spec; compatibility gaps tracked in F-032/F-038
  # Registry: docs/features.md -> F-050
  # Tests: logging::tests::*, js::diagnostics::tests::*,
  #        diagnostics::uncaught_script_error_is_logged_with_context,
  #        diagnostics::glide_log_warn_suppresses_informational_console_messages,
  #        compat::subprocess::tests::first_uncaught_from_structured_log, manual M-037

  Background:
    Given Glide is started from a terminal

  Scenario: Script exceptions include page and script context
    Given a local page with a script that throws
    When the script runs
    Then an error log line includes the page URL, script id, line number and message
    And the log line is formatted as structured key=value fields

  Scenario Outline: Known compatibility gaps include a root-cause hint
    Given a page script raises "<message>"
    When the exception is logged
    Then the log includes error kind "<kind>"
    And the log includes a hint for the root cause

    Examples:
      | message | kind |
      | ReferenceError: atob is not defined | missing_api |
      | ReferenceError: MutationObserver is not defined | missing_api |
      | TypeError: window.matchMedia is not a function | missing_api |
      | TypeError: console.info is not a function | missing_api |
      | TypeError: Cannot redefine property: innerHTML | incompatible_descriptor |
      | TypeError: Invalid URL | invalid_url |

  Scenario: Log verbosity is configurable
    Given GLIDE_LOG is set to "warn"
    When an informational diagnostic is emitted
    Then no informational log line is printed
    And warnings and errors are still printed
