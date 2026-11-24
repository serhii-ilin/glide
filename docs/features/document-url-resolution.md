---
slug: "document-url-resolution"
kind: "feature"
title: "Document address and reflected URL attributes"
status: "done"
aliases:
  [
    "F-080"
  ]
requirements:
  [
    "document-address-reflected-url-attributes"
  ]
gherkin: "tests/features/document-url-resolution.feature"
tests:
  [
    "`js::integration::{location_origin_and_href_are_absolute_strings,document_url_and_base_uri_match_page_address,parser_async_script_src_resolves_against_document,link_href_and_anchor_href_resolve_against_document,url_constructor_matches_relative_base_and_throws_on_undefined_href,url_constructor_parses_location_href,glide_url_parse_accepts_document_href,document_current_script_null_for_module_and_set_for_async_classic}`",
    "`headless_js::webpack_loadable_chunk_axios_url_init_succeeds`",
    "manual document-url-resolution#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "`GLIDE_LOG=error cargo run --release -- https://meduza.io/` (page JavaScript on). Let scripts settle",
      "expected": "The homepage loads without `js.uncaught` `invalid_url` / `TypeError: Invalid URL`; article shell and navigation render instead of a blank app",
      "aliases": [
        "M-061"
      ]
    }
  ]
---

The document address, `location.origin`, and reflected `src`/`href` attributes resolve against the active page URL, so webpack chunk loaders and axios URL init on meduza.io do not throw `Invalid URL`.

## Notes

Domino keeps `document._address` at `createWindow('')` until Glide syncs it on init/navigation (reflected `src`/`href` use `_resolve`). The `URL` constructor must not pass an explicit `undefined` base into `__glide_url_parse` (that became base `"undefined"` and broke `new URL(location.href)` in axios chunk 860).
