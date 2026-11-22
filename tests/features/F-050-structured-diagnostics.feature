Feature: F-050 Structured diagnostics
  As a developer
  I want browser and page-script failures logged with context
  So that I can identify the error and its root cause from a terminal log

  # Source / basis: product observability spec; compatibility gaps tracked in F-032/F-038
  # Registry: docs/features.md -> F-050
  # Tests: logging::tests::*, js::diagnostics::tests::*,
  #        diagnostics::uncaught_script_error_is_logged_with_context,
  #        diagnostics::glide_log_warn_suppresses_informational_console_messages,
  #        diagnostics::dynamically_inserted_script_is_reported_as_unsupported,
  #        diagnostics::module_script_is_reported_as_skipped,
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

  Scenario: An unimplemented page feature leaves a record
    Given a page script inserts another script element
    When the inserted script is mirrored but not executed
    Then a js.unsupported warning carries the page URL, feature and detail
    And the per-page summary reports status "degraded"

  Scenario: A skipped script kind leaves a record
    Given a page whose only script has type "module"
    When the loader skips it
    Then a js.unsupported warning names feature "module-script"

  Scenario: Silent feature detection is visible on demand
    Given a page probes Web APIs Glide does not provide
    When Glide runs with GLIDE_LOG=debug
    Then one js.caps record lists the missing capabilities for that page

  Scenario: A blank page after scripts leaves a record
    Given a page whose scripts run without errors but render no visible text
    When the load settles
    Then a js.page warning says the page rendered no visible text
    And the hint points at js.caps and GLIDE_LOG=debug

  Scenario: Log verbosity is configurable
    Given GLIDE_LOG is set to "warn"
    When an informational diagnostic is emitted
    Then no informational log line is printed
    And warnings and errors are still printed
