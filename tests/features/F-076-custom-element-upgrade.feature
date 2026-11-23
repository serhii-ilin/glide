Feature: F-076 Custom element upgrade and :defined
  As a page author
  I want custom elements to upgrade with lifecycle callbacks and a real :defined pseudo-class
  So that web component shells and widgets behave like in a full browser

  # Source / basis: HTML Living Standard (custom elements, reaction queue, :defined);
  #                CSS Selectors Level 4 (:defined / :not(:defined))
  # Registry: docs/features.md -> F-076
  # Tests: js::integration::{custom_element_upgrade_runs_lifecycle_callbacks,custom_element_is_customized_builtin_is_rejected},
  #        headless_js::custom_element_upgrade_fixture,
  #        custom_elements::tests::built_in_elements_match_defined_before_upgrade_hammer
  # MCP: n/a (engine behavior; observable via glide_eval, glide_dom and glide_snapshot_text)

  Background:
    Given local file fixtures without network access

  Scenario: connectedCallback runs when a definition upgrades an existing element
    Given a script that defines x-pick and upgrades markup already in the document
    When the page settles
    Then the element's text reflects connectedCallback

  Scenario: attributeChangedCallback runs for observed attributes
    Given a script that defines a custom element with observedAttributes
    When the script sets an observed attribute after define
    Then attributeChangedCallback ran before the task ends

  Scenario: disconnectedCallback runs when an upgraded element is removed
    Given an upgraded custom element appended then removed in one script task
    When the page settles
    Then disconnectedCallback was recorded

  Scenario: Customized built-ins with is= are rejected
    When a script calls customElements.define with an extends option
    Then a NotSupportedError is thrown and the feature is logged once

  Scenario: Built-in elements match :defined while autonomous custom tags do not until upgrade
    Given an HTML document with my-widget and a span
    When the document is parsed in Rust
    Then html and span carry the DEFINED state bit and my-widget does not