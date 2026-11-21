Feature: F-054 Safari-style toolbar, address field and tab row
  As a user
  I want a Safari-like toolbar with a centered address field and clean icon buttons
  So that the browser chrome is familiar and easy to use

  # Source / basis: Apple Human Interface Guidelines (toolbars, tab bars, search fields);
  #                HTML Living Standard (form and input semantics)
  # Registry: docs/features.md -> F-054
  # Tests: chrome::tests::toolbar_has_navigation_address_and_actions,
  #        chrome::tests::address_field_shows_the_page_title,
  #        chrome::tests::toolbar_uses_borderless_icon_buttons,
  #        tabs::tests::ensure_one_tab_opens_a_default_tab, manual M-040

  Background:
    Given a browser window with one open tab

  Scenario: The toolbar is laid out like a browser toolbar
    When the chrome is rendered
    Then the toolbar has a leading group with navigation buttons
    And it has a centered address field
    And it has a trailing group with the bookmark and JavaScript controls

  Scenario: The address field shows the page title
    Given the active page is titled "Example Domain"
    When the chrome is rendered
    Then the address field shows "Example Domain"
    And the address field keeps the page URL as its editable value

  Scenario: Toolbar buttons are borderless icons
    When the chrome is rendered
    Then navigation and action buttons use the borderless icon style
    And they are grouped on the left and right of the address field

  Scenario: The window always shows at least one tab
    Given a session with no tabs
    When the window opens
    Then the tab row shows one tab
