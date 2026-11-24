---
slug: "structured-diagnostics"
kind: "feature"
title: "Structured diagnostics"
status: "done"
aliases:
  [
    "F-050"
  ]
requirements:
  [
    "browser-page-script-failures-logged"
  ]
gherkin: "tests/features/structured-diagnostics.feature"
tests:
  [
    "`logging::tests::*`",
    "`js::diagnostics::tests::*`",
    "`diagnostics::uncaught_script_error_is_logged_with_context`",
    "`diagnostics::glide_log_warn_suppresses_informational_console_messages`",
    "`diagnostics::dynamically_inserted_script_runs`",
    "`diagnostics::dynamic_script_fetch_failure_is_reported`",
    "`diagnostics::module_script_runs`",
    "`diagnostics::{module_import_missing_dependency_is_logged,bare_module_specifier_is_reported_as_unsupported}`",
    "`compat::subprocess::tests::first_uncaught_from_structured_log`",
    "manual structured-diagnostics#1"
  ]
mcp: "Schema and level rules live in `AGENTS.md` (\"Logging and observability\"). `js.unsupported` reports unsupported module imports and dynamic script fetch failures with `feature` + `detail`; `js.caps` lists capability probes that failed on that page; `js.page` logs script discovery and one \"settled\" summary (`status=clean|degraded`, counts, `error_kinds`) per load, at `debug` when clean and `info` when degraded. Per-script timing is `debug`. The compat runner parses structured `msg=` fields."
manual:
  [
    {
      "steps": "Run `cargo run --release -- tests/fixtures/js-throw.html` from a terminal, then repeat with `GLIDE_LOG=error`",
      "expected": "The first run prints one `level=ERROR target=\"js.uncaught\"` record with `page_url`, `script`, `line`, `error_kind` and `hint`; console info is also logged. With `GLIDE_LOG=error`, only error records are printed",
      "aliases": [
        "M-037"
      ]
    }
  ]
---

Script, loader, DOM-batch and profile failures, unimplemented page features, missing capability probes and blank renders are logged as parseable `key=value` records with page/script context, a kind and a root-cause hint. `GLIDE_LOG` controls verbosity (`off`–`debug`).
