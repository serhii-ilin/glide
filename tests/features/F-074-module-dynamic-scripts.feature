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
  #        headless_js::parser_async_script_runs_before_load_event,
  #        headless_js::external_module_script_runs,
  #        headless_js::module_import_resolves_relative_dependency,
  #        headless_js::module_import_missing_dependency_leaves_page_intact,
  #        headless_js::module_graph_handles_import_cycle,
  #        headless_js::dynamic_import_resolves_module,
  #        headless_js::import_meta_url_matches_module_resource,
  #        headless_js::bare_module_specifier_is_rejected,
  #        headless_js::http_module_import_resolves,
  #        headless_js::dynamically_inserted_script_runs,
  #        headless_js::dynamically_inserted_external_script_runs_and_fires_load,
  #        headless_js::inner_html_scripts_do_not_run,
  #        diagnostics::dynamically_inserted_script_runs,
  #        diagnostics::dynamic_script_fetch_failure_is_reported,
  #        diagnostics::module_script_runs,
  #        diagnostics::module_import_missing_dependency_is_logged,
  #        diagnostics::bare_module_specifier_is_reported_as_unsupported,
  #        js::modules::tests::*,
  #        js::apply::tests::parse_data_url_percent_decodes_plain_payload
  # Manual: M-055
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

  Scenario: A module with imports resolves and runs
    Given a module that imports a relative file URL
    When the page loads
    Then the imported binding is available and the module body runs

  Scenario: A missing module dependency stops the graph
    Given a module that imports a missing file URL
    When module instantiation asks the loader for the dependency
    Then the module body does not run
    And a js.loader module fetch failed record is logged

  Scenario: Bare specifiers are rejected
    Given a module that imports a bare specifier
    When the loader resolves the import
    Then the module body does not run
    And a module-import js.unsupported record is logged

  Scenario: Dynamic import() returns a promise
    Given a module that calls import() for a relative dependency
    When the promise settles
    Then the dependency namespace is available

  Scenario: import.meta.url reflects the module URL
    Given a module script loaded from file
    When the module evaluates
    Then import.meta.url matches the module resource URL

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

  Scenario: Classic scripts with nomodule are skipped
    Given a nomodule classic script and a module script on the same page
    When the page loads
    Then only the module script runs

  Scenario: Dynamic import from a classic script resolves modules
    Given a classic script that calls import() for a relative dependency
    When the page loads and pumps
    Then the imported binding is available

  Scenario: Two module tags with the same src evaluate once
    Given two script elements with the same module src
    When the page loads
    Then the module initializer runs one time

  Scenario: Dynamic import is fulfilled after timer pumps
    Given a module that schedules a timer then calls import()
    When the page pumps timers
    Then statements after import() run before the dependency side effect

  Scenario: Parser-inserted async classic scripts run before the load event
  # Source / basis: HTML Living Standard — async parser scripts delay the load event
    Given a page with inline classic, DOMContentLoaded, load listeners and an async script
    When the page finishes loading
    Then scripts run in order classic, dom, async, load
