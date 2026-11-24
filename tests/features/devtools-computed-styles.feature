Feature: devtools-computed-styles DevTools computed styles and highlight
  As a page author
  I want computed styles and a page highlight for the selected Elements node
  So that I can inspect layout and CSS as in browser devtools

  # Source / basis: Chrome DevTools Elements computed pane; CSSOM computed values (Stylo)
  # Registry: docs/features/devtools-computed-styles.md
  # Tests: devtools::computed::tests::*,
  #        devtools::highlight::tests::*,
  #        devtools::tests::sync_elements_pane_includes_computed_styles,
  #        chrome::tests::devtools_elements_computed_pane_markup,
  #        control::page_ops::tests::computed_style_map_for_selector,
  #        tests/mcp_stdio.rs (glide_computed_style),
  #        manual devtools-computed-styles#1
  # MCP: glide_computed_style

  Background:
    Given JavaScript is enabled and a page has loaded in the active tab
    And the developer tools panel is open on the Elements tab

  Scenario: Selecting an element shows curated computed properties
  # Automated: devtools::computed::tests::computed_rows_include_fixture_styles

  Scenario: Text and comment nodes show an empty computed state
  # Automated: devtools::computed::tests::computed_empty_state_for_non_element_nodes

  Scenario: Computed values are escaped once in the pane HTML
  # Automated: devtools::computed::tests::computed_html_escapes_values_once

  Scenario: The page shows a highlight overlay for the selected element
  # Automated: devtools::highlight::tests::overlay_scroll_offset_moves_top_up

  Scenario: Highlight hides when selection clears or DevTools closes
  # Automated: devtools::highlight::tests::overlay_hidden_when_inactive

  Scenario: An agent can read computed styles by selector via MCP
  # Automated: glide_computed_style in tests/mcp_stdio.rs
