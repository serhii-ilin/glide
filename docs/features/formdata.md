---
slug: "formdata"
kind: "feature"
title: "FormData and multipart bodies"
status: "done"
aliases:
  [
    "F-082"
  ]
requirements:
  [
    "page-scripts-build-formdata-including"
  ]
gherkin: "tests/features/formdata.feature"
tests:
  [
    "`http::tests::{encode_multipart_escapes_disposition_params,encode_multipart_text_value_normalizes_lf_to_crlf,encode_multipart_file_part_includes_filename_and_content_type,encode_url_search_params_uses_url_standard_not_html_newlines}`",
    "`js::fetch_body::tests::{url_search_params_body_sets_form_content_type,url_search_params_content_type_is_exact_charset_suffix,form_data_keeps_existing_content_type_header,blob_body_uses_declared_type}`",
    "`js::apply::tests::{build_fetch_request_formdata_multipart_posts_to_loopback,build_fetch_request_formdata_respects_request_content_type,build_fetch_request_url_search_params_sets_urlencoded}`",
    "`js::integration::{form_data_get_returns_first_entry_set_replaces_first_only,form_data_append_set_and_iterate,file_last_modified_zero_preserved,form_data_from_form_matches_control_values,form_data_from_form_with_submitter_includes_button,fetch_formdata_posts_multipart_to_loopback,fetch_formdata_blob_without_filename_posts_default_blob_name,xhr_url_search_params_sends_urlencoded}`",
    "manual formdata#1"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual:
  [
    {
      "steps": "Serve a local POST echo on `127.0.0.1` (or run the automated loopback tests with `cargo nextest run -E 'test(form_data)'`). In the window Console, run: `const fd = new FormData(); fd.append('q','hello'); fd.append('f', new File(['x'], 'a.txt', {type:'text/plain'})); await fetch('http://127.0.0.1:…/', {method:'POST', body: fd}).then(r=>r.text())` and `const x = new XMLHttpRequest(); x.open('POST','http://127.0.0.1:…/'); x.send(new URLSearchParams({a:'1'}));`",
      "expected": "Server sees `multipart/form-data` with a boundary and both text and file parts; `URLSearchParams` POST uses `application/x-www-form-urlencoded`; DevTools Network shows POST with the expected Content-Type",
      "aliases": [
        "M-064"
      ]
    }
  ]
---

Page scripts can use `FormData` (empty, from a form with submitter, append/set/get/getAll/has/delete, and iteration) with `Blob`/`File` filenames; `fetch()` and `XMLHttpRequest.send()` send `multipart/form-data` with a correct boundary, `URLSearchParams` sends `application/x-www-form-urlencoded`, and a `Blob` body uses `blob.type` as Content-Type, with UTF-8 and HTML escaping for names and filenames.

## Notes

Form entry lists reuse `blitz_dom::form::construct_entry_list`. Multipart encoding is hand-written in `src/http.rs` (no new crate). RPC bodies are JSON (`kind`: `formData` / `urlSearchParams` / `blob` / `text`). Caller `Content-Type` on FormData fetch/XHR is preserved while multipart bytes are still sent.
