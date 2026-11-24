Feature: form-control-mirror Form control value read-back
  As a page script author
  I want input.value and input.checked to reflect what the user typed in the real control
  So that forms and tests see the same state Blitz uses for submission

  # Source / basis: HTML Living Standard (form controls, value/sanitization);
  #                product decision: domino mirrors Blitz on getter reads and before submit
  # Registry: docs/features/form-control-mirror.md
  # Tests: js::form_controls::tests::control_field_reads_value_attribute,
  #        headless_js::headless_post_form_navigation_submits_urlencoded_body
  # MCP: n/a (observable via glide_eval on value/checked after typing in the window)

  Background:
    Given a page with a text input the user can type into

  Scenario: input.value returns the Blitz text after typing
    When the user types "hello" into the field without mutating the domino snapshot
    Then a script reading input.value sees "hello"

  Scenario: Controls sync before form.requestSubmit
    Given a form whose submit handler reads input.value
    When the user typed into the input and the page calls requestSubmit
    Then the submit handler sees the typed value
    And the navigation uses the same value in the entry list