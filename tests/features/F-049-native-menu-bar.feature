Feature: F-049 Native macOS menu bar
  As a macOS user
  I want browser commands in the system menu bar
  So that Glide behaves like other Mac apps

  # Source / basis: Apple Human Interface Guidelines (The menu bar; Keyboard shortcuts);
  #                Safari for macOS menus as structural benchmark
  # Registry: docs/features.md -> F-049
  # Tests: menu::tests, chrome::tests and keyboard::tests on every OS; manual M-056 on macOS
  # MCP: n/a (chrome UI; same commands reachable via glide_* navigation tools)

  Background:
    Given a pure menu model built from chrome view state

  # Apple HIG: the menu bar lists the app menu first and Help last
  # Test: menu::tests::mac_native_menu_titles_and_order
  Scenario: macOS menu titles and order
    When the model is built for the macOS native menu bar
    Then the menus are Glide, File, Edit, View, History, Bookmarks, Develop, Window, Help

  # Test: menu::tests::in_window_menu_titles_match_f045
  Scenario: Other platforms keep the in-window menu titles
    When the model is built for the in-window menu bar
    Then the menus are File, Edit, View, History, Bookmarks, Tabs, Help

  # Apple HIG: shortcuts are shown in the order Control, Option, Shift, Command
  # Test: menu::tests::shortcut_display_macos_glyph_order, menu::tests::shortcut_display_covers_brackets_and_control
  Scenario Outline: Shortcuts use Apple glyph order
    Given a shortcut "<shortcut>"
    Then it is displayed as "<glyphs>"

    Examples:
      | shortcut      | glyphs |
      | Shift+Cmd+W   | ⇧⌘W    |
      | Alt+Cmd+I     | ⌥⌘I    |
      | Cmd+[         | ⌘[     |
      | Ctrl+Cmd+F    | ⌃⌘F    |

  # Test: menu::tests::history_items_enabled_state
  Scenario: Back and Forward follow history availability
    Given the active tab cannot go back or forward
    Then Back and Forward are disabled
    When the tab can go back and forward
    Then Back and Forward are enabled

  # Test: menu::tests::close_tab_stays_enabled_with_one_tab_on_mac_native, menu::tests::window_menu_next_prev_tab_need_two_tabs
  Scenario: Tab commands need more than one tab
    Given only one tab is open
    Then Show Previous Tab and Show Next Tab are disabled
    And Close Tab stays enabled so AppKit still delivers Cmd+W, which closes the window on the last tab

  # Test: menu::tests::find_next_prev_enabled_only_with_find_bar_open
  Scenario: Find Next and Previous need the find bar
    Given the find bar is closed
    Then Find Next and Find Previous are disabled and Cmd+G is not captured
    When the find bar is open
    Then Find Next and Find Previous are enabled

  # Test: menu::tests::select_tab_command_routes_to_tab_selection
  Scenario: A Window menu tab item selects that tab
    When the item for a tab is chosen
    Then the SelectTab command round-trips through its id and maps to that tab's selection

  # Test: keyboard::tests::ctrl_alt_b_and_ctrl_m_yield_to_text_input_off_macos
  Scenario: Non-macOS chords yield to text input
    Given a platform other than macOS and a focused text input
    Then Ctrl+Alt+B and Ctrl+M do not trigger Show Bookmarks or Minimize

  # Test: menu::tests::native_menu_warning_uses_schema_fields
  Scenario: Native menu failures are logged in the diagnostics schema
    When the native menu cannot be built or updated
    Then a menu.native warning carries feature, error_kind, detail and hint fields

  # Test: menu::tests::stop_enabled_while_loading
  Scenario: Stop is enabled only while loading
    Given the page is idle
    Then Stop is disabled
    When the page is loading
    Then Stop is enabled

  # Test: menu::tests::bookmark_label_toggles
  Scenario: Bookmark label follows the bookmarked state
    Given the current page is bookmarked
    Then the item reads "Remove Bookmark"

  # Test: menu::tests::disable_js_checkmark
  Scenario: Disable JavaScript shows a checkmark when JavaScript is off
    Given page JavaScript is off
    Then the Disable JavaScript item is checked

  # Test: menu::tests::develop_menu_label_follows_devtools_state
  Scenario: Web Inspector label follows DevTools state
    Given DevTools is open
    Then the item reads "Hide Web Inspector"

  # Test: menu::tests::window_menu_tab_list
  Scenario: Window menu lists tabs with the active one checked
    Given two tabs are open and the second is active
    Then the Window menu has one item per tab and only the second is checked

  # Test: menu::tests::undo_and_redo_stay_disabled
  Scenario: Undo and Redo are disabled
    Then Undo and Redo are disabled because text editing has no undo stack

  # Test: chrome::tests::in_window_menu_bar_omitted_when_disabled, chrome::tests::in_window_dropdown_renders_from_menu_model
  Scenario: In-window menu bar is absent on macOS and present elsewhere
    Given in_window_menu is false
    Then the chrome document has no menu-bar element
    When in_window_menu is true
    Then the menu-bar dropdowns are rendered from the in-window model

  # Test: menu::tests::every_model_command_id_round_trips, menu::tests::native_menu_command_ids_are_unique, menu::tests::unknown_menu_id_is_rejected
  Scenario: Every menu command id round-trips
    Then every command in the model maps to a unique id that parses back to the same command

  # Test: menu::tests::menu_shortcuts_match_keyboard
  Scenario: Menu shortcuts agree with keyboard classification
    Then each item's shortcut maps through keyboard::browser_shortcut to the same command

  # Test: keyboard::tests::menu_edit_copy_matches_shortcut, keyboard::tests::menu_edit_paste_inserts_into_address_bar, keyboard::tests::menu_edit_copy_in_page_input_matches_shortcut
  Scenario: Edit menu commands act on the focused field like the keys
    Given the address bar holds text and has focus
    When Edit > Copy is chosen
    Then the clipboard holds the address text
    And Edit > Paste inserts clipboard text into the address bar
    And with a page input focused, Edit > Copy copies the page input text

  # Test: keyboard::tests::menu_edit_without_editable_focus_is_not_handled_and_falls_back_to_a_key_press
  Scenario: Edit chords with no editable focus fall back to the key press
    Given nothing editable is focused
    When Edit > Copy is chosen
    Then the edit action reports unhandled and the chord is replayed as a key press for Blitz's default handling

  # Test: menu::tests::app_menu_quit_uses_the_quit_shortcut_path; manual M-056 for the session
  Scenario: Quit saves the session
    When Glide > Quit Glide is chosen
    Then the same code runs as for the Quit shortcut and the session is saved
