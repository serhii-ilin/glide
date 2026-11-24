---
slug: "form-control-mirror"
kind: "feature"
title: "Form control value read-back (Blitz → domino)"
status: "partial"
aliases:
  [
    "F-039"
  ]
requirements:
  [
    "glide-complete-google-searchs-common",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "tests/features/form-control-mirror.feature"
tests:
  [
    "`js::form_controls::tests::control_field_reads_value_attribute`",
    "`js::integration::input_value_round_trips_same_turn_without_stale_readback`",
    "`headless_js::{headless_post_form_navigation_submits_urlencoded_body,headless_post_checkbox_checked_sends_on,gbv_hidden_value_script_update_is_posted}`",
    "`js::apply::tests::hidden_input_entry_list_uses_value_attribute_not_stale_editor`"
  ]
mcp: "n/a (observable via `glide_eval` after typing in the window)"
manual:
  [
    {
      "steps": "Serve a local POST echo (or use `tests/fixtures` with a loopback test page). In the window, type into a form field, run `glide --mcp` headless POST test, or use DevTools → Network after a headless POST fixture",
      "expected": "Typed `input.value` in the Console REPL matches what you typed; headless POST shows the urlencoded body on the server; DevTools Network lists the document row with method **POST**; Back after POST does not resubmit (GET reload of the form URL only)",
      "aliases": [
        "M-056"
      ],
      "covers": [
        "page-navigation"
      ]
    }
  ]
---

Form control `value` and `checked` read back from Blitz into the page DOM on getter access and before form submission, so scripts see the same text and checkedness the user typed.

## Notes

`controlField` and `syncFormControls` DOM RPC in `src/js/form_controls.rs` with getters patched in `dom-glue.js`. Hidden inputs use updated `value` attributes in Blitz entry lists (google-compat). Select/multiple-value controls still mirror the attribute default only.
