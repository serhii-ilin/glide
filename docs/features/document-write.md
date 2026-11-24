---
slug: "document-write"
kind: "feature"
title: "`document.write`"
status: "done"
aliases:
  [
    "F-034"
  ]
requirements:
  [
    "glide-complete-google-searchs-common",
    "wpt-dom-nodes-15-subtests"
  ]
gherkin: "backfill pending"
tests:
  [
    "`src/js/dom-glue.js`",
    "`tests/headless_js.rs::{document_write_inserts_after_current_script,async_document_write_does_not_wipe_the_page,document_write_fetches_external_script_before_load}`",
    "`docs/compat/REPORT.md`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`)"
manual: []
---

`document.write` inserts markup during parsing and replaces the document after load.

## Notes

Inserts after `document.currentScript` during a classic parser script. Async and defer writes are ignored. After load, writes from timers and event handlers replace the document. Inline scripts in the written markup run; external `src` scripts inserted before `load` are fetched and executed (google-compat).
