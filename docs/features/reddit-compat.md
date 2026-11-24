---
slug: "reddit-compat"
kind: "feature"
title: "Reddit compatibility"
status: "done"
aliases:
  [
    "F-085"
  ]
requirements:
  [
    "glide-exposes-dom-tree-script"
  ]
gherkin: "tests/features/reddit-compat.feature"
tests:
  [
    "`js::integration::{reddit_dom_api_get_root_node_connected,reddit_get_root_node_shadow_host_composed,custom_element_define_get_root_node_in_connected_callback,reddit_dom_api_is_connected,reddit_dom_api_contains,reddit_dom_api_closest,reddit_dom_api_toggle_attribute,reddit_dom_api_get_attribute_names,reddit_dom_api_replace_children,reddit_dom_api_append_prepend,reddit_dom_api_before_after_replace_with,reddit_parent_node_append_null_stringifies,reddit_append_moves_document_fragment_children,reddit_append_inclusive_ancestor_throws_and_leaves_tree,reddit_replace_children_self_throws,reddit_document_append_text,reddit_css_style_sheet_constructs_and_applies_adopted_sheet,reddit_attach_shadow_renders_and_reports_root,reddit_attach_shadow_content_is_rendered,reddit_shadow_root_adopted_style_sheets_apply_inside_root}`",
    "`headless_js::{reddit_custom_element_get_root_node_fixture,reddit_polyfills_loader_loopback,reddit_polyfills_loader_missing_fires_error,reddit_js_challenge_redirect,reddit_js_challenge_plain_home_rejected,reddit_dom_api_smoke_fixture}`",
    "`diagnostics::reddit_compat_composed_fixture_clean_diagnostics`",
    "manual reddit-compat#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`, `glide_snapshot_text`, structured logs)"
manual:
  [
    {
      "steps": "Run `GLIDE_LOG=debug cargo run --release -- --dump-text tests/fixtures/reddit-compat/reddit-composed.html`",
      "expected": "Dump text contains `REDDIT-COMPOSED-OK`; stderr has `msg=\"page scripts settled\"` with `status=\"clean\"` and no `target=\"js.uncaught\"`",
      "aliases": [
        "M-066"
      ]
    }
  ]
---

Reddit's client checks can call `getRootNode` (with `{composed:true}` across shadow-root stubs), spec ParentNode APIs on `Document` and elements, load polyfill scripts from loopback `src` URLs with accurate `load`/`error` handlers, complete a `js_challenge` redirect to the home document without `js.uncaught` or a degraded `js.page` summary, construct and adopt a `CSSStyleSheet`, and attach shadow roots from custom elements (`attachShadow`) so components such as Reddit's Lit elements render instead of throwing.

## Notes

DOM patches in `dom-glue.js` `installDomTreeApiPatches`. `contains`/`closest`/`toggleAttribute`/`getAttributeNames`/`before`/`after`/`replaceWith` are domino-native regression tests. `CSSStyleSheet` is implemented in `bootstrap.js`; adopted sheets are applied by inserting a `<style data-glide-adopted>` element, and `replace`/`replaceSync` share one rule list. `attachShadow` returns a `display:contents` container element inserted as the host's first child (Blitz has no shadow trees); `host`/`mode`/`shadowRoot` (null for closed)/`adoptedStyleSheets`/`getElementById`/`ShadowRoot` instanceof and `getRootNode` (composed crosses to the host) work, a second attach throws `InvalidStateError`. Limits: styles are not scoped to the root, `<slot>` is not projected (light children follow the container), and the container is visible in `host.children`. Deferred (non-critical review findings, no crash or hang): `document.adoptedStyleSheets` getter returns a copy so in-place `push` is lost; `disabled` and `media` are ignored; `@import` is not stripped in `replaceSync`; `cssRules` entries lack `selectorText`, `style` and per-type `type`; a stray `}` collapses later rules into one; the adopted `<style>` is not re-appended after later head styles.
