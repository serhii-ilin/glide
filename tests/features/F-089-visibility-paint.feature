Feature: F-089 Paint visible descendants of visibility:hidden ancestors
  As a person browsing a page that hides its shell with visibility until it is ready
  I want elements that set visibility:visible to be drawn
  So that pages such as Reddit are not painted blank

  # Source / basis: CSS Display Module / CSS 2.1 section 11.2 (visibility: descendants may
  #                set visibility:visible and are then drawn); Reddit's body is hidden.
  # Registry: docs/features.md -> F-089
  # Tests: shot::tests::visible_child_of_hidden_body_is_painted,
  #        shot::tests::visibility_hidden_element_is_not_painted
  # MCP: n/a (engine painting; observable via glide_screenshot)

  Background:
    Given tests/fixtures/visibility-child-of-hidden.html is laid out

  Scenario: A visible child of a hidden body is drawn
    When the page is painted
    Then the child with visibility:visible shows its red background

  Scenario: A hidden element itself is not drawn
    When the page is painted
    Then the sibling that inherits visibility:hidden does not show its blue background
