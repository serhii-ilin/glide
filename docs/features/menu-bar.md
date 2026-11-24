---
slug: "menu-bar"
kind: "feature"
title: "In-window menu bar"
status: "done"
aliases:
  [
    "F-045"
  ]
requirements:
  [
    "menus-expose-file-edit-view",
    "provide-safari-like-chrome-tab"
  ]
gherkin: "tests/features/menu-bar.feature"
tests:
  [
    "`chrome::tests::menu_bar_renders_all_menus`",
    "`chrome::tests::in_window_menu_bar_omitted_when_disabled`",
    "`chrome::tests::in_window_dropdown_renders_from_menu_model`",
    "`chrome::tests::menu_open_command_exposes_items`",
    "`chrome::tests::menu_close_hides_dropdown`",
    "`chrome::tests::menu_action_closes_dropdown`",
    "`chrome::tests::toggle_open_menu_switches_or_closes`",
    "`chrome::tests::pointer_on_dropdown_or_trigger_keeps_menu_open`",
    "`chrome::tests::pointer_outside_dropdown_dismisses_menu`",
    "`chrome::tests::history_disables_back_and_forward_when_unavailable`",
    "`chrome::tests::history_enables_back_and_forward_when_available`",
    "`chrome::tests::close_tab_is_disabled_with_a_single_tab`",
    "`chrome::tests::tab_cycling_is_disabled_with_a_single_tab`",
    "`keyboard::tests::escape_closes_menu_before_stop`",
    "manual menu-bar#1"
  ]
mcp: "n/a (chrome UI)"
manual:
  [
    {
      "steps": "Open each of the File, Edit, View, History, Bookmarks, Tabs and Help menus; choose New Tab from File, Back from History and Zoom In from View. Open File again, click the page, reopen File and click the File title, reopen File and press Esc (with no text field focused), reopen File and press the toolbar Back button, reopen File and Cmd-Tab (or click another app) to lose focus. Start loading a slow page and press Esc with no menu open",
      "expected": "Each dropdown opens and closes; each action runs on the active tab; opening another menu replaces the previous dropdown. A click on the page closes the open dropdown and still reaches the page. Clicking the open menu title toggles it closed. Escape closes the open menu without stopping an in-flight load. Clicking Back closes the dropdown and still goes back. Losing window focus closes the dropdown. Escape with no menu open still stops a load",
      "aliases": [
        "M-034"
      ]
    }
  ]
---

A File / Edit / View / History / Bookmarks / Tabs / Help menu bar opens dropdowns whose items run browser commands.

## Notes

macOS-style menus: a compact borderless bar sharing the toolbar surface, opaque rounded dropdowns with accent-highlighted items, system-font shortcut hints (⇧, ⌘, ⇥ via the Apple Symbols fallback), separators, headings and disabled items; unavailable commands are disabled (or hidden) rather than active. Dropdowns also dismiss on any pointer press outside the dropdown or a menu title, on clicking the open title (toggle), on Escape (when no text field is focused), and when the window loses focus; hover-to-switch between titles is not implemented. The in-window bar is shown on non-macOS platforms only (on macOS it is replaced by the native menu bar, native-menu-bar) and its dropdowns are rendered from the shared model in `src/menu.rs`.
