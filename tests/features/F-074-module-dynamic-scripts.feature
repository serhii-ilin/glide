Feature: F-074 Module and dynamic script execution
  As a page author
  I want module scripts and scripts my code inserts to run
  So that app bundles, polyfill loaders and chunk loaders work instead of being skipped

  # Source / basis: HTML Living Standard (script processing, module scripts,
  #                "prepare a script", script load/error events); ECMAScript (modules)
  # Registry: docs/features.md -> F-074
  # Tests: js::scripts::tests::module_scripts_are_deferred_or_async,
  #        headless_js::inline_module_script_runs,
  #        headless_js::module_runs_between_parser_scripts_and_dom_content_loaded,
  #        headless_js::external_module_script_runs,
  #        headless_js::module_import_is_left_to_the_loader,
  #        headless_js::dynamically_inserted_script_runs,
  #        headless_js::dynamically_inserted_external_script_runs_and_fires_load,
  #        headless_js::inner_html_scripts_do_not_run,
  #        diagnostics::dynamically_inserted_script_runs,
  #        diagnostics::dynamic_script_fetch_failure_is_reported,
  #        diagnostics::module_script_runs,
  #        diagnostics::module_import_is_reported_as_unsupported,
  #        js::apply::tests::parse_data_url_percent_decodes_plain_payload
  # MCP: n/a (engine behavior; observable through glide_eval, glide_dom and glide_snapshot_text)

  Background:
    Given a local page that runs page JavaScript

  Scenario: An inline module runs after parser scripts
    Given a page with an inline script and an inline module
    When the page loads
    Then the inline script runs first
    And the module sets the page text before DOMContentLoaded

  Scenario: An external module runs
    Given a module script whose src is a data URL
    When the page loads
    Then the module source runs

  Scenario: A module with imports does not run
    Given a module with an import statement
    When module instantiation asks the loader for the dependency
    Then the module body does not run
    And a module-import record is logged

  Scenario: A script inserted by a script runs once
    Given a parser script that appends a script element
    When the inserting task finishes
    Then the inserted script's code runs
    And it runs only once

  Scenario: An external dynamic script runs and fires load
    Given a script element inserted with a data URL source and an onload handler
    When the loader fetches and executes it
    Then the script source runs
    And the onload handler runs after execution

  Scenario: Scripts parsed from innerHTML do not run
    Given a script element containing script markup set through innerHTML
    When the host element is appended
    Then the parsed script is not executed
