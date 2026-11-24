Feature: devtools-elements-tree DevTools Elements tree
  As a page author
  I want to inspect the page DOM in the Elements tab
  So that I can see structure, attributes, and node text safely

  # Source / basis: Chrome DevTools Elements panel; product decision (user-open-docked-developer-panel)
  # Registry: docs/features/devtools-elements-tree.md
  # Tests: devtools::elements::tests::*,
  #        devtools::tests::sync_elements_pane_renders_tree,
  #        chrome::tests::devtools_elements_pane_markup,
  #        keyboard::tests::dispatch_devtools_elements_tree_keys,
  #        manual devtools-elements-tree#1

  Background:
    Given JavaScript is enabled and a page has loaded in the active tab
    And the developer tools panel is open on the Elements tab

  Scenario: The tree lists tags with escaped markup
  # Automated: devtools::elements::tests::escape_elements_text_neutralizes_html_injection

  Scenario: Deep trees are capped with a more indicator
  # Automated: devtools::elements::tests::flatten_tree_respects_row_cap_and_more_indicator

  Scenario: Arrow keys move, expand, and collapse tree rows
  # Automated: keyboard::tests::dispatch_devtools_elements_tree_keys,
  #            keyboard::tests::dispatch_devtools_elements_keys_after_row_select_focus

  Scenario: Clicking the twisty toggles expansion without changing selection
  # Automated: devtools::elements::tests::toggle_expanded_via_state_matches_toggle_command_behavior,
  #            chrome::tests::classifies_devtools_commands (toggle URL)

  Scenario: The tree updates when text or attributes change without new nodes
  # Automated: devtools::elements::tests::prepare_elements_render_reflects_text_and_attribute_edits_without_node_count_change,
  #            devtools::elements::tests::tree_row_escapes_text_and_attributes_once

  Scenario: Selecting a node shows its attributes in the detail pane
  # Automated: devtools::tests::sync_elements_pane_renders_tree

  Scenario: Each tab keeps its own tree UI state
  # Manual: devtools-elements-tree#1 step for per-tab isolation
