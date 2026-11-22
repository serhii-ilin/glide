Feature: F-062 DevTools Elements tree
  As a page author
  I want to inspect the page DOM in the Elements tab
  So that I can see structure, attributes, and node text safely

  # Source / basis: Chrome DevTools Elements panel; product decision (R-029)
  # Registry: docs/features.md -> F-062
  # Tests: devtools::elements::tests::*,
  #        devtools::tests::sync_elements_pane_renders_tree,
  #        chrome::tests::devtools_elements_pane_markup,
  #        keyboard::tests::dispatch_devtools_elements_tree_keys,
  #        manual M-046

  Background:
    Given JavaScript is enabled and a page has loaded in the active tab
    And the developer tools panel is open on the Elements tab

  Scenario: The tree lists tags with escaped markup
  # Automated: devtools::elements::tests::escape_elements_text_neutralizes_html_injection

  Scenario: Deep trees are capped with a more indicator
  # Automated: devtools::elements::tests::flatten_tree_respects_row_cap_and_more_indicator

  Scenario: Arrow keys move, expand, and collapse tree rows
  # Automated: keyboard::tests::dispatch_devtools_elements_tree_keys

  Scenario: Selecting a node shows its attributes in the detail pane
  # Automated: devtools::tests::sync_elements_pane_renders_tree

  Scenario: Each tab keeps its own tree UI state
  # Manual: M-046 step for per-tab isolation
