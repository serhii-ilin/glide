---
slug: "iterable-selectors-document-collections"
kind: "feature"
title: "Iterable selectors, document collections, and `Image`"
status: "done"
aliases:
  [
    "F-043"
  ]
requirements:
  [
    "crash-hang-panic-common-pages",
    "run-page-javascript-allow-turning",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`js::integration::query_selector_all_and_live_collections_are_iterable`",
    "`js::integration::form_owner_resolves_named_query_control`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Selector results and document collections are iterable, and `Image` constructs an `img` element.

## Notes

`querySelectorAll` returns a static iterable list. `document.forms` / `images` / `links` / `scripts` no longer throw. `form.q`, `form.elements.q`, and `document.f` resolve named controls. A control's `.form` is its owner, so `input.form.q` is not null. `body.onload` is the window load handler. `new Image(w, h)` creates an `<img>`
