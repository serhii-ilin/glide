---
slug: "text-editing-macos-standard-keybindings"
kind: "feature"
title: "Text editing with macOS standard keybindings"
status: "done"
aliases:
  [
    "F-006"
  ]
requirements:
  [
    "documented-keyboard-shortcut-text-editing"
  ]
gherkin: "backfill pending"
tests:
  [
    "`keyboard::tests::{macos_standard_keybinding_table,headless_address_bar_typing_and_backspace,headless_word_delete_in_address_bar,headless_address_bar_select_all_then_backspace_clears,headless_address_bar_cmd_backspace_clears_the_line,focus_address_field_selects_text_so_backspace_clears_it,headless_address_bar_copy_and_paste,headless_page_input_copy_ignores_unidentified_logical_key,headless_page_selection_copy}`",
    "manual browser-shortcuts-cmd-ctrl-l#5",
    "text-editing-macos-standard-keybindings#1–text-editing-macos-standard-keybindings#4",
    "text-editing-macos-standard-keybindings#3–text-editing-macos-standard-keybindings#2"
  ]
mcp: "n/a (chrome UI)"
manual:
  [
    {
      "steps": "Focus the address bar, type `hello`, Cmd/Ctrl+A, Cmd/Ctrl+C, move caret to end, Cmd/Ctrl+V",
      "expected": "Field shows `hellohello`",
      "aliases": [
        "M-014"
      ]
    },
    {
      "steps": "Open a page with a paragraph, drag to select a word, press Cmd/Ctrl+C, focus the address bar, Cmd/Ctrl+V",
      "expected": "The selected word is pasted into the address bar",
      "aliases": [
        "M-031"
      ]
    },
    {
      "steps": "Focus the address bar on a URL, press Cmd+A then Backspace. Separately, click at the end of the URL and press Cmd+Backspace",
      "expected": "Both clear the address bar",
      "aliases": [
        "M-030"
      ]
    },
    {
      "steps": "On macOS: type `one two`, Option+Left/Right moves by word; Option+Backspace deletes the word before the caret",
      "expected": "Caret and text follow macOS editing conventions",
      "aliases": [
        "M-015"
      ]
    }
  ]
---

The address bar and page text fields support normal editing keys and macOS word/line shortcuts.

## Notes

Cmd/Ctrl+A selects the field. Cmd/Ctrl+C/X/V copy, cut, and paste in the address bar, a page field, and a page text selection. Cmd+Backspace deletes to the start of the line. Cmd+L selects the address bar text
