---
slug: "custom-element-rendering"
kind: "feature"
title: "Server-rendered custom-element fallbacks"
status: "done"
aliases:
  [
    "F-073"
  ]
requirements:
  [
    "page-hides-custom-element-shell"
  ]
gherkin: "tests/features/custom-element-rendering.feature"
tests:
  [
    "`custom_elements::tests::{unupgraded_custom_element_stays_visible,custom_element_names_skip_standard_elements,streamed_template_replaces_placeholder,unmatched_placeholder_is_left_alone,empty_template_keeps_placeholder,built_in_elements_match_defined_before_upgrade_hammer}`"
  ]
mcp: "n/a (engine behavior; observable through `glide_dom`, `glide_snapshot_text`, `glide_screenshot`)"
manual: []
---

A page that hides its custom-element shell until upgrade still shows the server-rendered markup, and streamed `<template for>` content replaces its `suspense-placeholder`.

## Notes

Built-in HTML elements get `ElementState::DEFINED` at parse so global `:not(:defined)` no longer hides `html`/`body`. The UA visibility rule still unhides unupgraded custom-element subtrees when definitions never run (`{name}:not(:defined) *`). Upgraded elements match `:defined` (custom-element-upgrade). Reddit's streamed `<template for>` suspense runs while parsing.
