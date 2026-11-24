---
slug: "native-menu-bar"
kind: "feature"
title: "Native macOS menu bar"
status: "done"
aliases:
  [
    "F-049"
  ]
requirements:
  [
    "menus-expose-file-edit-view",
    "provide-safari-like-chrome-tab"
  ]
gherkin: "tests/features/native-menu-bar.feature"
tests:
  [
    "`menu::tests::mac_native_menu_titles_and_order`",
    "`menu::tests::in_window_menu_titles_match_f045`",
    "`menu::tests::shortcut_display_macos_glyph_order`",
    "`menu::tests::menu_shortcuts_match_keyboard`",
    "`menu::tests::every_model_command_id_round_trips`",
    "`menu::tests::native_menu_command_ids_are_unique`",
    "`menu::tests::history_items_enabled_state`",
    "`menu::tests::close_tab_stays_enabled_with_one_tab_on_mac_native`",
    "`menu::tests::find_next_prev_enabled_only_with_find_bar_open`",
    "`menu::tests::select_tab_command_routes_to_tab_selection`",
    "`menu::tests::native_menu_warning_uses_schema_fields`",
    "`menu::tests::stop_enabled_while_loading`",
    "`menu::tests::bookmark_label_toggles`",
    "`menu::tests::disable_js_checkmark`",
    "`menu::tests::window_menu_tab_list`",
    "`menu::tests::app_menu_quit_uses_the_quit_shortcut_path`",
    "`chrome::tests::in_window_menu_bar_omitted_when_disabled`",
    "`keyboard::tests::menu_edit_copy_matches_shortcut`",
    "`keyboard::tests::menu_edit_paste_inserts_into_address_bar`",
    "`keyboard::tests::menu_edit_copy_in_page_input_matches_shortcut`",
    "`keyboard::tests::menu_edit_without_editable_focus_is_not_handled_and_falls_back_to_a_key_press`",
    "`keyboard::tests::ctrl_alt_b_and_ctrl_m_yield_to_text_input_off_macos`",
    "`keyboard::tests::readme_shortcut_table`",
    "manual native-menu-bar#1"
  ]
mcp: "n/a (chrome UI; same commands reachable via glide_* navigation tools)"
manual:
  [
    {
      "steps": "On macOS launch Glide with `cargo run`. Open each menu (Glide, File, Edit, View, History, Bookmarks, Develop, Window, Help) and compare with the native-menu-bar spec; confirm no menu bar is drawn inside the window. Then: Cmd+T, Cmd+W (with 2+ tabs), Cmd+L; Cmd+C/Cmd+V in the address bar and in a page text input, Cmd+C with page text selected; Cmd+F, Cmd+G, Shift+Cmd+G; Alt+Cmd+I; Shift+Cmd+J; Cmd+. while a page loads; Cmd+H, Cmd+M; open several tabs and check the Window menu lists them with a check on the active tab and selecting one switches to it. Open a few tabs and quit with Cmd+Q (and via Glide > Quit Glide), then relaunch. With the find bar closed and Back disabled, press Cmd+G, Cmd+Z in a page text input and Cmd+[ (disabled items must let the chord through), and Cmd+W on the last tab; select text in the DevTools panel and press Cmd+C",
      "expected": "All nine menus appear in order with the listed items and shortcut glyphs; Back/Forward/Stop are greyed when unavailable (Close Tab stays enabled); the Bookmark item reads Add/Remove Bookmark and Develop shows a check when JavaScript is off; every shortcut acts exactly once (no double new tab, no double paste); copy/paste work in the address bar and page inputs; Quit restores the same tabs on relaunch; with the find bar closed Cmd+G does nothing, Cmd+Z undoes in the page input as before, Cmd+[ behaves as before, Cmd+W on the last tab closes the window, and Cmd+C copies the DevTools selection",
      "aliases": [
        "M-059"
      ]
    }
  ]
---

On macOS the browser's menus live in the system menu bar (Glide, File, Edit, View, History, Bookmarks, Develop, Window, Help) like Safari's, and every item runs the same action as its keyboard shortcut.

## Notes

One pure menu model (`src/menu.rs`) feeds both the native menu (`src/native_menu.rs`, macOS only, `muda`) and the in-window menu (non-macOS). AppKit consumes key equivalents before winit sees `keyDown:`, so menu events are queued, wake the event loop and run the same code as the key path: browser commands through `apply_browser_shortcut` (Quit saves the session), edit commands through `keyboard::apply_edit_action` (page iframe first, then the focused chrome field). Menu state is updated in place only when the model changes; the Window tab list is rebuilt when tabs change. Find Next/Previous are enabled only while the find bar is open (so Cmd+G is otherwise not captured). Close Tab stays enabled in the native menu because AppKit may swallow the chord of a disabled item (Cmd+W on the last tab closes the window). Edit chords that nothing editable handles are replayed as a synthetic key press so Blitz's default handling still runs. Off macOS, Ctrl+Alt+B and Ctrl+M yield to a focused text input (AltGr is Ctrl+Alt on Windows). Undo/Redo stay disabled (no undo stack in Blitz). Adds Cmd+Alt+B, Cmd+. , Cmd+M, Cmd+Shift+W (menu/keyboard parity) and Cmd+Shift+/ shortcuts; on macOS AppKit reserves Shift+Cmd+/ for Help-menu search, so that binding only fires on other platforms. Zoom In shows as Cmd+= in the menu: muda's only "+" key (`NumpadAdd`) sets the numeric-pad flag and would stop matching the main-keyboard key, so `Equal` is kept (the chord still zooms in).
