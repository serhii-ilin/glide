Feature: F-068 MCP page input
  As an agent driving Glide through the control layer
  I want to click elements and type text into a page
  So that page scripts and link navigation react as they do for a user

  # Source / basis: HTML Living Standard (activation behavior, click() and implicit form
  #                submission); UI Events spec (pointer and keyboard events); Model Context
  #                Protocol specification (tools)
  # Registry: docs/features.md -> F-068
  # Tests: control_session::{click_selector_runs_onclick,click_coordinates_hit_the_button,click_link_navigates,type_text_updates_input_listener,type_submit_presses_enter,click_unknown_selector_is_error,click_hidden_element_is_error,click_without_target_is_error}; control::page_ops::tests::{element_center_uses_layout_box,hidden_element_has_no_center,unknown_selector_has_no_center}; control::protocol::tests::click_params_require_selector_or_point
  # MCP: glide_click, glide_type

  Background:
    Given a headless control session with JavaScript enabled
    And the session is on the MCP form fixture

  Scenario: Clicking by selector runs the element's onclick handler
    When the session clicks selector "#btn"
    Then the outcome reports a target with id "btn"
    And the page text includes "clicked"

  Scenario: Clicking by coordinates hits the element at that point
    Given the button has a layout box
    When the session clicks its centre coordinates
    Then the page text includes "clicked"

  Scenario: Clicking a link navigates and reports navigated
    When the session clicks selector "#link"
    Then the input outcome reports navigated
    And the page title is "Page B"

  Scenario: Typing into an input updates the page
    When the session types "Alice" into selector "#name"
    Then the echo paragraph shows the input listener ran
    And the outcome is not navigated

  Scenario: Typing with submit presses Enter on the focused input
    When the session types "query" into selector "#q" with submit
    Then the input receives a keydown event
    And the page either navigates to "Page B" or records that Blitz does not submit file forms

  Scenario: An unknown selector is an error
    When the session clicks selector "#nope"
    Then the control layer returns an error that no element matches

  Scenario: A hidden element is an error
    When the session clicks selector "#hidden"
    Then the control layer returns an error that the element is not visible

  Scenario: A click without a selector or a point is an error
    When the session clicks with neither a selector nor both coordinates
    Then the control layer returns an error asking for a selector or both x and y

  @mcp-stdio
  Scenario: The MCP server exposes click and type tools
    Given glide is started in MCP mode
    When the client lists tools
    Then the tool list includes glide_click and glide_type
