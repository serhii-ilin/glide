Feature: F-044 Tab strip UI
  As a user
  I want to see my open tabs in a strip above the page
  So that I can tell which page is active and switch or close tabs with a click

  # Source / basis: Apple Human Interface Guidelines (tabs); Safari for macOS (separate
  #                tab bar) as the visual benchmark; HTML Living Standard
  #                (browsing contexts - each tab is a top-level browsing context)
  # Registry: docs/features.md -> F-044
  # Tests: chrome::tests::tab_strip_renders_tabs_and_controls,
  #        chrome::tests::active_tab_is_marked_in_tab_strip,
  #        chrome::tests::tab_close_buttons_carry_tab_ids,
#        chrome::tests::tabs_are_equal_width_segments,
  #        app::tests::titles_changed_detects_a_page_title_update, manual M-032

  Background:
    Given a browser window with the tab strip enabled

  Scenario: The strip shows one tab for each open page
    Given two tabs are open with titles "First" and "Second"
    When the chrome is rendered
    Then the tab strip contains a tab element for each open tab
    And each tab shows its page title, or a placeholder while it has none
    And the strip contains a new-tab button

  Scenario: A tab title appears after the page loads
    Given one tab has no title yet
    When its page reports a title
    Then the tab strip shows that title

  Scenario: The active tab is visually distinct
    Given the second of two tabs is active
    When the chrome is rendered
    Then exactly one tab carries the active state
    And that tab is the second tab

  Scenario: Each tab has a close control
    Given three tabs are open
    When the chrome is rendered
    Then every tab has a close control carrying its own tab id
    And the close control sits at the leading edge of its tab
    And clicking a close control requests that tab to close

  Scenario: The new-tab button creates a tab
    Given one tab is open
    When I click the new-tab button
    Then a new tab is created and becomes active
