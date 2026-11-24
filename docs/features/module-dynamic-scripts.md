---
slug: "module-dynamic-scripts"
kind: "feature"
title: "Module and dynamic script execution"
status: "done"
aliases:
  [
    "F-074"
  ]
requirements:
  [
    "page-es-module-scripts-scripts"
  ]
gherkin: "tests/features/module-dynamic-scripts.feature"
tests:
  [
    "`js::scripts::tests::{module_scripts_are_deferred_or_async,nomodule_classic_scripts_are_omitted}`",
    "`js::modules::tests::*`",
    "`headless_js::{inline_module_script_runs,module_runs_between_parser_scripts_and_dom_content_loaded,parser_async_script_runs_before_load_event,parser_async_missing_script_fires_error_before_load_event,external_module_script_runs,module_import_resolves_relative_dependency,module_import_missing_dependency_leaves_page_intact,module_graph_handles_import_cycle,dynamic_import_resolves_module,import_meta_url_matches_module_resource,bare_module_specifier_is_rejected,http_module_import_resolves,http_module_script_fetch_cookie_same_origin_and_not_cross_port,dynamically_inserted_script_runs,dynamically_inserted_external_script_runs_and_fires_load,inner_html_scripts_do_not_run,classic_script_dynamic_import_resolves_relative_module,classic_script_bare_dynamic_import_rejects_without_aborting,nomodule_classic_script_is_skipped,module_cache_evaluates_same_url_once,dynamic_import_runs_after_timers_on_pump,module_imports_data_url}`",
    "`diagnostics::{dynamically_inserted_script_runs,dynamic_script_fetch_failure_is_reported,module_script_runs,module_import_missing_dependency_is_logged,bare_module_specifier_is_reported_as_unsupported}`",
    "`js::apply::tests::parse_data_url_percent_decodes_plain_payload`"
  ]
mcp: "n/a (engine behavior; observable through `glide_eval`, `glide_dom` and `glide_snapshot_text`)"
manual:
  [
    {
      "steps": "Run `cargo run -- --dump-text tests/fixtures/module-import-ok.html` with JS enabled (default)",
      "expected": "Dump text contains `imported 7` from the resolved `./module-dep.js` graph",
      "aliases": [
        "M-055"
      ]
    }
  ]
---

ES module scripts and scripts inserted by page scripts run (inline or fetched), fire load/error events, and follow module defer/async ordering, so app bundles, polyfill loaders and chunk loaders take effect.

## Notes

Module scripts are deferred by default. Parser scripts run classic, then defer, then `DOMContentLoaded`, then async classic/module bodies in discovery order, then failed parser scripts get `error`, then window `load`. Parser-inserted classic `async` scripts delay `load`; failed parser scripts fire `error` before `load`. Classic scripts with `nomodule` are skipped. `import()` from classic scripts resolves against the script URL or document base. The glide-js worker resolves static import graphs and dynamic `import()` with a bounded per-URL module cache (512 entries), cookie-aware `http(s)` script fetch (`SameOrigin` unless `crossorigin=use-credentials`), DOM fetch for `file://`/`data:`, and `import.meta.url` from the module resource name; bare specifiers log `js.unsupported` (`module-import`). Dynamic `import()` returns a promise immediately and loads on the next timer pump. Module MIME types and CORS response checks are not enforced. Parser and dynamically inserted scripts fire `load`/`error` on the `<script>` element. Failed fetches log `js.loader` and mark the page `degraded`. Dynamically inserted scripts follow HTML parser-inserted rules; `innerHTML` scripts do not run. Known gaps: every external source resolves before any script runs (`DOMContentLoaded` does not run sooner when only async blocks); inline `<script type="module" async>` is not spec-ordered relative to other modules; dynamically inserted scripts do not delay load. Import maps are implemented in import-maps. Manual module-dynamic-scripts#1, import-maps#1.
