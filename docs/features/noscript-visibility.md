---
slug: "noscript-visibility"
kind: "feature"
title: "Noscript visibility follows the scripting mode"
status: "done"
aliases:
  [
    "F-053"
  ]
requirements:
  [
    "noscript-fallback-markup-hidden-scripting"
  ]
gherkin: "tests/features/noscript-visibility.feature"
tests:
  [
    "`parse::tests::noscript_is_hidden_when_scripting_is_enabled`",
    "`parse::tests::noscript_is_visible_when_scripting_is_disabled`",
    "manual noscript-visibility#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/noscript-refresh.html` with page JavaScript enabled",
      "expected": "The noscript fallback meta tag is not shown as text; \"Visible page content\" starts at the top. Toggle JavaScript off and reload: the fallback content is visible",
      "aliases": [
        "M-039"
      ]
    }
  ]
---

Fallback `<noscript>` markup, including meta-refresh tags, is hidden while page scripts are enabled and shown when scripts are disabled.

## Notes

Blitz's `@media (scripting)` UA rule is not evaluated by Stylo here, so Glide appends an explicit UA `noscript { display: none !important; }` rule when scripting is enabled.
