Feature: F-049 Native macOS menu bar
  As a macOS user
  I want browser commands in the system menu bar
  So that Glide behaves like other Mac apps

  # Source / basis: Apple Human Interface Guidelines (The menu bar; Keyboard shortcuts);
  #                Safari for macOS as structural benchmark
  # Registry: docs/features.md -> F-049
  # Tests: menu::tests on all platforms; manual M-056 on macOS
  # MCP: n/a (chrome UI; same commands reachable via glide_* navigation tools)

  Background:
    Given a pure menu model built from chrome view state

  Scenario: macOS native menu titles and order
  # Unit-tested in menu::tests::mac_native_menu_titles_and_order

  Scenario: Menu item shortcuts use Apple glyph order
  # Unit-tested in menu::tests::shortcut_display_macos

  Scenario: History items reflect back and forward availability
  # Unit-tested in menu::tests::history_items_enabled_state

  Scenario: Close tab disabled with one tab
  # Unit-tested in menu::tests::close_tab_disabled_single_tab

  Scenario: Stop is enabled only while loading
  # Unit-tested in menu::tests::stop_enabled_while_loading

  Scenario: Bookmark label follows bookmarked state
  # Unit-tested in menu::tests::bookmark_label_toggles

  Scenario: Disable JavaScript shows checkmark when JS is off
  # Unit-tested in menu::tests::disable_js_checkmark

  Scenario: Window menu lists tabs with active checkmark
  # Unit-tested in menu::tests::window_menu_tab_list

  Scenario: In-window menu bar absent on macOS and present elsewhere
  # Unit-tested in chrome::tests::in_window_menu_bar_omitted_when_disabled

  Scenario: Every menu command id round-trips
  # Unit-tested in menu::tests::menu_command_id_round_trip

  Scenario: Menu shortcuts agree with keyboard classification
  # Unit-tested in menu::tests::menu_shortcuts_match_keyboard

  Scenario: Routing Copy with address bar focused copies text
  # Unit-tested in keyboard::tests::menu_edit_copy_matches_shortcut

  Scenario: Quit saves the session
  # Manual M-056; keyboard::tests::quit_path saves session covered by existing quit tests