Feature: devtools-shell DevTools panel shell
  As a user
  I want to open a docked developer tools panel in the browser window
  So that I can inspect console, DOM and network activity for the active page

  # Source / basis: product decision (browser developer tools UX); Chrome DevTools dock model
  # Registry: docs/features/devtools-shell.md
  # Tests: devtools::tests::*, chrome::tests::classifies_devtools_commands,
  #        chrome::tests::closed_devtools_leaves_stage_without_open_class,
  #        chrome::tests::open_bottom_dock_sets_height_with_ui_var,
  #        chrome::tests::open_right_dock_sets_width_with_ui_var,
  #        chrome::tests::devtools_tab_switch_updates_active_pane,
  #        keyboard::tests::{readme_shortcut_table,dispatch_devtools_esc_closes_panel,devtools_toggle_restores_page_input_focus},
  #        profile::tests::{settings_round_trip,devtools_settings_default_on_legacy_file},
  #        manual devtools-shell#1..devtools-shell#3

  Background:
    Given a browser window with one open tab

  Scenario: F12 toggles the developer tools panel
    When the user presses F12
    Then the developer tools panel opens docked at the bottom
    When the user presses F12 again
    Then the developer tools panel is closed

  Scenario: Meta+Alt+I and Ctrl+Shift+I toggle the panel
    When the user presses Meta+Alt+I
    Then the developer tools panel opens
  # Automated coverage: keyboard::tests::readme_shortcut_table (Ctrl+Shift+I chord)

  Scenario: A closed panel leaves the page iframe filling the stage
    Given the developer tools panel is closed
    When the chrome is rendered
    Then the page iframe occupies the full stage

  Scenario: Bottom dock sets panel height with UI counter-scale
    Given the developer tools panel is open on the bottom
    When the panel size is 240px
    Then the panel height uses calc(240px * var(--ui))

  Scenario: Right dock sets panel width with UI counter-scale
    Given the developer tools panel is open on the right
    When the panel size is 200px
    Then the panel width uses calc(200px * var(--ui))

  Scenario: Panel size clamps between 120px and 70% of the stage
    When the user resizes the panel beyond the stage limit
    Then the stored size stays within the clamp range

  Scenario: Dock side and size persist across restarts while the panel starts closed
    Given the dock is right and the size is 360px
    When the user restarts Glide
    Then the panel is still closed
    And the saved dock and size are restored when the panel opens

  Scenario: glide://devtools commands open, dock and switch tabs
    When the user navigates to glide://devtools-tab?tab=elements
    Then the Elements tab is active

  Scenario: Escape with focus inside the panel closes it
    Given the developer tools panel is open and focused
    When the user presses Escape
    Then the developer tools panel closes
