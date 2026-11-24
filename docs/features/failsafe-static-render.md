---
slug: "failsafe-static-render"
kind: "feature"
title: "Fail-safe static rendering"
status: "done"
aliases:
  [
    "F-087"
  ]
requirements:
  [
    "pages-scripts-leave-rendered-page"
  ]
gherkin: "tests/features/failsafe-static-render.feature"
tests:
  [
    "`headless_js::{failsafe_blank_body_falls_back_to_static_content,failsafe_hidden_then_throw_falls_back_to_static_content,failsafe_keeps_scripted_content_when_it_renders}`",
    "`control_session::blank_scripted_page_falls_back_to_static_text`",
    "`diagnostics::failsafe_fallback_is_logged`",
    "`headless::tests::noscript_text_hidden_when_scripting_parse_but_shown_for_static_parse`"
  ]
mcp: "n/a (applies to every headless load; observable via `glide_snapshot_text`, `glide_screenshot`)"
manual: []
---

If a page's scripts finish without rendering anything visible, Glide shows the page's server-rendered text and images (with `noscript` content) instead of a blank page, and logs why. Pages that do render are left alone.

## Notes

`headless::static_fallback` re-parses the fetched HTML with scripting off when the scripted document has no visible content (`has_visible_content`) and the static one does; used by `load`/`--dump-text`/screenshot and the MCP headless session (the session's JS host becomes the inert static one). One `js.page` warning with `status="fallback"` records it. A page that is blank either way keeps the existing blank-page warning. Not applied to partially rendered pages, only to blank ones.
