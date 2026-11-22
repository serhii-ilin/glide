Feature: F-045 In-window menu bar
  As a user
  I want a menu bar inside the browser window
  So that browser commands are discoverable with the mouse on every platform

  # Source / basis: Apple Human Interface Guidelines (menu bar and menus);
  #                product decision: in-window menus first, native macOS menu deferred
  # Registry: docs/features.md -> F-045
  # Tests: chrome::tests::menu_bar_renders_all_menus,
  #        chrome::tests::menu_open_command_exposes_items,
  #        chrome::tests::menu_close_hides_dropdown,
  #        chrome::tests::menu_action_closes_dropdown,
  #        chrome::tests::toggle_open_menu_switches_or_closes,
  #        chrome::tests::pointer_on_dropdown_or_trigger_keeps_menu_open,
  #        chrome::tests::pointer_outside_dropdown_dismisses_menu,
  #        chrome::tests::history_disables_back_and_forward_when_unavailable,
  #        chrome::tests::history_enables_back_and_forward_when_available,
  #        chrome::tests::close_tab_is_disabled_with_a_single_tab,
  #        chrome::tests::tab_cycling_is_disabled_with_a_single_tab,
  #        keyboard::tests::escape_closes_menu_before_stop,
  #        manual M-034

  Background:
    Given a browser window with the menu bar enabled

  Scenario: All top-level menus are shown
    When the chrome is rendered
    Then the menu bar shows File, Edit, View, History, Bookmarks, Tabs and Help
    And no dropdown is open

  Scenario: Clicking a menu opens its dropdown
    Given no dropdown is open
    When I open the Tabs menu
    Then the Tabs dropdown lists New Tab, Next Tab and Previous Tab
    And the open dropdown is marked as active

  Scenario: Menu actions are wired to browser commands
    Given the History menu is open
    When I choose Back
    Then the active tab navigates back
    And the dropdown closes

  Scenario: Opening another menu replaces the dropdown
    Given the View menu is open
    When I open the File menu
    Then only the File dropdown is open

  Scenario: Clicking outside the dropdown closes it
    Given the File menu is open
    When I click the page
    Then no dropdown is open

  Scenario: Clicking the open menu title closes its dropdown
    Given the File menu is open
    When I click the File menu title
    Then no dropdown is open

  Scenario: Escape closes the open menu
    Given the File menu is open
    When I press Escape
    Then no dropdown is open
    And the page load is not stopped

  Scenario: Losing window focus closes the open menu
    Given the File menu is open
    When the window loses focus
    Then no dropdown is open

  Scenario: Unavailable commands are disabled, not active
    Given a single tab with no back or forward history
    When the History and File menus are rendered
    Then Back, Forward and Close Tab are disabled
    And they are not links

  Scenario: Available commands are enabled
    Given a tab with back and forward history
    When the History menu is rendered
    Then Back and Forward are links

