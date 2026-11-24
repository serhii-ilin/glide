Feature: tab-shortcuts Tab management and shortcuts
  As a user
  I want to create, close and switch tabs with the mouse and keyboard
  So that I can work in several pages without leaving the window

  # Source / basis: Safari and browser conventions; Apple Human Interface Guidelines
  #                (keyboard equivalents for menu commands); product spec
  # Registry: docs/features/tab-shortcuts.md
  # Tests: tabs::tests::*, keyboard::tests::tab_shortcut_table,
  #        chrome::tests::classifies_tab_commands, manual tab-bar#1, tab-shortcuts#1

  Background:
    Given a browser window with an empty tab manager

  Scenario: A new tab becomes active
    Given one tab is open
    When I create a new tab
    Then the tab count is two
    And the new tab is active

  Scenario: Closing the active tab selects its neighbour
    Given tabs "A", "B" and "C" are open and "B" is active
    When I close the active tab
    Then "C" is active
    And only "A" and "C" remain

  Scenario: Closing the last tab leaves no active tab
    Given one tab is open
    When I close it
    Then no tab is active

  Scenario: Next and previous wrap around
    Given tabs "A", "B" and "C" are open and "C" is active
    When I select the next tab
    Then "A" is active
    When I select the previous tab
    Then "C" is active

  Scenario: Number shortcuts select a tab by position
    Given four tabs are open and the first is active
    When I press Cmd+3 (Ctrl+3 on Linux)
    Then the third tab is active

  Scenario: Standard tab shortcuts are documented and recognised
    When I look at the shortcut table
    Then Cmd/Ctrl+T, Cmd/Ctrl+W, Cmd/Ctrl+Tab, Cmd/Ctrl+Shift+Tab,
      Cmd/Ctrl+1..9 and Cmd+Shift+]/[ map to tab actions
