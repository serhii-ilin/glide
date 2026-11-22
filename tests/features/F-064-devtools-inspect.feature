Feature: F-064 DevTools click-to-inspect
  As a page author
  I want to pick a page element with the inspect tool
  So that the Elements tree selects what I clicked without activating the page

  # Source / basis: Chrome DevTools element picker; product decision
  # Registry: docs/features.md -> F-064
  # Tests: devtools::inspect::tests::*,
  #        devtools::tests::*,
  #        chrome::tests::*,
  #        keyboard::tests::*,
  #        manual M-049
  # MCP: n/a (UI mode; agents select via glide_query / glide_computed_style)

  Background:
    Given DevTools is open on the Elements tab and inspect mode is available

  Scenario: Toggle inspect mode from the toolbar or shortcut
  # Automated: devtools::inspect::tests::toggle_sets_button_pressed_state,
  #            keyboard::tests::readme_shortcut_table (Cmd/Ctrl+Shift+C)

  Scenario: Hover highlights the element under the pointer
  # Automated: devtools::inspect::tests::hit_test_maps_nested_element,
  #            devtools::inspect::tests::overlay_uses_layout_mapping

  Scenario: Click selects the node and exits inspect mode without navigating
  # Automated: devtools::inspect::tests::pick_does_not_follow_link,
  #            devtools::inspect::tests::pointer_gate_cases

  Scenario: Inspect mode exits on Esc, tab change, and navigation
  # Automated: devtools::inspect::tests::exit_clears_overlay,
  #            devtools::inspect::tests::navigation_exits_inspect_mode_on_commit_effects,
  #            keyboard::tests::inspect_mode_escape_exits_inspect_not_devtools,
  #            devtools::tests::inspect_exits_when_devtools_tab_changes
