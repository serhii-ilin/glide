---
slug: "web-api-shims"
kind: "feature"
title: "Common Web API compatibility shims"
status: "done"
aliases:
  [
    "F-051"
  ]
requirements:
  [
    "common-web-apis-used-third"
  ]
gherkin: "tests/features/web-api-shims.feature"
tests:
  [
    "`headless_js::common_web_api_shims_work`",
    "`js::integration::url_search_params_iterates_and_mutates`",
    "`diagnostics::glide_log_warn_suppresses_informational_console_messages`",
    "manual web-api-shims#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "Run `cargo run --release -- --dump-text tests/fixtures/web-api-shims.html`",
      "expected": "The page reads `SHIMS-OK`; no `atob`, `matchMedia`, `console.info`, storage, `AbortController`, `MutationObserver` or `DOMStringMap`/`dataset` errors appear in the terminal",
      "aliases": [
        "M-038"
      ]
    }
  ]
---

Base64 helpers, `matchMedia`, `console.info`/`debug`, Web Storage (see web-storage), `AbortController`, a constructible `MutationObserver`, `DOMStringMap`/`dataset`, and a complete iterable `URLSearchParams` are available to page scripts so third-party code does not crash on missing globals.

## Notes

Shims only for `matchMedia` (`matches` always false) and abort (does not cancel host work). Functional `MutationObserver` delivery is observers-idle; persistent Web Storage is web-storage. New probe keys were added; the full compat report is refreshed on the next network run.
