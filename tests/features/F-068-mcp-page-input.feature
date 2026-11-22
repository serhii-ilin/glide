Feature: F-068 MCP page input
  As an agent driving Glide through the control layer
  I want to click elements and type text into a page
  So that page scripts and link navigation react as they do for a user

  # Source / basis: HTML Living Standard (activation behavior, click() and implicit form
  #                submission); CSSOM View (client rects for inline fragments); UI Events
  #                spec (pointer and keyboard events); Model Context Protocol specification
  #                (tools)
  # Registry: docs/features.md -> F-068
  # Tests: control_session::{click_selector_runs_onclick,click_coordinates_hit_the_button,click_coordinates_ignore_blank_selector,click_link_navigates,click_plain_inline_link_navigates,click_wrapped_link_navigates,click_javascript_link_runs_onclick_without_error,click_mailto_link_is_ignored,click_link_to_missing_file_reports_clicked,click_covered_element_is_error,type_text_updates_input_listener,type_submit_presses_enter,type_submit_file_form_does_not_navigate,type_into_button_is_error,type_without_focus_is_error,click_unknown_selector_is_error,click_hidden_element_is_error,click_without_target_is_error}; control::page_ops::tests::{element_center_uses_layout_box,inline_link_center_uses_fragment_box,query_reports_inline_link_box,hidden_element_has_no_center,unknown_selector_has_no_center}; control::protocol::tests::click_params_require_selector_or_point; control::session::tests::type_text_writes_into_blitz_editor
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
    Then the outcome target is the button
    And the page text includes "clicked"

  Scenario: Clicking a plain inline link navigates
    When the session clicks selector "#inline"
    Then the input outcome reports navigated
    And the page title is "Page B"

  Scenario: Clicking a wrapped inline link uses its first line box
    When the session clicks selector "#wrapped"
    Then the input outcome reports navigated
    And the page title is "Page B"

  Scenario: Clicking a link ignores a javascript: destination but still runs onclick
    When the session clicks selector "#js-link"
    Then the page text includes "js clicked"
    And the outcome is not navigated and not an error

  Scenario: Clicking a mailto: link is ignored
    When the session clicks selector "#mailto-link"
    Then the outcome is not navigated and not an error

  Scenario: Clicking a covered element is an error
    When the session clicks selector "#covered"
    Then the control layer returns an error that the element is covered

  Scenario: A failed link navigation reports that the click happened
    When the session clicks selector "#missing-link"
    Then the control layer returns an error starting "clicked; navigation to"

  Scenario: Typing into an input updates the page
    When the session types "Alice" into selector "#name"
    Then the echo paragraph shows the input event reached page JavaScript
    And the outcome is not navigated

  Scenario: Typing with submit submits a data: form with the typed value
    When the session types "query" into selector "#q" with submit
    Then the input outcome reports navigated
    And the page title is "Submitted"
    And the page URL contains "q=query"

  Scenario: A file:// form action does not navigate
    When the session types "query" into selector "#fileq" with submit
    Then the input outcome is not navigated
    And the keydown event reached the page

  Scenario: Typing into a non-text target is an error
    When the session types into selector "#btn"
    Then the control layer returns an error that the element is not a text field

  Scenario: Typing without a focused text field is an error
    When the session types without a selector and no focused text field
    Then the control layer returns an error asking for a selector

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
