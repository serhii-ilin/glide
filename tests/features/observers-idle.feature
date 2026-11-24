Feature: observers-idle Observers, idle scheduling and feature-detect shims
  As a page author
  I want mutation, intersection and resize observers, idle callbacks, and CSS and custom-element feature detection
  So that modern app shells render instead of taking silent fallback paths

  # Source / basis: DOM Standard (MutationObserver); Intersection Observer Standard;
  #                Resize Observer Standard; HTML Living Standard (requestIdleCallback,
  #                custom elements); CSSOM (CSS.supports / CSS.escape)
  # Registry: docs/features/observers-idle.md
  # Tests: js::integration::mutation_observer_reports_child_and_attribute_changes,
  #        js::integration::mutation_observer_honors_attribute_filter_and_character_data,
  #        js::integration::mutation_observer_accepts_text_and_comment_targets,
  #        js::integration::intersection_observer_fires_for_visible_and_hidden_elements,
  #        js::integration::resize_observer_reports_initial_size_once,
  #        js::integration::idle_callback_runs_with_a_deadline,
  #        js::integration::css_supports_escape_and_custom_elements_registry,
  #        headless_js::observers_and_idle_work

  Background:
    Given a local page that runs page JavaScript

  Scenario: MutationObserver reports DOM changes
    When a script observes the document body for child and attribute mutations
    And it appends an element and sets an attribute
    Then the observer receives childList and attributes records
    And takeRecords drains pending batches and disconnect stops further delivery

  Scenario: MutationObserver honors filters and character data
    When a script observes with an attribute filter and characterData
    Then only matching attribute mutations are reported
    And text node data changes produce characterData records

  Scenario: MutationObserver accepts Text and Comment targets
  # Source / basis: DOM Standard — observe(target) accepts any Node including CharacterData
    When a script observes a Text node and a Comment node with characterData
    And it changes each node's data
    Then takeRecords returns exactly one characterData record whose target is the observed node
    And observing a non-Node still throws TypeError

  Scenario: MutationObserver reports detached subtree character data
  # Source / basis: DOM Standard — observing a node observes its subtree regardless of
  #                whether the tree is connected to a document
    When a script observes a detached element and a cloned detached element with
         characterData and subtree
    And it changes the data of a text node already in each subtree
    Then each observer receives exactly one characterData record for its text node

  Scenario: IntersectionObserver reports viewport intersection
    When a script observes visible and display:none elements
    Then the visible element reports isIntersecting true
    And the hidden element reports isIntersecting false

  Scenario: ResizeObserver reports element size
    When a script observes a sized element
    Then the callback runs once with content rect dimensions

  Scenario: requestIdleCallback exposes a deadline
    When a script schedules requestIdleCallback
    Then the callback runs with a positive timeRemaining budget

  Scenario: CSS.supports, CSS.escape and customElements registry
    When a script queries CSS.supports and CSS.escape
    And registers a custom element name
    Then known CSS properties return true and unknown properties false
    And duplicate definitions throw and get/whenDefined work
