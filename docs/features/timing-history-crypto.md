---
slug: "timing-history-crypto"
kind: "feature"
title: "Performance timing, history state and crypto"
status: "done"
aliases:
  [
    "F-056"
  ]
requirements:
  [
    "page-scripts-read-performance-timing"
  ]
gherkin: "tests/features/timing-history-crypto.feature"
tests:
  [
    "`js::integration::{performance_timing_and_entries_work,performance_timing_snapshot_is_stable,history_push_state_updates_location_and_state,history_back_restores_initial_url,history_go_coercion_and_bounds,history_onpopstate_property_fires,history_forward_and_push_truncates_forward_stack,history_location_aliases_match_after_push_state,history_push_state_invalid_url_throws_syntax_error,history_and_performance_buffers_are_bounded,crypto_get_random_values_and_uuid_work,crypto_rejects_non_integer_views,navigator_host_hints_work}`",
    "`headless_js::{timing_history_crypto_work,navigator_languages_work}`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

Page scripts can read `performance.timing`/navigation entries, update the URL with `history.pushState`/`replaceState` and receive `popstate`, use `crypto.getRandomValues`/`randomUUID`, and read navigator language, platform and concurrency hints, so app shells do not fall back when these globals are missing.

## Notes

`performance.timing` uses the page clock and is not a real network timing breakdown. Session history stack unification is unified-session-history. `history.state` is cloned via JSON round-trip. `crypto.subtle` is not implemented. Relative `pushState` URLs on `about:blank` or `data:` pages are not supported when the document origin cannot be resolved for SecurityError checks.
