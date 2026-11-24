---
slug: "import-maps"
kind: "feature"
title: "Import maps"
status: "done"
aliases:
  [
    "F-084"
  ]
requirements:
  [
    "page-es-module-scripts-scripts",
    "parser-discovered-script-type-importmap"
  ]
gherkin: "tests/features/import-maps.feature"
tests:
  [
    "`js::import_map::tests::*`",
    "`js::scripts::tests::collects_import_map_scripts_in_document_order`",
    "`headless_js::{import_map_resolves_bare_module_specifier,import_map_dynamic_import_resolves_bare_specifier,invalid_import_map_is_ignored_and_later_map_applies,bare_specifier_without_import_map_entry_is_rejected,html_script_element_supports_importmap}`",
    "`diagnostics::{invalid_import_map_logs_loader_error,unmapped_bare_specifier_with_import_map_logs_hint}`",
    "manual import-maps#1"
  ]
mcp: "n/a (engine behavior; observable through `glide_eval` and `glide_snapshot_text`)"
manual:
  [
    {
      "steps": "Run `cargo run -- --dump-text tests/fixtures/importmap-basic.html` with JS enabled (default)",
      "expected": "Dump text contains `mapped 88` from a bare `app-dep` import resolved through the inline import map",
      "aliases": [
        "M-065"
      ]
    }
  ]
---

Inline `<script type="importmap">` JSON merges in document order and resolves bare specifiers for module graphs and dynamic `import()`, with scoped and trailing-slash prefix maps; bad JSON or keys log `js.loader` and are skipped; `HTMLScriptElement.supports('importmap')` is true.

## Notes

Resolution and merge live in `src/js/import_map.rs`; the glide-js worker applies maps on `RegisterImportMap` before each parser script batch. Integrity fields log `js.loader` and are not enforced. Module workers are not implemented yet; resolution reuses the same worker hook when they land.
