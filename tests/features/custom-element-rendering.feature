Feature: custom-element-rendering Server-rendered custom-element fallbacks
  As a page author
  I want my server-rendered markup shown even though the browser does not upgrade custom elements
  So that sites whose shell is hidden until upgrade, or whose content streams in after the shell, are not blank

  # Source / basis: HTML Living Standard (custom elements, :defined, template element);
  #                CSS Selectors Level 4 (:defined / :not(:defined)); product decision:
  #                unupgraded custom elements render as plain elements
  # Registry: docs/features/custom-element-rendering.md
  # Tests: custom_elements::tests::unupgraded_custom_element_stays_visible,
  #        custom_elements::tests::custom_element_names_skip_standard_elements,
  #        custom_elements::tests::streamed_template_replaces_placeholder,
  #        custom_elements::tests::unmatched_placeholder_is_left_alone,
  #        custom_elements::tests::empty_template_keeps_placeholder
  # MCP: n/a (engine behavior; observable through glide_dom, glide_snapshot_text and glide_screenshot)

  Background:
    Given a page that hides its custom-element shell with :not(:defined)
    And the page has not defined the elements because module scripts do not run

  Scenario: Unupgraded custom elements and their descendants stay visible
    Given a page rule :not(:defined) { visibility: hidden }
    And a custom element my-widget containing a span
    When the document is laid out
    Then my-widget and the span inside it are visible

  Scenario: The fallback does not change standard elements
    Given a page with a div and a my-widget element
    When the document is parsed
    Then only my-widget is treated as a custom element name

  Scenario: Streamed template content replaces its suspense placeholder
    Given a suspense-placeholder with id s_1 and a template for="s_1" containing an article
    When the document is parsed
    Then the placeholder is gone
    And the article's text is in the document

  Scenario: A placeholder without a matching template is left alone
    Given a suspense-placeholder with id s_2 and a template for="s_1"
    Then the s_2 placeholder keeps its fallback text
    And the s_1 template content is not shown

  Scenario: An empty template does not remove the placeholder
    Given a suspense-placeholder with id s_1 and an empty template for="s_1"
    Then the placeholder keeps its fallback text
