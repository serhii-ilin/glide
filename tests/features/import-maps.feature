# Source / basis: HTML Living Standard (import maps, module script resolution); ECMA-262 (modules)
# Registry: docs/features/import-maps.md
# Tests: js::import_map::tests::*, js::scripts::tests::collects_import_map_scripts_in_document_order,
#        headless_js::import_map_*, diagnostics::invalid_import_map_logs_loader_error,
#        diagnostics::unmapped_bare_specifier_with_import_map_logs_hint
# MCP: n/a (engine behavior; observable through glide_eval and glide_snapshot_text)

Feature: import-maps Import maps
  As a page author using ES modules
  I want import maps to remap bare specifiers
  So that bundled apps can load without bare-import failures

  Background:
    Given a local file fixture under tests/fixtures/

  Scenario: Inline import map resolves a static bare import
    Given importmap-basic.html defines a map for app-dep
    When the module script runs
    Then visible text contains mapped 88

  Scenario: Invalid import map is ignored
  while a later valid map still applies
    Given importmap-invalid.html has broken JSON then a valid map
    When the page settles
    Then visible text contains after 88
    And a js.loader import map registration failed record is logged

  Scenario: Bare specifier with no mapping is rejected
    Given importmap-bare-unmapped.html maps only other specifiers
    When the module script imports missing-pkg
    Then visible text stays before
    And a js.unsupported record mentions no import map entry

  Scenario: HTMLScriptElement.supports reports importmap
    Given importmap-supports.html runs a classic probe script
    When HTMLScriptElement.supports is called
    Then visible text contains supports-ok