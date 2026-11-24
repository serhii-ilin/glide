---
slug: "devtools-shell"
kind: "feature"
title: "DevTools panel shell"
status: "done"
aliases:
  [
    "F-059"
  ]
requirements:
  [
    "user-open-docked-developer-panel"
  ]
gherkin: "tests/features/devtools-shell.feature"
tests:
  [
    "`devtools::tests::*`",
    "`chrome::tests::classifies_devtools_commands`",
    "`chrome::tests::closed_devtools_leaves_stage_without_open_class`",
    "`chrome::tests::open_bottom_dock_sets_height_with_ui_var`",
    "`chrome::tests::open_right_dock_sets_width_with_ui_var`",
    "`chrome::tests::devtools_tab_switch_updates_active_pane`",
    "`keyboard::tests::{readme_shortcut_table,dispatch_devtools_esc_closes_panel,devtools_toggle_restores_page_input_focus}`",
    "`profile::tests::{settings_round_trip,devtools_settings_default_on_legacy_file}`",
    "manual devtools-shell#1–devtools-shell#3"
  ]
mcp: "planned mcp-tabs-devtools-state-js (`glide_devtools_state`)"
manual:
  [
    {
      "steps": "Open a local page. Press F12, then Meta+Alt+I or Ctrl+Shift+I (whichever is not the chord you used), then View → Toggle Developer Tools",
      "expected": "Each chord and the menu item toggle the panel open and closed; when closed the page fills the stage",
      "aliases": [
        "M-041"
      ]
    },
    {
      "steps": "With DevTools open bottom-docked, drag the splitter; quit and relaunch; switch to right dock via glide://devtools-dock-right",
      "expected": "Dragged size clamps visibly; size and dock survive restart; right dock resizes horizontally and page zoom does not change panel chrome size",
      "aliases": [
        "M-042"
      ]
    },
    {
      "steps": "Open DevTools, click a tab header, focus the panel (tab strip or aside), press Esc",
      "expected": "Tab links switch Console / Elements / Network; Escape closes the panel",
      "aliases": [
        "M-043"
      ]
    }
  ]
---

The user can toggle a docked developer tools panel (bottom or right) with Console, Elements and Network tabs; size and dock persist but the panel always starts closed.

## Notes

Network tab body is placeholder content; splitter drag resize is manual (devtools-shell#2).
