Feature: F-054 Safari-style toolbar, address field and tab row
  As a user
  I want a Safari-like toolbar with a centered address field and clean icon buttons
  So that the browser chrome is familiar, quiet and easy to use

  # Source / basis: Apple Human Interface Guidelines (toolbars, tab bars, search fields,
  #                menus, SF system font and SF Symbols line icons); Safari for macOS
  #                (light appearance) as the visual benchmark; HTML Living Standard
  #                (form and input semantics)
  # Registry: docs/features.md -> F-054
  # Tests: chrome::tests::toolbar_has_navigation_address_and_actions,
  #        chrome::tests::address_field_shows_the_page_title,
  #        chrome::tests::toolbar_uses_borderless_icon_buttons,
  #        chrome::tests::chrome_uses_the_system_font_stack,
  #        chrome::tests::navigation_buttons_use_svg_icons,
  #        chrome::tests::stop_state_uses_an_svg_icon,
  #        chrome::tests::bookmark_star_uses_svg_icons,
  #        chrome::tests::reload_sits_inside_the_address_field,
  #        chrome::tests::address_field_is_borderless_with_a_focus_ring,
  #        chrome::tests::tabs_are_equal_width_segments,
  #        chrome::tests::find_bar_has_chevrons_and_done,
  #        chrome::tests::chrome_palette_is_neutral,
  #        chrome::tests::devtools_console_input_fits_the_panel,
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

  # Apple HIG: SF is the system font on macOS; system-ui resolves to it.
  Scenario: The chrome uses the system font
    When the chrome is rendered
    Then the chrome font stack starts with the system UI font
    And it falls back to a symbol font so shortcut glyphs like ⇧ and ⇥ render

  # Apple HIG: toolbar items use one consistent family of line icons.
  Scenario Outline: Toolbar actions use SVG line icons
    When the chrome is rendered in the "<state>" state
    Then the "<control>" button shows an inline SVG line icon
    And it shows no text glyph

    Examples:
      | state      | control  |
      | idle       | back     |
      | idle       | forward  |
      | idle       | reload   |
      | loading    | stop     |
      | idle       | bookmark |
      | bookmarked | bookmark |

  # Safari for macOS places reload at the trailing edge of the address field.
  Scenario: Reload sits inside the address field
    When the chrome is rendered
    Then the reload button is inside the address field form
    And it is the last control in that field

  # Apple HIG search fields: filled, borderless, with a focus ring when editing.
  Scenario: The address field is a borderless filled field with a focus ring
    When the chrome is rendered
    Then the address field has no border and a subtle fill
    And it shows a leading magnifier icon
    When the address field is focused
    Then it turns white with an accent focus ring and shows the URL left-aligned

  # Safari "separate" tab bar: equal-width flat segments, close at the leading edge.
  Scenario: Tabs are equal-width flat segments with a leading close control
    Given three tabs are open
    When the chrome is rendered
    Then every tab takes an equal share of the tab row
    And tabs have no rounded tops or gaps between them
    And each tab's close control comes before its title
    And the new-tab button stays a fixed-width icon at the end of the row

  # Safari find banner: search field, match count, chevrons and a Done button.
  Scenario: The find bar looks like Safari's find banner
    When the find bar is open
    Then it shows a filled search field with a magnifier
    And previous and next matches are borderless chevron icon buttons
    And the close control is a borderless "Done" button

  # Safari light appearance: neutral greys and white, no warm tones.
  Scenario: The chrome palette is neutral
    When the chrome is rendered
    Then no warm beige or tan colour from the old palette remains
    And the colours come from one set of shared design tokens

  Scenario: The DevTools console input fits inside the panel
    Given DevTools is open on the Console tab
    When the chrome is rendered
    Then the console input is sized with its border box
    And it never overflows the right edge of the panel

  Scenario: The window always shows at least one tab
    Given a session with no tabs
    When the window opens
    Then the tab row shows one tab
