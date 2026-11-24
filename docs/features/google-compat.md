---
slug: "google-compat"
kind: "feature"
title: "Google Search compatibility"
status: "done"
aliases:
  [
    "F-078"
  ]
requirements:
  [
    "glide-complete-google-searchs-common"
  ]
gherkin: "tests/features/google-compat.feature"
tests:
  [
    "`http::tests::{user_agent_names_glide_not_firefox_81,user_agent_matches_blitz_net_export,navigation_request_includes_accept_language_and_sec_fetch,user_activated_navigation_includes_sec_fetch_user,subresource_request_includes_sec_fetch_and_glide_user_agent,sec_fetch_site_classifies_registrable_domains,script_subresource_sends_script_dest,image_subresource_sends_image_dest,fetch_api_sends_empty_dest_and_cors_mode}`",
    "`js::apply::tests::hidden_input_entry_list_uses_value_attribute_not_stale_editor`",
    "`headless_js::{gbv_hidden_value_script_update_is_posted,form_submit_posts_hidden_value_updated_same_turn,onload_runs_external_challenge_before_load,async_script_onload_reads_challenge_from_global,document_write_fetches_external_script_before_load,document_write_after_load_fetches_external_script}`",
    "`js::diagnostics::tests::{classify_known_fetch_missing_is_missing_api,classify_page_function_missing_is_other_not_missing_api,classify_crypto_subtle_missing_is_missing_api}`",
    "manual google-compat#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval` and structured logs)"
manual:
  [
    {
      "steps": "Run `GLIDE_LOG=warn cargo run --release -- https://www.google.com` (or DuckDuckGo HTML if offline). Let the page settle without typing",
      "expected": "Page loads without `js.uncaught` on the initial navigation; DevTools → Network (or structured stderr) shows a Glide `User-Agent` (not Firefox/81) with `Sec-Fetch-*` on the document request; if you submit a search, the POST body includes `gbv=2` when scripting is enabled",
      "aliases": [
        "M-058"
      ]
    }
  ]
---

Top-level HTTP navigations and subresources send an honest Glide user agent (naming the host OS), `Accept-Language`, and per-request `Sec-Fetch-*` / `Accept` headers; hidden form fields updated by scripts (for example Google's `gbv`) reach the POST entry list; `document.write` fetches and runs external scripts before and after `load`; missing host API names (including dotted names) classify as `missing_api` while page function names classify as `other`.

## Notes

`FetchMetadata` on each HTTP hop drives dest/mode/initiator; `blitz_net::user_agent()` is shared with vendored `third_party/blitz-net`. `form.submit()` skips stale Blitz pull like `requestSubmit`.
