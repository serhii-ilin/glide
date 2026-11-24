Feature: wheel-scroll-target Wheel scroll targets the inner pane under the pointer
  As a user reading a page with nested scroll areas
  I want the mouse wheel to scroll the pane under the pointer
  So that sidebars, drawers and lists scroll like in other browsers

  # Source / basis: CSSOM View (scrolling boxes); UI Events (wheel); product decision (Reddit inner panes)
  # Registry: docs/features/wheel-scroll-target.md
  # Tests: js::host::tests::{page_subdoc_hover_is_none_*,wheel_scrolls_inner_pane_after_hover_refresh},
  # app::tests::{wheel_should_refresh_page_hover_when_subdoc_hover_missing,
  # wheel_should_not_refresh_page_hover_when_subdoc_hover_set,
  # wheel_should_not_refresh_page_hover_while_button_held,
  # wheel_should_not_refresh_page_hover_for_non_wheel_events},
  # manual wheel-scroll-target#1
  # MCP: n/a (window input; headless MCP tools cannot dispatch a wheel)

  Background:
    Given a local page with a fixed-height overflow:auto pane and tall page content

  Scenario: Wheel over an inner pane scrolls the pane, not the page
    Given the pointer is over the inner pane
    When the user scrolls the mouse wheel down over the pane
    Then the pane's scroll offset increases
    And the page viewport scroll stays unchanged while the pane can still absorb the wheel

  Scenario: Wheel over an inner pane right after navigation without moving the pointer
    Given the user clicks the in-pane reload link and the page navigates
    And the pointer stays over the pane
    When the user scrolls the mouse wheel over the pane
    Then the pane's scroll offset increases
    And the page behind the pane does not jump first

  Scenario: A wheel at the pane end chains scroll to the page
    Given the inner pane is scrolled to its bottom edge
    When the user scrolls the mouse wheel down again over the pane
    Then remaining wheel delta scrolls the page viewport

  Scenario: Wheel over a pane when hover is already valid still scrolls the pane
    Given the pointer already moved over the inner pane
    When the user scrolls the mouse wheel down over the pane
    Then the pane's scroll offset increases
    And the page behind the pane does not jump first
