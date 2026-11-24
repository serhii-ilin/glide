Feature: structured-diagnostics Structured diagnostics
  As a developer
  I want browser and page-script failures logged with context
  So that I can identify the error and its root cause from a terminal log

  # Source / basis: product observability spec; compatibility gaps tracked in missing-web-apis-dom-bindings/mutationobserver
  # Registry: docs/features/structured-diagnostics.md
  # Tests: logging::tests::*, js::diagnostics::tests::*,
  #        diagnostics::uncaught_script_error_is_logged_with_context,
  #        diagnostics::glide_log_warn_suppresses_informational_console_messages,
  #        diagnostics::dynamically_inserted_script_runs,
  #        diagnostics::dynamic_script_fetch_failure_is_reported,
  #        diagnostics::module_script_runs,
  #        diagnostics::module_import_missing_dependency_is_logged,
  #        diagnostics::bare_module_specifier_is_reported_as_unsupported,
  #        compat::subprocess::tests::first_uncaught_from_structured_log, manual structured-diagnostics#1

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

  Scenario: A dynamic script that cannot be loaded leaves a record
    Given a page script inserts a script element whose source is missing
    When the fetch fails
    Then a js.unsupported warning names feature "dynamic-script" and the source
    And the element's error handler runs
    And the per-page summary reports status "degraded"

  Scenario: An ES module with dependencies leaves a record
    Given a page whose only script is a module with an import
    When the module graph cannot be resolved
    Then a js.unsupported warning names feature "module-import"
    And the module body does not run

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
